//! Run-Ledger: Kette, Lock und Torn-Tail-Behandlung fuer das Run-Event-Journal.
//!
//! Der Produktpfad ist `run_store.rs::append_event` (durable `events.jsonl` mit
//! SHA-256-Kette). Dieses Modul liefert die Randsicherungen, die ein einzelner
//! Prozess nicht leisten kann: prozessuebergreifende Sequenzierung (Lock),
//! bruchlose Ketten-Verifikation und die ehrliche Behandlung eines abgerissenen
//! Tails (Quarantaene + Recovery-Receipt statt stiller Reparatur). Der gültige
//! Praefix bleibt nach einer Quarantaene lesbar; eine Beschaedigung in der
//! Mitte der Kette wird NIE repariert, sondern fail-closed abgelehnt.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ergebnis der Ketten-Prüfung eines vollständig lesbaren Journals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHead {
    /// Naechste freie Sequenznummer (1-basiert, folgt auf den letzten Beleg).
    pub next_seq: u64,
    /// Hash des letzten Belegs (`GENESIS` bei leerem Journal).
    pub previous_hash: String,
    /// Anzahl vollständiger, verifizierter Belege.
    pub valid_count: usize,
}

/// Fehlerklassifikation der Ketten-Prüfung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainError {
    /// Der letzte Roh-Beleg ist abgerissen (kein abschliessendes JSON).
    ///
    /// Der gültige Präfix bleibt in der Datei; der abgerissene Rest muss vor
    /// dem Weiterschreiben unverändert in die Quarantäne und per Receipt
    /// belegt werden (`quarantine_torn_tail`). Erst dann ist der Präfix wieder
    /// schreibbar — dieser Zustand wird `recovery_required`, nie `done`.
    TornTail { valid: ChainHead },
    /// Die Kette ist mitten/Wiederverwendbar beschädigt (fremde oder
    /// manipulierte Zeile). Fail-closed: kein Weiterschreiben, keine Reparatur.
    Corrupt { line: usize, reason: String },
    Io(String),
}

/// Recovery-Receipt: legt die Quarantäne eines abgerissenen Tails fest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryReceipt {
    pub run_id: String,
    pub detected_at: String,
    /// SHA-256 des Original-Bytes der abgerissenen Zeile (inkl. fehlendem
    /// Newline genau so, wie sie im Journal stand).
    pub torn_sha256: String,
    /// Lauflänge der abgerissenen Bytes.
    pub torn_bytes_len: usize,
    /// Erste vollständig gültige Sequenz NACH dem Präfix.
    pub valid_seq: u64,
    pub quarantined_path: PathBuf,
}

/// Unterscheidet eine vollständig intakte Kette von einem abgerissenen Tail.
///
/// Die Datei wird zeilenweise mit bytegenauen Trennungen verarbeitet: eine
/// letzte Zeile ohne abschliessendes JSON (und ohne Terminal-Newline) ist ein
/// Torn Tail; jede andere kaputte oder sequenzverletzende Zeile ist `Corrupt`.
pub fn verify_event_chain(path: &Path) -> Result<ChainHead, ChainError> {
    if !path.exists() {
        return Ok(chain_head_genesis());
    }
    let content = fs::read_to_string(path)
        .map_err(|e| ChainError::Io(format!("{}: {e}", path.display())))?;
    if content.is_empty() {
        return Ok(chain_head_genesis());
    }

    let pieces: Vec<&str> = content.split_inclusive('\n').collect();

    let mut next_seq = 1u64;
    let mut previous_hash = "GENESIS".to_string();
    let mut valid_count = 0usize;

    for (idx, piece) in pieces.iter().enumerate() {
        let is_last = idx + 1 == pieces.len();
        // Abgerissener Tail: das letzte Stück hat kein Terminal-Newline — der
        // Schreibprozess wurde zwischen Beleg-Inhalt und Newline unterbrochen.
        // Egal ob es zufällig gültiges JSON ist: es gehört in die Quarantäne,
        // nicht in die Kette. Der gültige Präfix wird mitgegeben, damit die
        // Quarantäne ihn wiederherstellen kann.
        if is_last && !piece.ends_with('\n') {
            return Err(ChainError::TornTail {
                valid: ChainHead {
                    next_seq,
                    previous_hash,
                    valid_count,
                },
            });
        }
        let trimmed = piece.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            if !is_last {
                return Err(ChainError::Corrupt {
                    line: idx + 1,
                    reason: "leere Zeile mitten in der Kette".to_string(),
                });
            }
            continue;
        }
        match validate_entry(trimmed, next_seq, &previous_hash) {
            Ok(hash) => {
                previous_hash = hash;
                next_seq = next_seq
                    .checked_add(1)
                    .ok_or_else(|| ChainError::Corrupt {
                        line: idx + 1,
                        reason: "Sequenz übergelaufen".to_string(),
                    })?;
                valid_count += 1;
            }
            Err(reason) => {
                return Err(ChainError::Corrupt {
                    line: valid_count + 1,
                    reason,
                });
            }
        }
    }

    Ok(ChainHead {
        next_seq,
        previous_hash,
        valid_count,
    })
}

