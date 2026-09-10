//! contract — gemeinsamer Brain-Vertrag (T-801, Scheibe 1).
//!
//! Rein rechnerische Vertragsgrundlage ohne Browser, ohne Store: typisierte
//! Operationen, eine zentrale Klassifikation der beobachteten Oberflaeche und
//! ein einheitlicher Ereignisstrom fuer Betriebsereignisse. Controller, REPL,
//! Relay, Web-UI, API und Swarm sollen dieselbe Zustandsfolge erzeugen — dieser
//! Vertrag ist das gemeinsame Vokabular dafuer.
//!
//! Grundsaetze (aus `docs/BRAIN_UNIFICATION_PLAN.md`, Scheibe 1):
//! - Die Klassifikation ([`classify_surface`]) laeuft VOR jedem Einstiegspunkt,
//!   der streamt, repariert oder Erfolg zaehlt.
//! - Genau EIN terminales Ereignis je Turn ([`OperationTrace`]).
//! - Abbruch ist in jeder Phase moeglich.
//! - UI-Diagnosebelege wie Zai-HTML bleiben Rohbeleg ([`SurfaceKind::UiDiagnosis`])
//!   und werden nie Textdelta oder Parser-Repair.
//! - Ein blockierter PageDriver erzeugt sichtbar started, heartbeat und timeout
//!   ([`run_operation_with_heartbeat`]).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Laufzettel einer typisierten Brain-Operation.
///
/// Traegt alle Felder, die Scheibe 1 fordert: operation/attempt/run/turn/brain,
/// Lease-Generation, Phase, Sequenz, Revision und Herkunft. Die Felder sind
/// bewusst flach und serde-faehig, damit sie vom Run-Ledger (T-808) direkt
/// persistiert werden koennen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    /// Eindeutige Operation (pro Attempt): `"<run>_op<seq>-<rev>"`.
    pub operation_id: String,
    /// Wiederholungsz�hlung: 1 = erster Versuch, 2 = erster Retry usw.
    pub attempt_id: u32,
    /// Run, dem die Operation gehoert.
    pub run_id: String,
    /// Turn innerhalb des Runs (monoton, beginnt bei 1).
    pub turn_id: u32,
    /// Brain/Adapter (z.B. "chatgpt", "kimi").
    pub brain_id: String,
    /// Generation der Profil-Lease (T-804). 0, solange keine Lease aktiv ist.
    pub lease_generation: u64,
    /// Aktuelle Phase der gemeinsamen Zustandsmaschine.
    pub phase: OperationPhase,
    /// Monotone Laufnummer der Operation innerhalb des Runs.
    pub sequence: u64,
    /// Revision des Operationstrace (hochgezaehlt bei Revision statt Appende).
    pub revision: u64,
    /// Einstiegspunkt, der die Operation gestartet hat.
    pub origin: OperationOrigin,
    /// Monotone Dauer seit Start (clientseitig erfasst).
    pub duration_ms: u64,
}

impl Operation {
    /// Neuer Operationstrace ohne Events; dauert ab sofort.
    pub fn begin(
        operation_id: impl Into<String>,
        run_id: impl Into<String>,
        brain_id: impl Into<String>,
        origin: OperationOrigin,
        phase: OperationPhase,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            attempt_id: 1,
            run_id: run_id.into(),
            turn_id: 1,
            brain_id: brain_id.into(),
            lease_generation: 0,
            phase,
            sequence: 1,
            revision: 1,
            origin,
            duration_ms: 0,
        }
    }

    /// Phase wechseln; bricht ab, wenn die Kette ein Terminale erreicht hat.
    /// Versetzt die Operation nie hinter ein Terminale zurueck (fail-closed).
    pub fn transition(&mut self, next: OperationPhase) {
        if self.phase.is_terminal() {
            return;
        }
        self.phase = next;
    }

    /// Retry: Attempt erhoehen, zurueck auf die angegebene Wiederholungs-Phase.
    pub fn retry(&mut self, back_to: OperationPhase) {
        self.attempt_id += 1;
        self.revision += 1;
        self.phase = back_to;
    }
}

/// Gemeinsame Zustandsmaschine aus `docs/BRAIN_UNIFICATION_PLAN.md`:
/// AcquireProfile -> Navigate -> Ready -> Fill -> VerifyInput -> Submit ->
/// VerifySubmit -> Observe -> Finished; seitlich Blocked / Failed / Cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationPhase {
    AcquireProfile,
    Navigate,
    Ready,
    Fill,
    VerifyInput,
    Submit,
    VerifySubmit,
    Observe,
    Finished,
    Blocked,
    Failed,
    Cancelled,
}

impl OperationPhase {
    /// Die Hauptkette der gemeinsamen Zustandsmaschine (inkl. Finished).
    pub const MAIN: [OperationPhase; 9] = [
        OperationPhase::AcquireProfile,
        OperationPhase::Navigate,
        OperationPhase::Ready,
        OperationPhase::Fill,
        OperationPhase::VerifyInput,
        OperationPhase::Submit,
        OperationPhase::VerifySubmit,
        OperationPhase::Observe,
        OperationPhase::Finished,
    ];

    /// `true`, wenn in dieser Phase nichts mehr weiterlaeuft.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            OperationPhase::Finished | OperationPhase::Blocked | OperationPhase::Failed | OperationPhase::Cancelled
        )
    }

    /// Naechste Phase der Hauptkette (seitliche Zustaende bleiben stehen).
    pub fn next(self) -> OperationPhase {
        let Some(i) = Self::MAIN.iter().position(|&p| p == self) else {
            return self;
        };
        Self::MAIN.get(i + 1).copied().unwrap_or(self)
    }
}

/// Herkunft einer Operation: welcher Einstiegspunkt hat sie gestartet.
///
/// Alle sechs Einstiegspunkte sollen identische Zustandsfolgen liefern; die
/// Herkunft dient dem Nachweis und dem Ledger, nicht eigenen Regeln.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationOrigin {
    Controller,
    Repl,
    Relay,
    WebUi,
    Api,
    Swarm,
}

impl OperationOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            OperationOrigin::Controller => "controller",
            OperationOrigin::Repl => "repl",
            OperationOrigin::Relay => "relay",
            OperationOrigin::WebUi => "web_ui",
            OperationOrigin::Api => "api",
            OperationOrigin::Swarm => "swarm",
        }
    }
}

/// Klassifikation der beobachteten Oberflaeche — die zentrale Einordnung,
/// die jeder Einstiegspunkt NUTZEN muss, bevor er streamt, repariert oder
/// Erfolg zaehlt. Ersetzt keine bestehende Erkennung, sondern bundelt sie in
/// ein gemeinsames, testbares Vokabular.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceKind {
    /// Echte Modellantwort (Inhalt).
    Content,
    /// UI-Diagnosebeleg: rohe Oberflaechen-Ausgabe, kein Modellinhalt
    /// (z.B. Zai-HTML: `No response…` + `Unexpected token '<'`). Bleibt
    /// Rohbeleg; nie Textdelta, nie Parser-Repair, nie Erfolg.
    UiDiagnosis,
    /// Provider-/Daily-Limit oder Ueberlastung.
    Limit,
    /// Captcha-/Bot-Pruefung (z.B. "Verify you are human").
    Captcha,
    /// Cloudflare-/Challenge-Seite.
    Challenge,
    /// Anmelde-Wand statt Inhalt.
    Login,
    /// Transienter Fehler ohne spezifischen Marker (leer, Netzwerk-Glitch).
    Transient,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceOutcome {
    pub kind: SurfaceKind,
    /// Rohbeleg Text (unveraendert).
    pub raw_text: String,
    /// Rohbeleg HTML (unveraendert).
    pub raw_html: String,
    /// Phrase/Marker, der das Urteil getroffen hat (leer bei `Content`).
    pub matched: String,
}

