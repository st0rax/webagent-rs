//! Einheitliches Login für alle Brains — einmalig, sequenziell.
//!
//! Statt sich 8 Mal händisch einzuloggen, öffnet `login_all()` pro Brain einen
//! headed Browser, wartet auf den Login-Erkennungs-Check und schreibt das
//! eingeloggte Profil nach `profiles/<brain>` (die **canonical Login-Quelle**,
//! siehe Groks Entscheidung 2026-07-17). Danach kann `/swarm` (oder jeder
//! andere Multi-Brain-Lauf) dieses Profil als Vorlage für isolierte
//! Laufzeit-Teilkopien nutzen — siehe `config::prepare_swarm_profile`.
//!
//! `profiles/reference/<brain>` ist **optional**: nur wer bewusst eine
//! „goldene" Vorlage getrennt vom Alltags-Login pflegen will, legt sie an;
//! `prepare_swarm_profile` nutzt sie dann gegenüber `profiles/<brain>`.
//!
//! Login ist bewusst **sequenziell**: 8 parallel geöffnete Chromium-Fenster
//! sind RAM-lastig und fehleranfällig (Race um das Shared-Profil). Ein Fenster
//! nach dem anderen ist robust und für den einmaligen Setup-Schritt schnell
//! genug. Parallelität ist opt-in via `--parallel N` (max 2–3, nie Default).
//!
//! **Ein Login-Weg, ein Profil-Ort.** `login-all` tut pro Brain exakt das, was
//! `login` tut. Frueher lenkte es die Google-SSO-Brains auf ein geteiltes
//! `profiles/google-sso`, damit das Passwort nur einmal faellig wird — der
//! Betrieb liest aber `profiles/<brain>`. Dadurch gab es zwei Profil-Layouts,
//! und eine Anmeldung war je nach benutztem Befehl vorhanden oder unsichtbar.
use std::path::PathBuf;
use std::time::Duration;

use crate::browser::WebBrainBackend;
use crate::config::available_brain_ids;

/// Ergebnis eines einzelnen Login-Versuchs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginResult {
    pub brain_id: String,
    pub ok: bool,
    pub skipped: bool,
    pub message: String,
}

/// Öffnet für jedes Brain nacheinander einen Browser, wartet auf Login.
/// Das Profil landet in `profiles/<brain>` — der einzigen canonical Quelle.
///
/// `timeout_per_brain` gilt pro Brain (nicht gesamt).
/// `parallel` (0 = sequenziell, sonst max 2–3) ist experimentell.
/// `force` überspringt den „bereits eingeloggt"-Check.
pub fn login_all(timeout_per_brain: Duration, parallel: usize, force: bool) -> Vec<LoginResult> {
    let brains = available_brain_ids();
    // Parallelität ist spezifiziert (max 2–3), aber noch nicht implementiert:
    // Shared-Pool + 8 headed Windows brauchen getrennte Runtimes pro Slot.
    // Bis dahin immer sequenziell — parallel>0 nur als Hinweis loggen.
    if parallel > 0 {
        eprintln!(
            "[login-all] Hinweis: --parallel {parallel} noch nicht implementiert, laufe sequenziell"
        );
    }
    login_all_sequential(&brains, timeout_per_brain, force)
}

fn login_all_sequential(brains: &[String], timeout: Duration, force: bool) -> Vec<LoginResult> {
    // Im Shared-Betrieb (WEBAGENT_USE_SHARED_BROWSER=1) ist `profiles/shared`
    // das read-only Master-Hauptprofil mit den Logins ALLER Brains. login_all
    // loggt deshalb direkt dort ein (statt `profiles/<brain>`) — nur so hat das
    // Master alle Sessions, aus denen der Betrieb dann sparsam klont.
    let results: Vec<LoginResult> = brains
        .iter()
        .map(|brain| login_one(brain, timeout, force, None))
        .collect();
    if crate::config::use_shared_browser() {
        crate::config::seal_master_profile();
    }
    results
}

/// Ein nachgewiesener Login raeumt die Breaker-Sperre desselben Brains mit weg.
///
/// Der Breaker kann sich aus einer harten Sperre („Login nötig", Quota) nicht
/// selbst befreien, solange sie gilt — er fragt das Brain ja gerade nicht.
/// Der Login ist der Moment, in dem die Ursache nachweislich behoben ist, also
/// ist er auch die Stelle, an der der Eintrag faellt. Gilt auch fuer den
/// „bereits eingeloggt"-Fall: dass die Erkennung anschlaegt, ist genau der
/// Nachweis, dass ein alter „Login nötig"-Eintrag veraltet ist.
pub fn clear_breaker(brain_id: &str) {
    if crate::circuit_breaker::clear(brain_id) {
        println!("[login] {brain_id}: Breaker-Sperre aufgehoben.");
    }
}

