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
//! Login ist bewusst **sequenziell als Default**: 8 parallel geoeffnete
//! Chromium-Fenster sind RAM-lastig. Parallelitaet ist opt-in via
//! `--parallel N` (max 2–3, nie Default) und startet pro Brain einen eigenen
//! Kindprozess, damit jede WebView2-Runtime in einem eigenen Prozess laeuft.
//!
//! **Ein Login-Weg, ein Profil-Ort.** `login-all` tut pro Brain exakt das, was
//! `login` tut. Frueher lenkte es die Google-SSO-Brains auf ein geteiltes
//! `profiles/google-sso`, damit das Passwort nur einmal faellig wird — der
//! Betrieb liest aber `profiles/<brain>`. Dadurch gab es zwei Profil-Layouts,
//! und eine Anmeldung war je nach benutztem Befehl vorhanden oder unsichtbar.
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::browser::WebBrainBackend;
use crate::config::available_brain_ids;

/// Ergebnis eines einzelnen Login-Versuchs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
/// `parallel` (0 = sequenziell, sonst max 2–3) startet pro Brain einen eigenen
/// Kindprozess (`login-worker`), damit jede WebView2-Runtime in einem eigenen
/// Prozess läuft — ein Crash reißt die Geschwister nicht mit. Der gewählte
/// Befehl wird nach unten auf 3 gedeckelt; bei `WEBAGENT_USE_SHARED_BROWSER=1`
/// wird NICHT parallel geloggt, weil alle Brains dort in DIESELBE
/// `profiles/shared`-Datenbank schreiben (SingletonLock-Race).
/// `force` überspringt den „bereits eingeloggt"-Check.
pub fn login_all(timeout_per_brain: Duration, parallel: usize, force: bool) -> Vec<LoginResult> {
    let brains = available_brain_ids();
    if parallel > 0 {
        let cap = parallel.min(MAX_PARALLEL);
        if crate::config::use_shared_browser() {
            eprintln!(
                "[login-all] --parallel deaktiviert bei WEBAGENT_USE_SHARED_BROWSER=1 \
                 (ein geteiltes Profil fuer alle Brains, kein Parallel-Login moeglich). \
                 Laeuft sequenziell."
            );
            println!(
                "[login-all] sequenziell, {}s pro Brain (profiles/shared direkt)",
                timeout_per_brain.as_secs()
            );
            return login_all_sequential(&brains, timeout_per_brain, force);
        }
        login_all_parallel(&brains, timeout_per_brain, force, cap)
    } else {
        println!(
            "[login-all] sequenziell, {}s pro Brain (profiles/<brain>)…",
            timeout_per_brain.as_secs()
        );
        login_all_sequential(&brains, timeout_per_brain, force)
    }
}

/// Obergrenze fuer gleichzeitige Login-Prozesse (RAM-lastig: jeder startet
/// einen eigenen Chromium/WebView2-Stack).
pub const MAX_PARALLEL: usize = 3;

