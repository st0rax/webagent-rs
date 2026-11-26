//! Single-turn relay (Python `relay_single_turn`) — send+wait, kein Controller/Shell.

use std::time::Instant;

#[cfg(test)]
use crate::brain::BrainResponse;
use crate::brain::{BackendFactory, ImageGenerationBackend, RelayBackend, SessionState};
use crate::inference_types::{BrowserAttachment, ClientPresence};
use crate::timeouts::resolve_timeout;
use crate::WebBackendFactory;

#[derive(Debug)]
pub struct RelayError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub mime_type: String,
    pub base64: String,
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Praefix einer Breaker-Sperre. Erzeuger (`circuit_open_message`) und Parser
/// (`open_circuit_remaining_secs`) teilen sich denselben Wortlaut, damit die
/// Uebersetzung in HTTP 503 nicht leise kaputtgeht, wenn der Text sich aendert.
const CIRCUIT_OPEN_PREFIX: &str = "circuit_open: ";

/// Einheitliche Meldung, wenn der Breaker ein Brain ueberspringt.
pub(crate) fn circuit_open_message(brain_id: &str, remaining_secs: i64) -> String {
    format!("{CIRCUIT_OPEN_PREFIX}{brain_id} uebersprungen, noch {remaining_secs}s Cooldown")
}

/// Restliche Cooldown-Sekunden, falls `error` von einem offenen Circuit-Breaker
/// stammt. `None` heisst: kein offener Breaker, die Meldung kuendigt keine
/// Wartezeit an.
pub(crate) fn open_circuit_remaining_secs(error: &str) -> Option<i64> {
    let rest = error.strip_prefix(CIRCUIT_OPEN_PREFIX)?;
    let marker = "noch ";
    let idx = rest.rfind(marker)?;
    let digits: String = rest[idx + marker.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse::<i64>().ok().filter(|seconds| *seconds > 0)
}

// T-1006: `RelayBackend` is now a public core trait in `crate::brain`. The
// turn loop stays generic over it, so the T-947 mocks keep working unchanged.

/// Begruendung eines fehlgeschlagenen Turns, dessen Modellausgabe ungueltig
/// war. Stabiler brain_score-Grund, damit Fehler der Modellausgabe nicht als
/// Erfolg oder als Transportfehler erscheinen (T-953).
pub const MODEL_OUTPUT_FAILURE_REASON: &str = "model_output_envelope_invalid";

/// Stabiler Grund fuer einen Turn, der abgebrochen wurde, weil der Aufrufer
/// die Verbindung getrennt hat (T-946). Kein Brain-Fehler, kein Erfolg.
pub const CLIENT_GONE_REASON: &str = "client_disconnected";

/// Pruefung der Modellausgabe, bevor der Turn als Erfolg verbucht wird.
///
/// `validate` bewertet den Rohtext. Schlaegt sie fehl, sendet die Turn-Schleife
/// `correction_message` hoechstens einmal im selben Chat (kein `new_chat`) und
/// wertet die Antwort erneut. Ohne diese Grenze zaehlte relay.rs einen Turn mit
/// kaputtem Werkzeugumschlag als Erfolg, weil die Antwort vor der Parse-Pruefung
/// verbucht wurde (T-953).
pub struct OutputCheck<'a> {
    pub validate: &'a dyn Fn(&str) -> Result<(), String>,
    pub correction_message: &'a dyn Fn(&str) -> String,
}

/// Fuehrt einen Browserturn aus und wartet auf ein echtes Bildartefakt statt
/// auf Text. Das ist bewusst ein eigener Pfad: Bildgeneratoren koennen ein Bild
/// fertigstellen, ohne jemals eine auswertbare Assistenten-Textnachricht zu
/// erzeugen.
pub fn relay_image_generation(
    brain_id: &str,
    prompt: &str,
    headless: bool,
    timeout_override: Option<f64>,
) -> Result<GeneratedImage, RelayError> {
    // T-970: Bild-Turns schreiben Turn-Beobachtungen, buchen aber keinen Score-
    // Event. Der Scope verhindert, dass diese Reste in einen spaeteren Turn
    // desselben Threads wandern.
    let _scope = crate::brain_score::PendingScope::new();
    let trace = std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some();
    if let Some(remaining) = crate::circuit_breaker::check(brain_id) {
        return Err(RelayError(circuit_open_message(brain_id, remaining)));
    }
    let started = Instant::now();
    // T-1009: api knows only the core trait; the browser backend is supplied by
    // the factory. This removes the last `relay -> browser` crate edge.
    let mut backend = WebBackendFactory::create_image_backend(brain_id).map_err(RelayError)?;
    relay_image_turn(
        backend.as_mut(),
        brain_id,
        prompt,
        headless,
        timeout_override,
        started,
        trace,
    )
}

/// Image turn against an abstract backend. Kept separate from construction so
/// the lifecycle can be tested without a browser (T-1009).
fn relay_image_turn(
    backend: &mut dyn ImageGenerationBackend,
    brain_id: &str,
    prompt: &str,
    headless: bool,
    timeout_override: Option<f64>,
    started: Instant,
    trace: bool,
) -> Result<GeneratedImage, RelayError> {
    let ready_timeout = resolve_timeout("ensure_ready", brain_id, "", timeout_override);
    let wait_timeout = resolve_timeout("wait_response", brain_id, prompt, timeout_override);
    backend.image_start(headless).map_err(RelayError)?;
    if trace {
        eprintln!(
            "[image] browser started after {:.1}s",
            started.elapsed().as_secs_f64()
        );
    }
    let state = backend
        .image_ensure_ready(ready_timeout)
        .unwrap_or(SessionState::Error);
    if state.is_hard_block() {
        let _ = backend.image_stop();
        return Err(RelayError(format!("session_state={state:?}")));
    }
    if state.is_soft_login_doubt() {
        // Weicher Zweifel darf kein hartes "geht nicht" sein: trotzdem versuchen.
        // Kommt ein Bild zurueck, war die Vorabpruefung ein False Positive.
        crate::bench_events::eprint_line(&format!(
            "[image] {brain_id}: Login-Vorabpruefung={state:?} (weich) — erzeuge trotzdem; das Ergebnis entscheidet"
        ));
    }
    if trace {
        eprintln!(
            "[image] ready after {:.1}s",
            started.elapsed().as_secs_f64()
        );
    }
    if let Err(error) = backend.image_new_chat() {
        let _ = backend.image_stop();
        return Err(RelayError(error));
    }
    if let Err(error) = backend.image_enable_generation_mode() {
        let _ = backend.image_stop();
        return Err(RelayError(error));
    }
    // Das Aktivieren des Bildtools rendert selbst Icons/Previews. Erst danach
    // markieren, damit diese UI-Bilder nie als Generator-Ergebnis gelten.
    if let Err(error) = backend.image_mark_baseline() {
        let _ = backend.image_stop();
        return Err(RelayError(error));
    }
    if let Err(error) = backend.image_send(prompt) {
        let _ = backend.image_stop();
        return Err(RelayError(error));
    }
    if trace {
        eprintln!(
            "[image] submitted after {:.1}s",
            started.elapsed().as_secs_f64()
        );
    }
    let total_budget = std::time::Duration::from_secs_f64(wait_timeout.max(1.0));
    let remaining = total_budget.saturating_sub(started.elapsed());
    let deadline = Instant::now() + remaining;
    let result = loop {
        backend.image_wake_renderer();
        match backend.image_latest() {
            Ok(Some((mime_type, base64))) => {
                if trace {
                    eprintln!(
                        "[image] artifact after {:.1}s",
                        started.elapsed().as_secs_f64()
                    );
                }
                break Ok(GeneratedImage { mime_type, base64 });
            }
            Ok(None) => {}
            Err(error) => break Err(RelayError(error)),
        }
        if let Some(error) = backend.image_provider_error() {
            break Err(RelayError(format!(
                "provider_unavailable: {brain_id}: {error}"
            )));
        }
        if Instant::now() >= deadline {
            let report = backend.image_report();
            if trace {
                eprintln!("[image] timeout report: {report}");
            }
            break Err(RelayError(format!(
                "kein generiertes Bild innerhalb des absoluten Budgets von {:.1}s gefunden; {report}",
                wait_timeout,
            )));
        }
        std::thread::sleep(crate::timeouts::relay_image_poll());
    };
    let _ = backend.image_stop();
    result
}

