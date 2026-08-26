//! Shared browser pool — ein WebView-Runtime, ein Tab pro Brain (Python `browser_pool.py`).

#[cfg(any(feature = "webview", test))]
use std::collections::HashMap;
#[cfg(any(feature = "webview", test))]
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "webview")]
use std::time::Duration;

#[cfg(feature = "webview")]
use crate::brain::BrainBackend;
#[cfg(any(feature = "webview", test))]
use crate::browser::WebBrainBackend;
#[cfg(any(feature = "webview", test))]
use crate::config::persist_browser_tabs;
#[cfg(feature = "webview")]
use crate::config::{
    encapsulated_profile_dir, prepare_shared_profile_for_clone, runtime_pool_profile_dir,
    shared_profile_dir, ProfileClonePlanner,
};
#[cfg(feature = "webview")]
use crate::page_driver::PageDriver;
#[cfg(feature = "webview")]
use crate::webview_runtime::{WebViewPageDriver, WebViewRuntime};

/// Max. Versuche den geteilten Browser zu nutzen, bevor ein Brain auf die
/// gekapselte Fallback-Instanz faellt. Spiegelt circuit_breaker::DEFAULT_MAX_FAILURES.
/// Nur im `webview`-Pfad referenziert; ohne das Feature (CI-Kernbuild) ungenutzt.
#[cfg(any(feature = "webview", test))]
const POOL_FALLBACK_RETRIES: u32 = 3;

#[cfg(any(feature = "webview", test))]
struct PooledTab {
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    view_id: u64,
    #[cfg(feature = "webview")]
    driver_proto: WebViewPageDriver,
    refs: u32,
}

/// Gekapselte, isolierte Browser-Instanz (Fallback), die nach dem Scheitern des
/// geteilten Browsers fuer ein Brain gestartet wird. Eigenes WebView-Runtime
/// (eigener Prozess) auf einem Linked-Clone/Delta des kanonischen Profils
/// (`profiles/encapsulated/<brain>_<runstamp>`). Nie zurueckgeschrieben.
#[cfg(feature = "webview")]
struct EncapsulatedInstance {
    runtime: WebViewRuntime,
    profile_dir: PathBuf,
    driver_proto: WebViewPageDriver,
    refs: u32,
}

/// Prozessweiter Singleton: ein persistentes Profil, lazy Tab je `brain_id`.
pub struct BrowserPool {
    #[cfg(feature = "webview")]
    runtime: Option<WebViewRuntime>,
    #[cfg(any(feature = "webview", test))]
    tabs: HashMap<String, PooledTab>,
    #[cfg(feature = "webview")]
    encapsulated: HashMap<String, EncapsulatedInstance>,
}

/// Buchhaltung nach einem Navigationsversuch auf einer BESTEHENDEN Instanz.
///
/// Der Refcount darf erst nach ERFOLGREICHER Navigation steigen. Vorher wurde
/// er davor erhoeht; schlug die Navigation fehl, blieb er dauerhaft zu hoch
/// und der Tab wurde nie geschlossen, weil `stop_brain` nur bis auf den
/// geleakten Wert herunterzaehlt.
///
/// Bewusst eine freie Funktion ueber `&mut u32` statt einer Methode auf
/// `PooledTab`: dessen `driver_proto` haengt an `feature = "webview"`, eine
/// Methode waere ohne echten Browser nicht pruefbar. So laeuft im Test
/// GENAU der Code, den auch der Produktionspfad aufruft.
#[cfg(any(feature = "webview", test))]
pub(crate) fn note_navigation(refs: &mut u32, navigated: Result<(), String>) -> Result<(), String> {
    navigated?;
    *refs = refs.saturating_add(1);
    Ok(())
}