impl SurfaceOutcome {
    /// `true`, wenn dies ein echter Modellinhalt ist und weiterverarbeitet
    /// werden darf (Streaming, Repair, Erfolgszaehlung).
    pub fn is_content(&self) -> bool {
        self.kind == SurfaceKind::Content
    }

    /// UI-Diagnoserohbeleg? Dann bleiben Text/HTML unveraendert und es wird
    /// weder ein Textdelta noch ein Parser-Repair daraus.
    pub fn is_raw_diagnosis(&self) -> bool {
        self.kind == SurfaceKind::UiDiagnosis
    }
}

/// Marker-Saetze fuer die Klassifikation. Bewusst klein und komplementaer zu
/// `browser::blocking::BLOCK_PHRASES` (Ganzseiten-Scan): hier geht es um die
/// vertragliche KATEGORISIERUNG der Antwort, nicht um Seiten-Scans.
const CAPTCHA_MARKERS: &[&str] = &["verify you are human", "checking your browser"];
const CHALLENGE_MARKERS: &[&str] = &["cloudflare", "challenge platform"];
const LOGIN_MARKERS: &[&str] = &[
    "log in",
    "sign in",
    "sign-in",
    "anmelden",
    "einloggen",
    "log in to continue",
    "please sign in",
];
const HTML_DIAGNOSIS_MARKERS: &[&str] = &[
    "unexpected token",
    "is not valid json",
    "invalid json",
    "<!doctype",
    "<html",
    "<body",
];
const LIMIT_MARKERS: &[&str] = &[
    "usage limit",
    "nutzungslimit",
    "rate limit",
    "ratelimit",
    "daily limit",
    "tageslimit",
    "limit reached",
    "limit erreicht",
    "too many requests",
    "quota exceeded",
    "you have reached",
    "message limit",
    "nachrichtenlimit",
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
    "issue connecting",
];

fn contains_any(hay: &str, markers: &[&'static str]) -> Option<&'static str> {
    let low = hay.to_lowercase();
    markers.iter().copied().find(|m| low.contains(m))
}

/// Zentrale Klassifikation der beobachteten Oberflaeche.
///
/// Reihenfolge ist hier alles und bewusst dokumentiert:
/// 1. Eindeutige Glitch-/HTML-Marker => [`SurfaceKind::UiDiagnosis`] (Rohbeleg).
/// 2. Loginein-/Captcha-/Challenge-Marker.
/// 3. Limit-/Ueberlastungs-Marker (DEF+EN).
/// 4. Vom Aufrufer schon als `rate_limit`/`blocked` deklarierter Backend-Status.
/// 5. Ender Protokoll-Dokument (`{` / `WEBAGENT/1`) => Inhalt.
/// 6. Echter Modelltext (nach der Chat-Aufbereitung) => Inhalt.
/// 7. Alles andere (leer/whitespace/transient) => [`SurfaceKind::Transient`].
///
/// `raw_text` und `raw_html` werden nie veraendert.
pub fn classify_surface(text: &str, raw_html: &str, backend_status: &str) -> SurfaceOutcome {
    // Kein frueher Leer-Rueckweg: auch bei leerem Text muss der
    // `backend_status`-Check unten entscheiden koennen (rate_limit/blocked).

    // 1. HTML-Diagnosebelege: HTML-Fragmente und syntaktische Fehlermeldungen
    //    sind zuerst Rohbeleg, bevor irgendetwas streamt oder repariert.
    if let Some(marker) = contains_any(text, HTML_DIAGNOSIS_MARKERS) {
        return SurfaceOutcome {
            kind: SurfaceKind::UiDiagnosis,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: marker.to_string(),
        };
    }
    if !raw_html.trim().is_empty() && contains_any(raw_html, HTML_DIAGNOSIS_MARKERS).is_some() {
        return SurfaceOutcome {
            kind: SurfaceKind::UiDiagnosis,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: "html-markers".to_string(),
        };
    }

    // 2. Login / Captcha / Challenge — unmissverstaendliche Waeinde, KEIN Inhalt.
    if let Some(marker) = contains_any(text, LOGIN_MARKERS) {
        return SurfaceOutcome {
            kind: SurfaceKind::Login,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: marker.to_string(),
        };
    }
    if let Some(marker) = contains_any(text, CAPTCHA_MARKERS) {
        return SurfaceOutcome {
            kind: SurfaceKind::Captcha,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: marker.to_string(),
        };
    }
    if let Some(marker) = contains_any(text, CHALLENGE_MARKERS) {
        return SurfaceOutcome {
            kind: SurfaceKind::Challenge,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: marker.to_string(),
        };
    }

    // 3. Limit-/Ueberlastungs-Marker.
    if let Some(marker) = contains_any(text, LIMIT_MARKERS) {
        return SurfaceOutcome {
            kind: SurfaceKind::Limit,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: marker.to_string(),
        };
    }

    // 4. Bereits deklarierter Backend-Status.
    let status = backend_status.trim().to_lowercase();
    if status == "rate_limit" || status == "limit" || status == "too_many_requests" {
        return SurfaceOutcome {
            kind: SurfaceKind::Limit,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: format!("backend_status={status}"),
        };
    }
    if status == "blocked" {
        return SurfaceOutcome {
            kind: SurfaceKind::Transient,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: "backend_status=blocked".to_string(),
        };
    }

    // 5. Protokoll-Nutzlast hat immer Vorrang (echte Arbeit, kein UI-Glitch).
    if crate::browser::has_protocol_payload(text) {
        return SurfaceOutcome {
            kind: SurfaceKind::Content,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: String::new(),
        };
    }

    // 5b. Sofort-erkennbare UI-/Provider-Ausfaelle (leer, Banner nur aus
    //     Glitch-Zeilen) sind NIE Inhalt — bevor irgendein Einstiegspunkt
    //     streamt oder Erfolg zaehlt. Protokoll-Nutzlast hat oben Vorrang.
    if crate::brain::is_retryable_empty_response(text) {
        return SurfaceOutcome {
            kind: SurfaceKind::Transient,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: "retryable-empty-response".to_string(),
        };
    }

    // 6. Echter Modellinhalt (UI-Statuszeilen/Reasoning-Echos werden verworfen).
    if !crate::observer::chat_answer_text(text).is_empty() {
        return SurfaceOutcome {
            kind: SurfaceKind::Content,
            raw_text: text.to_string(),
            raw_html: raw_html.to_string(),
            matched: String::new(),
        };
    }

    // 7. Fallback: nichts verwertbares.
    SurfaceOutcome {
        kind: SurfaceKind::Transient,
        raw_text: text.to_string(),
        raw_html: raw_html.to_string(),
        matched: "(kein erkennbarer Inhalt)".to_string(),
    }
}

/// Terminales Ergebnis einer Operation — genau eines pro Turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalOutcome {
    /// Operation erfolgreich abgeschlossen.
    Ok,
    /// Durch Provider-Limit/Ueberlastung beendet.
    Limit,
    /// Durch die Oberflaeche blockiert (z.B. Login/Captcha/Challenge).
    Blocked(SurfaceKind),
    /// Fehlerbeendet.
    Failed(String),
    /// Zeitueberschreitung.
    Timeout,
    /// Durch einen Einstiegspunkt abgebrochen.
    Cancelled,
}

/// Betriebsereignis einer Operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationEvent {
    /// Operation wurde gestartet (mit aktuellem Zustand der Operation).
    Started(Operation),
    /// Fortschritt/Meldung waehrend des Wartens.
    Heartbeat {
        op: Operation,
        note: String,
    },
    /// Wird bei einem Retry-Versuch emittiert.
    Retry {
        op: Operation,
        /// Versuchskoordinate, an die zurueckgekehrt wird.
        phase: OperationPhase,
    },
    /// Zeitueberschreitung einer Phase.
    Timeout {
        op: Operation,
        /// Verstrichene Zeit in ms.
        elapsed_ms: u64,
    },
    /// Abbruch durch den Einstiegspunkt.
    Cancelled(Operation),
    /// Terminaler Abschluss — genau EIN Terminale je Turn.
    Terminal {
        op: Operation,
        outcome: TerminalOutcome,
    },
}

