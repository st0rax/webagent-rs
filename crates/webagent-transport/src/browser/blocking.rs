//! Banner-/Blockade-Erkennung des Web-Backends: die Phrasenliste, die
//! Echo-Pruefung (eigene Frage vs. Anbieter-Banner) und die
//! Vollstaendigkeits-Entscheidung einer laufenden Antwort.
//!
//! Extrahiert aus `mod.rs` am 2026-08-09 (browser-Split). Aufgerufen von
//! `send` (Banner-Scan im Sende-Pfad) und `backend` (Rate-Limit-Pfad); die
//! Tests in `mod.rs` und externe Aufrufer greifen ueber die Re-Exports aus
//! `mod.rs` zu.

use webagent_core::circuit_breaker::contains_at_word_boundary;
use webagent_core::observer::is_limit_response_text;
use webagent_core::protocol::{has_protocol_payload, is_possibly_truncated, BLOCK_PHRASES};

// Die gemeinsame Phrasenliste und der reine Text-Klassifikator liegen jetzt in
// `webagent_core::protocol`; `mod.rs` re-exportiert sie weiterhin fuer bestehende Pfade.

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

/// Normalisierte Vergleichsform fuer die Stabilitaets-Uhr in Phase 2: reiner
/// Whitespace-/Formatierungs-Churn (stille Reflows, unsichtbare Zeichen) startet
/// die Uhr nicht neu — sonst kostet jeder stille Reflow bis zu 8 s extra,
/// obwohl die Antwort laengst steht. Echte Tokens aendern immer auch diese
/// Form; geerntet wird weiter der Rohtext. Siehe `wait_response_streaming`.
pub(crate) fn stable_norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

/// Erkennt kurze technische Kategorienlisten aus Code/Diagnostik, die der
/// Ganzseiten-Scan als vermeintlichen Provider-Banner ausschneiden kann.
///
/// Ausser Slash-Listen auch der Fall vom 17.08.2026: der Scan schneidet
/// Identifier (`is_cloudflare_blocked`) oder einen Vorschlagstitel
/// (`Rate Limiting`) aus. Die Phrase steckt dann nur als Wortbestandteil
/// drin, nie als eigenes Wort.
pub(crate) fn is_technical_block_phrase_list(text: &str) -> bool {
    let low = text.to_lowercase();
    let substring_hits: Vec<&str> = BLOCK_PHRASES
        .iter()
        .copied()
        .filter(|phrase| low.contains(*phrase))
        .collect();
    if substring_hits.is_empty() {
        return false;
    }
    if low.contains('/') && substring_hits.len() >= 2 {
        return true;
    }
    !substring_hits
        .iter()
        .any(|phrase| contains_at_word_boundary(&low, phrase))
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
function ident(c){{return /[0-9_]/.test(c)||c.toLowerCase()!==c.toUpperCase();}}
function atBound(s,i,n){{var before=i===0||!ident(s.charAt(i-1));var after=i+n>=s.length||!ident(s.charAt(i+n));return before&&after;}}
for(var i=0;i<pats.length;i++){{var p=pats[i];var k=0;while((k=low.indexOf(p,k))>=0){{if(atBound(low,k,p.length)){{return b.slice(Math.max(0,k-20),k+120);}}k+=1;}}}}
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
    // Die Rate-Limit-Erkennung wird nur dort aktiviert, wo das Profil einen
    // passenden Banner-Selektor gemeldet hat. Die Textprüfung selbst ist
    // generisch; ob sie terminal wirkt, entscheidet der Aufrufer.
    if rate_limit_aware && is_limit_response_text(text) {
        return Completion::RateLimited;
    }

    // Chrome / CoT-only snapshots (Thinking..., UI clock, Kimi reasoning echo)
    // must keep the waiter polling — never finish with status text as the answer.
    let text_ready = !webagent_core::observer::chat_answer_text(text).is_empty();
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
    if webagent_core::protocol::parse(text).valid {
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

#[cfg(test)]
mod tests {
    use super::stable_norm;

    #[test]
    fn stable_norm_ignores_whitespace_churn() {
        assert_eq!(stable_norm("Hallo  Welt\n"), stable_norm("Hallo Welt"));
        assert_eq!(stable_norm(""), stable_norm("   \n\t "));
    }

    #[test]
    fn stable_norm_keeps_real_tokens() {
        assert_ne!(stable_norm("Hallo Welt"), stable_norm("Hallo Welt!"));
        assert_ne!(
            stable_norm("Antwort steht"),
            stable_norm("Antwort steht fast")
        );
    }
}