/// Schliesst einen vom Pool besessenen Runtime-Klon, sobald keine laufende
/// Referenz mehr davon abhaengt. Persistierte Tabs mit `refs == 0` duerfen
/// waehrend einer noch aktiven Shared-Ausfuehrung offen bleiben; sie duerfen
/// aber nicht den letzten Stop am erforderlichen Write-back vorbeileiten.
#[cfg(any(feature = "webview", test))]
fn complete_required_teardown<F>(
    owns_runtime: bool,
    has_live_refs: bool,
    teardown: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    if owns_runtime && !has_live_refs {
        teardown()?;
    }
    Ok(())
}

/// Besitzt ein frisch angelegtes Profil-Klon-Verzeichnis und entfernt es
/// wieder, solange es nicht per [`CloneGuard::keep`] uebernommen wurde.
///
/// Vorher raeumte jeder Fehlerpfad einzeln auf (`remove_dir_all` an drei
/// Stellen). Das ist Symptomkur: der naechste hinzugefuegte Fehlerpfad
/// vergisst es wieder, und jeder Fehlversuch laesst dann eine weitere
/// verwaiste Profilkopie im Datenverzeichnis zurueck. Mit Drop-Semantik kann
/// das Aufraeumen nicht mehr vergessen werden — es passiert beim Verlassen
/// des Gueltigkeitsbereichs, auch bei frueher Rueckkehr oder Panic.
///
/// Bewusst NICHT hinter `#[cfg(feature = "webview")]`: so ist die Invariante
/// ohne echten Browser pruefbar.
#[cfg(any(feature = "webview", test))]
pub(crate) struct CloneGuard {
    dir: Option<PathBuf>,
}

#[cfg(any(feature = "webview", test))]
impl CloneGuard {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir: Some(dir) }
    }

    /// Uebernimmt den Klon endgueltig: ab hier wird NICHT mehr geloescht.
    pub(crate) fn keep(mut self) -> PathBuf {
        self.dir.take().unwrap_or_default()
    }
}

#[cfg(any(feature = "webview", test))]
impl Drop for CloneGuard {
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

impl BrowserPool {
    fn new() -> Self {
        Self {
            #[cfg(feature = "webview")]
            runtime: None,
            #[cfg(feature = "webview")]
            encapsulated: HashMap::new(),
            #[cfg(any(feature = "webview", test))]
            tabs: HashMap::new(),
        }
    }

    /// Globaler Pool (serialisiert über Mutex).
    pub fn global() -> &'static Mutex<BrowserPool> {
        static POOL: OnceLock<Mutex<BrowserPool>> = OnceLock::new();
        POOL.get_or_init(|| Mutex::new(BrowserPool::new()))
    }

    #[cfg(test)]
    pub fn reset_for_tests() {
        if let Ok(mut pool) = Self::global().lock() {
            pool.shutdown_force();
        }
    }