/// Eine Send+Wait-Runde gegen ein Brain; kein Controller, keine Shell-Actions.
///
/// `model` wechselt das Zielmodell in DERSELBEN Sitzung, in der auch gefragt
/// wird. Ein Wechsel in einer separaten Sitzung waere wirkungslos: jeder neue
/// Browserstart faellt aufs Standardmodell zurueck. Der Wechsel passiert
/// deshalb genau einmal, vor der Turn-Schleife — `new_chat` in der Schleife
/// startet nur eine neue Konversation, das Modell der Sitzung bleibt.
pub fn relay_single_turn(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
) -> Result<String, RelayError> {
    relay_single_turn_streaming(
        brain_id,
        message,
        headless,
        timeout_override,
        model,
        &mut |_| {},
    )
}

/// Single-Turn-Relay mit wachsenden Text-Snapshots fuer Transport-Streaming.
pub fn relay_single_turn_streaming(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
    on_update: &mut dyn FnMut(&str),
) -> Result<String, RelayError> {
    relay_single_turn_with_attachments_streaming(
        brain_id,
        message,
        headless,
        timeout_override,
        model,
        &[],
        on_update,
    )
}

/// Single-Turn-Relay mit Dateien, die vor dem Senden an den Browser-Composer
/// angehaengt werden. Der leere Slice ist der identische Textpfad aus
/// [`relay_single_turn_streaming`].
pub fn relay_single_turn_with_attachments_streaming(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
) -> Result<String, RelayError> {
    let always = crate::inference_types::AlwaysConnected;
    relay_single_turn_checked(
        brain_id,
        message,
        headless,
        timeout_override,
        model,
        attachments,
        on_update,
        None,
        &always,
    )
}

/// Wie [`relay_single_turn_with_attachments_streaming`], aber die Modellausgabe
/// wird vor der Erfolgsverbuchung geprueft und bei einem Fehler genau einmal
/// in derselben Konversation zur Korrektur aufgefordert (T-953).
#[allow(clippy::too_many_arguments)]
pub fn relay_single_turn_with_attachments_streaming_checked(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
    output_check: &OutputCheck<'_>,
    presence: &dyn ClientPresence,
) -> Result<String, RelayError> {
    relay_single_turn_checked(
        brain_id,
        message,
        headless,
        timeout_override,
        model,
        attachments,
        on_update,
        Some(output_check),
        presence,
    )
}

#[allow(clippy::too_many_arguments)]
fn relay_single_turn_checked(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
    output_check: Option<&OutputCheck<'_>>,
    presence: &dyn ClientPresence,
) -> Result<String, RelayError> {
    // T-970: ein frischer Beobachtungsbereich pro Relay-Lauf. Alle Events und
    // set_pending_*-Aufrufe dieses Laufs liegen im selben Thread.
    let _scope = crate::brain_score::PendingScope::new();
    // Ein Brain, das gerade wiederholt blockiert/rate-limitiert war, wird fuer eine
    // Cooldown-Zeit uebersprungen statt erneut in den vollen Timeout zu laufen.
    if let Some(remaining) = crate::circuit_breaker::check(brain_id) {
        return Err(RelayError(circuit_open_message(brain_id, remaining)));
    }
    let started = Instant::now();
    let prompt_chars = message.chars().count();
    let mut backend = WebBackendFactory::create_relay_backend(brain_id).map_err(RelayError)?;
    let (ready_timeout, wait_timeout) =
        resolve_attach_aware_timeouts(brain_id, message, timeout_override, !attachments.is_empty());

    backend.relay_start(headless).map_err(RelayError)?;
    // T-976: ein geparktes WebView antwortet per CDP nicht zuverlaessig.
    // Aufdecken vor dem Turn; der Backend-Drop parkt wieder.
    if let Err(e) = backend.relay_reveal_for_turn() {
        crate::bench_events::eprint_line(&format!(
            "[relay] {brain_id}: Turn-Reveal fehlgeschlagen ({e}); fahre ohne Aufdecken fort"
        ));
    }
    let state = backend
        .relay_ensure_ready(ready_timeout)
        .unwrap_or(SessionState::Error);
    if state.is_hard_block() {
        let _ = backend.relay_stop();
        let reason = format!("session_state={state:?}");
        crate::circuit_breaker::record_failure(brain_id, &reason);
        crate::brain_score::record_event(
            brain_id,
            false,
            Some(&reason),
            started.elapsed().as_millis() as u64,
            prompt_chars,
        );
        return Err(RelayError(reason));
    }
    // Weicher Login-Zweifel (Unbestimmt/LoginRequired) ist kein Beleg gegen
    // eine funktionierende Sitzung — Selektor-Drift oder eine veraltete
    // Login-Knopf-Regel koennen beides ausloesen. Nicht blockieren, sondern
    // den echten Prompt entscheiden lassen; ein positives Ergebnis markieren
    // wir unten als bestaetigten False Positive.
    let soft_login_doubt = if state.is_soft_login_doubt() {
        crate::bench_events::eprint_line(&format!(
            "[relay] {brain_id}: Login-Vorabpruefung={state:?} (weich) — prompt trotzdem; die echte Antwort entscheidet"
        ));
        Some(state)
    } else {
        None
    };
    if let Some(want) = model {
        match backend.relay_switch_model(want) {
            Ok(now) => {
                // „bereits aktiv" ist kein Wechsel: die Beschriftung trug das
                // Ziel schon vorher, also hat dieser Lauf den Antrieb nicht
                // belegt. Erst ein gemessener Wechsel ist ein Live-Beweis.
                if !now.contains("bereits aktiv") {
                    crate::capability_proof::record_route_proof(
                        brain_id,
                        "model_switch",
                        &format!("relay --model '{want}' (Beschriftung nachgeprueft)"),
                        started.elapsed().as_millis() as u64,
                    );
                }
            }
            Err(e) => {
                let reason = format!("model_switch({want}): {e}");
                crate::circuit_breaker::record_failure(brain_id, &reason);
                crate::brain_score::record_event(
                    brain_id,
                    false,
                    Some(&reason),
                    started.elapsed().as_millis() as u64,
                    prompt_chars,
                );
                return Err(RelayError(reason));
            }
        }
    }
    run_turn_loop(
        &mut *backend,
        brain_id,
        message,
        attachments,
        wait_timeout,
        started,
        prompt_chars,
        soft_login_doubt,
        on_update,
        output_check,
        presence,
    )
}