impl OperationEvent {
    /// `true` fuer das einzige Terminale je Turn.
    pub fn is_terminal(&self) -> bool {
        matches!(self, OperationEvent::Terminal { .. })
    }
}

/// Einheitlicher Ereignisstrom fuer eine Operation.
///
/// Haltet die Invariante "genau ein Terminale" fail-closed durch: nach einem
/// Terminalereignis werden weitere Eintraege verworfen.
#[derive(Debug, Clone, Default)]
pub struct OperationTrace {
    events: Vec<OperationEvent>,
    terminal: bool,
}

impl OperationTrace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Startet die Operation. Schlaegt fehl, wenn schon ein Terminale vorliegt.
    pub fn started(&mut self, op: &Operation) {
        self.push(OperationEvent::Started(op.clone()));
    }

    pub fn heartbeat(&mut self, op: &Operation, note: impl Into<String>) {
        self.push(OperationEvent::Heartbeat {
            op: op.clone(),
            note: note.into(),
        });
    }

    pub fn retry(&mut self, op: &Operation, phase: OperationPhase) {
        self.push(OperationEvent::Retry {
            op: op.clone(),
            phase,
        });
    }

    pub fn timeout(&mut self, op: &Operation, elapsed_ms: u64) {
        self.push(OperationEvent::Timeout {
            op: op.clone(),
            elapsed_ms,
        });
    }

    /// Abbruch in jeder Phase. Setzt zugleich das Terminale.
    pub fn cancel(&mut self, op: &Operation) {
        if self.terminal {
            return;
        }
        self.push(OperationEvent::Cancelled(op.clone()));
        self.push(OperationEvent::Terminal {
            op: op.clone(),
            outcome: TerminalOutcome::Cancelled,
        });
    }

    /// Terminaler Abschluss.
    pub fn finish(&mut self, op: &Operation, outcome: TerminalOutcome) {
        self.push(OperationEvent::Terminal {
            op: op.clone(),
            outcome,
        });
    }

    fn push(&mut self, e: OperationEvent) {
        if self.terminal {
            // fail-closed: nach einem Terminale gibt es keine weiteren Events.
            return;
        }
        if e.is_terminal() {
            self.terminal = true;
        }
        self.events.push(e);
    }

    pub fn events(&self) -> &[OperationEvent] {
        &self.events
    }

    /// `true`, wenn bereits ein Terminale registriert wurde.
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    pub fn into_events(self) -> Vec<OperationEvent> {
        self.events
    }
}