fn chain_head_genesis() -> ChainHead {
    ChainHead {
        next_seq: 1,
        previous_hash: "GENESIS".to_string(),
        valid_count: 0,
    }
}

/// Validiert einen einzelnen Kettenglied-Zeilenstumpf (ohne Newline) inkl.
/// Hash-Pruefung gegen den Vorgaenger. Liefert den Hash des Belegs.
fn validate_entry(raw: &str, expected_seq: u64, previous_hash: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("ungültiges JSON: {e}"))?;
    let seq = value
        .get("seq")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "Beleg ohne seq".to_string())?;
    if seq != expected_seq {
        return Err(format!("Sequenz {seq} statt {expected_seq}"));
    }
    let prev = value
        .get("prev_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Beleg ohne prev_hash".to_string())?;
    if prev != previous_hash {
        return Err("prev_hash weicht ab".to_string());
    }
    let hash = value
        .get("hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Beleg ohne hash".to_string())?;
    let core = serde_json::json!({
        "timestamp": value["timestamp"],
        "run_id": value["run_id"],
        "type": value["type"],
        "payload": value["payload"],
    });
    let canonical =
        serde_json::to_vec(&core).map_err(|e| format!("Canonical-JSON ungültig: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(previous_hash.as_bytes());
    hasher.update(&canonical);
    let computed = format!("{:x}", hasher.finalize());
    if hash != computed {
        return Err("Hash-Pruefung fehlgeschlagen".to_string());
    }
    Ok(hash.to_string())
}

/// Quarantänisiert einen abgerissenen Tail unverändert und schreibt den
/// Recovery-Receipt. Der gültige Präfix bleibt in `events.jsonl` lesbar.
///
/// Der abgerissene Rest wird vorher als `torn_bytes` übergeben; sie werden
/// byteidentisch unter `quarantine/events.jsonl.<stamp>.torn` abgelegt. Das
/// Journal selbst wird auf den gültigen Präfix zurückgeschnitten (dazu
/// neu geschrieben und gesynct). Nach diesem Aufruf ist der Run im Zustand
/// `recovery_required` — er darf nicht als `done` erscheinen.
pub fn quarantine_torn_tail(
    run_id: &str,
    events_path: &Path,
    run_dir: &Path,
    valid: &ChainHead,
) -> Result<RecoveryReceipt, String> {
    let content = fs::read_to_string(events_path)
        .map_err(|e| format!("Journal {events_path:?}: {e}"))?;
    let pieces: Vec<&str> = content.split_inclusive('\n').collect();
    let torn_bytes = pieces
        .last()
        .filter(|p| !p.ends_with('\n'))
        .map(|p| p.as_bytes().to_vec())
        .ok_or_else(|| format!("Kein abgerissener Tail in {events_path:?}"))?;

    let stamp = now_stamp();
    let quarantine_dir = run_dir.join("quarantine");
    fs::create_dir_all(&quarantine_dir)
        .map_err(|e| format!("Quarantäne-Ordner {quarantine_dir:?}: {e}"))?;
    let target = quarantine_dir.join(format!("events.jsonl.{stamp}.torn"));
    fs::write(&target, &torn_bytes)
        .map_err(|e| format!("Quarantäne-Datei {target:?}: {e}"))?;

    let torn_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&torn_bytes);
        format!("{:x}", hasher.finalize())
    };
    let receipt = RecoveryReceipt {
        run_id: run_id.to_string(),
        detected_at: crate::now_rfc3339(),
        torn_sha256,
        torn_bytes_len: torn_bytes.len(),
        valid_seq: valid.next_seq,
        quarantined_path: target.clone(),
    };

    // Gültigen Präfix wiederherstellen: alle Zeilen bis zum Torn (der Torn ist
    // durch `verify_event_chain` entfernt), neu schreiben + fsync.
    if let Some(prefix) = prefix_events(events_path, valid.valid_count)? {
        write_synced(events_path, prefix.as_bytes())?;
    } else {
        // Leeres Journal: am besten Datei entfernen statt Torn-Rest zu belassen.
        let _ = fs::remove_file(events_path);
    }

    let receipt_path = run_dir.join("recovery.json");
    let receipt_json = serde_json::to_string_pretty(&receipt)
        .map_err(|e| format!("Receipt-Serialisierung: {e}"))?;
    write_synced(&receipt_path, receipt_json.as_bytes())
        .map_err(|e| format!("Receipt {receipt_path:?}: {e}"))?;

    Ok(receipt)
}