/// Kern der Turn-Schleife: bis zu drei volle Turns (new_chat + send +
/// wait_response), Zeitbudget ueber die ganze Anfrage, Sendungen gezaehlt.
///
/// Generic ueber [`RelayBackend`], damit die DoD (T-947) "ein stummer Anbieter
/// erhaelt genau eine Sendung" mit einem Mock-Backend testbar ist.
#[allow(clippy::too_many_arguments)]
fn run_turn_loop<B: RelayBackend + ?Sized>(
    backend: &mut B,
    brain_id: &str,
    message: &str,
    attachments: &[BrowserAttachment],
    wait_timeout: f64,
    started: Instant,
    prompt_chars: usize,
    soft_login_doubt: Option<SessionState>,
    on_update: &mut dyn FnMut(&str),
    output_check: Option<&OutputCheck<'_>>,
    presence: &dyn ClientPresence,
) -> Result<String, RelayError> {
    // T-970: verschachtelter Scope fuer den direkten Testaufruf dieses Loops;
    // ein aeusserer Relay-Scope wird beim Verlassen wiederhergestellt.
    let _scope = crate::brain_score::PendingScope::new();
    // Bis zu drei volle Turns (new_chat + send + wait_response). Web-UIs ohne API
    // sind unvermeidlich flakig: Submit oder Antworterkennung koennen scheitern.
    // Jeder Turn startet mit einem frischen `new_chat`, also entsteht kein
    // Doppel-Post im selben Thread.
    //
    // Rate-Limit wird NICHT wiederholt: das ist ein echtes "spaeter wieder", kein
    // transienter Fehler. Retries gehen sichtbar nach stderr.
    //
    // T-947: Das Warte-Budget gilt fuer die GANZE Anfrage, nicht je Turn — sonst
    // wartet der Client bis zu dreimal so lange wie zugesagt (856 s statt 270 s).
    // Und ein Turn, dessen Senden bereits erfolgreich war, wird bei leerer
    // Antwort (timeout_no_message) NICHT wiederholt: ob der Anbieter die
    // Nachricht verarbeitet hat, ist dann unbekannt, ein erneutes Senden
    // dupliziert den Prompt (massenhaftes identisches Senden = Kontorisiko).
    // Attachment-Turns: ein Versuch (Upload-Fehler sind deterministisch).
    const MAX_TURNS: usize = 3;
    let max_turns = if attachments.is_empty() { MAX_TURNS } else { 1 };
    let deadline = started + std::time::Duration::from_secs_f64(wait_timeout.max(1.0));
    let mut last_err = format!("kein Versuch ausgefuehrt fuer {brain_id}");
    let mut sends: u32 = 0;
    let mut answer: Option<String> = None;
    for turn in 0..max_turns {
        // T-950: der Quellenhinweis-Puffer ist prozessglobal. Mit jedem Turn
        // neu leeren, damit ein abgelehnter Versuch seine Hinweise nicht in
        // den naechsten, verbuchten Turn schleppt.
        crate::brain_score::set_pending_source_hints(Vec::new());
        // T-946: kein Browser-Turn fuer einen Aufrufer, der schon in der
        // Warteschlange vor der Brain-Sperre verschwunden ist. Der Turn wird
        // als verwaist markiert (brain_score) und die Sperre wird nicht
        // gehalten. Ein Client, der mitten im Turn verschwindet, wird unten
        // ueber `cancelled` im Wait-Poll erkannt.
        if !presence.connected() {
            let _ = backend.relay_stop();
            crate::brain_score::record_event_with_sends(
                brain_id,
                false,
                Some(CLIENT_GONE_REASON),
                started.elapsed().as_millis() as u64,
                prompt_chars,
                sends,
            );
            return Err(RelayError(format!(
                "{CLIENT_GONE_REASON}: Aufrufer von {brain_id} ist weg; Turn nicht gestartet"
            )));
        }
        if turn > 0 {
            crate::bench_events::eprint_line(&format!(
                "[relay] {brain_id}: Wiederholung {turn}/{}  (vorher: {last_err})",
                max_turns.saturating_sub(1)
            ));
            std::thread::sleep(crate::timeouts::relay_response_poll());
        }
        if let Err(e) = backend.relay_new_chat() {
            last_err = e;
            continue;
        }
        // T-950: die Websuche ist eine Eigenschaft der jeweiligen Konversation
        // und kann nach `new_chat` wieder aktiv sein. Deshalb genau hier,
        // unmittelbar vor dem Senden, messen und ausschalten. Ohne brauchbaren
        // Umschalter bleibt das Brain im Event als nicht isolierbar markiert.
        if let Some(web_search) = backend.relay_web_search_state() {
            if !web_search.is_off() {
                crate::bench_events::eprint_line(&format!(
                    "[relay] {brain_id}: Websuche nicht isolierbar ({})",
                    web_search.status
                ));
            }
            crate::brain_score::set_pending_web_search(web_search);
        }
        // Gesamtbudget erschoepft (nur moeglich, wenn ein frueherer Turn bereits
        // Wartezeit verbraucht hat): kein weiterer Versuch.
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            last_err = format!(
                "keine Antwort innerhalb des Gesamtbudgets ({wait_timeout:.0}s, {sends} Sendungen)"
            );
            break;
        }
        let turn_wait = remaining.as_secs_f64().min(wait_timeout);
        let baseline = match backend.relay_send_with_attachments(message, attachments) {
            Ok(b) => {
                sends += 1;
                b
            }
            Err(e) => {
                last_err = e;
                if is_deterministic_send_failure(&last_err) {
                    crate::bench_events::eprint_line(&format!(
                        "[relay] {brain_id}: kein Retry fuer deterministischen Sendefehler"
                    ));
                    break;
                }
                continue;
            }
        };
        let cancelled = || !presence.connected();
        let response = match backend
            .relay_wait_response_streaming(baseline, turn_wait, on_update, &cancelled)
        {
            Ok(r) => r,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        if response.backend_status == CLIENT_GONE_REASON {
            let _ = backend.relay_stop();
            crate::brain_score::record_event_with_sends(
                brain_id,
                false,
                Some(CLIENT_GONE_REASON),
                started.elapsed().as_millis() as u64,
                prompt_chars,
                sends,
            );
            return Err(RelayError(format!(
                "{CLIENT_GONE_REASON}: Aufrufer von {brain_id} hat die Verbindung getrennt; Turn als verwaist beendet"
            )));
        }
        if response.backend_status == "rate_limit" {
            let _ = backend.relay_stop();
            crate::circuit_breaker::record_failure(brain_id, "rate_limit");
            crate::brain_score::record_event_with_sends(
                brain_id,
                false,
                Some("rate_limit"),
                started.elapsed().as_millis() as u64,
                prompt_chars,
                sends,
            );
            return Err(RelayError(
                "rate_limited: Brain ist aktuell limitiert/nicht verfuegbar".into(),
            ));
        }
        // Externe Blockierung (Rate-/Nachrichtenlimit, Login, Cloudflare) auf der
        // Seite erkannt. Terminal — ein Retry hilft nicht. Distinkt mit "blocked:"-
        // Praefix, damit Messungen es flaggen statt als Tool-Defekt zu werten.
        if response.backend_status == "blocked" {
            let _ = backend.relay_stop();
            crate::circuit_breaker::record_failure(brain_id, "blocked");
            crate::brain_score::record_event_with_sends(
                brain_id,
                false,
                Some("blocked"),
                started.elapsed().as_millis() as u64,
                prompt_chars,
                sends,
            );
            return Err(RelayError(format!(
                "blocked: {brain_id}: {}",
                response.text.trim()
            )));
        }
        // Leerer Text = Timeout ohne erkannte Antwort. wait_response gibt das als
        // Ok mit leerem Text zurueck; ohne diese Pruefung zaehlte ein Timeout als
        // Erfolg. UI-Chrome / CoT-Echo zaehlen ebenfalls als leer.
        let raw = response.text.trim().to_string();
        let text = crate::observer::chat_answer_text(&raw);
        if text.is_empty() {
            let total = started.elapsed().as_secs_f64();
            last_err = format!(
                "keine Antwort erhalten (timeout_budget={wait_timeout:.0}s, gesamt={total:.0}s, sendungen={sends}, backend_status={}, generation_complete={}, raw_chars={})",
                response.backend_status,
                response.generation_complete,
                raw.chars().count()
            );
            // Senden war erfolgreich: ob der Anbieter die Nachricht verarbeitet
            // hat, ist unbekannt. Nicht erneut senden (T-947).
            break;
        }
        // T-950: ein Quellenhinweis der Oberflaeche belegt einen Provider-Abruf,
        // der im Client-Transkript unsichtbar bleibt. Er wird protokolliert,
        // nicht aus dem Antworttext entfernt (keine inhaltliche Filterung).
        let source_hints = crate::observer::source_hints(&text);
        if !source_hints.is_empty() {
            crate::bench_events::eprint_line(&format!(
                "[relay] {brain_id}: {} Quellenhinweis(e) in der Antwort erkannt",
                source_hints.len()
            ));
            crate::brain_score::set_pending_source_hints(source_hints);
        }
        if is_provider_error_page(&text) {
            last_err = format!(
                "Anbieter-Fehlerseite statt Antwort: {}",
                text.chars().take(160).collect::<String>()
            );
            continue;
        }
        // T-953: die Modellausgabe pruefen, BEVOR der Turn verbucht wird. Vorher
        // zaehlte ein kaputter Werkzeugumschlag als Erfolg, weil parse_response
        // erst NACH der Erfolgsverbuchung in browser_inference.rs lief. Bei einem
        // Fehler geht genau EINE Korrektur in derselben Konversation raus (kein
        // new_chat), mit der Parserfehlermeldung als Inhalt. Scheitert sie, ist
        // der Turn ein Fehlschlag mit eigenem Grund — kein Retry, kein Erfolg.
        //
        // Der Circuit-Breaker wird bewusst NICHT geoeffnet: das Brain ist
        // erreichbar und antwortet, nur die Ausgabe war ungueltig. Der eigene
        // brain_score-Grund macht den Unterschied zur Nichtverfuegbarkeit sichtbar.
        let mut final_text = text;
        if let Some(check) = output_check {
            let fail = |detail: String, current_sends: u32| -> RelayError {
                crate::brain_score::record_event_with_sends(
                    brain_id,
                    false,
                    Some(MODEL_OUTPUT_FAILURE_REASON),
                    started.elapsed().as_millis() as u64,
                    prompt_chars,
                    current_sends,
                );
                RelayError(format!("{MODEL_OUTPUT_FAILURE_REASON}: {detail}"))
            };
            if let Err(parse_error) = (check.validate)(&final_text) {
                crate::bench_events::eprint_line(&format!(
                    "[relay] {brain_id}: ungueltige Modellausgabe, Korrektur in derselben Konversation: {parse_error}"
                ));
                let correction = (check.correction_message)(&parse_error);
                let correction_baseline =
                    match backend.relay_send_with_attachments(&correction, &[]) {
                        Ok(baseline) => {
                            sends += 1;
                            baseline
                        }
                        Err(error) => {
                            return Err(fail(
                                format!("{parse_error}; Korrektur nicht gesendet: {error}"),
                                sends,
                            ));
                        }
                    };
                let correction_wait = deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64();
                if correction_wait <= 0.0 {
                    return Err(fail(
                        format!("{parse_error}; kein Zeitbudget fuer Korrektur"),
                        sends,
                    ));
                }
                let correction_response = match backend.relay_wait_response_streaming(
                    correction_baseline,
                    correction_wait,
                    on_update,
                    &cancelled,
                ) {
                    Ok(response) => response,
                    Err(error) => {
                        return Err(fail(
                            format!("{parse_error}; Korrektur-Antwort fehlgeschlagen: {error}"),
                            sends,
                        ));
                    }
                };
                let correction_raw = correction_response.text.trim().to_string();
                let correction_text = crate::observer::chat_answer_text(&correction_raw);
                if correction_text.is_empty() {
                    return Err(fail(
                        format!("{parse_error}; Korrektur lieferte keine Antwort"),
                        sends,
                    ));
                }
                match (check.validate)(&correction_text) {
                    Ok(()) => final_text = correction_text,
                    Err(second_error) => {
                        return Err(fail(
                            format!("{parse_error}; auch nach Korrektur ungueltig: {second_error}"),
                            sends,
                        ));
                    }
                }
            }
        }
        answer = Some(final_text);
        break;
    }
    // Shared-Pool: `stop` respektiert `persist_browser_tabs()` (Tab bleibt offen).
    let _ = backend.relay_stop();
    let latency_ms = started.elapsed().as_millis() as u64;
    match answer {
        Some(text) => {
            if let Some(state) = soft_login_doubt {
                // Die echte Antwort ist der Beleg: die Vorabpruefung war falsch.
                // Sichtbar protokollieren, damit ein solcher Fall nicht erneut
                // als "Login noetig" fehlinterpretiert wird.
                crate::bench_events::eprint_line(&format!(
                    "[relay] {brain_id}: Login-False-Positive bestaetigt (Vorabpruefung={state:?}, echte Antwort {}-Zeichen)",
                    text.chars().count()
                ));
            }
            crate::circuit_breaker::record_success(brain_id);
            crate::brain_score::record_event_with_sends(
                brain_id,
                true,
                None,
                latency_ms,
                prompt_chars,
                sends,
            );
            // Send+Wait gegen den echten Browser = das Brain kann Text
            // senden und eine Antwort lesen. Das ist der Beleg fuer "chat".
            crate::capability_proof::record_route_proof(
                brain_id,
                "chat",
                "relay_single_turn ok (send+wait gegen echten Browser)",
                latency_ms,
            );
            Ok(text)
        }
        None => {
            // Fehlender Datei-Upload ist eine bekannte Faehigkeitsgrenze fuer
            // multimodale Requests, kein Ausfall des Text-Brains. Er darf
            // deshalb weder den Circuit Breaker oeffnen noch den allgemeinen
            // Brain-Score verschlechtern: Ein spaeterer text-only Turn kann
            // mit demselben Brain problemlos funktionieren.
            if !is_attachment_capability_failure(&last_err) {
                crate::circuit_breaker::record_failure(brain_id, &last_err);
                crate::brain_score::record_event_with_sends(
                    brain_id,
                    false,
                    Some(&last_err),
                    latency_ms,
                    prompt_chars,
                    sends,
                );
            }
            Err(RelayError(last_err))
        }
    }
}