    /// Startet oder reaktiviert das Brain-Tab und hängt den Driver ans Backend.
    ///
    /// `profile_override` erlaubt es, statt des Shared-Profils ein isoliertes
    /// Laufzeit-Profil zu nutzen (z.B. eine Swarm-Teilkopie aus
    /// `config::prepare_swarm_profile`). `None` → `shared_profile_dir()`.
    #[cfg(any(feature = "webview", test))]
    pub fn start_brain(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        profile_override: Option<PathBuf>,
    ) -> Result<(), String> {
        #[cfg(not(feature = "webview"))]
        {
            let _ = (backend, headless, profile_override);
            Err(crate::page_driver::webview_unavailable().to_string())
        }
        #[cfg(feature = "webview")]
        {
            let brain_id = backend.brain_id().to_lowercase();
            self.ensure_runtime(headless)?;

            let runtime = self
                .runtime
                .as_ref()
                .ok_or("Shared-WebView nicht gestartet")?;

            if let Some(tab) = self.tabs.get_mut(&brain_id) {
                let mut driver = tab.driver_proto.clone();
                let navigated = driver
                    .navigate(backend.brain_url(), Duration::from_secs(30))
                    .map_err(|e| e.to_string());
                note_navigation(&mut tab.refs, navigated)?;
                backend.attach_page_driver(Box::new(driver));
                return Ok(());
            }

            // Master-Hauptprofil NIE direkt öffnen: der Betrieb arbeitet in
            // einer frischen Laufzeit-Kopie (siehe runtime_pool_profile_dir),
            // damit die Logins im Master unangetastet bleiben.
            let profile = match profile_override {
                Some(profile) => profile,
                None => runtime_pool_profile_dir()?,
            };
            let mut driver = runtime
                .open_page(&profile, backend.brain_url(), headless, &brain_id)
                .map_err(|e| e.to_string())?;
            // Der Tab ist offen, aber noch nirgends registriert. Scheitert die
            // Navigation, faende ihn kein `stop_brain` je wieder — er bliebe
            // als Waise im Runtime stehen und haielte sein Profil belegt.
            if let Err(e) = driver.navigate(backend.brain_url(), Duration::from_secs(30)) {
                let _ = runtime.close_page(driver.view_id());
                return Err(e.to_string());
            }
            let view_id = driver.view_id();
            let driver_proto = driver.clone();
            self.tabs.insert(
                brain_id,
                PooledTab {
                    view_id,
                    driver_proto,
                    refs: 1,
                },
            );
            backend.attach_page_driver(Box::new(driver));
            Ok(())
        }
    }

    /// Wie `start_brain`, aber mit Resilienz: der geteilte Browser wird bis zu
    /// `POOL_FALLBACK_RETRIES` Mal versucht (pro Brain gezaehlt); scheitert er
    /// durchgehend, faellt das Brain auf eine gekapselte, isolierte Instanz zurueck
    /// (eigener WebView-Prozess, Linked-Clone des kanonischen Profils).
    #[cfg(feature = "webview")]
    pub fn start_brain_resilient(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        profile_override: Option<PathBuf>,
    ) -> Result<(), String> {
        let brain_id = backend.brain_id().to_lowercase();

        // Bereits gekapselt -> reuse (Refs++), kein erneuter Shared-Versuch.
        if self.encapsulated.contains_key(&brain_id) {
            return self.start_brain_encapsulated(backend, headless, &brain_id);
        }

        // Expliziter Override (isoliertes Profil) -> kein Shared-Pfad, direkt delegieren.
        if profile_override.is_some() {
            return self.start_brain(backend, headless, profile_override);
        }

        // 1) Shared-Pool-Pfad (Default), bis zu POOL_FALLBACK_RETRIES Versuche.
        for _ in 0..POOL_FALLBACK_RETRIES {
            match self.start_brain(backend, headless, None) {
                Ok(()) => return Ok(()),
                Err(e) => crate::bench_events::eprint_line(&format!(
                    "[browser_pool] Shared-Versuch fuer Brain '{}' fehlgeschlagen: {}",
                    brain_id, e
                )),
            }
        }

        // 2) Fallback: gekapselte Instanz (eigenes Profil-Image, eigenes Runtime).
        crate::bench_events::eprint_line(&format!(
            "[browser_pool] Shared-Browser fuer Brain '{}' nach {} Versuchen fehlgeschlagen -> gekapselte Instanz",
            brain_id, POOL_FALLBACK_RETRIES
        ));
        self.start_brain_encapsulated(backend, headless, &brain_id)
    }

    /// Wie `start_brain`, aber ohne Shared-Pool — Fehlerpfad ohne WebView.
    #[cfg(all(not(feature = "webview"), test))]
    pub fn start_brain_resilient(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        profile_override: Option<PathBuf>,
    ) -> Result<(), String> {
        let _ = (backend, headless, profile_override);
        Err(crate::page_driver::webview_unavailable().to_string())
    }