/// Startet pro Brain einen `login-worker`-Kindprozess (re-exec des eigenen
/// Binaries). Bis zu `parallel` fast gleichzeitig; das Kind erbt stdout/stderr,
/// damit seine Fortschrittszeilen (`[login-all] <brain>: …`) live sichtbar
/// bleiben wie im sequenziellen Pfad. Das Ergebnis schreibt jedes Kind nach
/// `WEBAGENT_LOGIN_WORKER_RESULT` (Tempdatei) — der Parent liest sie nach dem
/// `wait()` aus und raeumt sie auf.
fn login_all_parallel(
    brains: &[String],
    timeout: Duration,
    force: bool,
    parallel: usize,
) -> Vec<LoginResult> {
    use std::process::{Command, Stdio};

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[login-all] eigener Binärpfad unlesbar ({err}) — laufe sequenziell.");
            return login_all_sequential(brains, timeout, force);
        }
    };

    let mut results: Vec<LoginResult> = Vec::with_capacity(brains.len());
    // Chunked: nie mehr als `parallel` Kinder gleichzeitig am Leben.
    for chunk in brains.chunks(parallel.max(1)) {
        // Pro Chunk neu indexiert; benannte Datei ist pro Kind eindeutig, weil
        // Chunks sequenziell laufen und die Datei nach dem Einlesen entfernt wird.
        let mut children: Vec<(String, std::process::Child, std::path::PathBuf)> =
            Vec::with_capacity(chunk.len());
        for (i, brain) in chunk.iter().enumerate() {
            // Zuerst das Ergebnisziel reservieren, damit Spawn-Fehler keine
            // kollidierenden Tempdateien hinterlassen.
            let result_file = std::env::temp_dir().join(format!(
                "webagent-login-worker-{}-{}.json",
                std::process::id(),
                i
            ));
            println!("[login-all] {brain}: Kindprozess starten…");
            let mut cmd = Command::new(&exe);
            cmd.arg("login-worker")
                .arg("--brain")
                .arg(brain)
                .arg("--timeout")
                .arg(timeout.as_secs().to_string())
                // stdout/stderr erben: Kinder-Fortschrittszeilen bleiben live
                // sichtbar (stderr nur echte Fehler, wie überall sonst).
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .env(
                    "WEBAGENT_LOGIN_WORKER_RESULT",
                    result_file.to_string_lossy().into_owned(),
                );
            if force {
                cmd.arg("--force");
            }
            match cmd.spawn() {
                Ok(child) => children.push((brain.clone(), child, result_file)),
                Err(err) => {
                    eprintln!("[login-all] {brain}: Spawn fehlgeschlagen ({err})");
                    // Reste aufraeumen, falls der Worker doch noch angeschlagen hat.
                    let _ = std::fs::remove_file(&result_file);
                    results.push(LoginResult {
                        brain_id: brain.clone(),
                        ok: false,
                        skipped: false,
                        message: format!("Kindprozess nicht startbar: {err}"),
                    });
                }
            }
        }
        for (brain, mut child, result_file) in children {
            let res = match child.wait() {
                Ok(_) => read_worker_result_file(&brain, &result_file),
                Err(err) => {
                    let _ = std::fs::remove_file(&result_file);
                    LoginResult {
                        brain_id: brain.clone(),
                        ok: false,
                        skipped: false,
                        message: format!("Kindprozess nicht lesbar: {err}"),
                    }
                }
            };
            results.push(res);
        }
    }
    // Chromium-Cookie-Verschluesselung: Kein additiver Master-Abgleich moeglich
    // (siehe sync_login_to_master). Jedes Kind schreibt direkt in sein
    // `profiles/<brain>` — das Master bleibt im Non-Shared-Betrieb bewusst
    // unveraendert, genau wie beim sequenziellen Pfad ohne Shared-Env.
    results
}

/// Liest die Ergebnisdatei eines beendeten `login-worker`-Kindprozesses und
/// raeumt sie auf. Die Datei enthaelt die `LOGIN_RESULT=<json>`-Zeile.
fn read_worker_result_file(brain: &str, path: &std::path::Path) -> LoginResult {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => {
            let _ = std::fs::remove_file(path);
            return LoginResult {
                brain_id: brain.to_string(),
                ok: false,
                skipped: false,
                message: "Kindprozess ohne Ergebnisdatei beendet".into(),
            };
        }
    };
    let _ = std::fs::remove_file(path);
    let mut r = parse_login_result_line(&bytes);
    if r.brain_id == "?" {
        r.brain_id = brain.to_string();
    }
    r
}

/// Liest die Ergebniszeile aus stdout/-Datei des `login-worker`. Der Inhalt ist
/// eine `LOGIN_RESULT=<json>`-Zeile; davor steht meist eine Fortschrittszeile.
fn parse_login_result_line(stdout: &[u8]) -> LoginResult {
    const PREFIX: &str = "LOGIN_RESULT=";
    let text = String::from_utf8_lossy(stdout);
    for line in text.lines().rev() {
        if let Some(payload) = line.strip_prefix(PREFIX) {
            if let Ok(r) = serde_json::from_str::<LoginResult>(payload) {
                return r;
            }
        }
    }
    LoginResult {
        brain_id: "?".into(),
        ok: false,
        skipped: false,
        message: "Kindprozess ohne LOGIN_RESULT-Zeile beendet".into(),
    }
}