/// Timeout-Aufloesung fuer Relay-Turns.
///
/// Ohne Attachments bleibt `--timeout-secs` / `timeout_override` ein Minimum
/// (bisheriges Verhalten). Mit Attachments und gesetztem Override ist der Wert
/// ein **absolutes** Budget (AutoRouter-Attach fail-closed vor Client-240s).
fn resolve_attach_aware_timeouts(
    brain_id: &str,
    message: &str,
    timeout_override: Option<f64>,
    has_attachments: bool,
) -> (f64, f64) {
    if has_attachments {
        if let Some(cap) = timeout_override.filter(|v| *v > 0.0) {
            let ready = resolve_timeout("ensure_ready", brain_id, "", None).min(cap);
            let wait = resolve_timeout("wait_response", brain_id, message, None).min(cap);
            return (ready.max(1.0), wait.max(1.0));
        }
    }
    (
        resolve_timeout("ensure_ready", brain_id, "", timeout_override),
        resolve_timeout("wait_response", brain_id, message, timeout_override),
    )
}

/// Manche Sendefehler beschreiben einen stabilen UI-Zustand, bei dem ein
/// weiterer kompletter Browserturn nur Zeit verbraucht: sichtbare Blockade,
/// deaktivierter Sendeknopf oder fehlender Absende-Beweis. Transiente CDP- und
/// Navigationsfehler bleiben dagegen retry-faehig.
/// Fehlerseite oder UI-Glitch des Anbieters statt einer Modellantwort (T-960).
///
/// Die Erkennung existiert in `brain::is_retryable_empty_response`, war aber nur
/// im Agenten-Harness verdrahtet; am 2026-09-14 lieferte die API-Bridge
/// ChatGPTs „Something went wrong. If this issue persists …" mit HTTP 200 aus.
/// Nur kurze Texte ohne Werkzeug-Umschlag: Anbieter-Banner sind kurz, und eine
/// echte Antwort oder ein Werkzeugargument darf Phrasen wie „usage limit"
/// enthalten, ohne verworfen zu werden.
fn is_provider_error_page(text: &str) -> bool {
    const MAX_BANNER_CHARS: usize = 400;
    !text.contains("WEBAGENT_INFERENCE/1")
        && text.chars().count() <= MAX_BANNER_CHARS
        && crate::brain::is_retryable_empty_response(text)
}

