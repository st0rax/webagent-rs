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
//! Google-SSO-Brains (siehe `GOOGLE_SSO_BRAINS`) teilen sich EIN gemeinsames
//! Profil, damit die Google-Session physisch im selben Profil liegt: der
//! ERSTE Brain macht das echte Google-Login (Passwort einmal), danach
//! completet jeder weitere Brain den OAuth mit einem „Continue with Google"-
//! Durchklick (kein Passwort). Cross-Origin-Cookie-Kopie auf Geschwister-
//! Brains ist bewusst ausgeschlossen — sie funktioniert bei unabhängigen
//! Diensten nicht.

use std::path::PathBuf;
use std::time::Duration;

use crate::browser::WebBrainBackend;
use crate::config::available_brain_ids;

/// Google-SSO-Brains teilen EIN gemeinsames Profil (`profiles/google-sso`),
/// damit die Google-Session physisch im selben Profil liegt: der ERSTE Brain
/// macht das echte Google-Login (Passwort einmal), danach completet jeder
/// weitere Brain den OAuth mit einem „Continue with Google"-Durchklick (kein
/// Passwort). Cross-Origin-Cookie-Kopie auf Geschwister-Brains ist bewusst
/// ausgeschlossen — sie funktioniert bei unabhängigen Diensten nicht.
const GOOGLE_SSO_BRAINS: &[&str] = &[
    "chatgpt", "deepseek", "kimi", "gemini", "qwen", "claude", "mistral", "zai",
];

/// Liefert `true`, wenn das Brain über Google-SSO angemeldet werden kann und
/// daher das gemeinsame SSO-Profil nutzen soll.
fn is_google_sso(brain_id: &str) -> bool {
    GOOGLE_SSO_BRAINS.contains(&brain_id)
}

/// Gemeinsames Profil für die Google-SSO-Brains (physisch geteilte Session).
fn shared_sso_profile_dir() -> PathBuf {
    crate::config::profiles_dir().join("google-sso")
}

/// Ergebnis eines einzelnen Login-Versuchs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginResult {
    pub brain_id: String,
    pub ok: bool,
    pub skipped: bool,
    pub message: String,
}

/// Öffnet für jedes Brain nacheinander einen Browser, wartet auf Login.
/// Das Profil landet in `profiles/<brain>` (canonical Quelle), bzw. im
/// gemeinsamen SSO-Profil für Google-SSO-Brains.
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
    // Google-SSO-Brains teilen ein physisches Profil, damit die Google-Session
    // über den ersten Login hinaus für die nachfolgenden Brains sichtbar bleibt.
    let sso_profile = shared_sso_profile_dir();
    let mut results = Vec::new();
    for brain in brains {
        let override_dir = if is_google_sso(brain) {
            Some(sso_profile.clone())
        } else {
            None
        };
        results.push(login_one(brain, timeout, force, override_dir));
    }
    results
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
/// `profile_override` lenkt das Brain auf ein geteiltes Profil (genutzt für die
/// Google-SSO-Gruppe), statt auf sein eigenes `profiles/<brain>`.
fn login_one(
    brain_id: &str,
    timeout: Duration,
    force: bool,
    profile_override: Option<PathBuf>,
) -> LoginResult {
    println!("[login-all] {brain_id}: Browser öffnen…");
    let mut backend = match WebBrainBackend::from_config(brain_id) {
        Ok(b) => b,
        Err(e) => {
            return LoginResult {
                brain_id: brain_id.to_string(),
                ok: false,
                skipped: false,
                message: format!("Backend-Fehler: {e}"),
            };
        }
    };
    if let Some(p) = profile_override {
        backend = backend.with_profile_override(p);
    }

    // Bereits eingeloggt? Nur wenn nicht --force.
    if !force {
        if let Ok(true) = backend.is_logged_in_quick() {
            return LoginResult {
                brain_id: brain_id.to_string(),
                ok: true,
                skipped: true,
                message: "bereits eingeloggt — übersprungen (--force zum erzwingen)".into(),
            };
        }
    }

    match backend.interactive_login(timeout) {
        Ok(true) => {
            let profile = backend.effective_profile_dir().clone();
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
    }
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
    fn test_google_sso_grouping() {
        // Storax-Entscheidung (Claude 061522): ALLE 8 Brains nutzen Google-SSO
        // und teilen das gemeinsame Profil.
        for b in [
            "chatgpt", "deepseek", "kimi", "gemini", "qwen", "claude", "mistral", "zai",
        ] {
            assert!(is_google_sso(b), "{b} sollte Google-SSO nutzen");
        }
    }

    #[test]
    fn test_shared_sso_profile_dir_under_profiles() {
        let p = shared_sso_profile_dir();
        assert!(p.ends_with("google-sso"));
        assert_eq!(crate::config::profiles_dir().join("google-sso"), p);
    }
}