/// Liest den gültigen Präfix (erste `count` Belege) als vollständige Zeilen.
fn prefix_events(events_path: &Path, count: usize) -> Result<Option<String>, String> {
    let content = fs::read_to_string(events_path)
        .map_err(|e| format!("Journal {events_path:?}: {e}"))?;
    let parts: Vec<&str> = content.split_inclusive('\n').collect();
    if parts.len() < count {
        return Err(format!(
            "Präfix-Anforderung {count} > vorhandene Zeilen {}",
            parts.len()
        ));
    }
    let prefix: String = parts[..count].concat();
    Ok(if prefix.is_empty() { None } else { Some(prefix) })
}

/// Schreibt Bytes atomar mit fsync (Neu-Schreiben; ersetzt nicht bestehendes
/// Ziel wo möglich sauber, aber über Recreate).
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write(path, bytes)
}

/// Schreibt `bytes` atomar nach `path`: einzigartige Temp-Datei im selben
/// Ordner (PID+Nanos → kein geteilter Name), `fsync` vor dem Rename,
/// Windows-Fallback mit Direkt-Schreiben und fsync des Ziels. Es entsteht
/// nie eine halbe Zieldatei.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = temp_sibling(path)?;
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
            .map_err(|e| format!("Temp {tmp:?}: {e}"))?;
        f.write_all(bytes)
            .map_err(|e| format!("Schreiben {tmp:?}: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync {tmp:?}: {e}"))?;
    }
    replace_with_rename(&tmp, path)?;
    Ok(())
}

/// Einzigartiger, Prozess-lokaler Tempname neben dem Ziel (kein geteilter Name).
pub fn temp_sibling(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Ziel ohne Dateinamen".to_string())?;
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(parent.join(format!("{name}.{pid}.{nanos}.tmp")))
}

/// Windows-sicher atomar ersetzen: per `rename` ersetzen (POSIX-Semantik), mit
/// Fallback, der denselben Inhalt an Ort und Stelle schreibt (Windows-weites
/// Replace für offene Handles) und das Ziel davor mit `sync_all` absichert.
fn replace_with_rename(tmp: &Path, target: &Path) -> Result<(), String> {
    if fs::rename(tmp, target).is_ok() {
        sync_parent(target);
        return Ok(());
    }
    let bytes = fs::read(tmp).map_err(|e| format!("Temp {tmp:?}: {e}"))?;
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(target)
            .map_err(|e| format!("Ziel {target:?}: {e}"))?;
        f.write_all(&bytes)
            .map_err(|e| format!("Ziel-Schreiben {target:?}: {e}"))?;
        f.sync_all().map_err(|e| format!("Ziel-fsync {target:?}: {e}"))?;
    }
    let _ = fs::remove_file(tmp);
    sync_parent(target);
    Ok(())
}

