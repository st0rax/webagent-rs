//! Gemessene Eingabelängen der Brain-Oberflächen — einmal ermitteln, dauerhaft
//! nutzen.
//!
//! Warum gemessen und nicht geschätzt: die Grenze ist nicht das Kontextfenster
//! des Modells, sondern die Eingabelänge der jeweiligen Weboberfläche. Die ist
//! je Anbieter verschieden, nirgends dokumentiert und ändert sich, wenn ein
//! Anbieter sein Frontend anfasst. Ein geratener Wert ist entweder zu klein
//! (dann liest sich ein Brain scheibchenweise durch Dateien) oder zu groß (dann
//! lehnt die Oberfläche ab und der Turn ist verloren).
//!
//! Die Messung läuft einmal je Brain und landet in
//! `<data>/brain_limits.json`. Kommt später ein Brain dazu, fehlt sein Eintrag
//! und es wird beim nächsten Lauf nachgemessen — ohne die bereits bekannten
//! erneut zu befragen.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Ein gemessener Eintrag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainLimit {
    /// Größte Zeichenzahl, die nachweislich angenommen wurde.
    pub accepted_chars: usize,
    /// Kleinste Zeichenzahl, die nachweislich abgelehnt wurde (falls bekannt).
    #[serde(default)]
    pub rejected_chars: Option<usize>,
    /// Wann gemessen (`now_rfc3339`).
    pub measured_at: String,
    /// Womit die Ablehnung erkannt wurde — für die Nachvollziehbarkeit.
    #[serde(default)]
    pub note: String,
}

/// Beobachtetes **Nachrichtenkontingent** einer Oberflaeche.
///
/// Absichtlich getrennt von [`BrainLimit`]: das eine misst, wie *lang* eine
/// Nachricht sein darf, das hier, wie *viele* in einem Zeitfenster erlaubt
/// sind. Zwei verschiedene Grenzen — und nur die zweite hat mistral und qwen
/// im Korpus gestoppt (Befund Claude 02:49: 51x „Nachrichtenlimit erreicht"
/// bei mistral, 8x „daily usage limit" bei qwen).
///
/// Eigene Karte statt Feldern in `BrainLimit`, weil [`unmeasured`] ueber
/// `contains_key` geht: ein Kontingent-Eintrag wuerde ein Brain sonst als
/// laengenvermessen ausweisen, ohne dass je gemessen wurde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageQuota {
    /// Wie oft eine Kontingent-Blockade beobachtet wurde.
    pub blocks: u32,
    /// Zeitpunkt der letzten Blockade (`now_rfc3339`).
    pub last_block_at: String,
    /// Aus der Meldung gelesenes Ruecksetz-Fenster in Minuten, falls erkennbar.
    #[serde(default)]
    pub window_minutes: Option<u32>,
    /// Unix-Sekunde, ab der wieder gesendet werden darf (wie
    /// `circuit_breaker::open_until`). Nur gesetzt, wenn das Fenster
    /// erkennbar war.
    #[serde(default)]
    pub blocked_until_unix: Option<u64>,
    /// Die Meldung im Wortlaut — damit nachvollziehbar bleibt, woher der Wert
    /// stammt.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LimitStore {
    #[serde(default)]
    pub brains: BTreeMap<String, BrainLimit>,
    /// Nachrichtenkontingente je Brain — siehe [`MessageQuota`].
    #[serde(default)]
    pub quotas: BTreeMap<String, MessageQuota>,
}

pub fn store_path() -> PathBuf {
    crate::config::data_dir().join("brain_limits.json")
}

pub fn load() -> LimitStore {
    load_at(&store_path())
}

pub fn load_at(path: &PathBuf) -> LimitStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_at(store: &LimitStore, path: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(store)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, body)
}

/// Gemessene Annahmegrenze eines Brains, falls vorhanden.
pub fn accepted_chars(brain_id: &str) -> Option<usize> {
    load().brains.get(brain_id).map(|l| l.accepted_chars)
}