/// Spiegelt die frische Anmeldung aus `profiles/<brain>` ins Master-Profil.
///
/// # Warum das noetig ist
///
/// Der Worker-Pool oeffnet seine Tabs IMMER aus einer Laufzeit-Kopie von
/// `profiles/shared` ([`crate::config::runtime_pool_profile_dir`]) — er ist ein
/// GETEILTER Browser und braucht ein Profil, das die Sitzungen aller Brains
/// kennt. Der Login schreibt aber nur dann direkt ins Master, wenn
/// `WEBAGENT_USE_SHARED_BROWSER=1` gesetzt ist; sonst landet er in
/// `profiles/<brain>`.
///
/// Ohne diese Bruecke bleibt das Master stehen, waehrend die Brain-Profile
/// frisch sind. Gemessen am 06.08.2026: nach einem erfolgreichen `login-all`
/// waren alle acht `profiles/<brain>` um 21:52 geschrieben, `profiles/shared`
/// stand unveraendert auf dem 03.08. — der naechste Pool-Lauf waere trotz
/// Anmeldung wieder ausgeloggt gewesen. Genau so wurden 6 von 8 Brains
/// "Login noetig".
///
/// Kopiert wird sparsam ueber dieselbe Whitelist wie der Klon-Weg; das Master
/// wird dafuer kurz entsiegelt und danach wieder versiegelt.
fn sync_login_to_master(brain_id: &str) {
    // Im Shared-Betrieb schreibt der Login ohnehin direkt ins Master.
    if crate::config::use_shared_browser() {
        return;
    }
    let src = crate::config::profiles_dir().join(brain_id);
    if !src.is_dir() {
        return;
    }
    let master = crate::config::shared_profile_dir();
    crate::config::unseal_master_profile();
    let result = crate::config::copy_dir_sparse(&src, &master);
    crate::config::seal_master_profile();
    match result {
        Ok(()) => println!("[login] {brain_id}: Sitzung ins Hauptprofil gespiegelt."),
        Err(e) => eprintln!(
            "[login] {brain_id}: Spiegelung ins Hauptprofil fehlgeschlagen: {e} \
             (der Worker-Pool bleibt dann auf dem alten Stand)"
        ),
    }
}

/// Optionale Spiegelung des frisch eingeloggten Profils nach
/// `profiles/reference/<brain>` — nur wenn `WEBAGENT_LOGIN_TO_REFERENCE=1`
/// gesetzt ist. Canonical bleibt `profiles/<brain>`; die Referenz ist eine
/// optionale „goldene" Vorlage, die `prepare_swarm_profile` bevorzugt.
fn maybe_copy_to_reference(brain_id: &str) {
    if std::env::var("WEBAGENT_LOGIN_TO_REFERENCE")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        let src = crate::config::profiles_dir().join(brain_id);
        let dst = crate::config::reference_profile_dir(brain_id);
        if !src.is_dir() {
            return;
        }
        let _ = std::fs::create_dir_all(&dst);
        match crate::config::copy_dir_all(&src, &dst) {
            Ok(()) => println!("[login-all] Referenz-Profil gespiegelt → {:?}", dst),
            Err(e) => eprintln!("[login-all] Referenz-Spiegelung fehlgeschlagen: {e}"),
        }
    }
}