fn is_deterministic_send_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    is_attachment_capability_failure(error)
        || [
            "kein absende-beweis",
            "absendeknopf ist deaktiviert",
            "blockiert:",
            "usage limit",
            "nachrichtenlimit",
            "rate limit",
            "cloudflare",
            "login required",
            "composer-feld nicht gefunden",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Der Brain-Score und der allgemeine Breaker beschreiben Textverfügbarkeit.
/// Ein Brain ohne Datei-Input ist für eine Anfrage mit Bild/Audio nicht
/// fehlerhaft; die Capability-Grenze muss als lokaler, nicht wiederholbarer
/// Requestfehler zurückkommen.
fn is_attachment_capability_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "no_file_input",
        "no_file_input_and_paste_not_confirmed",
        "keinen nutzbaren datei-upload",
        "kein nutzbarer datei-upload",
        "dateien uebernommen",
        "dateien übernommen",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_page_is_not_an_answer() {
        // Real 2026-09-14 18:54, chatgpt ueber webagent/auto.
        assert!(is_provider_error_page(
            "Something went wrong. If this issue persists please contact us through our help center at help.openai.com. Erneut versuchen"
        ));
        assert!(!is_provider_error_page("Hallo, wie kann ich helfen?"));
    }

    #[test]
    fn circuit_open_meldung_und_parser_passen_zusammen() {
        // DoD T-945: die restliche Cooldown-Zeit muss aus der Meldung
        // zurueckgelesen werden koennen -- sonst kann die Bridge kein
        // `Retry-After` setzen.
        let msg = circuit_open_message("qwen", 812);
        assert_eq!(msg, "circuit_open: qwen uebersprungen, noch 812s Cooldown");
        assert_eq!(open_circuit_remaining_secs(&msg), Some(812));
    }

    #[test]
    fn parser_ignoriert_fremde_und_kaputte_meldungen() {
        assert_eq!(open_circuit_remaining_secs("timeout_no_text"), None);
        assert_eq!(open_circuit_remaining_secs(""), None);
        // Praefix fehlt: keine Wartezeit, auch wenn "noch Ns Cooldown" vorkommt.
        assert_eq!(
            open_circuit_remaining_secs("qwen uebersprungen, noch 5s Cooldown"),
            None
        );
        // Praefix da, aber keine Zahl.
        assert_eq!(
            open_circuit_remaining_secs("circuit_open: qwen uebersprungen, noch s Cooldown"),
            None
        );
        // 0 Sekunden ist kein sinnvolles Retry-After.
        assert_eq!(
            open_circuit_remaining_secs("circuit_open: qwen uebersprungen, noch 0s Cooldown"),
            None
        );
    }

    /// Bild-Backend ohne Browser: liefert sofort ein Artefakt und protokolliert
    /// die Reihenfolge der Lebenszyklus-Aufrufe (T-1009).
    struct BildMock {
        calls: Vec<&'static str>,
    }

    impl ImageGenerationBackend for BildMock {
        fn image_start(&mut self, _headless: bool) -> Result<(), String> {
            self.calls.push("start");
            Ok(())
        }
        fn image_ensure_ready(&mut self, _timeout: f64) -> Result<SessionState, String> {
            self.calls.push("ensure_ready");
            Ok(SessionState::Ready)
        }
        fn image_new_chat(&mut self) -> Result<(), String> {
            self.calls.push("new_chat");
            Ok(())
        }
        fn image_enable_generation_mode(&mut self) -> Result<(), String> {
            self.calls.push("enable_generation_mode");
            Ok(())
        }
        fn image_mark_baseline(&mut self) -> Result<(), String> {
            self.calls.push("mark_baseline");
            Ok(())
        }
        fn image_send(&mut self, _prompt: &str) -> Result<(), String> {
            self.calls.push("send");
            Ok(())
        }
        fn image_wake_renderer(&mut self) {
            self.calls.push("wake_renderer");
        }
        fn image_latest(&self) -> Result<Option<(String, String)>, String> {
            Ok(Some(("image/png".into(), "YWJj".into())))
        }
        fn image_provider_error(&self) -> Option<String> {
            None
        }
        fn image_report(&self) -> String {
            "mock".into()
        }
        fn image_stop(&mut self) -> Result<(), String> {
            self.calls.push("stop");
            Ok(())
        }
    }

    #[test]
    fn image_turn_uses_the_core_trait_lifecycle() {
        let mut backend = BildMock { calls: Vec::new() };
        let image = relay_image_turn(
            &mut backend,
            "mock-image",
            "ein Bild",
            false,
            Some(1.0),
            Instant::now(),
            false,
        )
        .expect("mock image");
        assert_eq!(image.mime_type, "image/png");
        assert_eq!(image.base64, "YWJj");
        assert_eq!(
            backend.calls,
            vec![
                "start",
                "ensure_ready",
                "new_chat",
                "enable_generation_mode",
                "mark_baseline",
                "send",
                "wake_renderer",
                "stop",
            ]
        );
    }

    /// Mock-Backend, das jedes Senden zaehlt und nie eine Antwort liefert.
    struct StummerAnbieter {
        sends: u32,
    }

    impl RelayBackend for StummerAnbieter {
        fn relay_new_chat(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn relay_send_with_attachments(
            &mut self,
            _text: &str,
            _attachments: &[BrowserAttachment],
        ) -> Result<i32, String> {
            self.sends += 1;
            Ok(1)
        }
        fn relay_wait_response_streaming(
            &mut self,
            _baseline: i32,
            _timeout: f64,
            _on_update: &mut dyn FnMut(&str),
            _cancelled: &dyn Fn() -> bool,
        ) -> Result<BrainResponse, String> {
            Ok(BrainResponse {
                text: String::new(),
                backend_status: "timeout_no_message".to_string(),
                ..Default::default()
            })
        }
        fn relay_stop(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    /// DoD T-947: ein Anbieter, dessen Senden erfolgreich war und der dann
    /// stumm bleibt (timeout_no_message), darf den Prompt nicht erneut
    /// erhalten. Vorher lief die Schleife bis zu dreimal mit je einer neuen
    /// Konversation.
    #[test]
    fn stummer_anbieter_erhaelt_genau_eine_sendung() {
        let mut backend = StummerAnbieter { sends: 0 };
        let result = run_turn_loop(
            &mut backend,
            "mock",
            "hallo",
            &[],
            1.0,
            Instant::now(),
            5,
            None,
            &mut |_| {},
            None,
            &crate::inference_types::AlwaysConnected,
        );
        assert!(
            result.is_err(),
            "stummer Anbieter darf keinen Erfolg melden"
        );
        assert_eq!(
            backend.sends, 1,
            "der Prompt darf nur einmal gesendet werden"
        );
        let msg = result.unwrap_err().0;
        assert!(
            msg.contains("sendungen=1"),
            "Fehlermeldung nennt Sendungen: {msg}"
        );
    }

    /// Gegenprobe: scheitert das Senden selbst, wird weiter versucht — hier
    /// erreicht genau KEINE Sendung den Anbieter, also darf der Retry laufen
    /// (3 Versuche), ohne dass sich die Sperre als Erfolg tarnt.
    #[test]
    fn sendefehler_ohne_absenden_wird_weiter_versucht() {
        struct Sendefehler {
            attempts: u32,
        }
        impl RelayBackend for Sendefehler {
            fn relay_new_chat(&mut self) -> Result<(), String> {
                Ok(())
            }
            fn relay_send_with_attachments(
                &mut self,
                _text: &str,
                _attachments: &[BrowserAttachment],
            ) -> Result<i32, String> {
                self.attempts += 1;
                Err("transient: CDP-Ausfall".to_string())
            }
            fn relay_wait_response_streaming(
                &mut self,
                _baseline: i32,
                _timeout: f64,
                _on_update: &mut dyn FnMut(&str),
                _cancelled: &dyn Fn() -> bool,
            ) -> Result<BrainResponse, String> {
                unreachable!("ohne erfolgreichen Send darf nicht gewartet werden")
            }
            fn relay_stop(&mut self) -> Result<(), String> {
                Ok(())
            }
        }
        let mut backend = Sendefehler { attempts: 0 };
        let result = run_turn_loop(
            &mut backend,
            "mock",
            "hallo",
            &[],
            1.0,
            Instant::now(),
            5,
            None,
            &mut |_| {},
            None,
            &crate::inference_types::AlwaysConnected,
        );
        assert!(result.is_err());
        assert!(backend.attempts > 0, "Retry-Send muss versucht werden");
    }

    #[test]
    fn tool_envelope_and_long_answers_with_banner_phrases_stay_answers() {
        let envelope = "WEBAGENT_INFERENCE/1\n{\"tool_calls\":[{\"id\":\"call_1\",\"name\":\"bash\",\"arguments\":{\"command\":\"grep -rn 'something went wrong' src\"}}]}";
        assert!(!is_provider_error_page(envelope));
        let long = format!(
            "{} Ihr usage limit laesst sich im Konto nachsehen.",
            "Ausfuehrliche Erklaerung. ".repeat(30)
        );
        assert!(!is_provider_error_page(&long));
    }

    #[test]
    fn relay_error_on_bad_brain_id() {
        let err = relay_single_turn("nonexistent_brain_xyz", "hi", true, None, None);
        assert!(err.is_err());
    }

    #[test]
    fn deterministic_send_failures_are_not_retried() {
        assert!(is_deterministic_send_failure(
            "Absenden fehlgeschlagen: kein Absende-Beweis nach 5 Versuchen"
        ));
        assert!(is_deterministic_send_failure(
            "blockiert: kein Absende-Beweis -- Seite zeigt: Login required"
        ));
        assert!(is_deterministic_send_failure(
            "Absendeknopf ist deaktiviert, obwohl der Text vollstaendig im Composer steht"
        ));
        assert!(!is_deterministic_send_failure("CDP connection reset"));
    }

    #[test]
    fn missing_file_input_is_a_capability_error_not_a_brain_failure() {
        let error =
            "Browseroberflaeche stellt keinen nutzbaren Datei-Upload bereit (no_file_input)";
        assert!(is_attachment_capability_failure(error));
        assert!(is_deterministic_send_failure(error));
        assert!(is_attachment_capability_failure(
            "Browseroberflaeche stellt keinen nutzbaren Datei-Upload \
             (no_file_input_and_paste_not_confirmed)"
        ));
        assert!(!is_attachment_capability_failure("CDP connection reset"));
    }

    #[test]
    fn attach_override_is_absolute_budget_not_minimum() {
        let (ready, wait) = resolve_attach_aware_timeouts("chatgpt", "hi", Some(40.0), true);
        assert!(wait <= 40.0, "attach override must cap wait, got {wait}");
        assert!(ready <= 40.0, "attach override must cap ready, got {ready}");
        let (_r, wait_text) = resolve_attach_aware_timeouts("chatgpt", "hi", Some(40.0), false);
        // text path keeps minimum semantics: computed.max(40)
        assert!(
            wait_text >= 40.0,
            "text override remains minimum, got {wait_text}"
        );
    }

    #[test]
    fn partial_upload_counts_as_attachment_capability_failure() {
        assert!(is_attachment_capability_failure(
            "Browseroberflaeche hat nur 0 von 1 Dateien uebernommen"
        ));
    }

    /// Mock-Backend mit Antwortfolge: protokolliert Senden und `new_chat`, damit
    /// die Grenze "hoechstens eine Korrektur in derselben Konversation" pruefbar
    /// ist (T-953).
    struct Antwortfolge {
        antworten: Vec<String>,
        index: usize,
        sends: u32,
        new_chats: u32,
        gesendet: Vec<String>,
    }

    impl Antwortfolge {
        fn new(antworten: &[&str]) -> Self {
            Self {
                antworten: antworten.iter().map(|s| s.to_string()).collect(),
                index: 0,
                sends: 0,
                new_chats: 0,
                gesendet: Vec::new(),
            }
        }
    }

    impl RelayBackend for Antwortfolge {
        fn relay_new_chat(&mut self) -> Result<(), String> {
            self.new_chats += 1;
            Ok(())
        }
        fn relay_send_with_attachments(
            &mut self,
            text: &str,
            _attachments: &[BrowserAttachment],
        ) -> Result<i32, String> {
            self.sends += 1;
            self.gesendet.push(text.to_string());
            Ok(1)
        }
        fn relay_wait_response_streaming(
            &mut self,
            _baseline: i32,
            _timeout: f64,
            _on_update: &mut dyn FnMut(&str),
            _cancelled: &dyn Fn() -> bool,
        ) -> Result<BrainResponse, String> {
            let fallback = self.antworten.last().cloned().unwrap_or_default();
            let text = self.antworten.get(self.index).cloned().unwrap_or(fallback);
            self.index += 1;
            Ok(BrainResponse {
                text,
                backend_status: "ok".to_string(),
                ..Default::default()
            })
        }
        fn relay_stop(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    fn letztes_score_ereignis(brain_id: &str) -> serde_json::Value {
        let path = crate::config::data_dir()
            .join("brain_score")
            .join("events.jsonl");
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        content
            .lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|event| event.get("brain_id").and_then(|v| v.as_str()) == Some(brain_id))
            .unwrap_or_else(|| panic!("kein brain_score-Ereignis fuer {brain_id}"))
    }

    fn validate_mock(text: &str) -> Result<(), String> {
        if text.contains("WEBAGENT_INFERENCE/1") {
            Ok(())
        } else {
            Err("MODEL_OUTPUT_ERROR: Ungueltiger Browser-Tool-Call-Umschlag".to_string())
        }
    }

    fn correction_mock(parse_error: &str) -> String {
        format!("Korrigiere deine Ausgabe: {parse_error}")
    }

    fn ungueltiger_umschlag() -> OutputCheck<'static> {
        static VALIDATE: fn(&str) -> Result<(), String> = validate_mock;
        static CORRECTION: fn(&str) -> String = correction_mock;
        OutputCheck {
            validate: &VALIDATE,
            correction_message: &CORRECTION,
        }
    }

    /// DoD T-953 (1+2): ein ungueltiger Umschlag ist ein Fehlschlag mit dem
    /// eigenen Grund `model_output_envelope_invalid` und keiner Korrektur als
    /// Erfolg getarnt.
    #[test]
    fn ungueltiger_umschlag_ist_fehlschlag_mit_eigenem_grund() {
        let brain_id = "mock_modellausgabe_fehler";
        let mut backend = Antwortfolge::new(&["kaputter umschlag", "immer noch kaputt"]);
        let check = ungueltiger_umschlag();
        let result = run_turn_loop(
            &mut backend,
            brain_id,
            "hallo",
            &[],
            5.0,
            Instant::now(),
            5,
            None,
            &mut |_| {},
            Some(&check),
            &crate::inference_types::AlwaysConnected,
        );
        assert!(
            result.is_err(),
            "ungueltiger Umschlag darf kein Erfolg sein"
        );
        let msg = result.unwrap_err().0;
        assert!(msg.contains("MODEL_OUTPUT_ERROR"), "{msg}");
        assert_eq!(
            backend.new_chats, 1,
            "Korrektur im selben Chat, kein neues new_chat"
        );
        assert_eq!(backend.sends, 2, "ein Original + genau eine Korrektur");
        assert!(
            backend.gesendet[1].contains("Korrigiere deine Ausgabe"),
            "Korrektur enthaelt die Parserfehlermeldung: {:?}",
            backend.gesendet
        );
        let event = letztes_score_ereignis(brain_id);
        assert_eq!(
            event.get("success").and_then(|v| v.as_bool()),
            Some(false),
            "brain_score muss Fehlschlag verbuchen: {event}"
        );
        assert_eq!(
            event.get("reason").and_then(|v| v.as_str()),
            Some(MODEL_OUTPUT_FAILURE_REASON),
            "eigener Grund fehlt: {event}"
        );
    }

    /// DoD T-953 (3): hoechstens eine Korrektur, kein Retry mit neuem Chat —
    /// auch wenn der Anbieter beliebig oft antwortet.
    #[test]
    fn korrektur_nur_einmal_in_derselben_konversation() {
        let mut backend = Antwortfolge::new(&["kaputt", "kaputt", "kaputt", "kaputt"]);
        let check = ungueltiger_umschlag();
        let result = run_turn_loop(
            &mut backend,
            "mock_nur_einmal",
            "hallo",
            &[],
            5.0,
            Instant::now(),
            5,
            None,
            &mut |_| {},
            Some(&check),
            &crate::inference_types::AlwaysConnected,
        );
        assert!(result.is_err());
        assert_eq!(backend.new_chats, 1);
        assert_eq!(backend.sends, 2, "hoechstens eine Korrektur, kein Retry");
    }

    /// Gegenprobe: die Korrektur fuehrt zum gueltigen Umschlag, dann erst wird
    /// Erfolg verbucht — und genau einmal.
    #[test]
    fn erfolgreiche_korrektur_wird_als_erfolg_verbucht() {
        let brain_id = "mock_korrektur_erfolg";
        let mut backend =
            Antwortfolge::new(&["kaputt", "WEBAGENT_INFERENCE/1\n{\"tool_calls\":[]}"]);
        let check = ungueltiger_umschlag();
        let result = run_turn_loop(
            &mut backend,
            brain_id,
            "hallo",
            &[],
            5.0,
            Instant::now(),
            5,
            None,
            &mut |_| {},
            Some(&check),
            &crate::inference_types::AlwaysConnected,
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(backend.new_chats, 1);
        assert_eq!(backend.sends, 2);
        let event = letztes_score_ereignis(brain_id);
        assert_eq!(event.get("success").and_then(|v| v.as_bool()), Some(true));
    }

    // ---- T-946: verwaisste Anfragen -------------------------------------

    /// Mock-Backend, das einen laufenden Turn nachbildet: `wait` pollt den
    /// `cancelled`-Closure und beendet den Turn mit `client_disconnected`,
    /// sobald der Aufrufer verschwunden ist. Zaehlt new_chat/send/stop, damit
    /// der Beleg "kein Turn gestartet" und "relay_stop gerufen" pruefbar ist.
    struct AbbruchAnbieter {
        new_chats: u32,
        sends: u32,
        stops: u32,
    }

    impl AbbruchAnbieter {
        fn new() -> Self {
            Self {
                new_chats: 0,
                sends: 0,
                stops: 0,
            }
        }
    }

    impl RelayBackend for AbbruchAnbieter {
        fn relay_new_chat(&mut self) -> Result<(), String> {
            self.new_chats += 1;
            Ok(())
        }
        fn relay_send_with_attachments(
            &mut self,
            _text: &str,
            _attachments: &[BrowserAttachment],
        ) -> Result<i32, String> {
            self.sends += 1;
            Ok(1)
        }
        fn relay_wait_response_streaming(
            &mut self,
            _baseline: i32,
            _timeout: f64,
            _on_update: &mut dyn FnMut(&str),
            cancelled: &dyn Fn() -> bool,
        ) -> Result<BrainResponse, String> {
            // Wie WebBrainBackend::wait_response_streaming_with_cancel: der
            // Wait-Poll beobachtet den Aufrufer und bricht terminal ab.
            for _ in 0..400 {
                if cancelled() {
                    return Ok(BrainResponse {
                        text: String::new(),
                        backend_status: CLIENT_GONE_REASON.to_string(),
                        ..Default::default()
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Ok(BrainResponse {
                text: "zu spaet".to_string(),
                backend_status: "ok".to_string(),
                ..Default::default()
            })
        }
        fn relay_stop(&mut self) -> Result<(), String> {
            self.stops += 1;
            Ok(())
        }
    }

    /// DoD T-946: ein Aufrufer, der schon verschwunden ist, bevor der Turn
    /// beginnt, startet keinen Browser-Turn. Es gibt kein new_chat, kein send,
    /// aber ein relay_stop und ein brain_score-Event mit dem stabilen Grund.
    #[test]
    fn verwaiste_anfrage_startet_keinen_turn() {
        let brain_id = "mock_verwaist_start";
        let mut backend = AbbruchAnbieter::new();
        let presence = crate::api_bridge::TestPresence::connected(false);
        let result = run_turn_loop(
            &mut backend,
            brain_id,
            "hallo",
            &[],
            5.0,
            Instant::now(),
            5,
            None,
            &mut |_| {},
            None,
            &presence,
        );
        assert!(result.is_err(), "{result:?}");
        assert!(
            result.unwrap_err().0.contains(CLIENT_GONE_REASON),
            "stabiler Grund fehlt"
        );
        assert_eq!(backend.new_chats, 0, "kein Browser-Turn starten");
        assert_eq!(backend.sends, 0, "kein Prompt senden");
        assert_eq!(backend.stops, 1, "relay_stop muss gerufen werden");
        let event = letztes_score_ereignis(brain_id);
        assert_eq!(
            event.get("success").and_then(|v| v.as_bool()),
            Some(false),
            "verwaiste Anfrage ist kein Erfolg: {event}"
        );
        assert_eq!(
            event.get("reason").and_then(|v| v.as_str()),
            Some(CLIENT_GONE_REASON),
            "Grund fehlt: {event}"
        );
    }

    /// DoD T-946: verschwindet der Aufrufer waehrend des Turns, beendet der
    /// `cancelled`-Closure den Turn terminal. Der Follow-up muss nicht auf das
    /// volle Wartebudget warten — hier bricht der Turn nach wenigen ms ab,
    /// nicht nach den konfigurierten 5 s. relay_stop und brain_score folgen.
    #[test]
    fn abbruch_im_warteloop_beendet_turn_frueh() {
        let brain_id = "mock_verwaist_mitten";
        let mut backend = AbbruchAnbieter::new();
        let presence = crate::api_bridge::TestPresence::connected(true);
        let flip = presence.clone();
        let flipper = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            flip.set(false);
        });
        let started = Instant::now();
        let result = run_turn_loop(
            &mut backend,
            brain_id,
            "hallo",
            &[],
            5.0,
            started,
            5,
            None,
            &mut |_| {},
            None,
            &presence,
        );
        flipper.join().expect("flipper");
        assert!(result.is_err(), "{result:?}");
        assert!(
            result.unwrap_err().0.contains(CLIENT_GONE_REASON),
            "stabiler Grund fehlt"
        );
        assert_eq!(backend.new_chats, 1, "der Turn wurde gestartet");
        assert_eq!(backend.sends, 1, "genau ein Prompt gesendet");
        assert_eq!(backend.stops, 1, "relay_stop muss gerufen werden");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "Turn muss frueh enden, nicht das volle Budget verbrauchen"
        );
        let event = letztes_score_ereignis(brain_id);
        assert_eq!(
            event.get("reason").and_then(|v| v.as_str()),
            Some(CLIENT_GONE_REASON),
            "Grund fehlt: {event}"
        );
    }
}