/// Brains ohne Messwert — genau die, die noch befragt werden müssen.
pub fn unmeasured(brains: &[String]) -> Vec<String> {
    let store = load();
    brains
        .iter()
        .filter(|b| !store.brains.contains_key(*b))
        .cloned()
        .collect()
}

/// Ergebnis eintragen und sichern.
pub fn record(brain_id: &str, limit: BrainLimit) -> std::io::Result<()> {
    let path = store_path();
    let mut store = load_at(&path);
    store.brains.insert(brain_id.to_string(), limit);
    save_at(&store, &path)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Liest aus einer Blockademeldung, wann wieder gesendet werden darf.
///
/// Erkennt die Formen, die im Korpus wirklich vorkommen — deutsch
/// („Ihr Limit wird um in 3 Stunden zurueckgesetzt", „in 35 Minuten") und
/// englisch („Please wait 7 hours before trying again"). Eine Zahl zaehlt nur,
/// wenn innerhalb der naechsten zwei Wortteile eine Zeiteinheit folgt; damit
/// bleibt „Qwen3.7-Plus" eine Modellbezeichnung und keine Dauer.
pub fn parse_reset_minutes(msg: &str) -> Option<u32> {
    let lower = msg.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();

    for (i, tok) in tokens.iter().enumerate() {
        let Ok(n) = tok.parse::<u32>() else { continue };
        if n == 0 {
            continue;
        }
        for unit in tokens.iter().skip(i + 1).take(2) {
            if unit.starts_with("stunde") || unit.starts_with("hour") {
                return Some(n.saturating_mul(60));
            }
            if unit.starts_with("minute") || *unit == "min" || *unit == "mins" {
                return Some(n);
            }
        }
    }
    None
}

/// Ob eine Blockademeldung wirklich ein **Nachrichtenkontingent** ist.
///
/// Bewusst eng: `blocked` deckt auch Cloudflare, Login-Zwang und
/// Verfuegbarkeitsstoerungen ab. Wer die alle als Kontingent bucht, baut
/// wieder eine Zahl ohne Bedeutung.
pub fn looks_like_quota_block(msg: &str) -> bool {
    let l = msg.to_lowercase();
    l.contains("nachrichtenlimit")
        || l.contains("message limit")
        || l.contains("usage limit")
        || l.contains("rate limit")
        || l.contains("limit erreicht")
        || l.contains("limit reached")
        || l.contains("limit wird")
}

/// Eine beobachtete Kontingent-Blockade festhalten.
pub fn record_block(brain_id: &str, note: &str) -> std::io::Result<()> {
    record_block_at(&store_path(), brain_id, note, now_unix())
}

/// Wie [`record_block`], aber mit explizitem Pfad und Zeitpunkt — testbar.
pub fn record_block_at(
    path: &PathBuf,
    brain_id: &str,
    note: &str,
    now_secs: u64,
) -> std::io::Result<()> {
    let mut store = load_at(path);
    let window_minutes = parse_reset_minutes(note);
    let previous = store.quotas.get(brain_id).map(|q| q.blocks).unwrap_or(0);
    store.quotas.insert(
        brain_id.to_string(),
        MessageQuota {
            blocks: previous.saturating_add(1),
            last_block_at: crate::now_rfc3339(),
            window_minutes,
            blocked_until_unix: window_minutes
                .map(|m| now_secs.saturating_add(u64::from(m).saturating_mul(60))),
            note: note.trim().to_string(),
        },
    );
    save_at(&store, path)
}

/// Unix-Sekunde, bis zu der das Brain gesperrt ist — `None`, wenn nichts
/// bekannt ist oder das Fenster nicht erkannt wurde.
pub fn blocked_until(brain_id: &str) -> Option<u64> {
    load().quotas.get(brain_id).and_then(|q| q.blocked_until_unix)
}

/// Ob gerade nicht gesendet werden sollte.
///
/// Bewusst konservativ: ohne erkanntes Fenster gilt das Brain als frei. Lieber
/// einmal ins Limit laufen als ein funktionierendes Brain dauerhaft aussperren.
pub fn is_blocked(brain_id: &str) -> bool {
    blocked_until(brain_id).is_some_and(|until| now_unix() < until)
}

/// Ergebnis eines einzelnen Sendeversuchs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Die Oberflaeche hat die Eingabe angenommen.
    Accepted,
    /// Die Oberflaeche hat wegen der Laenge abgelehnt.
    Rejected,
    /// Kein Laengenproblem (Login, Kontingent, Netz): Suche abbrechen.
    Aborted(String),
}