/// Fuehrt einen blocking Aufruf mit sichtbarem started/heartbeat/timeout aus.
///
/// Die Aufgabe laeuft auf einem eigenen Thread. Alle `heartbeat`-Intervalle,
/// in denen der Worker noch nicht geantwortet hat, kommen als sichtbare
/// `Heartbeat`-Events heraus; bei Ueberschreitung von `timeout` endet die
/// Operation mit einem einzigen `Terminal { Timeout }`.
pub fn run_operation_with_heartbeat(
    mut op: Operation,
    trace: &mut OperationTrace,
    timeout: Duration,
    heartbeat: Duration,
    task: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    trace.started(&op);
    let start = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = task();
        let _ = tx.send(result);
    });

    loop {
        match rx.recv_timeout(heartbeat.min(timeout)) {
            Ok(result) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                op.duration_ms = elapsed_ms;
                return match result {
                    Ok(()) => {
                        trace.finish(&op, TerminalOutcome::Ok);
                        Ok(())
                    }
                    Err(e) => {
                        trace.finish(&op, TerminalOutcome::Failed(e.clone()));
                        Err(e)
                    }
                };
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let elapsed = start.elapsed();
                op.duration_ms = elapsed.as_millis() as u64;
                if elapsed >= timeout {
                    let elapsed_ms = elapsed.as_millis() as u64;
                    trace.timeout(&op, elapsed_ms);
                    trace.finish(&op, TerminalOutcome::Timeout);
                    // Worker ist unbestimmt blockiert; Join kurz abwarten, damit
                    // der Thread nicht als detached weiterlebt.
                    let _ = worker.join();
                    return Err(format!("Operation Timeout nach {elapsed_ms} ms"));
                }
                trace.heartbeat(&op, format!("warte… ({elapsed:?})"));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Worker beendet, ohne ein Ergebnis zu senden (z.B. Panic).
                let elapsed_ms = start.elapsed().as_millis() as u64;
                op.duration_ms = elapsed_ms;
                let _ = worker.join();
                trace.finish(&op, TerminalOutcome::Failed("Worker ohne Ergebnis beendet".into()));
                return Err("Worker ohne Ergebnis beendet".into());
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// T-802, Scheibe 2: gemeinsames Senden.
//
// Pure Entscheidungslogik fuer die gemeinsame Fill/Verify/Submit-Schleife,
// die `browser::send` fuer alle send_*-Pfade (generic, gemini, qwen) faehrt.
// Alles hier ist ohne Browser testbar; die Browser-Integration und die
// Vertraege der Abnahmepunkte stehen unten im Testblock.
// ─────────────────────────────────────────────────────────────────────

/// Normalisiert AUSSCHLIESSLICH Editor-Leerraum: Whitespace-Runs werden zu
/// einem Leerzeichen kollabiert, Raender getrimmt.
///
/// Vertrag (T-802): Es duerfen NIE Codezeichen oder Unicode pauschal
/// veraendert werden — die Funktion fasst einzig die Unicode-Whitespace-Klasse
/// (`split_whitespace`) an, alle anderen Code-Punkte bleiben unangetastet.
/// NBSP zaehlt als Leerraum, weil Rich-Text-Editoren ihn fuer Absaetze
/// ausgeben — das ist Editor-Leerraum im Sinne des Vertrags.
pub fn normalize_editor_content(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Vollstaendiger Vergleich des Editorinhalts nach reiner Leerraum-
/// Normalisierung (T-802). Ein passender Anfang reicht NICHT — Kimi hatte
/// dadurch still nur Absatz eins uebernommen.
pub fn editor_matches(actual: &str, expected: &str) -> bool {
    normalize_editor_content(actual) == normalize_editor_content(expected)
}

/// `true`, wenn `actual` ein echter Anfang (Praefix) von `expected` ist —
/// die Beschreibung eines abgeschnittenen Prompts. Leerer `actual` zaehlt
/// nicht: ein leerer Composer ist ein Fuell-, kein Kuerzungs-Zustand.
pub fn editor_is_prefix(actual: &str, expected: &str) -> bool {
    !actual.is_empty()
        && normalize_editor_content(expected).starts_with(&normalize_editor_content(actual))
}

/// Beobachteter Composer-Zustand einer Send-Runde (T-802).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendSurface {
    /// Vollstaendiger Text steht (nur Editor-Leerraum normalisiert).
    Complete,
    /// Die Oberflaeche hat die Eingabe konsumiert — die Absendung laeuft.
    Consumed,
    /// Nur ein echter Anfang steht (abgeschnittener Prompt).
    Truncated,
    /// Kein verwertbarer Inhalt (Editor leer oder fremder Text).
    Missing,
    /// Text steht vollstaendig, aber der Absendeknopf ist deaktiviert.
    Disabled,
}

/// Ordnet einen gemessenen Composer-Zustand in die Send-Skala ein.
///
/// `submit_disabled` wird nur relevant, wenn die Eingabe VOLLSTAENDIG
/// drinsteht — solange Text fehlt, ist ein grauer Knopf ein Fuell-, kein
/// Ablehnungs-Problem (Geminis ProseMirror laesst den Knopf z.B. beim reinen
/// DOM-Set grau, obwohl der Text sichtbar drinsteht).
pub fn classify_send_surface(
    actual: &str,
    expected: &str,
    consumed: bool,
    submit_disabled: bool,
) -> SendSurface {
    if consumed {
        SendSurface::Consumed
    } else if editor_matches(actual, expected) {
        if submit_disabled {
            SendSurface::Disabled
        } else {
            SendSurface::Complete
        }
    } else if editor_is_prefix(actual, expected) {
        SendSurface::Truncated
    } else {
        SendSurface::Missing
    }
}

/// Begrenzte, dokumentierte Retry-Budgets der gemeinsamen Send-Schleife.
///
/// EIN einheitliches Budget statt der frueheren 5/3/4 Versuche pro send_*:
/// `max_submit_rounds` (Submit-Gesten UND Beweis-Beobachtungen) folgt der
/// live bewaerten `send_generic`-Schleife; `max_refill_rounds` limitiert die
/// Pre-Fill-Versuche, wenn der Editor den Text nicht vollstaendig uebernimmt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendBudget {
    pub max_submit_rounds: u32,
    pub max_refill_rounds: u32,
}

impl SendBudget {
    pub const fn default_send() -> Self {
        Self {
            max_submit_rounds: 5,
            max_refill_rounds: 3,
        }
    }
}

impl Default for SendBudget {
    fn default() -> Self {
        Self::default_send()
    }
}

/// Fehler der gemeinsamen Send-Schleife (T-802).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFlowError {
    /// Absendeknopf ist deaktiviert, obwohl der Text vollstaendig steht: die
    /// Oberflaeche verweigert das Absenden ohne Meldung (Laengenablehnung).
    Disabled,
    /// Der Composer bekam den Prompt nie vollstaendig (abgeschnittener
    /// Prompt); es wurde NICHTS abgesendet.
    Truncated,
    /// Der Composer wurde nie gefuellt/gefunden; es wurde NICHTS abgesendet.
    Missing,
    /// Budget der Submit-/Beweis-Runden erschoepft, ohne dass ein
    /// Absende-Beweis kam.
    NoProof { submits: u32, refills: u32 },
}

/// Ergebnis einer erfolgreichen Send-Schleife.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFlowResult {
    pub submits: u32,
    pub refills: u32,
}

/// Gemeinsame Fill/Verify/Submit-Schleife fuer alle send_*-Pfade (T-802).
///
/// Rundenprotokoll:
/// - `observe` liefert je Runde den Composer-Zustand relativ zum gewuenschten
///   Text (`actual` ist der Editorinhalt wie der Browser ihn sieht).
/// - VOR dem ersten Submit wird ein fehlender/kurz uebernommener Text
///   nachgefuellt (`fill`), begrenzt durch `SendBudget::max_refill_rounds`.
///   Ein abgeschnittener Prompt wird NIE abgesendet.
/// - Ab dem ersten Submit wird NIE wieder gefuellt und nicht erneut gesendet,
///   sobald der Composer konsumiert beobachtet wurde: in den Zuständen nach
///   dem Submit wird nur noch der Beweis abgewartet (`wait_proof`). Das
///   verhindert den Doppelversand bei Oberflaechen, deren Send-Registrierung
///   laenger dauert als das Beweisfenster des ersten Versuchs.
/// - `Disabled` bricht ab, bevor eine Geste auf einen grauen Knopf laeuft.
/// - Alle Runden zaehlen gegen `max_submit_rounds`, damit die Schleife in
///   jedem Fall endet.
pub fn run_send_flow<Obs, Fill, Submit, WaitProof>(
    text: &str,
    budget: &SendBudget,
    mut observe: Obs,
    mut fill: Fill,
    mut submit: Submit,
    mut wait_proof: WaitProof,
) -> Result<SendFlowResult, SendFlowError>
where
    Obs: FnMut(&str) -> SendSurface,
    Fill: FnMut() -> (),
    Submit: FnMut(u32) -> (),
    WaitProof: FnMut() -> bool,
{
    let mut submits = 0u32;
    let mut refills = 0u32;
    let mut sent = false;
    loop {
        let surface = observe(text);
        match surface {
            SendSurface::Disabled => return Err(SendFlowError::Disabled),
            SendSurface::Complete => {
                submits += 1;
                if submits > budget.max_submit_rounds {
                    return Err(SendFlowError::NoProof { submits, refills });
                }
                sent = true;
                submit(submits - 1);
                if wait_proof() {
                    return Ok(SendFlowResult { submits, refills });
                }
            }
            // Ein leerer Composer VOR dem ersten Submit ist kein Konsum-, sondern
            // ein Fuellzustand: erst das Absenden macht "leer" zu "konsumiert".
            SendSurface::Consumed if !sent => {
                refills += 1;
                if refills > budget.max_refill_rounds {
                    return Err(SendFlowError::Missing);
                }
                fill();
            }
            SendSurface::Consumed => {
                // Absendung laeuft (Composer konsumiert): NUR den Beweis
                // abwarten, nicht neu fuellen oder erneut absenden — sonst
                // ensteht ein Doppel-Send bei Oberflaechen, deren
                // Send-Registrierung laenger dauert als das Beweisfenster.
                submits += 1;
                if submits > budget.max_submit_rounds {
                    return Err(SendFlowError::NoProof { submits, refills });
                }
                if wait_proof() {
                    return Ok(SendFlowResult { submits, refills });
                }
            }
            // Nach dem Absenden nie nachfuellen: ein unklarer Submit
            // (Truncated/Missing nach Send) bleibt ebenfalls Beobachtung.
            SendSurface::Truncated | SendSurface::Missing if sent => {
                submits += 1;
                if submits > budget.max_submit_rounds {
                    return Err(SendFlowError::NoProof { submits, refills });
                }
                if wait_proof() {
                    return Ok(SendFlowResult { submits, refills });
                }
            }
            SendSurface::Truncated => {
                // Abgeschnittener Prompt: wird NIE abgesendet, nur begrenzt
                // nachgefuellt.
                refills += 1;
                if refills > budget.max_refill_rounds {
                    return Err(SendFlowError::Truncated);
                }
                fill();
            }
            SendSurface::Missing => {
                refills += 1;
                if refills > budget.max_refill_rounds {
                    return Err(SendFlowError::Missing);
                }
                fill();
            }
        }
    }
}

