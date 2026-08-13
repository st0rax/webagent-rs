//! Banner-/Blockade-Erkennung des Web-Backends: die Phrasenliste, die
//! Echo-Pruefung (eigene Frage vs. Anbieter-Banner) und die
//! Vollstaendigkeits-Entscheidung einer laufenden Antwort.
//!
//! Extrahiert aus `mod.rs` am 2026-08-09 (browser-Split). Aufgerufen von
//! `send` (Banner-Scan im Sende-Pfad) und `backend` (Rate-Limit-Pfad); die
//! Tests in `mod.rs` und externe Aufrufer greifen ueber die Re-Exports aus
//! `mod.rs` zu.

use crate::observer::{is_claude_limit_response_text, is_transient_response_text};
use crate::protocol::is_possibly_truncated;

use super::has_protocol_payload;

pub(crate) const STABILITY_SECONDS: f64 = 1.5;

/// Stabilitätsfenster für Text, der noch keine Protokoll-Nutzlast enthält.
/// Deutlich länger als `STABILITY_SECONDS`, weil ein Reasoning-Block zwischen
/// Denk- und Antwortphase sekundenlang stillstehen kann. Siehe
/// `classify_completion`.
pub(crate) const PROSE_STABILITY_SECONDS: f64 = 8.0;

/// Ein syntaktisch offener Protokollblock kann ein echter Stream-Zwischenstand
/// oder eine bereits beendete, fehlerhafte Modellantwort sein. Nach diesem
/// stabilen Fenster wird letzteres an den Parser-/Repair-Pfad weitergereicht,
/// statt bis zum Provider-Timeout auf Bytes zu warten, die nie mehr kommen.
pub(crate) const TRUNCATED_STABILITY_SECONDS: f64 = 8.0;

/// Phrasen, die auf eine externe Blockierung hindeuten (Tages-/Nachrichtenlimit,
/// Login, Cloudflare) — DE+EN. Geteilt zwischen `detect_block_banner` (JS-Scan der
/// ganzen Seite) und `block_phrase_in_text` (reine Rust-Pruefung des bereits
/// gelesenen Antworttexts), damit beide dieselbe Liste verwenden.
pub(crate) const BLOCK_PHRASES: &[&str] = &[
    "nachrichtenlimit",
    "message limit",
    "usage limit",
    "rate limit",
    "ratelimit",
    "daily limit",
    "tageslimit",
    "limit reached",
    "limit erreicht",
    "too many requests",
    "quota exceeded",
    "you have reached",
    "verify you are human",
    "checking your browser",
    // Kapazitaets-/Auslastungsmeldungen — anderes Muster als "Limit erreicht":
    // kein persoenliches Kontingent, sondern "der Dienst ist gerade ueberlastet".
    // Ausloeser fuer diese Ergaenzung: kimi zeigt unter Last einen Dialog/Overlay,
    // der den Composer blockiert; der genaue Wortlaut war zum Zeitpunkt des Fixes
    // nicht reproduzierbar (live_diagnose traf kimi im Ready-Zustand), daher eine
    // Bandbreite plausibler Formulierungen statt einer einzelnen bestaetigten Phrase.
    "too many users",
    "too many people",
    "high traffic",
    "currently busy",
    "server is busy",
    "at capacity",
    "zu viele nutzer",
    "zu viele anfragen",
    "überlastet",
    "derzeit ausgelastet",
    "cloudflare",
];

/// Prueft einen bereits gelesenen Antworttext (nicht die ganze Seite) auf eine
/// Block-Phrase. Faengt Faelle wie qwen, wo das Limit-Banner NICHT separat auf der
/// Seite steht, sondern als Text INNERHALB des Antwort-Containers erscheint — dort
/// sah `wait_response` es vorher nicht, weil der periodische Banner-Scan nur laeuft,
/// solange noch kein Text da ist, und ein bereits "vollstaendiger" Text-Block direkt
/// als echte Antwort durchgereicht wurde.
/// Obergrenze (Zeichen), bis zu der ein Text als Block-*Banner* gelten darf.
/// Echte Limit-/Auslastungs-Banner sind kurz (ein bis zwei Sätze). Eine lange
/// inhaltliche Antwort, die "rate limit"/"usage limit" nur ERWÄHNT (z.B. als
/// Verbesserungsvorschlag), ist KEIN Block — genau dieser False-Positive trat
/// im Swarm-Test "Verbesserungsvorschläge zu webagent-rs" auf: mistrals/
/// deepseeks legitime Essays empfahlen Rate-Limiting und wurden als "blocked"
/// verworfen.
pub(crate) const BLOCK_BANNER_MAX_CHARS: usize = 400;