/// Steuerung der Suche.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Erste Probengroesse.
    pub start: usize,
    /// Obergrenze, ueber die nicht hinaus gemessen wird.
    pub ceiling: usize,
    /// Genauigkeit: die Schachtelung endet, wenn der Abstand zwischen letztem
    /// angenommenen und erstem abgelehnten Wert darunter liegt.
    pub tolerance: usize,
    /// Notbremse gegen Endlosschleifen und Kontingentverbrauch.
    pub max_probes: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            start: 100_000,
            ceiling: 2_000_000,
            tolerance: 10_000,
            max_probes: 24,
        }
    }
}

/// Ergebnis der Suche.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchResult {
    /// Groesster nachweislich angenommener Wert.
    pub accepted: Option<usize>,
    /// Kleinster nachweislich abgelehnter Wert.
    pub rejected: Option<usize>,
    /// Anzahl tatsaechlich gesendeter Proben.
    pub probes: usize,
    /// Gesetzt, wenn abgebrochen wurde (kein Laengenproblem).
    pub aborted: Option<String>,
    /// Wahr, wenn die Obergrenze angenommen wurde und nie eine Ablehnung kam —
    /// dann ist `accepted` nur eine untere Schranke, KEINE Grenze.
    pub hit_ceiling: bool,
}

/// Sucht die echte Laengengrenze: erst verdoppeln bis zur ersten Ablehnung,
/// dann zwischen letztem angenommenen und erstem abgelehnten Wert schachteln.
///
/// Warum nicht die alte feste Leiter: die endete bei 100.000 und meldete deren
/// Annahme als „Grenze". Vier Brains standen daraufhin mit
/// `accepted=100000, rejected=null` in der Datei — das heisst „bis hierhin
/// angenommen, nie abgelehnt" und eben NICHT „hier ist Schluss".
///
/// `probe` ist eine reine Ja/Nein-Closure, damit die Suchlogik ohne Browser
/// testbar ist (wie `design_vote` seine Query-Closure hereinreicht).
pub fn search_limit<F>(cfg: &SearchConfig, mut probe: F) -> SearchResult
where
    F: FnMut(usize) -> ProbeOutcome,
{
    let mut out = SearchResult::default();
    let ceiling = cfg.ceiling.max(1);
    let tolerance = cfg.tolerance.max(1);
    let mut groesse = cfg.start.clamp(1, ceiling);

    // Phase 1: verdoppeln, bis abgelehnt wird oder die Decke erreicht ist.
    loop {
        if out.probes >= cfg.max_probes {
            return out;
        }
        out.probes += 1;
        match probe(groesse) {
            ProbeOutcome::Accepted => {
                out.accepted = Some(groesse);
                if groesse >= ceiling {
                    out.hit_ceiling = true;
                    return out;
                }
                groesse = groesse.saturating_mul(2).min(ceiling);
            }
            ProbeOutcome::Rejected => {
                out.rejected = Some(groesse);
                break;
            }
            ProbeOutcome::Aborted(grund) => {
                out.aborted = Some(grund);
                return out;
            }
        }
    }

    // Phase 2: Intervallschachtelung zwischen bekannt gut und bekannt schlecht.
    let mut lo = out.accepted.unwrap_or(0);
    let mut hi = out.rejected.expect("Phase 2 nur nach einer Ablehnung");
    while hi.saturating_sub(lo) > tolerance {
        if out.probes >= cfg.max_probes {
            return out;
        }
        let mid = lo + (hi - lo) / 2;
        if mid <= lo || mid >= hi {
            break;
        }
        out.probes += 1;
        match probe(mid) {
            ProbeOutcome::Accepted => {
                lo = mid;
                out.accepted = Some(mid);
            }
            ProbeOutcome::Rejected => {
                hi = mid;
                out.rejected = Some(mid);
            }
            ProbeOutcome::Aborted(grund) => {
                out.aborted = Some(grund);
                return out;
            }
        }
    }
    out
}