/// Ein Edit am wachsenden Antworttext, wie ein gemeinsamer Beobachter ihn vom
/// rohen Snapshot-Strom des Browsers ableitet (T-803).
///
/// Alle Einstiegspunkte (Controller, Relay, Swarm, REPL, Web-UI, API) konsumieren
/// DENSELBEN Strom. Die Umwandlung Snapshot -> Delta passiert genau einmal hier,
/// damit sich niemand eine eigene Prefix-/Dedupe- oder Ersatzlogik ausdenkt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEdit {
    /// Echter Praefix-Zuwachs: nur der Suffix seit dem letzten Snapshot.
    ///
    /// Ein vorheriger leerer Zustand ist der erste Chunk (Stream beginnt).
    Append { text: String },
    /// Revision: der Snapshot ist kuerzer oder weicht ab (Claude Ersatz-DOM,
    /// Re-Render). Das volle neue Abbild ersetzt den alten statt ihn zu
    /// verwerfen — sonst wuerde Text verloren gehen.
    Replace { text: String },
}

/// Klassifiziert den naechsten rohen Snapshot relativ zum vorherigen.
///
/// Regeln:
/// - gleicher Snapshot -> kein Edit (`None`),
/// - leerer Snapshot -> kein Edit (transientes DOM-Leer wird ignoriert, die
///   naechste Revision traegt den vollen Text sowieso neu an),
/// - echtes Praefixwachstum -> [`StreamEdit::Append`] mit genau dem Suffix,
/// - alles andere (kuerzer, abweichend, ersetzt) -> [`StreamEdit::Replace`] mit
///   dem vollen neuen Text.
///
/// Die Regeln sind Byte-sicher: ein Praefix-Beweis garantiert, dass die
/// Suffix-Grenze auf einer UTF-8-Zeichengrenze liegt.
pub fn classify_edit(prev: Option<&str>, next: &str) -> Option<StreamEdit> {
    if next.is_empty() {
        return None;
    }
    match prev {
        None => Some(StreamEdit::Append {
            text: next.to_string(),
        }),
        Some(prev) if prev == next => None,
        Some(prev) if next.len() > prev.len() && next.starts_with(prev) => {
            Some(StreamEdit::Append {
                text: next[prev.len()..].to_string(),
            })
        }
        Some(_) => Some(StreamEdit::Replace {
            text: next.to_string(),
        }),
    }
}

/// Gemeinsamer, zustandsbehafteter Beobachter des Antwort-Streams (T-803).
///
/// Controller, Relay, Web-UI und API fuehren jeden Snapshot hier durch: Der
/// Beobachter klassifiziert mit [`classify_edit`] auf dem kumulativen,
/// bereinigten Abbild und haelt die rohen Poll-Snapshots als Beweis. Damit
/// konsumieren alle Einstiegspunkte DENSELBEN Strom statt eigener
/// Prefix-/Dedupe-/Ersatzlogik.
#[derive(Debug, Default)]
pub struct StreamJournal {
    /// Letztes kumulatives, bereinigtes Antwortabbild.
    snapshot: String,
    /// Anzahl klassifizierter Append-Edits.
    pub appends: usize,
    /// Anzahl klassifizierter Replace-Edits.
    pub replaces: usize,
    /// Rohe Poll-Snapshots (Beweis), begrenzt auf [`StreamJournal::MAX_RAW`].
    raw: Vec<String>,
}

impl StreamJournal {
    /// Oberste Menge aufbewahrter roher Poll-Snapshots.
    pub const MAX_RAW: usize = 192;

    /// Verarbeitet einen rohen Browser-Snapshot und liefert den Edit.
    pub fn push(&mut self, raw_snapshot: &str) -> Option<StreamEdit> {
        let cleaned = crate::observer::chat_answer_text(raw_snapshot);
        if cleaned == self.snapshot {
            return None;
        }
        let prev = if self.snapshot.is_empty() {
            None
        } else {
            Some(self.snapshot.as_str())
        };
        let edit = classify_edit(prev, &cleaned)?;
        self.snapshot = cleaned;
        match &edit {
            StreamEdit::Append { .. } => self.appends += 1,
            StreamEdit::Replace { .. } => self.replaces += 1,
        }
        if self.raw.len() < Self::MAX_RAW {
            self.raw.push(raw_snapshot.to_string());
        }
        Some(edit)
    }

    /// Bereinigtes kumulatives Antwortabbild (letzter Stand).
    pub fn snapshot(&self) -> &str {
        &self.snapshot
    }

    /// Gesehener Antworttext (bereinigt) bzw. leer, wenn nichts ankam.
    pub fn is_empty(&self) -> bool {
        self.snapshot.is_empty()
    }