    /// Startet die gekapselte Fallback-Instanz: Linked-Clone/Delta des kanonischen
    /// Shared-Profils nach `profiles/encapsulated/<brain>_<runstamp>`, eigener
    /// WebView-Runtime (eigener Prozess, kein SingletonLock-Konflikt).
    #[cfg(feature = "webview")]
    fn start_brain_encapsulated(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        brain_id: &str,
    ) -> Result<(), String> {
        // Vorhandene gekapselte Instanz reuse (Refs++).
        if let Some(inst) = self.encapsulated.get_mut(brain_id) {
            let mut driver = inst.driver_proto.clone();
            let navigated = driver
                .navigate(backend.brain_url(), Duration::from_secs(30))
                .map_err(|e| e.to_string());
            note_navigation(&mut inst.refs, navigated)?;
            backend.attach_page_driver(Box::new(driver));
            return Ok(());
        }

        let runstamp = crate::now_run_stamp();
        let clone_dir = encapsulated_profile_dir(brain_id, &runstamp);
        // Der gemeinsame Master darf erst nach OS-Lock und Pending-Recovery gelesen
        // werden; ansonsten kÃ¶nnte ein Crashzustand in den Fallback geklont werden.
        let shared_profile_lock = prepare_shared_profile_for_clone()?;
        let master = shared_profile_dir();
        let plan = ProfileClonePlanner::plan_canonical(&master, &clone_dir, &runstamp);
        ProfileClonePlanner::materialize(&plan)
            .map_err(|e| format!("Profil-Klon fuer Brain '{brain_id}' fehlgeschlagen: {e}"))?;
        drop(shared_profile_lock);

        // Ab hier liegt ein Profil-Klon auf der Platte. Der Guard entfernt ihn
        // auf JEDEM Rueckkehrpfad; nur der Erfolgsfall gibt ihn per keep() frei.
        let guard = CloneGuard::new(clone_dir);
        let rt = WebViewRuntime::launch(guard.path(), headless).map_err(|e| e.to_string())?;
        let mut driver = rt
            .open_page(guard.path(), backend.brain_url(), headless, brain_id)
            .map_err(|e| e.to_string())?;
        if let Err(e) = driver.navigate(backend.brain_url(), Duration::from_secs(30)) {
            let _ = rt.close_page(driver.view_id());
            return Err(e.to_string());
        }
        let driver_proto = driver.clone();
        self.encapsulated.insert(
            brain_id.to_string(),
            EncapsulatedInstance {
                runtime: rt,
                profile_dir: guard.keep(),
                driver_proto,
                refs: 1,
            },
        );
        backend.attach_page_driver(Box::new(driver));
        Ok(())
    }

    /// Gibt eine Referenz frei. Persistierte Tabs bleiben offen, solange noch
    /// eine Shared-Referenz laeuft; beim vollstaendig idle gewordenen, besessenen
    /// Runtime-Klon folgen dagegen immer Teardown und Master-Write-back.
    #[cfg(any(feature = "webview", test))]
    pub fn stop_brain(&mut self, brain_id: &str, persist: Option<bool>) -> Result<(), String> {
        let bid = brain_id.to_lowercase();
        let keep = persist.unwrap_or_else(persist_browser_tabs);

        // Shared-Pool-Tab?
        if let Some(tab) = self.tabs.get_mut(&bid) {
            if tab.refs > 0 {
                tab.refs -= 1;
            }
            if tab.refs > 0 {
                return Ok(());
            }
            #[cfg(feature = "webview")]
            let view_id = tab.view_id;
            if !keep {
                self.tabs.remove(&bid);
                #[cfg(feature = "webview")]
                if let Some(rt) = self.runtime.as_ref() {
                    let _ = rt.close_page(view_id);
                }
            }

            #[cfg(feature = "webview")]
            {
                let owns_runtime = self.runtime.is_some();
                let has_live_refs = self.tabs.values().any(|tab| tab.refs > 0);
                complete_required_teardown(owns_runtime, has_live_refs, || {
                    self.shutdown_shared_runtime_with_result()
                })?;
            }
            return Ok(());
        }

        // Gekapselte Instanz? (eigenes Runtime + eigenes Profilverzeichnis)
        #[cfg(feature = "webview")]
        {
            if let Some(inst) = self.encapsulated.get_mut(&bid) {
                if inst.refs > 0 {
                    inst.refs -= 1;
                }
                if inst.refs > 0 || keep {
                    return Ok(());
                }
            }
            if let Some(inst) = self.encapsulated.remove(&bid) {
                // `inst.runtime` wird beim Verlassen des Blocks gedroppt (WebView-
                // Prozess beendet); danach das geklonte Profilverzeichnis entfernen.
                let EncapsulatedInstance {
                    runtime: _rt,
                    profile_dir,
                    ..
                } = inst;
                let _ = _rt;
                let _ = std::fs::remove_dir_all(&profile_dir);
            }
        }
        Ok(())
    }

