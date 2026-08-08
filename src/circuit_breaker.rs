//! circuit_breaker — pro Brain merken, ob es aktuell sinnlos ist, es zu befragen.
//!
//! Ohne das: ein blockiertes/rate-limitiertes Brain (qwen-Tageslimit, claude-
//! Session-Limit) wird bei jedem `/swarm`/`relay`-Aufruf erneut in den vollen
//! `wait_response`-Timeout gejagt, obwohl das Ergebnis vorhersehbar ist. Der
//! Breaker haelt fest: nach N aufeinanderfolgenden Fehlschlaegen fuer ein Brain
//! wird es fuer eine Cooldown-Zeit uebersprungen statt erneut versucht — degradiert
//! sichtbar (siehe [[external-blocks-flag-not-fail]]), statt den ganzen Lauf zu
//! blockieren.
//!
//! Zwei Cooldown-Laengen: gewoehnliche Fehlschlaege (Timeout, Protokollfehler)
//! sperren fuer `WEBAGENT_BREAKER_COOLDOWN_S` (15 min), deterministische Sperren
//! (`is_hard_block`: Login, Quota, Cloudflare) fuer
//! `WEBAGENT_BREAKER_HARD_COOLDOWN_S` (6 h) — lang, weil jeder Anlauf einen
//! vollen `ensure_ready`-Timeout kostet, aber **endlich**, damit ein behobener
//! Zustand von selbst wieder greift. Wer den Login von Hand erneuert, muss nicht
//! warten: `login`/`login-all` raeumen den Eintrag ueber [`clear`] gleich mit.
//!
//! Zustand ist prozessuebergreifend auf Disk (JSON, atomic write), weil `/swarm`
//! und `relay` typischerweise als separate Prozesse/Aufrufe laufen.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::config::data_dir;

const DEFAULT_MAX_FAILURES: u32 = 3;
const DEFAULT_COOLDOWN_SECS: i64 = 900; // 15 min
/// Cooldown fuer deterministische Sperren (fehlender Login, Quota, Cloudflare).
/// Lang genug, dass ein gesperrtes Brain nicht jede Runde erneut in seinen
/// `ensure_ready`-Timeout laeuft — aber endlich, damit ein behobener Zustand
/// (neuer Login, Tageslimit zurueckgesetzt) von selbst wieder greift.
const DEFAULT_HARD_COOLDOWN_SECS: i64 = 6 * 3600; // 6 h