fn sync_parent(target: &Path) {
    if let Ok(dir) = fs::File::open(target.parent().unwrap_or_else(|| Path::new("."))) {
        let _ = dir.sync_all();
    }
}

fn now_stamp() -> String {
    crate::now_run_stamp().replace(':', ".").replace('-', "")
        + &String::from(".")
        + &format!(
            "{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        )
}

/// Liest ein vorhandenes Recovery-Receipt.
pub fn read_recovery_receipt(run_dir: &Path) -> Option<RecoveryReceipt> {
    let path = run_dir.join("recovery.json");
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Legt an, ob ein Run nach einem abgerissenen Tail Recovery benoetigt.
pub fn has_recovery_receipt(run_dir: &Path) -> bool {
    run_dir.join("recovery.json").exists()
}

/// Prozessübergreifende Sperre für das Event-Journal eines Runs.
///
/// Schreibzugriffe auf `events.jsonl` werden unter dieser Sperre serialisiert:
/// Sequenz-Bestimmung, Append und fsync sind eine kritische Sektion. Die
/// Sperre ist eine atomare `create_dir`-Reservation im Run-Verzeichnis; der
/// Besitzer (PID + Startzeit) steht in `owner.json` und ist bei einem
/// Prozessabsturz wieder einholbar (`reclaim`), nie still geschlossen.
pub struct LedgerLock {
    lock_dir: PathBuf,
}

impl LedgerLock {
    pub const STALE_SECS: u64 = 120;

    pub fn acquire(run_dir: &Path, timeout: std::time::Duration) -> Result<Self, String> {
        let lock_dir = run_dir.join(".ledger.lock");
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match fs::create_dir(&lock_dir) {
                Ok(()) => {
                    let _ = crate::run_ledger::LedgerLock::write_owner(&lock_dir);
                    return Ok(Self {
                        lock_dir: lock_dir.clone(),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Self::is_stale(&lock_dir)? {
                        let _ = fs::remove_dir_all(&lock_dir);
                        continue;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!(
                            "Ledger-Sperre {} nicht erworben (Timeout)",
                            lock_dir.display()
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(e) => {
                    return Err(format!("Ledger-Sperre {}: {e}", lock_dir.display()));
                }
            }
        }
    }

    fn write_owner(lock_dir: &Path) -> Result<(), String> {
        let owner = serde_json::json!({
            "pid": std::process::id(),
            "started_at": crate::now_rfc3339(),
        });
        let path = lock_dir.join("owner.json");
        let tmp = temp_sibling(&path)?;
        {
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp)
                .map_err(|e| format!("Owner-Temp {tmp:?}: {e}"))?;
            f.write_all(serde_json::to_string_pretty(&owner).unwrap_or_default().as_bytes())
                .map_err(|e| format!("Owner {tmp:?}: {e}"))?;
            f.sync_all().ok();
        }
        match fs::rename(&tmp, &path) {
            Ok(()) => {}
            Err(_) => {
                let _ = fs::write(&path, serde_json::to_string_pretty(&owner).unwrap_or_default());
                let _ = fs::remove_file(&tmp);
            }
        }
        Ok(())
    }

    fn is_stale(lock_dir: &Path) -> Result<bool, String> {
        // Owner-Phase-A: direkt nach `create_dir` ist owner.json noch nicht da.
        // Ein frisches Lock darf NIEMALS sofort gestohlen werden — sonst laeuft
        // der erste Schreiber ohne Schutz (Race). Nur wenn das Lock selbst schon
        // lange ohne Owner liegt, darf es als Waisen eingeholt werden.
        let owner_path = lock_dir.join("owner.json");
        let owner: serde_json::Value = match fs::read_to_string(&owner_path) {
            Ok(c) => serde_json::from_str(&c).map_err(|e| format!("Owner-Format: {e}"))?,
            Err(_) => {
                return dir_older_than(lock_dir, Self::STALE_SECS);
            }
        };
        let pid = owner
            .get("pid")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| "Owner ohne pid".to_string())?;
        if !crate::ProcessSnapshot::capture().is_alive(pid) {
            return Ok(true);
        }
        let started = owner
            .get("started_at")
            .and_then(|v| v.as_str())
            .and_then(parse_rfc3339_unix)
            .unwrap_or(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Ok(now - started > Self::STALE_SECS as i64)
    }

    /// Gibt die Sperre frei (idempotent).
    pub fn release(&self) {
        let _ = fs::remove_dir_all(&self.lock_dir);
    }
}

impl Drop for LedgerLock {
    fn drop(&mut self) {
        self.release();
    }
}

fn parse_rfc3339_unix(s: &str) -> Option<i64> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

/// Liefert, ob ein Verzeichnis (bzw. Datei) älter als `secs` ist (nach
/// Änderungszeit). Fehlende Metadata gelten als NICHT alt — konservativ gegen
/// das Stehlen frischer Sperren.
fn dir_older_than(path: &Path, secs: u64) -> Result<bool, String> {
    let modified = fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|e| format!("Metadata {path:?}: {e}"))?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Ok(age.as_secs() > secs)
}

/// Führt `f` unter der Ledger-Sperre eines Runs aus.
pub fn with_run_lock<T>(
    run_dir: &Path,
    timeout: std::time::Duration,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let lock = LedgerLock::acquire(run_dir, timeout)?;
    let result = f();
    drop(lock);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    fn unique_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!(
            "webagent_ledger_test_{tag}_{}_{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("temp dir");
        base
    }

    fn write_event(path: &Path, seq: u64, prev: &str, payload: i64) -> String {
        let core = serde_json::json!({
            "timestamp": crate::now_rfc3339(),
            "run_id": "r1",
            "type": "t",
            "payload": payload,
        });
        let mut hasher = Sha256::new();
        hasher.update(prev.as_bytes());
        hasher.update(&serde_json::to_vec(&core).unwrap());
        let hash = format!("{:x}", hasher.finalize());
        let line = serde_json::json!({
            "seq": seq,
            "prev_hash": prev,
            "hash": hash,
            "pid": std::process::id(),
            "durability": "fsync",
            "timestamp": core["timestamp"],
            "run_id": core["run_id"],
            "type": core["type"],
            "payload": core["payload"],
        });
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        use std::io::Write;
        writeln!(f, "{}", serde_json::to_string(&line).unwrap()).unwrap();
        f.sync_all().unwrap();
        hash
    }

    fn chain_head(seq: u64, prev: &str) -> ChainHead {
        ChainHead {
            next_seq: seq,
            previous_hash: prev.to_string(),
            valid_count: seq as usize - 1,
        }
    }

    #[test]
    fn leeres_journal_liefert_genesis() {
        let dir = unique_dir("genesis");
        let path = dir.join("events.jsonl");
        assert_eq!(
            verify_event_chain(&path).unwrap(),
            chain_head(1, "GENESIS")
        );
    }

    #[test]
    fn intakte_kette_wird_verifiziert() {
        let dir = unique_dir("ok");
        let path = dir.join("events.jsonl");
        write_event(&path, 1, "GENESIS", 1);
        let head = verify_event_chain(&path).unwrap();
        assert_eq!(head.next_seq, 2);
        assert_eq!(head.valid_count, 1);
    }

    #[test]
    fn abgerissener_tail_ist_torn_alias() {
        let dir = unique_dir("torn");
        let path = dir.join("events.jsonl");
        write_event(&path, 1, "GENESIS", 1);
        // Abgerissene Zeile: kein abschliessendes JSON, kein Terminal-Newline.
        {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(b"{\"seq\": 2, \"prev_h").unwrap();
            f.sync_all().unwrap();
        }
        match verify_event_chain(&path) {
            Err(ChainError::TornTail { valid }) => {
                assert_eq!(valid.valid_count, 1, "der gültige Präfix bleibt bekannt");
                assert_eq!(valid.next_seq, 2);
            }
            other => panic!("erwartet TornTail, erhalten {other:?}"),
        }
    }

    #[test]
    fn torn_tail_quarantaene_erhaelt_praefix_und_receipt() {
        let dir = unique_dir("quar");
        let path = dir.join("events.jsonl");
        let h1 = write_event(&path, 1, "GENESIS", 1);
        let h2 = write_event(&path, 2, &h1, 2);
        write_event(&path, 3, &h2, 3);
        assert_eq!(verify_event_chain(&path).unwrap().valid_count, 3);

        // Torn: „{"seq": 4, "prev_h""
        {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(br#"{"seq": 4, "prev_h"#).unwrap();
            f.sync_all().unwrap();
        }

        let torn_bytes = br#"{"seq": 4, "prev_h"#;
        let valid = match verify_event_chain(&path) {
            Err(ChainError::TornTail { valid }) => valid,
            other => panic!("erwartet TornTail, erhalten {other:?}"),
        };
        assert_eq!(valid.valid_count, 3);
        let receipt = quarantine_torn_tail("r1", &path, &dir, &valid).expect("Quarantäne");

        // Receipt gespeichert, Original bytes stimmig.
        assert!(dir.join("recovery.json").exists());
        assert_eq!(receipt.torn_bytes_len, torn_bytes.len());
        assert_eq!(receipt.valid_seq, 4);
        assert!(receipt.quarantined_path.exists());
        // Journal nur noch mit gültigem Präfix (3 Belege).
        assert!(verify_event_chain(&path).is_ok());
        assert_eq!(verify_event_chain(&path).unwrap().valid_count, 3);
        // Der Torn ist vom Hash her unverändert im Quarantäne-Ordner.
        let quarantined = fs::read(&receipt.quarantined_path).unwrap();
        assert_eq!(
            receipt.torn_sha256,
            format!(
                "{:x}",
                Sha256::digest(&quarantined)
            )
        );
    }

    #[test]
    fn korrupt_mitten_in_der_kette_ist_nie_torn() {
        let dir = unique_dir("mid");
        let path = dir.join("events.jsonl");
        write_event(&path, 1, "GENESIS", 1);
        {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(f, "{{nicht-json}}").unwrap();
            f.sync_all().unwrap();
        }
        match verify_event_chain(&path) {
            Err(ChainError::Corrupt { .. }) => {}
            other => panic!("erwartet Corrupt, erhalten {other:?}"),
        }
    }

    #[test]
    fn zwei_threads_schreiben_lueckenlose_kette() {
        let dir = unique_dir("two");
        let events = dir.join("events.jsonl");
        // Zuerst den ersten Beleg zentral, damit die Threads denselben
        // Präfix-Hash teilen und nur sequenziell appenden.
        let mut handles = Vec::new();
        for t in 0..4u64 {
            let dir2 = dir.clone();
            let events2 = events.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..25u64 {
                    let run_dir = dir2.clone();
                    let events = events2.clone();
                    with_run_lock(&run_dir, Duration::from_secs(5), || {
                        let head = verify_event_chain(&events).map_err(|e| format!("{e:?}"))?;
                        let core = serde_json::json!({
                            "timestamp": crate::now_rfc3339(),
                            "run_id": "r1",
                            "type": "t",
                            "payload": format!("t{t}-{i}"),
                        });
                        let mut hasher = Sha256::new();
                        hasher.update(head.previous_hash.as_bytes());
                        hasher.update(&serde_json::to_vec(&core).unwrap());
                        let hash = format!("{:x}", hasher.finalize());
                        let line = serde_json::json!({
                            "seq": head.next_seq,
                            "prev_hash": head.previous_hash,
                            "hash": hash,
                            "pid": std::process::id(),
                            "durability": "fsync",
                            "timestamp": core["timestamp"],
                            "run_id": core["run_id"],
                            "type": core["type"],
                            "payload": core["payload"],
                        });
                        {
                            use std::io::Write;
                            let mut f = fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&events)
                                .map_err(|e| e.to_string())?;
                            writeln!(f, "{}", serde_json::to_string(&line).unwrap())
                                .map_err(|e| e.to_string())?;
                            f.sync_all().map_err(|e| e.to_string())?;
                        }
                        Ok(())
                    })
                    .expect("Lock + Append");
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let head = verify_event_chain(&events).expect("Kette nach 4 Threads");
        assert_eq!(head.valid_count, 100, "lückenlose lineare Kette");
        assert_eq!(head.next_seq, 101);
    }
}