    /// Wie `stop_brain(..., persist=true)` — Tab bleibt für den nächsten Hop offen.
    #[cfg(test)]
    pub fn detach_brain(&mut self, brain_id: &str) -> Result<(), String> {
        self.stop_brain(brain_id, Some(true))
    }

    #[cfg(test)]
    pub fn has_tab(&self, brain_id: &str) -> bool {
        self.tabs.contains_key(&brain_id.to_lowercase())
    }

    /// Brain-Namen aller offenen Tabs, alphabetisch.
    ///
    /// Stabile Reihenfolge, damit eine Kachel beim erneuten Anordnen nicht
    /// springt — ein Raster, in dem die Brains bei jedem Aufruf die Plaetze
    /// tauschen, ist unbrauchbar.
    #[cfg(any(feature = "webview", test))]
    pub fn open_brains(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tabs.keys().cloned().collect();
        names.sort();
        names
    }

    /// Ordnet die vorhandenen Brain-Fenster als Kachelraster an — oder parkt sie wieder.
    ///
    /// `Some(area)` verteilt die offenen Tabs auf Kacheln in `area`,
    /// `None` stellt den Zustand ohne Kachelansicht wieder her.
    ///
    /// Gibt zurueck, wie viele Fenster angeordnet wurden. Passen nicht alle in
    /// den Bereich (siehe [`crate::brain_grid::fitting_tile_count`]), werden die
    /// ueberzaehligen bewusst geparkt statt zu Briefmarken gequetscht — der
    /// Aufrufer sieht das an der Differenz zu [`Self::open_brains`] und muss es
    /// melden.
    #[cfg(feature = "webview")]
    /// Zweiter Arrange-Pfad — die ausgelieferte TUI nutzt `brain_wall`.
    #[allow(dead_code)]
    pub fn arrange_brain_grid(
        &self,
        area: Option<crate::brain_grid::Rect>,
    ) -> Result<usize, String> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Ok(0);
        };
        let names = self.open_brains();
        let Some(area) = area else {
            for name in &names {
                if let Some(tab) = self.tabs.get(name) {
                    runtime
                        .set_bounds(tab.view_id, None)
                        .map_err(|e| format!("{name}: {e}"))?;
                }
            }
            return Ok(0);
        };

        let fitting = crate::brain_grid::fitting_tile_count(area, names.len());
        let tiles = crate::brain_grid::grid_layout(area, fitting);
        for (index, name) in names.iter().enumerate() {
            let Some(tab) = self.tabs.get(name) else {
                continue;
            };
            let bounds = tiles.get(index).copied();
            runtime
                .set_bounds(tab.view_id, bounds)
                .map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(fitting)
    }

    /// Holt den Fokus ausdruecklich auf die `index`-te Kachel (0-basiert, in der
    /// stabilen Reihenfolge von [`Self::open_brains`]).
    ///
    /// Nur auf ausdrueckliche Anforderung (Alt+Nummer). Ohne diesen Aufruf
    /// bleiben alle Kachelfenster nicht aktivierbar, damit Enter im Terminal
    /// immer im Terminal landet.
    #[cfg(feature = "webview")]
    pub fn focus_brain_tile(&self, index: usize) -> Result<String, String> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Err("kein Brain-Fenster offen".to_string());
        };
        let names = self.open_brains();
        let name = names
            .get(index)
            .ok_or_else(|| format!("keine Kachel {}", index + 1))?
            .clone();
        let tab = self
            .tabs
            .get(&name)
            .ok_or_else(|| format!("{name}: Tab verschwunden"))?;
        runtime
            .focus_view(tab.view_id, true)
            .map_err(|e| format!("{name}: {e}"))?;
        Ok(name)
    }

    /// Gibt den Fokus von allen Kacheln ans Terminalfenster zurueck (Esc).
    ///
    /// Bewusst ueber alle offenen Tabs: welche Kachel gerade den Fokus hat,
    /// weiss dieser Prozess nicht zuverlaessig — der Nutzer kann sie auch
    /// angeklickt haben. Jede Kachel wieder auf „nicht aktivierbar" zu setzen,
    /// stellt den Normalzustand unabhaengig davon her.
    #[cfg(feature = "webview")]
    pub fn release_brain_focus(&self) -> Result<usize, String> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Ok(0);
        };
        let mut count = 0;
        for name in self.open_brains() {
            if let Some(tab) = self.tabs.get(&name) {
                runtime
                    .focus_view(tab.view_id, false)
                    .map_err(|e| format!("{name}: {e}"))?;
                count += 1;
            }
        }
        Ok(count)
    }

    #[cfg(test)]
    pub fn tab_ref_count(&self, brain_id: &str) -> u32 {
        self.tabs
            .get(&brain_id.to_lowercase())
            .map(|t| t.refs)
            .unwrap_or(0)
    }

    #[cfg(feature = "webview")]
    fn ensure_runtime(&mut self, headless: bool) -> Result<(), String> {
        if self.runtime.is_some() {
            return Ok(());
        }
        let profile = runtime_pool_profile_dir()?;
        let rt = WebViewRuntime::launch(&profile, headless).map_err(|e| e.to_string())?;
        self.runtime = Some(rt);
        Ok(())
    }

    fn teardown_runtime(&mut self) -> Result<(), String> {
        #[cfg(feature = "webview")]
        {
            let Some(runtime) = self.runtime.take() else {
                // Kein von diesem Pool besessener Runtime-Klon: insbesondere
                // darf ein wiederholter Shutdown keinen fremden/alten OnceLock-
                // Pfad erneut zurueckschreiben.
                return Ok(());
            };
            drop(runtime);
            // WebView2 flushes Cookies/Local State asynchronously during
            // Runtime drop. Retain the established grace period before
            // copying the profile back to the master.
            std::thread::sleep(std::time::Duration::from_secs(5));
            crate::config::write_back_session_to_master().map_err(|error| {
                crate::bench_events::emit(
                    crate::bench_events::Level::Warn,
                    None,
                    &format!("[master-profile] {error}"),
                );
                error
            })
        }
        #[cfg(not(feature = "webview"))]
        {
            Ok(())
        }
    }

    /// Schliesst ausschliesslich den Shared-Pool samt besessenem Runtime-Klon.
    /// Gekapselte Fallbacks haben eigene Runtimes/Profile und duerfen von einem
    /// letzten Shared-Stop nicht mitgerissen werden.
    #[cfg(feature = "webview")]
    fn shutdown_shared_runtime_with_result(&mut self) -> Result<(), String> {
        self.close_shared_tabs();
        self.teardown_runtime()
    }

    #[cfg(feature = "webview")]
    fn close_shared_tabs(&mut self) {
        for (_bid, tab) in self.tabs.drain() {
            if let Some(rt) = self.runtime.as_ref() {
                let _ = rt.close_page(tab.view_id);
            }
        }
    }

    /// Geordneter, beobachtbarer Browserpool-Abschluss.
    ///
    /// Explizite Aufrufer erhalten den Master-Write-back-Fehler als `Result`.
    /// [`Self::shutdown`] bleibt als Kompatibilitaetswrapper fuer Drop- und
    /// bestehende Testpfade erhalten.
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    pub fn shutdown_with_result(&mut self) -> Result<(), String> {
        #[cfg(feature = "webview")]
        {
            self.close_shared_tabs();
            for (_bid, inst) in self.encapsulated.drain() {
                let EncapsulatedInstance {
                    runtime: _rt,
                    profile_dir,
                    ..
                } = inst;
                let _ = _rt;
                let _ = std::fs::remove_dir_all(&profile_dir);
            }
            self.teardown_runtime()
        }
        #[cfg(not(feature = "webview"))]
        {
            self.teardown_runtime()
        }
    }

    /// Kompatibilitaetswrapper fuer nicht beobachtbare Drop-/Testpfade.
    #[allow(dead_code)]
    pub fn shutdown(&mut self) {
        if let Err(error) = self.shutdown_with_result() {
            crate::bench_events::emit(
                crate::bench_events::Level::Warn,
                None,
                &format!("[master-profile] geordneter Shutdown fehlgeschlagen: {error}"),
            );
        }
    }

    #[cfg(test)]
    fn shutdown_force(&mut self) {
        // Der Produktionspfad (TUI-Exit) schliesst die Tabs geordnet und
        // spielt die Sitzung ins Master zurueck — exakt das, was Tests nach
        // einem Lauf brauchen.
        self.shutdown_with_result()
            .expect("Pool-Shutdown darf im Test nicht scheitern");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Invarianten ohne echten WebView --------------------------------
    // Diese Tests rufen GENAU die Funktionen auf, die auch der
    // Produktionspfad benutzt (note_navigation, CloneGuard) — kein Nachbau
    // der Struktur, sondern der echte Code.

    #[test]
    fn refcount_steigt_nur_nach_erfolgreicher_navigation() {
        let mut refs = 1u32;
        let err = note_navigation(&mut refs, Err("navigate fehlgeschlagen".into()));
        assert!(err.is_err(), "Fehler wurde verschluckt");
        assert_eq!(refs, 1, "Refcount stieg trotz gescheiterter Navigation");

        note_navigation(&mut refs, Ok(())).expect("Erfolg darf nicht scheitern");
        assert_eq!(refs, 2, "Refcount stieg nach Erfolg nicht");
    }

    #[test]
    fn refcount_laeuft_nicht_ueber() {
        let mut refs = u32::MAX;
        note_navigation(&mut refs, Ok(())).unwrap();
        assert_eq!(refs, u32::MAX, "saturating_add verletzt");
    }

    #[test]
    fn letzter_stop_mit_runtime_muss_teardown_ausfuehren() {
        let calls = std::cell::Cell::new(0);
        complete_required_teardown(true, false, || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            calls.get(),
            1,
            "erforderliches Teardown wurde uebersprungen"
        );
    }

    #[test]
    fn letzter_stop_propagiert_writeback_fehler() {
        let result =
            complete_required_teardown(
                true,
                false,
                || Err("write-back fehlgeschlagen".to_string()),
            );

        assert_eq!(result, Err("write-back fehlgeschlagen".to_string()));
    }

    #[test]
    fn persistierte_tabs_bleiben_bei_aktiven_referenzen_offen() {
        let calls = std::cell::Cell::new(0);
        complete_required_teardown(true, true, || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            calls.get(),
            0,
            "aktive Shared-Session wurde trotz laufender Referenz beendet"
        );
    }

    #[test]
    fn stop_ohne_besessenen_runtime_klon_schreibt_nicht_zurueck() {
        let calls = std::cell::Cell::new(0);
        complete_required_teardown(false, false, || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();

        assert_eq!(calls.get(), 0, "Write-back ohne besessenen Runtime-Klon");
    }

    fn temp_klon() -> std::path::PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("webagent_cloneguard_{n}"));
        std::fs::create_dir_all(d.join("Default")).unwrap();
        std::fs::write(d.join("Default").join("Cookies"), b"x").unwrap();
        d
    }

    #[test]
    fn cloneguard_raeumt_den_klon_beim_verlassen_weg() {
        let dir = temp_klon();
        assert!(dir.exists());
        {
            let _g = CloneGuard::new(dir.clone());
        } // Fehlerpfad: Guard faellt, ohne dass keep() aufgerufen wurde
        assert!(
            !dir.exists(),
            "verwaister Profil-Klon blieb liegen: {}",
            dir.display()
        );
    }

    #[test]
    fn cloneguard_behaelt_den_klon_nach_keep() {
        let dir = temp_klon();
        let behalten = {
            let g = CloneGuard::new(dir.clone());
            g.keep()
        };
        assert_eq!(behalten, dir);
        assert!(
            dir.exists(),
            "uebernommener Klon wurde faelschlich geloescht"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Der Guard muss auch bei einem Panic aufraeumen — genau dafuer ist
    /// Drop-Semantik der manuellen Bereinigung ueberlegen.
    #[test]
    fn cloneguard_raeumt_auch_bei_panic_weg() {
        let dir = temp_klon();
        let d2 = dir.clone();
        let _ = std::panic::catch_unwind(move || {
            let _g = CloneGuard::new(d2);
            panic!("simulierter Abbruch mitten im Aufbau");
        });
        assert!(
            !dir.exists(),
            "Klon ueberlebte einen Panic: {}",
            dir.display()
        );
    }

    #[test]
    fn pool_refcount_without_webview() {
        BrowserPool::reset_for_tests();
        let mut pool = BrowserPool::new();
        pool.tabs.insert(
            "chatgpt".to_string(),
            PooledTab {
                view_id: 1,
                refs: 1,
                #[cfg(feature = "webview")]
                driver_proto: {
                    let (tx, _rx) = std::sync::mpsc::channel();
                    WebViewPageDriver {
                        view_id: 1,
                        page_tx: tx,
                    }
                },
            },
        );
        pool.stop_brain("chatgpt", Some(false)).unwrap();
        assert!(!pool.has_tab("chatgpt"));
    }

    #[test]
    fn pool_persist_keeps_tab() {
        BrowserPool::reset_for_tests();
        let mut pool = BrowserPool::new();
        pool.tabs.insert(
            "claude".to_string(),
            PooledTab {
                view_id: 2,
                refs: 1,
                #[cfg(feature = "webview")]
                driver_proto: {
                    let (tx, _rx) = std::sync::mpsc::channel();
                    WebViewPageDriver {
                        view_id: 2,
                        page_tx: tx,
                    }
                },
            },
        );
        pool.detach_brain("claude").unwrap();
        assert!(pool.has_tab("claude"));
        assert_eq!(pool.tab_ref_count("claude"), 0);
    }

    #[test]
    fn resilient_retry_threshold_matches_circuit_breaker() {
        // Nach POOL_FALLBACK_RETRIES (3) Fehlversuchen faellt das Brain auf die
        // gekapselte Instanz zurueck — analog circuit_breaker::DEFAULT_MAX_FAILURES.
        assert_eq!(POOL_FALLBACK_RETRIES, 3);
        let fallback_due = |failures: u32| failures >= POOL_FALLBACK_RETRIES;
        assert!(!fallback_due(0));
        assert!(!fallback_due(1));
        assert!(!fallback_due(2));
        assert!(fallback_due(3));
        assert!(fallback_due(4));
    }
}