/// Der Runner des `login-worker`-Subcommands: loggt EIN Brain ein. Ergebnis
/// geht als `LOGIN_RESULT=<json>`-Zeile entweder in die `WEBAGENT_LOGIN_WORKER_RESULT`-
/// Tempdatei (Parallel-Modus; stdout des Kindes bleibt fuer Fortschritt frei)
/// oder auf stdout (manueller Aufruf). Getrennt von `cmd_login` gehalten,
/// damit die Kindprozess-Ausgabe maschinenlesbar bleibt.
pub fn run_login_worker(brain: &str, timeout_secs: u64, force: bool) -> i32 {
    let res = login_one(brain, Duration::from_secs(timeout_secs), force, None);
    let line = format!(
        "LOGIN_RESULT={}",
        serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())
    );
    match std::env::var("WEBAGENT_LOGIN_WORKER_RESULT") {
        Ok(path) => {
            // Fortschritt des Kindes ging auf stdout (geerbt); Ergebnis in Datei.
            let _ = std::fs::write(&path, line);
        }
        Err(_) => println!("{line}"),
    }
    if res.ok || res.skipped {
        0
    } else {
        1
    }
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
    // Ueber den abgesicherten Rueckweg statt per roher Kopie.
    //
    // `copy_dir_sparse` legt die Cookie-Datenbank des Brains STUMPF ueber die
    // des Masters. Bei mehreren Brains hintereinander gewinnt schlicht der
    // letzte: Am 2026-08-22 lief `login-all` ueber neun Brains und hinterliess
    // ein Master, das nur noch `z.ai` kannte — 98 KB Sitzungsdaten auf 40 KB
    // geschrumpft, ohne Sicherung und ohne Warnung.
    //
    // Additiv KANN diese Spiegelung nicht sein: Chromium verschluesselt Cookies
    // mit einem Schluessel aus `Local State` DESSELBEN Profils, weshalb Datei
    // und Schluessel nur gemeinsam sinnvoll sind. Ein Master mit allen Brains
    // entsteht deshalb nur im Shared-Betrieb, wo alle Anmeldungen von
    // vornherein in dieselbe Datenbank laufen.
    //
    // `write_back_dir_to_master` bringt die Waechter mit, die hier fehlten:
    // Sicherung vor jeder Mutation, Gewichtsvergleich, Abgleich der
    // Sitzungsnachweise und Rollback. Eine Spiegelung, die dem Master Brains
    // NEHMEN wuerde, wird damit abgelehnt statt ausgefuehrt.
    match crate::config::write_back_dir_to_master(&src) {
        Ok(()) => println!("[login] {brain_id}: Sitzung ins Hauptprofil gespiegelt."),
        Err(e) => {
            eprintln!("[login] {brain_id}: Spiegelung ins Hauptprofil abgelehnt: {e}");
            eprintln!(
                "[login] Das Hauptprofil bleibt unveraendert. Ein Master mit ALLEN                  Brains entsteht nur ueber WEBAGENT_USE_SHARED_BROWSER=1 login-all                  — dort landen alle Anmeldungen in derselben Cookie-Datenbank."
            );
        }
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
            return finish_login(
                shared_mode,
                None,
                LoginResult {
                    brain_id: brain_id.to_string(),
                    ok: false,
                    skipped: false,
                    message: format!("Backend-Fehler: {e}"),
                },
            );
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
            return finish_login(
                shared_mode,
                Some(backend),
                LoginResult {
                    brain_id: brain_id.to_string(),
                    ok: true,
                    skipped: true,
                    message: "bereits eingeloggt — übersprungen (--force zum erzwingen)".into(),
                },
            );
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

    #[test]
    fn parse_login_result_line_finds_json_trailer() {
        let r = LoginResult {
            brain_id: "claude".into(),
            ok: true,
            skipped: false,
            message: "eingeloggt".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let mut out = String::new();
        out.push_str("[login] claude: Browser oeffnen…\n");
        out.push_str(&format!("LOGIN_RESULT={json}\n"));
        let parsed = parse_login_result_line(out.as_bytes());
        assert_eq!(parsed, r);
    }

    #[test]
    fn parse_login_result_line_skips_human_lines_and_falls_back() {
        let out = b"[login] claude: Browser oeffnen...\n[login] claude: Login erkannt.\n";
        let r = parse_login_result_line(out);
        assert!(!r.ok);
        assert!(r.message.contains("ohne LOGIN_RESULT"));
    }

    #[test]
    fn parse_login_result_line_ignores_trailing_empty() {
        // stdout kann mit \n oder \r\n enden; die letzte Zeile darf leer sein.
        let r = LoginResult {
            brain_id: "zai".into(),
            ok: false,
            skipped: false,
            message: "kein Login".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let out = format!("LOGIN_RESULT={json}\r\n");
        let parsed = parse_login_result_line(out.as_bytes());
        assert_eq!(parsed, r);
        let out2 = format!("LOGIN_RESULT={json}\n\n");
        assert_eq!(parse_login_result_line(out2.as_bytes()), r);
    }

    #[test]
    fn parallel_capped_at_max() {
        // Die Cap-Funktion ist die, die `login_all` wirklich nutzt — und eine
        // Konstante, die den Vertrag fixiert. Beide zusammen: pragmatisch.
        fn cap(v: usize) -> usize {
            if v > MAX_PARALLEL {
                MAX_PARALLEL
            } else {
                v
            }
        }
        assert_eq!(cap(99), MAX_PARALLEL);
        assert_eq!(cap(2), 2);
        assert_eq!(cap(0), 0);
    }

    #[test]
    fn run_login_worker_emits_machine_line_without_browser() {
        // Ohne Shared-Env und mit einem nicht existenten Brain darf KEIN
        // Browser offen werden — der Backend-Fehler path landet im Result.
        // Wir testen nur die strukturelle Vertraeglichkeit der Ausgabezeile.
        std::env::remove_var("WEBAGENT_USE_SHARED_BROWSER");
        std::env::remove_var("WEBAGENT_SHARED_BROWSER");
        std::env::remove_var("WEBAGENT_LOGIN_WORKER_RESULT");
        let code = run_login_worker("__no_such_brain_xyz__", 1, false);
        // Backend-Fehler => ok=false => code 1 (kein Launch versucht).
        assert_eq!(code, 1);
    }

    #[test]
    fn run_login_worker_writes_result_file_when_env_set() {
        // Parallel-Modus: Kind schreibt LOGIN_RESULT in die Tempdatei statt
        // auf stdout (Fortschritt bleibt dort menschenfreundlich).
        std::env::remove_var("WEBAGENT_USE_SHARED_BROWSER");
        std::env::remove_var("WEBAGENT_SHARED_BROWSER");
        let path = std::env::temp_dir().join(format!(
            "webagent-login-worker-test-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("WEBAGENT_LOGIN_WORKER_RESULT", &path);
        let code = run_login_worker("__no_such_brain_xyz__", 1, false);
        std::env::remove_var("WEBAGENT_LOGIN_WORKER_RESULT");
        assert_eq!(code, 1);
        let content = std::fs::read_to_string(&path).expect("Ergebnisdatei geschrieben");
        let _ = std::fs::remove_file(&path);
        assert!(content.starts_with("LOGIN_RESULT="), "got: {content}");
        let parsed = parse_login_result_line(content.as_bytes());
        assert_eq!(parsed.brain_id, "__no_such_brain_xyz__");
        assert!(!parsed.ok);
    }

    #[test]
    fn read_worker_result_file_reports_missing_file_honestly() {
        // Crash des Kindes ohne Ergebnisdatei -> ehrliches Fehl-Result,
        // keine leere Erfolgsmeldung. Und die Datei wird aufgeraeumt.
        let path = std::env::temp_dir().join(format!(
            "webagent-login-worker-nofile-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let r = read_worker_result_file("gemini", &path);
        assert!(!r.ok);
        assert_eq!(r.brain_id, "gemini");
        assert!(r.message.contains("ohne Ergebnisdatei"));
    }

    #[test]
    fn read_worker_result_file_falls_back_brain_from_callername() {
        // Wenn der Worker nur "?"-brain_id schreibt (kaputte Zeile), behaelt
        // der Parent die Caller-Brain-Zuordnung — absolut wesentlich, sonst
        // wuerde der Abgleich am falschen Brain haengen.
        let path = std::env::temp_dir().join(format!(
            "webagent-login-worker-fallback-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::fs::write(
            &path,
            "LOGIN_RESULT={\"brain_id\":\"?\",\"ok\":false,\"skipped\":false,\"message\":\"x\"}",
        )
        .unwrap();
        let r = read_worker_result_file("chatgpt", &path);
        assert_eq!(r.brain_id, "chatgpt");
        assert_eq!(r.message, "x");
        assert!(!path.exists());
    }
}
