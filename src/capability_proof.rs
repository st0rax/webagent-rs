//! capability_proof — Belege fuer Faehigkeits-Behauptungen.
//!
//! Ein Level zaehlt eine Faehigkeit erst, wenn ein bestandener Live-Lauf sie
//! belegt hat — nicht schon, wenn ein Selektor-Eintrag in einer JSON steht.
//! Dieses Modul ist der **rein rechnende** Teil: Store, Hash, Frist, Zustand.
//! Kein Browser. Die browserfahrende Verifikation liegt getrennt in
//! `src/browser/verify.rs` (§8 des Capability-Proof-Plans).
//!
//! [`crate::brain_probe::Verdict`] ist die Messung, [`ProofOutcome`] das Urteil:
//! die Messung sagt, was die Oberflaeche tat; das Urteil sagt, was das fuer das
//! Level bedeutet.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::capability::Capability;
use crate::config::data_dir;

lazy_static! {
    static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

/// Verfall einer frischen Belegs: 14 Tage — einstellbar, nach dem Muster von
/// `circuit_breaker::cooldown_secs()`. Kuerzer zu stellen bedeutet, Brains
/// haeufiger abzuschalten; das ist Absicht, nicht Nebenwirkung.
const DEFAULT_TTL_DAYS: u32 = 14;

/// Ein Verifikationsbefund. Append-only, eine Zeile JSON pro Lauf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofRecord {
    pub brain_id: String,
    pub capability: String,
    /// `crate::now_rfc3339()` — Lesart des Urteils.
    pub ts: String,
    pub outcome: ProofOutcome,
    /// Hash ueber die `needs`-Selektoren dieser Faehigkeit zum Zeitpunkt des Laufs.
    pub selector_hash: u32,
    /// Welcher Eintrag der Fallback-Kette getragen hat — aus `js_scan_indexed`
    /// (§5 des Plans). `None` bei Beleg-Formen ohne Selektoraufloesung.
    pub winning_selector: Option<String>,
    /// Klartext aus der Messung (before -> after, restored) bzw. bei
    /// `Unreachable` der externe Zustand. Festes Vokabular, damit es
    /// auszaehlbar bleibt: `blocked`, `rate_limit`, `cloudflare`,
    /// `logged_out`, `start_failed`, `circuit_open`.
    pub evidence: String,
    pub latency_ms: u64,
}

/// Was an der Oberflaeche gemessen wurde — quellenunabhaengig.
///
/// Alle Beleg-Formen liefern diesen Typ, damit es **einen** Weg in den Store
/// gibt. `brain_probe::Verdict` wird konvertiert (`From<&Verdict>`), nicht
/// nachgebaut. Bewusst hier und nicht in `brain_probe`: dieses Modul ist als
/// rein rechnend zugesichert und darf nicht von einem Typ des Browser-Umfelds
/// abhaengen.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub capability_key: String,
    pub before: String,
    pub after: String,
    pub proven: bool,
    /// Ausgangszustand wiederhergestellt? `None`, wenn nichts zu widerrufen war.
    pub restored: Option<bool>,
    pub note: String,
    /// Welcher Eintrag der Fallback-Kette getragen hat, falls aufgeloest wurde.
    pub winning_selector: Option<String>,
}

/// Das Urteil einer Messung — der Gegenspieler zu `brain_probe::Verdict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProofOutcome {
    Passed,
    Failed,
    /// Keine Aussage ueber die Faehigkeit moeglich — entzieht nie einen Beleg.
    Unreachable,
}

/// Zustand eines Belegs, wie ihn das Gate (Level) konsumiert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofState {
    Proven { at: String },
    Expired { at: String, reason: ExpiryReason },
    Failed { at: String, evidence: String },
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryReason {
    /// TTL abgelaufen — Drift der Website, die im Selektor-File keine Spur
    /// hinterlaesst.
    TtlElapsed,
    /// `needs`-Selektoren wurden geaendert — handgeschriebene Ansprucche werden
    /// damit sofort entwertet.
    SelectorsChanged,
}