/// Loggt ein einzelnes Brain ein. `interactive_login` schreibt bereits nach
/// `profiles/<brain>` (das `profile_dir` des Backends) — keine zusätzliche
/// Kopie nötig. Bei bereits eingeloggtem Profil (detectbar) wird übersprungen,
/// sofern `force` nicht gesetzt ist.
///
/// `profile_override` bleibt fuer Sonderfaelle erhalten (z.B. Tests), wird im
/// normalen Login-Weg aber NICHT mehr gesetzt — siehe `login_all_sequential`.
fn login_one(
    brain_id: &str,
    timeout: Duration,
    force: bool,
    profile_override: Option<PathBuf>,
) -> LoginResult {
    // Shared-Betrieb: das Master muss für den Login kurz beschreibbar sein.
    let shared_mode = crate::config::use_shared_browser();
    if shared_mode {
        crate::config::unseal_master_profile();
    }

    println!("[login-all] {brain_id}: Browser öffnen…");
    let mut backend = match WebBrainBackend::from_config(brain_id) {
        Ok(b) => b,
        Err(e) => {
            return finish_login(shared_mode, None, LoginResult {
                brain_id: brain_id.to_string(),
                ok: false,
                skipped: false,
                message: format!("Backend-Fehler: {e}"),
            });
        }
    };
    if let Some(p) = profile_override {
        backend = backend.with_profile_override(p);
    } else if shared_mode {
        // Nur das Master kennt alle Sessions: Login landet direkt in
        // `profiles/shared` statt `profiles/<brain>`.
        backend = backend.with_profile_override(crate::config::shared_profile_dir());
    }

    // Bereits eingeloggt? Nur wenn nicht --force.
    if !force {
        if let Ok(true) = backend.is_logged_in_quick() {
            clear_breaker(brain_id);
            // Auch hier spiegeln: das Brain-Profil ist gueltig, aber das
            // Master kann trotzdem veraltet sein. Genau dieser Fall ist am
            // 06.08.2026 eingetreten — `login-all` meldete Erfolg, das Master
            // blieb auf dem Stand vom 3. August.
            sync_login_to_master(brain_id);
            return finish_login(shared_mode, Some(backend), LoginResult {
                brain_id: brain_id.to_string(),
                ok: true,
                skipped: true,
                message: "bereits eingeloggt — übersprungen (--force zum erzwingen)".into(),
            });
        }
    }

    let result = match backend.interactive_login(timeout) {
        Ok(true) => {
            let profile = backend.effective_profile_dir().clone();
            clear_breaker(brain_id);
            sync_login_to_master(brain_id);
            maybe_copy_to_reference(brain_id);
            LoginResult {
                brain_id: brain_id.to_string(),
                ok: true,
                skipped: false,
                message: format!("eingeloggt, Profil → {:?}", profile),
            }
        }
        Ok(false) => LoginResult {
            brain_id: brain_id.to_string(),
            ok: false,
            skipped: false,
            message: format!(
                "kein Login in {}s erkannt — erneut versuchen mit --timeout",
                timeout.as_secs()
            ),
        },
        Err(e) => LoginResult {
            brain_id: brain_id.to_string(),
            ok: false,
            skipped: false,
            message: format!("Fehler: {e}"),
        },
    };
    finish_login(shared_mode, Some(backend), result)
}

/// Schließt das Backend (Browser bereits gestoppt durch `interactive_login`)
/// und versiegelt im Shared-Betrieb das Master-Profil wieder — nach jedem
/// Login-Pfad, auch nach "bereits eingeloggt" oder Backend-Fehler.
fn finish_login(
    shared_mode: bool,
    backend: Option<WebBrainBackend>,
    result: LoginResult,
) -> LoginResult {
    if shared_mode {
        drop(backend);
        crate::config::seal_master_profile();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_result_struct_fields() {
        let r = LoginResult {
            brain_id: "chatgpt".into(),
            ok: true,
            skipped: true,
            message: "skip".into(),
        };
        assert!(r.ok && r.skipped);
        assert_eq!(r.brain_id, "chatgpt");
    }

    #[test]
    fn test_available_brains_nonempty_for_login_all() {
        // Struktur-Test ohne echten Browser-Launch (login_all würde headed öffnen).
        let brains = available_brain_ids();
        assert!(!brains.is_empty());
        assert!(brains.iter().any(|b| b == "chatgpt"));
    }

    #[test]
    fn test_maybe_copy_to_reference_respects_env() {
        // Ohne Env: no-op (kein panic). Mit Env=1 und leerem/fehlendem src: no-op.
        std::env::remove_var("WEBAGENT_LOGIN_TO_REFERENCE");
        maybe_copy_to_reference("chatgpt");
        std::env::set_var("WEBAGENT_LOGIN_TO_REFERENCE", "1");
        maybe_copy_to_reference("__no_such_brain_for_test__");
        std::env::remove_var("WEBAGENT_LOGIN_TO_REFERENCE");
    }

    #[test]
    fn login_all_writes_to_the_same_place_as_login() {
        // Der Kern des Fixes vom 2026-07-29: es darf nur EINEN Profil-Ort
        // geben. `login-all` setzte fuer Google-SSO-Brains einen Override auf
        // `profiles/google-sso`, waehrend der Betrieb `profiles/<brain>` liest
        // (config::brains). Eine Anmeldung war dadurch je nach benutztem
        // Befehl vorhanden oder unsichtbar — gemini meldete direkt nach dem
        // Einloggen wieder `logged_in: false`.
        for brain in ["gemini", "chatgpt", "deepseek", "claude"] {
            let backend = WebBrainBackend::from_config(brain).expect("config");
            let erwartet = crate::config::profiles_dir().join(brain);
            assert_eq!(
                backend.effective_profile_dir(),
                &erwartet,
                "{brain}: Betrieb muss profiles/<brain> nutzen"
            );
        }
    }
}