lazy_static! {
    static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BrainState {
    consecutive_failures: u32,
    /// Unix-Sekunden, bis zu denen dieses Brain uebersprungen wird. `None`/
    /// vergangen heisst: Breaker zu, Brain darf befragt werden.
    open_until: Option<i64>,
    last_reason: Option<String>,
}

type StateMap = HashMap<String, BrainState>;

fn state_path() -> PathBuf {
    data_dir().join("circuit_breaker").join("state.json")
}

/// Zahl aus einer Env-Var, sonst Default. Getrennt, damit die drei Regler
/// (Schwelle, Cooldown, harter Cooldown) nachweislich dieselbe Lesart haben.
fn env_num<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn max_failures() -> u32 {
    env_num("WEBAGENT_BREAKER_MAX_FAILURES", DEFAULT_MAX_FAILURES)
}

fn cooldown_secs() -> i64 {
    env_num("WEBAGENT_BREAKER_COOLDOWN_S", DEFAULT_COOLDOWN_SECS)
}

/// Cooldown fuer deterministische Sperren — per `WEBAGENT_BREAKER_HARD_COOLDOWN_S`
/// ueberschreibbar.
fn hard_cooldown_secs() -> i64 {
    env_num(
        "WEBAGENT_BREAKER_HARD_COOLDOWN_S",
        DEFAULT_HARD_COOLDOWN_SECS,
    )
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load(path: &PathBuf) -> StateMap {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(path: &PathBuf, state: &StateMap) {
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(json) = serde_json::to_string_pretty(state) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// Wie lange (Sekunden) `brain_id` noch uebersprungen werden sollte, falls der
/// Breaker offen ist. `None` = Brain darf befragt werden.
pub fn check(brain_id: &str) -> Option<i64> {
    check_at(brain_id, &state_path())
}

fn check_at(brain_id: &str, path: &PathBuf) -> Option<i64> {
    let _guard = WRITE_LOCK.lock();
    let state = load(path);
    let entry = state.get(brain_id)?;
    // Wie lange gesperrt wird, entscheidet allein `open_until` — die Haerte der
    // Reason steckt in der *Laenge* des Cooldowns (siehe `record_failure_at`),
    // nicht in einem Sonderpfad hier.
    //
    // Frueher gab dieser Pfad fuer harte Reasons `(until - now).max(1)` zurueck
    // und sperrte damit unendlich: ein abgelaufenes `open_until` ergibt eine
    // negative Restzeit, `.max(1)` machte daraus wieder eine Sperre. Der Eintrag
    // verschwand nur durch `record_success` — das aber nie eintreten kann, weil
    // das Brain nie wieder gefragt wird. Beobachtet 2026-08-05: alle 8 Brains
    // trugen „Login nötig"-Eintraege vom 03.08. mit laengst abgelaufenem
    // `open_until`, der Benchmark klammerte 6 von 7 Brains mit exakt „(1s)" aus
    // und lief nur noch mit qwen.
    let until = entry.open_until?;
    let remaining = until - now_secs();
    if remaining > 0 {
        Some(remaining)
    } else {
        None
    }
}

/// Erfolgreicher Aufruf: setzt den Zaehler fuer `brain_id` zurueck.
pub fn record_success(brain_id: &str) {
    record_success_at(brain_id, &state_path());
}

fn record_success_at(brain_id: &str, path: &PathBuf) {
    let _guard = WRITE_LOCK.lock();
    let mut state = load(path);
    state.remove(brain_id);
    save(path, &state);
}

/// Sperre fuer `brain_id` aktiv aufheben — fuer den Fall, dass die Ursache
/// ausserhalb des Breakers behoben wurde.
///
/// Ohne das haelt ein erfolgreicher `webagent login` die Handarbeit nur zur
/// Haelfte fertig: das Profil ist frisch eingeloggt, der Breaker traegt aber
/// weiter „Login nötig" und laesst das Brain bis zum Ablauf des harten
/// Cooldowns aussen vor. Gibt `true` zurueck, wenn es tatsaechlich einen
/// Eintrag zu raeumen gab.
pub fn clear(brain_id: &str) -> bool {
    clear_at(brain_id, &state_path())
}

fn clear_at(brain_id: &str, path: &PathBuf) -> bool {
    let _guard = WRITE_LOCK.lock();
    let mut state = load(path);
    let hatte_eintrag = state.remove(brain_id).is_some();
    if hatte_eintrag {
        save(path, &state);
    }
    hatte_eintrag
}

/// Fehlschlag (Timeout/Rate-Limit/Blocked): erhoeht den Zaehler; oeffnet den
/// Breaker, sobald `WEBAGENT_BREAKER_MAX_FAILURES` erreicht ist.
pub fn record_failure(brain_id: &str, reason: &str) {
    record_failure_at(brain_id, reason, &state_path());
}

/// Deterministische Sperre: loest sich innerhalb eines Laufs NICHT von selbst.
///
/// Ein fehlender Login oder ein erschoepftes Tageslimit bleibt bestehen, egal
/// wie oft wir es erneut versuchen — jeder weitere Anlauf kostet einen vollen
/// `ensure_ready`-Timeout. Beobachtet 2026-07-26: gemini meldete „Login noetig",
/// der Breaker oeffnete aber erst nach `max_failures` (Default 3) Anlaeufen und
/// blaehte damit Phase A auf.
///
/// # Vorsicht: die Einstufung haengt am Wortlaut
///
/// Entschieden wird per Teilstring-Suche. Jede Meldung, in der „login"
/// vorkommt, wird damit zur harten Sperre — auch eine, die nur ERKLAERT, dass
/// ein Anmelde-Nachweis fehlt. Am 07.08.2026 hat das alle acht Brains sechs
/// Stunden lang aus dem Feld genommen, obwohl jedes angemeldet war.
///
/// Wer hier eine Meldung formuliert, entscheidet also ueber sechs Stunden
/// Sperre. Die Tests unten halten die beiden Faelle auseinander, die sich am
/// leichtesten verwechseln lassen.
pub(crate) fn is_hard_block(reason: &str) -> bool {
    let low = reason.to_lowercase();
    [
        "login",
        "quota",
        "rate_limit",
        "rate limit",
        "tageslimit",
        "daily limit",
        "cloudflare",
        "captcha",
        "blocked",
    ]
    .iter()
    .any(|p| low.contains(p))
}

fn record_failure_at(brain_id: &str, reason: &str, path: &PathBuf) {
    let _guard = WRITE_LOCK.lock();
    let mut state = load(path);
    let hard = is_hard_block(reason);
    let entry = state.entry(brain_id.to_string()).or_default();
    entry.consecutive_failures += 1;
    entry.last_reason = Some(reason.to_string());
    if hard || entry.consecutive_failures >= max_failures() {
        // Harte Sperren bekommen den langen Cooldown: ein fehlender Login oder
        // ein erschoepftes Tageslimit besteht nach 15 Minuten fast sicher noch,
        // und jeder Anlauf kostet einen vollen `ensure_ready`-Timeout
        // (beobachtet 2026-08-02: gemini verlor so ~144s pro Runde). Endlich
        // ist er trotzdem — sonst kaeme das Brain nie zurueck, auch nicht
        // nachdem der Login erneuert oder das Limit zurueckgesetzt wurde.
        let dauer = if hard {
            hard_cooldown_secs()
        } else {
            cooldown_secs()
        };
        entry.open_until = Some(now_secs() + dauer);
        let wie = if hard {
            "sofort (deterministische Sperre)".to_string()
        } else {
            format!("nach {} Fehlschlaegen", entry.consecutive_failures)
        };
        crate::bench_events::eprint_line(&format!(
            "[circuit_breaker] {brain_id}: offen fuer {dauer}s {wie} ({reason})"
        ));
    }
    save(path, &state);
}

/// Telemetrie-Snapshot eines Brains fuer `/breaker` und externe Diagnose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakerSnapshot {
    pub brain_id: String,
    /// `true` = Breaker offen, Brain wird uebersprungen.
    pub open: bool,
    pub consecutive_failures: u32,
    /// Unix-Sekunden, bis der Breaker zu ist (`None` wenn nie geoeffnet).
    pub open_until: Option<i64>,
    /// Verbleibende Cooldown-Sekunden, falls noch offen.
    pub remaining_secs: Option<i64>,
    pub last_reason: Option<String>,
}

/// Lese-API: alle bekannten Brain-Zustaende (sortiert nach `brain_id`).
/// Brains ohne Eintrag erscheinen nicht (analog zu `brain_score::leaderboard`).
pub fn snapshots() -> Vec<BreakerSnapshot> {
    snapshots_at(&state_path())
}

fn snapshots_at(path: &PathBuf) -> Vec<BreakerSnapshot> {
    let _guard = WRITE_LOCK.lock();
    let state = load(path);
    let now = now_secs();
    let mut out: Vec<BreakerSnapshot> = state
        .into_iter()
        .map(|(brain_id, entry)| {
            let remaining = entry.open_until.and_then(|until| {
                let r = until - now;
                if r > 0 {
                    Some(r)
                } else {
                    None
                }
            });
            BreakerSnapshot {
                brain_id,
                open: remaining.is_some(),
                consecutive_failures: entry.consecutive_failures,
                open_until: entry.open_until,
                remaining_secs: remaining,
                last_reason: entry.last_reason,
            }
        })
        .collect();
    out.sort_by(|a, b| a.brain_id.cmp(&b.brain_id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Der belegte Fall bleibt hart — dafuer ist die harte Sperre da.
    #[test]
    fn gesehene_anmelde_wand_sperrt_hart() {
        // So meldet relay.rs den Zustand: session_state={state:?}
        assert!(is_hard_block("session_state=LoginRequired"));
        assert!(is_hard_block("Login nötig (/login)"));
    }

    /// Der unbelegte Fall darf NICHT hart sperren.
    ///
    /// Am 07.08.2026 wurden alle acht Brains fuer sechs Stunden gesperrt,
    /// obwohl jedes angemeldet war: ein fehlender Anmelde-Nachweis (Seite noch
    /// nicht fertig, Selektor nach Website-Umbau) landete im selben Topf wie
    /// eine gesehene Anmelde-Wand.
    #[test]
    fn unbestimmt_sperrt_nicht_hart() {
        assert!(!is_hard_block("session_state=Unbestimmt"));
    }

    /// Die Erklaerung zum unbelegten Fall darf sich nicht selbst hart sperren.
    ///
    /// Die Einstufung laeuft ueber Teilstring-Suche: haette die Meldung das
    /// Wort „Login" enthalten, waere die Trennung wirkungslos gewesen. Dieser
    /// Test faengt eine spaetere, gut gemeinte Umformulierung ab.
    #[test]
    fn erklaerender_meldungstext_bleibt_weich() {
        assert!(!is_hard_block("Seite nicht bereit — kein Anmelde-Nachweis gefunden"));
    }

    fn unique_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("webagent_breaker_test_{nanos}_{n}.json"))
    }

    #[test]
    fn closed_breaker_allows_calls() {
        let path = unique_path();
        assert_eq!(check_at("kimi", &path), None);
    }

    #[test]
    fn opens_after_max_failures() {
        let path = unique_path();
        for _ in 0..DEFAULT_MAX_FAILURES {
            record_failure_at("qwen", "timeout_no_text", &path);
        }
        let remaining = check_at("qwen", &path).expect("breaker should be open");
        assert!(remaining > 0 && remaining <= DEFAULT_COOLDOWN_SECS);
    }

    #[test]
    fn stays_closed_below_threshold() {
        let path = unique_path();
        for _ in 0..(DEFAULT_MAX_FAILURES - 1) {
            record_failure_at("mistral", "timeout_no_text", &path);
        }
        assert_eq!(check_at("mistral", &path), None);
    }

    #[test]
    fn success_resets_failure_count() {
        let path = unique_path();
        record_failure_at("zai", "timeout", &path);
        record_failure_at("zai", "timeout", &path);
        record_success_at("zai", &path);
        for _ in 0..(DEFAULT_MAX_FAILURES - 1) {
            record_failure_at("zai", "timeout", &path);
        }
        // Zaehler wurde zurueckgesetzt, also noch nicht offen.
        assert_eq!(check_at("zai", &path), None);
    }

    #[test]
    fn other_brains_are_independent() {
        let path = unique_path();
        for _ in 0..DEFAULT_MAX_FAILURES {
            record_failure_at("qwen", "blocked", &path);
        }
        assert!(check_at("qwen", &path).is_some());
        assert_eq!(check_at("kimi", &path), None);
    }

    #[test]
    fn snapshots_report_open_and_partial_state() {
        let path = unique_path();
        // partial failures — closed breaker, but visible in telemetry
        record_failure_at("zai", "timeout", &path);
        record_failure_at("zai", "timeout", &path);
        // trip open
        for _ in 0..DEFAULT_MAX_FAILURES {
            record_failure_at("qwen", "blocked", &path);
        }
        // success removes entry entirely
        record_failure_at("kimi", "rate-limit", &path);
        record_success_at("kimi", &path);

        let snaps = snapshots_at(&path);
        assert_eq!(
            snaps.len(),
            2,
            "kimi reset should disappear; zai+qwen remain"
        );

        let zai = snaps.iter().find(|s| s.brain_id == "zai").expect("zai");
        assert!(!zai.open);
        assert_eq!(zai.consecutive_failures, 2);
        assert!(zai.open_until.is_none());
        assert_eq!(zai.remaining_secs, None);
        assert_eq!(zai.last_reason.as_deref(), Some("timeout"));

        let qwen = snaps.iter().find(|s| s.brain_id == "qwen").expect("qwen");
        assert!(qwen.open);
        assert_eq!(qwen.consecutive_failures, DEFAULT_MAX_FAILURES);
        assert!(qwen.open_until.is_some());
        assert!(qwen
            .remaining_secs
            .is_some_and(|r| r > 0 && r <= DEFAULT_HARD_COOLDOWN_SECS));
        assert_eq!(qwen.last_reason.as_deref(), Some("blocked"));

        // sorted by brain_id
        assert!(snaps.windows(2).all(|w| w[0].brain_id <= w[1].brain_id));
    }
    #[test]
    fn deterministische_sperre_oeffnet_den_breaker_sofort() {
        let path = unique_path();
        // Das echte Label aus repl.rs — frueher griff keine Klassifikation.
        record_failure_at("gemini", "Login nötig (/login)", &path);
        let state = load(&path);
        let e = state.get("gemini").expect("Eintrag fehlt");
        assert_eq!(e.consecutive_failures, 1);
        assert!(
            e.open_until.is_some(),
            "Breaker blieb nach deterministischer Sperre zu"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn harte_sperre_haelt_deutlich_laenger_als_ein_gewoehnlicher_fehlschlag() {
        let path = unique_path();
        record_failure_at("gemini", "Login nötig (/login)", &path);
        let rest = check_at("gemini", &path).expect("harte Sperre muss greifen");
        assert!(
            rest > DEFAULT_COOLDOWN_SECS,
            "harte Sperre darf nicht schon nach dem normalen Cooldown ablaufen (rest={rest}s)"
        );
        assert!(
            rest <= DEFAULT_HARD_COOLDOWN_SECS,
            "harte Sperre darf den harten Cooldown nicht ueberschreiten (rest={rest}s)"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn harte_sperre_verfaellt_nach_ablauf_des_harten_cooldowns() {
        // Der Kern des Fixes vom 2026-08-05: frueher gab `check_at` fuer harte
        // Reasons `(until - now).max(1)` zurueck. Ein abgelaufenes `open_until`
        // ergab damit dauerhaft „1s Restsperre" — das Brain kam nie zurueck,
        // weil nur `record_success` den Eintrag raeumt und dafuer das Brain
        // gefragt werden muesste. Beobachtet: alle 8 Brains mit „Login nötig"
        // vom 03.08., der Benchmark lief nur noch mit qwen.
        let path = unique_path();
        record_failure_at("gemini", "Login nötig (/login)", &path);
        let mut state = load(&path);
        state.get_mut("gemini").unwrap().open_until = Some(now_secs() - 1);
        save(&path, &state);
        assert_eq!(
            check_at("gemini", &path),
            None,
            "harte Sperre muss nach Ablauf verfallen, sonst bleibt das Brain fuer immer aus"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn abgelaufene_harte_sperre_ergibt_nie_die_alte_eine_sekunde() {
        // Regression auf das konkrete Symptom: der Benchmark meldete jedes
        // gesperrte Brain mit exakt „(1s)" — das war `.max(1)`, nicht ein echter
        // Rest-Cooldown. Keine abgelaufene Sperre darf so etwas mehr liefern.
        let path = unique_path();
        for (brain, reason) in [
            ("gemini", "Login nötig (/login)"),
            ("kimi", "rate_limit"),
            ("zai", "cloudflare challenge"),
        ] {
            record_failure_at(brain, reason, &path);
        }
        let mut state = load(&path);
        for e in state.values_mut() {
            // Zwei Tage alt — genau der Zustand aus state.json vom 05.08.
            e.open_until = Some(now_secs() - 2 * 24 * 3600);
        }
        save(&path, &state);
        for brain in ["gemini", "kimi", "zai"] {
            assert_eq!(check_at(brain, &path), None, "{brain} haengt weiter fest");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_hebt_eine_harte_sperre_sofort_auf() {
        // `webagent login` haengt hier dran: der Login behebt die Ursache, also
        // muss er auch den Eintrag raeumen — sonst bleibt das Brain trotz
        // frischer Session bis zum Ablauf des harten Cooldowns aussen vor.
        let path = unique_path();
        record_failure_at("gemini", "Login nötig (/login)", &path);
        assert!(
            check_at("gemini", &path).is_some(),
            "Vorbedingung: gesperrt"
        );
        assert!(
            clear_at("gemini", &path),
            "clear meldet den geraeumten Eintrag"
        );
        assert_eq!(check_at("gemini", &path), None);
        assert!(
            !clear_at("gemini", &path),
            "zweites clear hat nichts mehr zu raeumen"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_betrifft_nur_das_genannte_brain() {
        let path = unique_path();
        record_failure_at("gemini", "Login nötig (/login)", &path);
        record_failure_at("kimi", "Login nötig (/login)", &path);
        clear_at("gemini", &path);
        assert_eq!(check_at("gemini", &path), None);
        assert!(
            check_at("kimi", &path).is_some(),
            "ein Login fuer gemini darf kimi nicht entsperren"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn harter_cooldown_ist_per_env_ueberschreibbar() {
        // Nur die Lesart pruefen, nicht die echten Vars setzen: cargo test laeuft
        // parallel im selben Prozess, ein `set_var` wuerde andere Tests treffen.
        assert_eq!(
            hard_cooldown_secs(),
            DEFAULT_HARD_COOLDOWN_SECS,
            "ohne Env muss der Default gelten"
        );
        assert_eq!(env_num("WEBAGENT_BREAKER_TEST_UNSET_VAR", 42i64), 42);
        std::env::set_var("WEBAGENT_BREAKER_TEST_HARD_COOLDOWN", " 1234 ");
        assert_eq!(
            env_num(
                "WEBAGENT_BREAKER_TEST_HARD_COOLDOWN",
                DEFAULT_HARD_COOLDOWN_SECS
            ),
            1234,
            "Wert wird getrimmt und geparst"
        );
        std::env::set_var("WEBAGENT_BREAKER_TEST_HARD_COOLDOWN", "keine-zahl");
        assert_eq!(
            env_num(
                "WEBAGENT_BREAKER_TEST_HARD_COOLDOWN",
                DEFAULT_HARD_COOLDOWN_SECS
            ),
            DEFAULT_HARD_COOLDOWN_SECS,
            "Muell faellt auf den Default zurueck"
        );
        std::env::remove_var("WEBAGENT_BREAKER_TEST_HARD_COOLDOWN");
    }

    #[test]
    fn gewoehnlicher_fehlschlag_braucht_weiterhin_mehrere_anlaeufe() {
        let path = unique_path();
        record_failure_at("deepseek", "timeout_no_text", &path);
        let state = load(&path);
        assert!(
            state.get("deepseek").unwrap().open_until.is_none(),
            "ein einzelner Timeout darf den Breaker nicht oeffnen"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn harte_sperren_werden_von_gewoehnlichen_fehlern_unterschieden() {
        for r in [
            "Login nötig (/login)",
            "rate_limit",
            "Tageslimit erreicht",
            "cloudflare challenge",
            "blocked: reserve promoted",
        ] {
            assert!(is_hard_block(r), "nicht als harte Sperre erkannt: {r}");
        }
        for r in ["timeout_no_text", "protocol_error", "wall_timeout", ""] {
            assert!(!is_hard_block(r), "faelschlich als harte Sperre: {r}");
        }
    }
}