/// TTL in Tagen; `WEBAGENT_PROOF_TTL_DAYS=0` laesst jeden Beleg sofort verfallen.
pub fn ttl_days() -> u32 {
    std::env::var("WEBAGENT_PROOF_TTL_DAYS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_TTL_DAYS)
}

/// Hash ueber genau die Selektorlisten, die diese Faehigkeit laut Katalog
/// braucht. Kanonische Reihenfolge (`cap.needs`), damit die Reihenfolge in der
/// JSON egal ist. `fnv1a` wird aus `config.rs` wiederverwendet — nicht kopiert.
pub fn selector_hash_for(cap: &Capability, sel: &serde_json::Value) -> u32 {
    let mut repr = String::new();
    for key in cap.needs {
        repr.push_str(key);
        repr.push('|');
        let entry = sel.get(*key).cloned().unwrap_or(serde_json::Value::Null);
        repr.push_str(&serde_json::to_string(&entry).unwrap_or_default());
        repr.push('|');
    }
    crate::config::fnv1a(&repr)
}

fn proofs_path() -> PathBuf {
    data_dir().join("capability").join("proofs.jsonl")
}

/// Baut einen Record aus einer Messung. Getrennt, damit Tests dieselbe
/// Zusammenstellung pruefen, die der Store schreibt — keine zweite Kopie.
pub(crate) fn record_from_measurement(
    brain_id: &str,
    m: &Measurement,
    outcome: ProofOutcome,
    hash: u32,
    ms: u64,
) -> ProofRecord {
    ProofRecord {
        brain_id: brain_id.to_string(),
        capability: m.capability_key.clone(),
        ts: crate::now_rfc3339(),
        outcome,
        selector_hash: hash,
        winning_selector: m.winning_selector.clone(),
        evidence: evidence_for(m, outcome),
        latency_ms: ms,
    }
}

fn evidence_for(m: &Measurement, outcome: ProofOutcome) -> String {
    match outcome {
        ProofOutcome::Unreachable | ProofOutcome::Failed => m.note.clone(),
        ProofOutcome::Passed => {
            if m.restored == Some(false) {
                format!(
                    "{} | {} -> {} | Rueckweg misslungen",
                    m.note, m.before, m.after
                )
            } else {
                m.note.clone()
            }
        }
    }
}

/// **Der einzige Weg in den Store** — fuer `webagent verify` wie fuer
/// `probe --verify`.
pub fn record_measurement(
    brain_id: &str,
    m: &Measurement,
    outcome: ProofOutcome,
    hash: u32,
    ms: u64,
) {
    record_proof(record_from_measurement(brain_id, m, outcome, hash, ms));
}

pub fn record_proof(rec: ProofRecord) {
    record_proof_at(rec, &proofs_path());
}

/// Append-only schreiben. `pub(crate)`: Tests schreiben gegen ein Tempfile.
pub(crate) fn record_proof_at(rec: ProofRecord, path: &PathBuf) {
    let _guard = WRITE_LOCK.lock();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(line) = serde_json::to_string(&rec) else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

/// Lese-Semantik:
///
/// ```text
/// records = alle Zeilen fuer (brain_id, capability), in Schreibreihenfolge
/// records = records ohne Unreachable                 # keine Aussage ueber die Faehigkeit
/// last = records.last() else return Never
///
/// match last.outcome:
///     Failed  -> Failed { at, evidence }             # letztes Urteil gewinnt
///     Passed  ->
///         if last.selector_hash != current_hash -> Expired { SelectorsChanged }
///         if age(last.ts) > ttl()                -> Expired { TtlElapsed }
///         else                                   -> Proven { at }
/// ```
///
/// **Letztes Urteil gewinnt, unabhaengig von der TTL.** Alles andere hiesse, ein
/// Level zu halten, dessen Bruch man gerade beobachtet hat.
pub fn proof_state(brain_id: &str, capability: &str, current_hash: u32) -> ProofState {
    proof_state_at(brain_id, capability, current_hash, &proofs_path())
}

pub fn proof_state_at(
    brain_id: &str,
    capability: &str,
    current_hash: u32,
    path: &PathBuf,
) -> ProofState {
    let mut last: Option<ProofRecord> = None;
    if let Ok(contents) = std::fs::read_to_string(path) {
        for line in contents.lines() {
            let Ok(rec) = serde_json::from_str::<ProofRecord>(line) else {
                continue;
            };
            if rec.brain_id != brain_id || rec.capability != capability {
                continue;
            }
            if rec.outcome == ProofOutcome::Unreachable {
                continue;
            }
            last = Some(rec);
        }
    }
    let Some(last) = last else {
        return ProofState::Never;
    };
    match last.outcome {
        ProofOutcome::Failed => ProofState::Failed {
            at: last.ts.clone(),
            evidence: last.evidence.clone(),
        },
        ProofOutcome::Passed => {
            if last.selector_hash != current_hash {
                ProofState::Expired {
                    at: last.ts.clone(),
                    reason: ExpiryReason::SelectorsChanged,
                }
            } else if proof_age_days(&last.ts) >= f64::from(ttl_days()) {
                ProofState::Expired {
                    at: last.ts.clone(),
                    reason: ExpiryReason::TtlElapsed,
                }
            } else {
                ProofState::Proven { at: last.ts.clone() }
            }
        }
        ProofOutcome::Unreachable => unreachable!("oben ausgefiltert"),
    }
}

/// Alter eines Belegs in Tagen. Ein nicht parsebarer Zeitstempel gilt als
/// unendlich alt — konservativ, lieber einmal neu verifizieren als einen
/// fragwuerdigen Beleg halten.
///
/// Der Vergleich in `proof_state_at` ist `>=`: ein Beleg ist **hoechstens**
/// `ttl_days()` alt, dann verfaellt er. Damit schlaegt `WEBAGENT_PROOF_TTL_DAYS=0`
/// unmittelbar nach dem Schreiben zu (Alter 0.0 >= 0.0) — „sofort verfallen"
/// meint, was es sagt.
fn proof_age_days(ts: &str) -> f64 {
    use time::format_description::well_known::Rfc3339;
    match time::OffsetDateTime::parse(ts, &Rfc3339) {
        Ok(dt) => (time::OffsetDateTime::now_utc() - dt).whole_seconds() as f64 / 86_400.0,
        Err(_) => f64::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Eigenes Umfeld-String-Lock, damit `WEBAGENT_PROOF_TTL_DAYS` die uebrigen
    /// Tests nicht vergiftet — dasselbe Muster wie `timeouts::ENV_LOCK`, nur
    /// privat fuer dieses Modul. Die Variable ist prozessglobal; wer Zustand
    /// prueft, muss sie auf einen bekannten Wert stellen, sonst schreibt eine
    /// parallele Test-Instanz hinein.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Setzt die TTL fuer die Dauer von `f` und stellt sie danach wieder her.
    fn with_ttl(days: &str, f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior = std::env::var("WEBAGENT_PROOF_TTL_DAYS").ok();
        std::env::set_var("WEBAGENT_PROOF_TTL_DAYS", days);
        f();
        match prior {
            Some(v) => std::env::set_var("WEBAGENT_PROOF_TTL_DAYS", v),
            None => std::env::remove_var("WEBAGENT_PROOF_TTL_DAYS"),
        }
    }

    fn unique_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("webagent_proof_test_{nanos}_{n}.jsonl"))
    }

    fn rec(outcome: ProofOutcome, brain: &str, cap: &str, hash: u32) -> ProofRecord {
        ProofRecord {
            brain_id: brain.to_string(),
            capability: cap.to_string(),
            ts: crate::now_rfc3339(),
            outcome,
            selector_hash: hash,
            winning_selector: None,
            evidence: "befund".into(),
            latency_ms: 0,
        }
    }

    #[test]
    fn kein_beleg_ist_never() {
        with_ttl("14", || {
            let path = unique_path();
            assert_eq!(
                proof_state_at("claude", "chat", 1, &path),
                ProofState::Never
            );
        });
    }

    #[test]
    fn passed_ist_proven_bei_passendem_hash() {
        with_ttl("14", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            assert!(matches!(
                proof_state_at("claude", "chat", 42, &path),
                ProofState::Proven { .. }
            ));
        });
    }

    #[test]
    fn passed_dann_failed_ist_failed() {
        // Letztes Urteil gewinnt: ein beobachteter Bruch entzieht den Beleg,
        // unabhaengig von jeder TTL.
        with_ttl("14", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            record_proof_at(rec(ProofOutcome::Failed, "claude", "chat", 42), &path);
            assert!(matches!(
                proof_state_at("claude", "chat", 42, &path),
                ProofState::Failed { .. }
            ));
        });
    }

    #[test]
    fn passed_dann_unreachable_bleibt_proven() {
        // Keine Aussage ueber die Faehigkeit — ein Limit-Banner unterbricht den
        // Lauf, aber das Level darf deshalb nicht fallen.
        with_ttl("14", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            record_proof_at(rec(ProofOutcome::Unreachable, "claude", "chat", 42), &path);
            assert!(matches!(
                proof_state_at("claude", "chat", 42, &path),
                ProofState::Proven { .. }
            ));
        });
    }

    #[test]
    fn neuere_urteile_gewinnen_gegen_aeltere() {
        with_ttl("14", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Failed, "claude", "chat", 42), &path);
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            // Der zweite, neuere Passed-Befund schlaegt den ersten Failed.
            assert!(matches!(
                proof_state_at("claude", "chat", 42, &path),
                ProofState::Proven { .. }
            ));
        });
    }

    #[test]
    fn hash_aenderung_entwertet_sofort() {
        with_ttl("14", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            assert!(matches!(
                proof_state_at("claude", "chat", 43, &path),
                ProofState::Expired {
                    reason: ExpiryReason::SelectorsChanged,
                    ..
                }
            ));
        });
    }

    #[test]
    fn ttl_null_macht_jeden_beleg_frisch_abgelaufen() {
        with_ttl("0", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            assert!(matches!(
                proof_state_at("claude", "chat", 42, &path),
                ProofState::Expired {
                    reason: ExpiryReason::TtlElapsed,
                    ..
                }
            ));
        });
    }

    #[test]
    fn record_baut_evidence_aus_restore_fehler() {
        let m = Measurement {
            capability_key: "reasoning_toggle".into(),
            before: "off".into(),
            after: "on".into(),
            proven: true,
            restored: Some(false),
            note: "Zustandswechsel belegt".into(),
            winning_selector: Some("[data-testid='think']".into()),
        };
        let rec = record_from_measurement("claude", &m, ProofOutcome::Passed, 7, 120);
        assert!(rec.evidence.contains("Rueckweg misslungen"), "{rec:?}");
        assert_eq!(rec.winning_selector.as_deref(), Some("[data-testid='think']"));
        // Ein misslungener Rueckweg ist ein Aufraeumfehler, kein Beweisfehler:
        // der Record zaehlt als Passed.
        assert_eq!(rec.outcome, ProofOutcome::Passed);
    }

    #[test]
    fn selector_hash_ueber_needs_in_kanonischer_reihenfolge() {
        let cap = crate::capability::capability("model_switch").unwrap();
        let sel = serde_json::json!({
            "model_menu": ["a", "b"],
            "model_option": ["c"],
        });
        let h1 = selector_hash_for(cap, &sel);

        // Reihenfolge in der JSON egal.
        let sel2 = serde_json::json!({
            "model_option": ["c"],
            "model_menu": ["a", "b"],
        });
        assert_eq!(selector_hash_for(cap, &sel2), h1);

        // Neuer Fallback in einem needs-Schluessel aendert den Hash.
        let sel3 = serde_json::json!({
            "model_menu": ["a", "b", "d"],
            "model_option": ["c"],
        });
        assert_ne!(selector_hash_for(cap, &sel3), h1);
    }

    #[test]
    fn selector_hash_ignoriert_fremde_schluessel() {
        let cap = crate::capability::capability("chat").unwrap();
        let sel = serde_json::json!({
            "composer": ["#x"],
            "send_button": ["#y"],
            "assistant_message": ["#z"],
        });
        let h1 = selector_hash_for(cap, &sel);
        // Ein Eintrag in einem fremden Schluessel darf den chat-Beleg nicht
        // wegwerfen — sonst entwertet ein neuer Fallback fuer canvas_button den
        // chat-Beleg.
        let sel2 = serde_json::json!({
            "composer": ["#x"],
            "send_button": ["#y"],
            "assistant_message": ["#z"],
            "canvas_button": ["#c"],
        });
        assert_eq!(selector_hash_for(cap, &sel2), h1);
    }

    #[test]
    fn proof_state_trennt_brain_und_faehigkeit() {
        with_ttl("14", || {
            let path = unique_path();
            record_proof_at(rec(ProofOutcome::Passed, "claude", "chat", 42), &path);
            record_proof_at(rec(ProofOutcome::Failed, "claude", "new_chat", 42), &path);
            record_proof_at(rec(ProofOutcome::Failed, "qwen", "chat", 42), &path);
            // claude/chat ist belegt, obwohl dieselbe Faehigkeit bei qwen scheiterte.
            assert!(matches!(
                proof_state_at("claude", "chat", 42, &path),
                ProofState::Proven { .. }
            ));
            assert!(matches!(
                proof_state_at("qwen", "chat", 42, &path),
                ProofState::Failed { .. }
            ));
        });
    }
}