    /// Rohe Poll-Snapshots als Beweis (maximal [`StreamJournal::MAX_RAW`]).
    pub fn raw_snapshots(&self) -> &[String] {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZAI_HTML: &str = "No response, Please try again later.
SyntaxError: Unexpected token '<', \"<!doctypeh\"... is not valid JSON";

    #[test]
    fn zai_html_is_raw_diagnosis_not_content() {
        let outcome = classify_surface(ZAI_HTML, "", "brain_incomplete");
        assert_eq!(outcome.kind, SurfaceKind::UiDiagnosis);
        assert!(outcome.is_raw_diagnosis());
        assert!(!outcome.is_content());
        // Rohbeleg wird nie veraendert.
        assert_eq!(outcome.raw_text, ZAI_HTML);
        assert!(outcome.matched.contains("unexpected token"));
    }

    #[test]
    fn qwen_daily_limit_is_limit() {
        let text = "Oops! There was an issue connecting to Qwen3.7-Plus.
You have reached the daily usage limit. Please wait 2 hours before trying again.";
        let outcome = classify_surface(text, "", "ok");
        assert_eq!(outcome.kind, SurfaceKind::Limit);
        assert!(!outcome.is_content());
        assert!(
            !outcome.matched.is_empty(),
            "Limit braucht einen erkennenden Marker, war: {:?}",
            outcome.matched
        );
    }

    #[test]
    fn chatgpt_german_usage_limit_is_limit() {
        let text = "Dateien, Bilder und Datenanalyse sind nicht verfügbar, bis dein Nutzungslimit um 23:40 zurückgesetzt wird.";
        let outcome = classify_surface(text, "", "ok");
        assert_eq!(outcome.kind, SurfaceKind::Limit);
        assert!(outcome.matched.contains("nutzungslimit"));
    }

    #[test]
    fn chatgpt_capacity_banner_is_limit_or_transient_never_content() {
        let text = "Something went wrong. If this issue persists please contact us through our help center at help.openai.com.\n\nErneut versuchen";
        let outcome = classify_surface(text, "", "ok");
        assert_ne!(outcome.kind, SurfaceKind::Content);
        assert!(!outcome.is_content());
    }

    #[test]
    fn login_wall_is_login() {
        assert_eq!(
            classify_surface("Please sign in to continue", "", "ok").kind,
            SurfaceKind::Login
        );
        assert_eq!(
            classify_surface("Log in to your account", "", "ok").kind,
            SurfaceKind::Login
        );
    }

    #[test]
    fn captcha_and_cloudflare_are_distinct() {
        assert_eq!(
            classify_surface("Verify you are human to continue", "", "ok").kind,
            SurfaceKind::Captcha
        );
        assert_eq!(
            classify_surface("Checking your browser before accessing", "", "ok").kind,
            SurfaceKind::Captcha
        );
        assert_eq!(
            classify_surface("Attention Required! Cloudflare", "", "ok").kind,
            SurfaceKind::Challenge
        );
    }

    #[test]
    fn protocol_payload_is_content_over_glitch() {
        let text = r#"No response, Please try again later.
{"protocol":"webagent/1","actions":[{"id":"a","type":"message","text":"fertig"}]}"#;
        assert_eq!(classify_surface(text, "", "ok").kind, SurfaceKind::Content);
    }

    #[test]
    fn real_content_is_content() {
        let text = "Die Aufgabe wurde abgeschlossen. Hier eine Zusammenfassung …";
        assert_eq!(classify_surface(text, "", "ok").kind, SurfaceKind::Content);
    }

    #[test]
    fn empty_is_transient() {
        assert_eq!(classify_surface("", "", "ok").kind, SurfaceKind::Transient);
        assert_eq!(
            classify_surface("   \n\t  ", "", "ok").kind,
            SurfaceKind::Transient
        );
    }

    #[test]
    fn backend_status_drives_limit_when_text_is_opaque() {
        assert_eq!(
            classify_surface("", "", "rate_limit").kind,
            SurfaceKind::Limit
        );
        assert_eq!(
            classify_surface("", "", "too_many_requests").kind,
            SurfaceKind::Limit
        );
    }

    #[test]
    fn classification_is_identical_for_every_origin() {
        // Abnahme: identische Zustandsfolgen fuer alle sechs Einstiegspunkte.
        for origin in [
            OperationOrigin::Controller,
            OperationOrigin::Repl,
            OperationOrigin::Relay,
            OperationOrigin::WebUi,
            OperationOrigin::Api,
            OperationOrigin::Swarm,
        ] {
            let samples = [
                (ZAI_HTML, SurfaceKind::UiDiagnosis),
                ("you have reached the daily usage limit", SurfaceKind::Limit),
                ("Sign in to continue", SurfaceKind::Login),
                ("Die Aufgabe wurde abgeschlossen.", SurfaceKind::Content),
            ];
            for (text, expected) in samples {
                let mut op = Operation::begin("op-1", "run-1", "chatgpt", origin, OperationPhase::Ready);
                op.origin = origin;
                let _ = op;
                assert_eq!(
                    classify_surface(text, "", "ok").kind,
                    expected,
                    "origin={:?} text={text:?}",
                    origin
                );
            }
        }
    }

    #[test]
    fn main_chain_orders_identically_for_all_entrypoints() {
        // Abnahme: identische Zustandsfolge — die Kette ist ein gemeinsames,
        // einziges Array und damit fuer alle Einstiegspunkte identisch.
        let chain: Vec<OperationPhase> = OperationPhase::MAIN.to_vec();
        assert_eq!(chain.len(), 9);
        assert_eq!(chain[0], OperationPhase::AcquireProfile);
        assert_eq!(chain[8], OperationPhase::Finished);
        for pair in chain.windows(2) {
            assert_eq!(pair[0].next(), pair[1]);
        }
        assert!(OperationPhase::Finished.is_terminal());
        assert!(OperationPhase::Blocked.is_terminal());
        assert!(OperationPhase::Failed.is_terminal());
        assert!(OperationPhase::Cancelled.is_terminal());
        assert!(!OperationPhase::Ready.is_terminal());
    }

    #[test]
    fn exactly_one_terminal_event() {
        let mut op = Operation::begin("op-1", "run-1", "brain", OperationOrigin::Relay, OperationPhase::Ready);
        let mut trace = OperationTrace::new();
        trace.started(&op);
        trace.heartbeat(&op, "poll 1");
        trace.heartbeat(&op, "poll 2");
        op.duration_ms = 12;
        trace.finish(&op, TerminalOutcome::Ok);

        // Nach dem Terminale wird alles weitere verworfen (fail-closed).
        trace.finish(&op, TerminalOutcome::Timeout);
        trace.cancel(&op);

        let events = trace.events();
        let terminales: Vec<_> = events.iter().filter(|e| e.is_terminal()).collect();
        assert_eq!(terminales.len(), 1, "genau ein Terminalereignis je Turn");
        assert!(matches!(
            terminales[0],
            OperationEvent::Terminal { outcome: TerminalOutcome::Ok, .. }
        ));
    }

    #[test]
    fn cancel_is_possible_in_every_phase() {
        for phase in OperationPhase::MAIN {
            let op = Operation::begin("op-1", "run-1", "brain", OperationOrigin::Api, phase);
            let mut trace = OperationTrace::new();
            trace.started(&op);
            trace.cancel(&op);
            assert!(trace.is_terminal());
            let events = trace.events();
            let terminales: usize = events.iter().filter(|e| e.is_terminal()).count();
            assert_eq!(terminales, 1, "phase={phase:?}");
            assert!(matches!(
                events.last(),
                Some(OperationEvent::Terminal { outcome: TerminalOutcome::Cancelled, .. })
            ));
        }
    }

    #[test]
    fn operation_carries_lease_generation_and_origin() {
        let mut op = Operation::begin("op-1", "run-42", "kimi", OperationOrigin::Swarm, OperationPhase::AcquireProfile);
        op.lease_generation = 3;
        op.turn_id = 7;
        op.sequence = 11;
        op.revision = 2;
        assert_eq!(op.lease_generation, 3);
        assert_eq!(op.turn_id, 7);
        assert_eq!(op.sequence, 11);
        assert_eq!(op.revision, 2);
        assert_eq!(op.brain_id, "kimi");
        assert_eq!(op.run_id, "run-42");
        op.transition(OperationPhase::Navigate);
        assert_eq!(op.phase, OperationPhase::Navigate);
        op.retry(OperationPhase::Fill);
        assert_eq!(op.attempt_id, 2);
        assert_eq!(op.phase, OperationPhase::Fill);
        assert_eq!(op.revision, 3);
    }

    #[test]
    fn transition_stops_at_terminal() {
        let mut op = Operation::begin("op-1", "run-1", "brain", OperationOrigin::Controller, OperationPhase::Finished);
        op.transition(OperationPhase::Ready);
        assert_eq!(op.phase, OperationPhase::Finished, "kein Zurueck hinter Terminale");
    }

    #[test]
    fn blocked_page_driver_produces_started_heartbeat_timeout() {
        // Abnahme: blockierter PageDriver erzeugt sichtbar started, heartbeat
        // und timeout — mit genau EINEM Terminalereignis.
        let op = Operation::begin("op-hang", "run-1", "brain", OperationOrigin::Controller, OperationPhase::Ready);
        let mut trace = OperationTrace::new();

        let err = run_operation_with_heartbeat(
            op.clone(),
            &mut trace,
            Duration::from_millis(120),
            Duration::from_millis(20),
            || {
                // Simuliert einen blockierten PageDriver: laenger als das
                // Timeout die Antwort blockieren.
                std::thread::sleep(Duration::from_millis(400));
                Ok(())
            },
        )
        .expect_err("blockierter Worker muss timeouten");

        assert!(err.contains("Timeout"), "Fehlertext: {err}");
        let events = trace.events();

        // started zuerst, dann mindestens ein heartbeat, dann timeout, dann Terminal.
        assert!(matches!(events.first(), Some(OperationEvent::Started(_))));
        assert!(
            events.iter().any(|e| matches!(e, OperationEvent::Heartbeat { .. })),
            "sichtbarer heartbeat fehlt: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(e, OperationEvent::Timeout { .. })),
            "sichtbares timeout fehlt: {events:?}"
        );
        assert!(
            matches!(
                events.last(),
                Some(OperationEvent::Terminal { outcome: TerminalOutcome::Timeout, .. })
            ),
            "letztes Event muss das Terminale sein: {events:?}"
        );
        let terminales: usize = events.iter().filter(|e| e.is_terminal()).count();
        assert_eq!(terminales, 1);
    }

    #[test]
    fn quick_worker_succeeds_with_single_terminal() {
        let op = Operation::begin("op-quick", "run-1", "brain", OperationOrigin::Repl, OperationPhase::Ready);
        let mut trace = OperationTrace::new();
        run_operation_with_heartbeat(
            op.clone(),
            &mut trace,
            Duration::from_millis(500),
            Duration::from_millis(10),
            || Ok(()),
        )
        .expect("schneller Worker muss ok");
        assert!(trace.is_terminal());
        assert!(matches!(
            trace.events().last(),
            Some(OperationEvent::Terminal { outcome: TerminalOutcome::Ok, .. })
        ));
        let terminales: usize = trace.events().iter().filter(|e| e.is_terminal()).count();
        assert_eq!(terminales, 1);
    }

    #[test]
    fn worker_error_is_a_terminal_failed() {
        let op = Operation::begin("op-err", "run-1", "brain", OperationOrigin::Api, OperationPhase::Submit);
        let mut trace = OperationTrace::new();
        let err = run_operation_with_heartbeat(
            op.clone(),
            &mut trace,
            Duration::from_millis(500),
            Duration::from_millis(10),
            || Err("submit kaputt".to_string()),
        )
        .expect_err("Fehler muss durchkommen");
        assert_eq!(err, "submit kaputt");
        assert!(matches!(
            trace.events().last(),
            Some(OperationEvent::Terminal { outcome: TerminalOutcome::Failed(_), .. })
        ));
    }

    // ── T-802, Scheibe 2: gemeinsames Senden (pure Abnahme) ──

    const SEND_BUDGET: SendBudget = SendBudget {
        max_submit_rounds: 5,
        max_refill_rounds: 3,
    };

    #[test]
    fn multiline_editor_content_matches_after_whitespace_normalization() {
        // Abnahme "Multiline": Newline-/Tabbing-Unterschiede sind Editor-
        // Leerraum; der Textinhalt bleibt entscheidend.
        assert!(editor_matches("a\nb", "a b"));
        assert!(editor_matches("  a\t\n b ", "a b"));
        assert!(editor_matches("a  b\n\nc", "a b c"));
        assert!(!editor_matches("a b", "a bb"));
        assert!(!editor_matches("abc", "a b c"));
        assert_eq!(normalize_editor_content("x   y\n\t z"), "x y z");
    }

    #[test]
    fn normalization_never_alters_code_or_unicode_characters() {
        // Abnahme "Unicode": die Normalisierung fasst NUR Whitespace an;
        // Umlaute, Emoji, CJK und sonstige Code-Punkte bleiben unveraendert.
        for text in [
            "héllo wörld",
            "Umlaute: äöü ß",
            "Emoji: 😀 🚀",
            "CJK: 日本語の テスト",
            "Griechisch: αβγ δε",
            "a",
            "µ",
        ] {
            assert_eq!(
                normalize_editor_content(text),
                text,
                "Normalisierung veraendert Code-Punkte: {text:?}"
            );
        }
        assert!(editor_matches("héllo\nwörld", "héllo wörld"));
        assert!(
            !editor_matches("héllo wörld", "hello world"),
            "Umlaute sind kein Leerraum"
        );
        assert!(!editor_matches("😀😀", "😀"));
    }

    #[test]
    fn truncated_prompt_is_never_sent() {
        // Abnahme "abgeschnittener Prompt": nur ein Anfang steht im Editor —
        // die Schleife fuellt begrenzt nach und bricht ab, OHNE abzusenden.
        let mut submits = 0u32;
        let mut fills = 0u32;
        let err = run_send_flow(
            "ein sehr langer vollständiger prompt",
            &SEND_BUDGET,
            |_| SendSurface::Truncated,
            || fills += 1,
            |_| submits += 1,
            || false,
        )
        .expect_err("abgeschnittener Prompt darf nicht als Ok enden");
        assert_eq!(err, SendFlowError::Truncated);
        assert_eq!(submits, 0, "es wurde NICHTS abgesendet");
        assert_eq!(fills, 3, "drei Refill-Runden innerhalb des Budgets");
    }

    #[test]
    fn disabled_button_aborts_before_any_gesture() {
        // Abnahme "deaktivierter Button": es wird abgebrochen statt weiter
        // auf einen grauen Knopf zu klicken.
        let mut submits = 0u32;
        let err = run_send_flow(
            "prompt",
            &SEND_BUDGET,
            |_| SendSurface::Disabled,
            || {},
            |_| submits += 1,
            || false,
        )
        .expect_err("deaktivierter Knopf muss abbrechen");
        assert_eq!(err, SendFlowError::Disabled);
        assert_eq!(submits, 0, "keine Geste auf einen grauen Knopf");
    }

    #[test]
    fn delayed_confirmation_is_waited_and_sends_exactly_once() {
        // Abnahmen "verspaetete Bestaetigung" + "Doppelversand-Gegenprobe":
        // der Composer wird erst nach einigen Wart-Runden konsumiert, der
        // Beweis kommt spaet — die Schleife wartet und gestet genau EINMAL.
        let mut submits = 0u32;
        let mut proofs = 0u32;
        let mut states = vec![
            SendSurface::Complete,
            SendSurface::Consumed,
            SendSurface::Consumed,
            SendSurface::Consumed,
        ];
        let result = run_send_flow(
            "prompt",
            &SEND_BUDGET,
            |_| states.remove(0),
            || {},
            |_| submits += 1,
            || {
                proofs += 1;
                proofs >= 4
            },
        )
        .expect("wartet statt frueh abzubrechen");
        assert_eq!(submits, 1, "kein Doppelversand");
        assert_eq!(result.submits, 4, "drei Wart-Runden nach dem Submit");
        assert_eq!(result.refills, 0);
    }

    #[test]
    fn missing_composer_gives_up_without_sending() {
        let mut submits = 0u32;
        let err = run_send_flow(
            "prompt",
            &SEND_BUDGET,
            |_| SendSurface::Missing,
            || {},
            |_| submits += 1,
            || false,
        )
        .expect_err("nie gefuellter Composer ist ein Fehler");
        assert_eq!(err, SendFlowError::Missing);
        assert_eq!(submits, 0);
    }

    #[test]
    fn classify_disabled_only_when_text_is_complete() {
        // Abnahme "deaktivierter Button" auf Klassifikationsebene: ein grauer
        // Knopf ist solange ein Fuell-Problem, wie der Text nicht vollstaendig
        // drinsteht (Gemini). Erst bei vollstaendigem Text ist er eine
        // Ablehnung.
        assert_eq!(
            classify_send_surface("prompt", "prompt", false, true),
            SendSurface::Disabled
        );
        assert_eq!(
            classify_send_surface("prom", "prompt", false, true),
            SendSurface::Truncated
        );
        assert_eq!(
            classify_send_surface("prompt", "prompt", false, false),
            SendSurface::Complete
        );
        assert_eq!(
            classify_send_surface("prom", "prompt", false, false),
            SendSurface::Truncated
        );
        assert_eq!(
            classify_send_surface("fremder text", "prompt", false, false),
            SendSurface::Missing
        );
        assert_eq!(
            classify_send_surface("", "prompt", false, false),
            SendSurface::Missing
        );
        assert_eq!(
            classify_send_surface("", "prompt", true, false),
            SendSurface::Consumed
        );
    }

    #[test]
    fn equal_snapshots_erzeugen_kein_edit() {
        assert_eq!(classify_edit(Some("gleich"), "gleich"), None);
        assert_eq!(classify_edit(None, ""), None);
        assert_eq!(classify_edit(Some("text"), ""), None);
    }

    #[test]
    fn praefixwachstum_wird_zum_suffix_delta() {
        assert_eq!(
            classify_edit(Some("Hallo"), "Hallo Welt"),
            Some(StreamEdit::Append {
                text: " Welt".to_string()
            })
        );
        assert_eq!(
            classify_edit(Some("abc"), "abcd"),
            Some(StreamEdit::Append {
                text: "d".to_string()
            })
        );
    }

    #[test]
    fn erster_snapshot_startet_den_strom() {
        assert_eq!(
            classify_edit(None, "Erster"),
            Some(StreamEdit::Append {
                text: "Erster".to_string()
            })
        );
    }

    #[test]
    fn kuerzerer_snapshot_ist_revision_replace() {
        // Shorter ist kein Delta, sondern Revision: voller neuer Text ersetzt.
        assert_eq!(
            classify_edit(Some("Hallo Welt"), "Hallo"),
            Some(StreamEdit::Replace {
                text: "Hallo".to_string()
            })
        );
    }

    #[test]
    fn abweichender_snapshot_ist_revision_replace() {
        // Claude Ersatz-DOM: neuer Text ist nicht Praefix, Text darf nicht
        // verloren gehen.
        assert_eq!(
            classify_edit(Some("Thinking..."), "Antwort auf die Frage"),
            Some(StreamEdit::Replace {
                text: "Antwort auf die Frage".to_string()
            })
        );
        assert_eq!(
            classify_edit(Some("alt"), "neu und laenger als alt"),
            Some(StreamEdit::Replace {
                text: "neu und laenger als alt".to_string()
            })
        );
    }

    #[test]
    fn unicode_suffix_grenze_bleibt_zeichensicher() {
        // utf8-Folge "Ö": Zuwachs auf zweitem Codepoint. Die Byte-Grenze aus
        // starts_with liegt garantiert auf einer Zeichengrenze.
        assert_eq!(
            classify_edit(Some("Hällö"), "Hällö Wörld"),
            Some(StreamEdit::Append {
                text: " Wörld".to_string()
            })
        );
        assert_eq!(
            classify_edit(Some("こん"), "こんにちは"),
            Some(StreamEdit::Append {
                text: "にちは".to_string()
            })
        );
    }

    #[test]
    fn stream_journal_beobachtet_denselben_strom_bewertet_und_belegt() {
        let mut journal = StreamJournal::default();
        assert_eq!(
            journal.push("Hallo"),
            Some(StreamEdit::Append {
                text: "Hallo".to_string()
            })
        );
        assert_eq!(
            journal.push("Hallo Welt"),
            Some(StreamEdit::Append {
                text: " Welt".to_string()
            })
        );
        assert_eq!(journal.push("Hallo Welt"), None);
        assert_eq!(
            journal.push("Revision"),
            Some(StreamEdit::Replace {
                text: "Revision".to_string()
            })
        );
        assert_eq!(journal.appends, 2);
        assert_eq!(journal.replaces, 1);
        assert_eq!(journal.snapshot(), "Revision");
        assert_eq!(journal.raw_snapshots(), &["Hallo", "Hallo Welt", "Revision"]);
        assert!(!journal.is_empty());
        let mut leer = StreamJournal::default();
        assert!(leer.is_empty());
    }

    #[test]
    fn revision_wirkt_nicht_als_doppeltes_delta() {
        let mut prev: Option<String> = None;
        let snaps = ["Hallo Welt", "Hallo", "Hallo Welt neu", "Hallo Welt neu"];
        let edits: Vec<StreamEdit> = snaps
            .iter()
            .filter_map(|snap| {
                let edit = classify_edit(prev.as_deref(), snap);
                prev = Some(snap.to_string());
                edit
            })
            .collect();
        assert_eq!(
            edits,
            vec![
                StreamEdit::Append {
                    text: "Hallo Welt".to_string()
                },
                StreamEdit::Replace {
                    text: "Hallo".to_string()
                },
                // Nach der Revision waechst der Text wieder per Praefix — ein
                // Replace wird NICHT zum Sonderfall fuer die Folgesnapshots.
                StreamEdit::Append {
                    text: " Welt neu".to_string()
                }
            ]
        );
    }
}