/// Erkennt an der Antwort (oder am Fehler), ob die Oberflaeche die Eingabe
/// abgelehnt hat.
///
/// Storax' Hinweis vom 30.07.2026: die Oberflaechen schneiden nicht still ab,
/// sie melden eine ueberschrittene Zeichenlaenge. Genau diese Meldungen werden
/// hier erkannt — plus die Faelle, in denen das Senden gar nicht erst gelingt.
pub fn looks_like_length_rejection(text: &str) -> bool {
    let low = text.to_lowercase();
    [
        "zu lang",
        "too long",
        "maximum length",
        "max length",
        "character limit",
        "zeichenlimit",
        "zeichenbegrenzung",
        "exceeds",
        "überschritten",
        "ueberschritten",
        "message is too long",
        "prompt is too long",
        "reduce the length",
        "kürzen",
        "kuerzen",
        // Deutsche Oberflaechen (Storax' Konten sind teils auf Deutsch)
        "zu viele zeichen",
        "maximale länge",
        "maximale laenge",
        "höchstlänge",
        "hoechstlaenge",
        "kürzere nachricht",
        "kuerzere nachricht",
        "verkürze",
        "verkuerze",
        "eingabe ist zu",
        "nachricht ist zu",
        // Englische Varianten weiterer Oberflaechen
        "too many characters",
        "shorten your",
        "shorter message",
        "input is too long",
        "text is too long",
        "context length",
        "conversation is too long",
        // Chinesische Oberflaechen (deepseek, kimi, zai)
        "太长",
        "过长",
        "字数限制",
        "超出长度",
        "超过字数",
    ]
    .iter()
    .any(|m| low.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> PathBuf {
        let n = std::process::id();
        std::env::temp_dir().join(format!("webagent_limits_test_{n}.json"))
    }

    /// Closure-Fabrik: alles bis `grenze` wird angenommen, darueber abgelehnt.
    fn brain_mit_grenze(grenze: usize) -> impl FnMut(usize) -> ProbeOutcome {
        move |n| {
            if n <= grenze {
                ProbeOutcome::Accepted
            } else {
                ProbeOutcome::Rejected
            }
        }
    }

    #[test]
    fn schachtelung_findet_die_grenze() {
        let cfg = SearchConfig {
            start: 100_000,
            ceiling: 2_000_000,
            tolerance: 10_000,
            max_probes: 40,
        };
        for grenze in [150_000usize, 320_000, 999_999, 1_500_000] {
            let r = search_limit(&cfg, brain_mit_grenze(grenze));
            let acc = r.accepted.expect("angenommener Wert");
            let rej = r.rejected.expect("abgelehnter Wert");
            assert!(acc <= grenze, "angenommen {acc} > echte Grenze {grenze}");
            assert!(rej > grenze, "abgelehnt {rej} <= echte Grenze {grenze}");
            assert!(
                rej - acc <= cfg.tolerance,
                "Intervall {acc}..{rej} nicht eingegrenzt"
            );
            assert!(!r.hit_ceiling);
        }
    }

    #[test]
    fn nie_abgelehnt_meldet_nur_untere_schranke() {
        let cfg = SearchConfig {
            start: 100_000,
            ceiling: 800_000,
            tolerance: 10_000,
            max_probes: 40,
        };
        let r = search_limit(&cfg, brain_mit_grenze(usize::MAX));
        assert_eq!(r.accepted, Some(800_000));
        assert_eq!(r.rejected, None, "ohne Ablehnung darf keine Grenze stehen");
        assert!(r.hit_ceiling, "Deckentreffer muss kenntlich sein");
    }

    #[test]
    fn schon_die_erste_probe_abgelehnt_sucht_nach_unten() {
        let cfg = SearchConfig {
            start: 100_000,
            ceiling: 2_000_000,
            tolerance: 5_000,
            max_probes: 40,
        };
        let r = search_limit(&cfg, brain_mit_grenze(20_000));
        let acc = r.accepted.expect("angenommener Wert");
        let rej = r.rejected.expect("abgelehnter Wert");
        assert!(acc <= 20_000 && rej > 20_000);
        assert!(rej - acc <= cfg.tolerance);
    }

    #[test]
    fn keine_endlosschleife_bei_widerspruechlichem_brain() {
        // Brain antwortet zufaellig-wechselnd: die Suche muss trotzdem enden.
        let mut i = 0usize;
        let cfg = SearchConfig {
            start: 100_000,
            ceiling: 2_000_000,
            tolerance: 1,
            max_probes: 12,
        };
        let r = search_limit(&cfg, |_| {
            i += 1;
            if i.is_multiple_of(2) {
                ProbeOutcome::Accepted
            } else {
                ProbeOutcome::Rejected
            }
        });
        assert!(r.probes <= 12, "Notbremse muss greifen: {}", r.probes);
    }

    #[test]
    fn abbruch_beendet_die_suche_ohne_wert() {
        let cfg = SearchConfig::default();
        let mut n = 0usize;
        let r = search_limit(&cfg, |_| {
            n += 1;
            ProbeOutcome::Aborted("nicht angemeldet".to_string())
        });
        assert_eq!(n, 1, "nach Abbruch darf nicht weitergeraten werden");
        assert_eq!(r.accepted, None);
        assert!(r.aborted.is_some());
    }

    #[test]
    fn deutsche_und_chinesische_ablehnungen_werden_erkannt() {
        assert!(looks_like_length_rejection("Deine Eingabe ist zu umfangreich — bitte verkürze sie"));
        assert!(looks_like_length_rejection("Zu viele Zeichen"));
        assert!(looks_like_length_rejection("消息太长，请缩短后重试"));
        assert!(looks_like_length_rejection("输入内容过长"));
        assert!(!looks_like_length_rejection("Alles klar, ich fange an."));
    }

    #[test]
    fn ablehnung_wird_an_der_meldung_erkannt() {
        assert!(looks_like_length_rejection(
            "Deine Nachricht ist zu lang. Bitte kürzen."
        ));
        assert!(looks_like_length_rejection(
            "Your message is too long. Please reduce the length."
        ));
        assert!(looks_like_length_rejection("Zeichenlimit überschritten"));
        // Eine normale Antwort darf nicht als Ablehnung gelten.
        assert!(!looks_like_length_rejection(
            "OK, ich habe die Datei gelesen und schlage folgende Aenderung vor."
        ));
    }

    #[test]
    fn speichern_und_lesen_ueberlebt_den_roundtrip() {
        let path = tmp_store();
        let _ = std::fs::remove_file(&path);
        let mut store = LimitStore::default();
        store.brains.insert(
            "deepseek".to_string(),
            BrainLimit {
                accepted_chars: 50_000,
                rejected_chars: Some(100_000),
                measured_at: "2026-07-30T18:00:00+00:00".to_string(),
                note: "Leiter".to_string(),
            },
        );
        save_at(&store, &path).expect("schreibbar");
        let gelesen = load_at(&path);
        assert_eq!(gelesen.brains["deepseek"].accepted_chars, 50_000);
        assert_eq!(gelesen.brains["deepseek"].rejected_chars, Some(100_000));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn kaputte_datei_ist_kein_harter_fehler() {
        let path =
            std::env::temp_dir().join(format!("webagent_limits_bad_{}.json", std::process::id()));
        std::fs::write(&path, "{ das ist kein json").expect("schreibbar");
        assert!(load_at(&path).brains.is_empty(), "leerer Store statt Panik");
        let _ = std::fs::remove_file(&path);
    }

    // --- Nachrichtenkontingent (Befund Claude 02:49) ---------------------

    fn tmp_quota_store(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "webagent_quota_{name}_{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn ruecksetzfenster_deutsch_stunden_und_minuten() {
        // Wortlaut aus brain_score/events.jsonl, mistral.
        assert_eq!(
            parse_reset_minutes(
                "blockiert: Nachrichtenlimit erreicht Sie haben Ihr Nachrichtenlimit \
                 erreicht. Ihr Limit wird um in 3 Stunden zurückgesetzt. Upgrade"
            ),
            Some(180)
        );
        // Wortlaut aus circuit_breaker/state.json.
        assert_eq!(
            parse_reset_minutes("Ihr Limit wird um in 35 Minuten zurückgesetzt."),
            Some(35)
        );
    }

    #[test]
    fn ruecksetzfenster_englisch() {
        // Wortlaut aus brain_score/events.jsonl, qwen.
        assert_eq!(
            parse_reset_minutes(
                "You have reached the daily usage limit. Please wait 7 hours before trying again."
            ),
            Some(420)
        );
    }

    #[test]
    fn modellname_ist_keine_dauer() {
        // Der qwen-Text nennt „Qwen3.7-Plus" — daraus darf kein Fenster werden.
        assert_eq!(
            parse_reset_minutes("Oops! There was an issue connecting to Qwen3.7-Plus."),
            None
        );
        assert_eq!(parse_reset_minutes("kein zeitbezug hier"), None);
        assert_eq!(parse_reset_minutes("in 0 Stunden"), None, "0 ist kein Fenster");
    }

    #[test]
    fn blockade_wird_gezaehlt_und_das_fenster_gesetzt() {
        let path = tmp_quota_store("count");
        let _ = std::fs::remove_file(&path);
        let now = 1_000_000u64;

        record_block_at(&path, "mistral", "Ihr Limit wird um in 3 Stunden zurückgesetzt.", now)
            .expect("schreibbar");
        record_block_at(&path, "mistral", "Ihr Limit wird um in 3 Stunden zurückgesetzt.", now)
            .expect("schreibbar");

        let q = &load_at(&path).quotas["mistral"];
        assert_eq!(q.blocks, 2, "zweite Blockade zaehlt hoch");
        assert_eq!(q.window_minutes, Some(180));
        assert_eq!(q.blocked_until_unix, Some(now + 180 * 60));
        assert!(q.note.contains("Nachrichtenlimit") || q.note.contains("Limit"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ohne_erkennbares_fenster_keine_sperre() {
        let path = tmp_quota_store("nowindow");
        let _ = std::fs::remove_file(&path);

        record_block_at(&path, "qwen", "blockiert: irgendeine Meldung ohne Zeitangabe", 42)
            .expect("schreibbar");

        let q = &load_at(&path).quotas["qwen"];
        assert_eq!(q.blocks, 1);
        assert_eq!(q.window_minutes, None);
        assert_eq!(
            q.blocked_until_unix, None,
            "lieber einmal ins Limit laufen als dauerhaft aussperren"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nur_echte_kontingent_meldungen_zaehlen() {
        assert!(looks_like_quota_block(
            "blockiert: Nachrichtenlimit erreicht Ihr Limit wird um in 3 Stunden zurückgesetzt."
        ));
        assert!(looks_like_quota_block(
            "You have reached the daily usage limit. Please wait 7 hours."
        ));
        // Andere Blockadearten duerfen NICHT als Kontingent gebucht werden.
        assert!(!looks_like_quota_block("cloudflare challenge"));
        assert!(!looks_like_quota_block("login_required"));
        assert!(!looks_like_quota_block("brain_unavailable"));
        assert!(!looks_like_quota_block(
            "Oops! There was an issue connecting to Qwen3.7-Plus."
        ));
    }

    #[test]
    fn kontingent_stoert_die_laengenmessung_nicht() {
        let path = tmp_quota_store("mixed");
        let _ = std::fs::remove_file(&path);

        record_block_at(&path, "mistral", "in 3 Stunden", 0).expect("schreibbar");
        let store = load_at(&path);

        assert!(
            !store.brains.contains_key("mistral"),
            "ein Kontingent-Eintrag darf mistral NICHT als laengenvermessen ausweisen"
        );
        assert!(store.quotas.contains_key("mistral"));

        let _ = std::fs::remove_file(&path);
    }
}