/// Normalisiert Text für den Echo-Vergleich: Kleinschreibung, Whitespace
/// kollabiert — genau wie das JS die Seite einliest.
pub(crate) fn normalize_for_echo(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `true`, wenn der gefundene „Banner"-Ausschnitt in Wahrheit unsere eigene,
/// gerade gesendete Frage ist.
///
/// `detect_block_banner` liest `document.body.innerText` — also die GANZE Seite
/// samt der eben abgeschickten Nachricht. Enthält die Aufgabe selbst eines der
/// Stichwörter („Nachrichtenlimit", „Login", „Cloudflare"), meldet die Erkennung
/// jedes Brain als blockiert. Real passiert am 2026-07-21: eine Swarm-Frage, die
/// eine Fehlerstatistik zitierte, ließ 4 von 8 Brains fälschlich als „blocked"
/// gelten — die Blockade-Meldung enthielt wörtlich den Fragetext.
///
/// Der Ausschnitt ist ein Fenster um den Treffer (20 Zeichen davor, 120 danach)
/// und daher meist an beiden Rändern angeschnitten. Verglichen wird deshalb der
/// längste zusammenhängende Kern, nicht der Ausschnitt als Ganzes.
pub(crate) fn banner_is_prompt_echo(banner: &str, prompt: &str) -> bool {
    if prompt.trim().is_empty() {
        return false;
    }
    let hay = normalize_for_echo(prompt);
    let needle = normalize_for_echo(banner);
    if needle.is_empty() {
        return false;
    }
    if hay.contains(&needle) {
        return true;
    }
    // Ränder abschneiden: an Wortgrenzen von beiden Seiten einkürzen, bis ein
    // hinreichend langer Kern übrig ist, der im Prompt vorkommt.
    let words: Vec<&str> = needle.split(' ').collect();
    const MIN_CORE_WORDS: usize = 5;
    for start in 0..words.len() {
        for end in (start + MIN_CORE_WORDS..=words.len()).rev() {
            let core = words[start..end].join(" ");
            if core.chars().count() >= 25 && hay.contains(&core) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn block_phrase_in_text(text: &str) -> Option<&'static str> {
    // Nur kurze Texte können ein Banner sein; in Fließtext ist die Phrase Inhalt.
    if text.chars().count() > BLOCK_BANNER_MAX_CHARS {
        return None;
    }
    let low = text.to_lowercase();
    let phrase_count = BLOCK_PHRASES
        .iter()
        .filter(|phrase| low.contains(**phrase))
        .count();
    // Kurze technische Aufzählungen wie `Tageslimit/Login/Cloudflare` kommen
    // in Code und Diagnostik vor. Provider-Banner formulieren dagegen eine
    // konkrete Meldung und listen nicht mehrere Kategorien per Slash auf.
    if low.contains('/') && phrase_count >= 2 {
        return None;
    }
    BLOCK_PHRASES
        .iter()
        .copied()
        // `cloudflare` allein ist in Code, Logs und technischen Antworten ein
        // normaler Begriff. Echte Challenges werden bereits durch die beiden
        // spezifischen Phrasen sowie den separaten Seiten-/URL-Check erkannt.
        .filter(|phrase| *phrase != "cloudflare")
        .find(|phrase| low.contains(phrase))
}

/// Erkennt kurze technische Kategorienlisten aus Code/Diagnostik, die der
/// Ganzseiten-Scan als vermeintlichen Provider-Banner ausschneiden kann.
pub(crate) fn is_technical_block_phrase_list(text: &str) -> bool {
    let low = text.to_lowercase();
    low.contains('/')
        && BLOCK_PHRASES
            .iter()
            .filter(|phrase| low.contains(**phrase))
            .take(2)
            .count()
            >= 2
}

/// Baut das JS der Block-Banner-Suche. Geteilt zwischen Implementierung und
/// Test — der Mock-Driver matcht auf die EXAKTE Zeichenkette.
pub(crate) fn block_banner_expr() -> String {
    let pats_js = BLOCK_PHRASES
        .iter()
        .map(|p| format!("'{p}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"(function(){{
var b=(document.body?document.body.innerText:'').replace(/\s+/g,' ');
var low=b.toLowerCase();
var pats=[{pats_js}];
for(var i=0;i<pats.length;i++){{var k=low.indexOf(pats[i]);if(k>=0){{return b.slice(Math.max(0,k-20),k+120);}}}}
return null;}})()"#
    )
}

/// Ergebnis der Vollständigkeitsprüfung einer laufenden Antwort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Completion {
    /// Antwort ist vollständig — zurückgeben.
    Complete,
    /// Noch nicht fertig — weiter beobachten.
    Continue,
    /// Rate-Limit/Usage-Banner statt echter Antwort.
    RateLimited,
}

/// Reine, testbare Entscheidung, ob eine Antwort vollständig ist.
///
/// Autoritatives Fertigsignal ist das Verschwinden des Stop-/Generating-Buttons
/// (bzw. ein bereits vollständiges Protokoll-Dokument). Reine Textstabilität ist
/// nur der Fallback für UIs ohne erkennbaren Stop-Button. Damit werden die zwei
/// Hauptprobleme adressiert: (a) Timeout mitten im Stream (wir warten, solange der
/// Stop-Button sichtbar ist) und (b) fälschlich „unvollständig" (ein vollständiges
/// JSON gilt sofort als fertig, unabhängig vom Stabilitätsfenster).
pub(crate) fn classify_completion(
    text: &str,
    has_stop_selectors: bool,
    stop_seen_ever: bool,
    stop_visible: bool,
    stable_secs: f64,
    rate_limit_aware: bool,
) -> Completion {
    // Die Rate-Limit-Erkennung ist Claude-spezifisch (`claude_rate_limited`) und wird
    // NUR fuer claude angewandt. Sonst schlug sie fuer andere Brains fehl: qwens
    // Ausgabe/UI-Chrome enthielt "…limit…", wurde faelschlich als Claude-Limit
    // gewertet und der (terminale) Rate-Limit-Pfad brach den Lauf ohne Retry ab.
    if rate_limit_aware && is_claude_limit_response_text(text) {
        return Completion::RateLimited;
    }

    let text_ready = !text.trim().is_empty() && !is_transient_response_text(text);
    if !text_ready {
        return Completion::Continue;
    }

    if is_possibly_truncated(text) {
        // Solange die UI noch generiert, ist dies ein echter Zwischenstand.
        // Ist der Text dagegen stabil und kein Stop-Signal mehr sichtbar,
        // muss der Parser den geschlossenen Transport als invalid reparieren
        // duerfen. Andernfalls kostet ein fehlender END-Marker jedes Mal den
        // vollen wait_response-Timeout.
        return if !stop_visible && stable_secs >= TRUNCATED_STABILITY_SECONDS {
            Completion::Complete
        } else {
            Completion::Continue
        };
    }

    // Ein vollständig geparstes Protokoll-Dokument ist immer fertig — auch wenn
    // der Stop-Button (durch Polling-Timing) noch kurz sichtbar wirkt.
    if crate::protocol::parse(text).valid {
        return Completion::Complete;
    }

    // Antwort ohne jede Protokoll-Nutzlast braucht ein deutlich längeres
    // Stabilitätsfenster.
    //
    // Grund (gemessen am 29.07.2026): kimis `stop_button`-Selektoren sind
    // geratene `aria-label*='Stop'`-Muster und greifen nie. Damit fiel die
    // Entscheidung auf das kurze Stabilitätsfenster zurück — und das war
    // erreicht, sobald der Reasoning-Block fertig war und der eigentliche
    // Antwort-Block noch nicht begonnen hatte. Ergebnis: mitten im Stream
    // geerntete Prosa (186–292 Zeichen, mitten im Wort abgebrochen), die als
    // `protocol_invalid` verworfen wurde und einen Repair-Roundtrip kostete.
    // Über alle 108 Läufe des Tages waren so 145 von ~500 Brain-Turns (29 %)
    // reine Verschwendung — der größte Produktivitätsverlust im Dauerlauf.
    //
    // Im Protokollmodus ist Text ohne Nutzlast entweder Zwischenstand oder
    // Regelbruch. Beides verträgt ein paar Sekunden Warten; ein
    // Repair-Roundtrip kostet 10–35 s.
    if !has_protocol_payload(text) && stable_secs < PROSE_STABILITY_SECONDS {
        return Completion::Continue;
    }

    if has_stop_selectors {
        if stop_seen_ever && !stop_visible {
            // Generierung war aktiv und ist nun beendet.
            Completion::Complete
        } else if !stop_seen_ever && stable_secs >= STABILITY_SECONDS * 1.5 {
            // Stop-Button wurde nie erfasst (sehr schnelle Antwort) — nach etwas
            // längerer Stabilität dennoch als fertig werten, statt zu blockieren.
            Completion::Complete
        } else {
            Completion::Continue
        }
    } else if stable_secs >= STABILITY_SECONDS {
        Completion::Complete
    } else {
        Completion::Continue
    }
}
