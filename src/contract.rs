//! Einheitlicher Brain-Vertrag (T-801).
//!
//! Gemeinsame Zustandsmaschine und Operationen fuer jeden Einstiegspunkt
//! (Controller, REPL, Relay, Web-UI, API, Swarm). Der Vertrag ergaenzt —
//! statt zu ersetzen — die bestehenden Typen: [`crate::brain::BrainBackend`]
//! bleibt der Backend-Trait, [`crate::session::EventStream`] bleibt der
//! UI-neutrale Run-Strom. Hier liegt die *Operationsebene*: typisierte,
//! nachvollziehbare Ablaeufe mit attempt_id/run_id/brain_id/Lease-Generation/
//! Phase/Revision und Herkunft, die `SurfaceOutcome` und `OperationEvent`
//! liefert, bevor irgendein Einstiegspunkt streamt, repariert oder Erfolg
//! zaehlt.
//!
//! Abnahmegarantien dieser Scheibe:
//! - identische Zustandsfolgen fuer alle Einstiegspunkte (ein gemeinsamer
//!   Uebergang pro Phase),
//! - GENAU EIN terminales OperationEvent je Turn,
//! - Abbruch in jeder Phase als `cancelled` (nie als Erfolg),
//! - Zai-HTML (`No response … Unexpected token '<'`) bleibt Rohbeleg und wird
//!   weder Textdelta noch Parser-Repair — `SurfaceOutcome::UiGlitch`,
//! - ein blockierter PageDriver erzeugt sichtbar `started` -> `heartbeat` ->
//!   `timeout`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Absolute Echtzeit in Millisekunden seit Unix-Epoch (UTC).
fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Einstiegspunkt einer Operation — die Herkunft jeder typisierten Aktion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Controller,
    Repl,
    Relay,
    WebUi,
    Api,
    Swarm,
    Fake,
    Probe,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Controller => "controller",
            Origin::Repl => "repl",
            Origin::Relay => "relay",
            Origin::WebUi => "web-ui",
            Origin::Api => "api",
            Origin::Swarm => "swarm",
            Origin::Fake => "fake",
            Origin::Probe => "probe",
        }
    }
}

/// Gemeinsame Zustandsmaschine jeder Brain-Operation.
///
/// Seitliche Phasen [`OperationPhase::Blocked`], [`OperationPhase::Failed`]
/// und [`OperationPhase::Cancelled`] sind terminal; `Finished` ist der
/// regulære Erfolgs-Abschluss. Ein Turn durchlæuft die Hauptkette genau
/// einmal in dieser Reihenfolge und endet mit GENAU EINEM terminalen
/// Zustand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationPhase {
    /// Profil-Lease erwerben (siehe T-804).
    AcquireProfile,
    /// Zur Conversation/Start-URL navigieren.
    Navigate,
    /// Seite stehend, bereit fuer Füll-/Sendeoperationen.
    Ready,
    /// Composer fuellen.
    Fill,
    /// Füllung gegenueber dem Editor verifizieren.
    VerifyInput,
    /// Senden ausloesen.
    Submit,
    /// Zustand nach dem Senden verifizieren.
    VerifySubmit,
    /// Antwort beobachten (Streaming).
    Observe,
    /// Regulaeres Ende.
    Finished,
    /// Anbieter-Blase/Captcha/etc. — terminal, kein Erfolg.
    Blocked,
    /// Fehlschlag — terminal.
    Failed,
    /// Abbruch — terminal, nie ein Erfolg.
    Cancelled,
}

impl OperationPhase {
    /// Ist das eine terminale Phase (Turn ist abgeschlossen)?
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            OperationPhase::Finished
                | OperationPhase::Blocked
                | OperationPhase::Failed
                | OperationPhase::Cancelled
        )
    }

    /// Naechste Phase der Hauptkette (ohne terminale Seitenschritte).
    pub fn next(self) -> Option<OperationPhase> {
        use OperationPhase::*;
        match self {
            AcquireProfile => Some(Navigate),
            Navigate => Some(Ready),
            Ready => Some(Fill),
            Fill => Some(VerifyInput),
            VerifyInput => Some(Submit),
            Submit => Some(VerifySubmit),
            VerifySubmit => Some(Observe),
            Observe => Some(Finished),
            Finished | Blocked | Failed | Cancelled => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OperationPhase::AcquireProfile => "acquire_profile",
            OperationPhase::Navigate => "navigate",
            OperationPhase::Ready => "ready",
            OperationPhase::Fill => "fill",
            OperationPhase::VerifyInput => "verify_input",
            OperationPhase::Submit => "submit",
            OperationPhase::VerifySubmit => "verify_submit",
            OperationPhase::Observe => "observe",
            OperationPhase::Finished => "finished",
            OperationPhase::Blocked => "blocked",
            OperationPhase::Failed => "failed",
            OperationPhase::Cancelled => "cancelled",
        }
    }
}

/// Klassifikation eines Oberflaechen-/Antwortzinhalts, BEVOR ein Einstiegspunkt
/// streamt, repariert oder Erfolg zaehlt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceOutcome {
    /// Echter Inhalt (Modell-/Tool-Antwort, Protokoll-Nutzlast).
    Content,
    /// UI-Diagnosefragment (Zai-HTML, `No response …`, JSON-Parser-Meldung) —
    /// bleibt Rohbeleg, wird nie Antwort oder erfolgreicher Repair.
    UiGlitch {
        /// Das rohe Seite-Zitat (ausgegeben, damit der Beleg erhalten bleibt).
        raw: String,
    },
    /// Provider-Kontingent/-Limit (usage limit, daily limit, …).
    Limit,
    /// Captcha-Seite.
    Captcha,
    /// Challenge (Cloudflare o.æ.).
    Challenge,
    /// Anmelde-Wand.
    Login,
    /// Transienter Fehler (Network, Timeout, Server-Glitch).
    Transient,
}

impl SurfaceOutcome {
    /// Ist das Inhalt, den ein Einstiegspunkt als Antwort weitergeben darf?
    pub fn is_content(&self) -> bool {
        matches!(self, SurfaceOutcome::Content)
    }

    /// Terminal-schlecht? (Alles ausser `Content` unterbricht einen Turn oder
    /// verlangt eine klassifizierte Sonderbehandlung — nie ungefilterten Erfolg.)
    pub fn is_terminal_problem(&self) -> bool {
        !matches!(self, SurfaceOutcome::Content)
    }

    pub fn label(&self) -> &'static str {
        match self {
            SurfaceOutcome::Content => "content",
            SurfaceOutcome::UiGlitch { .. } => "ui_glitch",
            SurfaceOutcome::Limit => "limit",
            SurfaceOutcome::Captcha => "captcha",
            SurfaceOutcome::Challenge => "challenge",
            SurfaceOutcome::Login => "login",
            SurfaceOutcome::Transient => "transient",
        }
    }
}

/// Ein diskretes Ereignis einer Brain-Operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationEvent {
    /// Operation begann — IMMER das erste Ereignis je attempt_id.
    Started {
        attempt_id: String,
        origin: Origin,
        phase: OperationPhase,
    },
    /// Operation laeuft noch (Herzschlag; blockierte Treiber bleiben sichtbar).
    Heartbeat {
        attempt_id: String,
        phase: OperationPhase,
        elapsed_ms: u64,
    },
    /// Wiederholungsversuch (begrenzt und dokumentiert, siehe T-802).
    Retry {
        attempt_id: String,
        reason: String,
    },
    /// Timeout in einer Phase.
    Timeout {
        attempt_id: String,
        phase: OperationPhase,
        elapsed_ms: u64,
    },
    /// Abbruch in einer beliebigen Phase — nie ein Erfolg.
    Cancelled {
        attempt_id: String,
        phase: OperationPhase,
    },
    /// Terminaler Abschluss eines Turns. GENAU EIN solches Event je Turn.
    Terminal {
        attempt_id: String,
        phase: OperationPhase,
        outcome: SurfaceOutcome,
    },
}

impl OperationEvent {
    pub fn terminal(attempt_id: &str, phase: OperationPhase, outcome: SurfaceOutcome) -> Self {
        OperationEvent::Terminal {
            attempt_id: attempt_id.to_string(),
            phase,
            outcome,
        }
    }

    pub fn attempt_id(&self) -> &str {
        match self {
            OperationEvent::Started { attempt_id, .. }
            | OperationEvent::Heartbeat { attempt_id, .. }
            | OperationEvent::Retry { attempt_id, .. }
            | OperationEvent::Timeout { attempt_id, .. }
            | OperationEvent::Cancelled { attempt_id, .. }
            | OperationEvent::Terminal { attempt_id, .. } => attempt_id,
        }
    }
}

/// Typisierte Operation eines Turns — traegt alle Vertragsfelder (Herkunft von
/// [`Origin`], Phase, attempt_id, run_id, brain_id, Lease-Generation, Revision).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub attempt_id: String,
    pub run_id: String,
    pub brain_id: String,
    pub origin: Origin,
    /// Profil-Lease-Generation bei Erwerb (siehe T-804; `0` bis dahin).
    pub lease_generation: u32,
    /// Revisionszæhler der Operation (gleich wie die Revision eines
    /// Editorzustands, den sie bearbeitet).
    pub revision: u32,
    pub phase: OperationPhase,
    pub created_at_ms: u128,
}

impl Operation {
    pub fn new(
        attempt_id: &str,
        run_id: &str,
        brain_id: &str,
        origin: Origin,
        lease_generation: u32,
        revision: u32,
        phase: OperationPhase,
    ) -> Self {
        Self {
            attempt_id: attempt_id.to_string(),
            run_id: run_id.to_string(),
            brain_id: brain_id.to_string(),
            origin,
            lease_generation,
            revision,
            phase,
            created_at_ms: unix_ms(),
        }
    }
}

/// Sammler fuer [`OperationEvent`]s eines Turns. Erzwingt die Vertragsgrenze:
/// `started` zuerst, GENAU EIN terminales Event, keine Events nach Abschluss.
#[derive(Debug, Clone, Default)]
pub struct OperationLedger {
    events: Vec<OperationEvent>,
    terminal: Option<OperationEvent>,
}

impl OperationLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Legt ein Event ab. Weist `started`-vor-allem, Terminal-Grenze und
    /// Abbruch-zählt-nie-als-Erfolg ab.
    pub fn record(&mut self, event: OperationEvent) -> Result<(), String> {
        // Kein Event darf ok vor `started` existieren.
        if self.events.is_empty() && !matches!(&event, OperationEvent::Started { .. }) {
            return Err(format!(
                "Vertrag: erstes Op-Event muss Started sein, erhalten {:?}",
                event
            ));
        }
        // Nach einem Terminal ist der Turn zu.
        if let Some(terminal) = &self.terminal {
            let phase_label = match terminal {
                OperationEvent::Terminal { phase, .. } | OperationEvent::Cancelled { phase, .. } => {
                    phase.label()
                }
                _ => "?",
            };
            return Err(format!(
                "Vertrag: {attempt} ist bereits terminal ({phase}); keine weiteren Events",
                attempt = event.attempt_id(),
                phase = phase_label
            ));
        }
        // Terminal nur einmal.
        if matches!(&event, OperationEvent::Terminal { .. }) {
            self.terminal = Some(event.clone());
        }
        if matches!(&event, OperationEvent::Cancelled { .. }) {
            // Abbruch ist ein Terminalereignis im Vertragssinne — es beendet
            // den Turn und darf nie als Erfolg weiterlaufen.
            self.terminal = Some(event.clone());
        }
        self.events.push(event);
        Ok(())
    }

    /// Terminales Ereignis des Turns, falls schon erreicht.
    pub fn terminal(&self) -> Option<&OperationEvent> {
        self.terminal.as_ref()
    }

    /// Alle Ereignisse in Reihenfolge.
    pub fn events(&self) -> &[OperationEvent] {
        &self.events
    }

    /// Anzahl der Ereignisse.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Hat der Turn ein terminales Ereignis erreicht (`Finished`|`Blocked`|
    /// `Failed`|`Cancelled`)?
    pub fn is_terminal(&self) -> bool {
        self.terminal.is_some()
    }
}

/// Læuft gegen einen blockierenden/trægen Operation-Poll und schreibt sichtbar
/// `heartbeat`-Events, bis `poll` `Ok` liefert oder `timeout` ablæuft.
///
/// Fuer die Abnahme „blockierter PageDriver erzeugt sichtbar started, heartbeat
/// und timeout" gedacht — der Aufrufer liest das Ledger anschliessend:
///
/// ```text
/// started -> heartbeat -> heartbeat -> … -> timeout (elapsed_ms ≈ timeout)
/// ```
pub fn poll_with_heartbeat<F>(
    ledger: &mut OperationLedger,
    attempt_id: &str,
    phase: OperationPhase,
    poll_ms: u64,
    timeout_ms: u64,
    mut poll: F,
) -> Result<(), OperationEvent>
where
    F: FnMut() -> Result<(), ()>,
{
    let started = Instant::now();
    loop {
        if poll().is_ok() {
            return Ok(());
        }
        let elapsed = started.elapsed().as_millis() as u64;
        if elapsed >= timeout_ms {
            // Timeout als eigenes Event UND als Terminal (outcome Transient).
            let _ = ledger.record(OperationEvent::Timeout {
                attempt_id: attempt_id.to_string(),
                phase,
                elapsed_ms: elapsed,
            });
            let terminal = OperationEvent::terminal(
                attempt_id,
                OperationPhase::Failed,
                SurfaceOutcome::Transient,
            );
            let _ = ledger.record(terminal.clone());
            return Err(terminal);
        }
        let _ = ledger.record(OperationEvent::Heartbeat {
            attempt_id: attempt_id.to_string(),
            phase,
            elapsed_ms: elapsed,
        });
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_turn(ledger: &mut OperationLedger, attempt: &str) {
        let mut phase = OperationPhase::AcquireProfile;
        while let Some(next) = phase.next() {
            let _ = ledger.record(OperationEvent::Heartbeat {
                attempt_id: attempt.to_string(),
                phase,
                elapsed_ms: 1,
            });
            phase = next;
        }
    }

    #[test]
    fn hauptkette_geht_acquire_bis_finished() {
        let mut phase = OperationPhase::AcquireProfile;
        let mut seen = Vec::new();
        while let Some(next) = phase.next() {
            seen.push(phase);
            phase = next;
        }
        assert_eq!(
            seen,
            vec![
                OperationPhase::AcquireProfile,
                OperationPhase::Navigate,
                OperationPhase::Ready,
                OperationPhase::Fill,
                OperationPhase::VerifyInput,
                OperationPhase::Submit,
                OperationPhase::VerifySubmit,
                OperationPhase::Observe,
            ]
        );
        assert_eq!(phase, OperationPhase::Finished);
        assert!(phase.is_terminal());
    }

    #[test]
    fn led_zulaesst_started_erst() {
        let mut ledger = OperationLedger::new();
        let err = ledger.record(OperationEvent::Heartbeat {
            attempt_id: "a".into(),
            phase: OperationPhase::Ready,
            elapsed_ms: 0,
        });
        assert!(err.is_err(), "Heartbeat vor Started muss abgelehnt werden");
    }

    #[test]
    fn led_erzwingt_genau_ein_terminal_event() {
        let mut ledger = OperationLedger::new();
        let _ = ledger.record(OperationEvent::Started {
            attempt_id: "a1".into(),
            origin: Origin::Controller,
            phase: OperationPhase::Ready,
        });
        full_turn(&mut ledger, "a1");
        let _ = ledger.record(OperationEvent::Terminal {
            attempt_id: "a1".into(),
            phase: OperationPhase::Finished,
            outcome: SurfaceOutcome::Content,
        });
        assert!(ledger.is_terminal());
        let err = ledger.record(OperationEvent::Heartbeat {
            attempt_id: "a1".into(),
            phase: OperationPhase::Observe,
            elapsed_ms: 5,
        });
        assert!(err.is_err(), "nach Terminal darf kein weiteres Event kommen");
        let second = ledger.record(OperationEvent::Terminal {
            attempt_id: "a1".into(),
            phase: OperationPhase::Finished,
            outcome: SurfaceOutcome::Content,
        });
        assert!(second.is_err(), "genau EIN Terminalereignis je Turn");
    }

    #[test]
    fn abbruch_zaehlt_nie_als_erfolg() {
        let mut ledger = OperationLedger::new();
        let _ = ledger.record(OperationEvent::Started {
            attempt_id: "cancel-1".into(),
            origin: Origin::Repl,
            phase: OperationPhase::Fill,
        });
        let _ = ledger.record(OperationEvent::Cancelled {
            attempt_id: "cancel-1".into(),
            phase: OperationPhase::Fill,
        });
        assert!(!ledger.is_terminal() || ledger.terminal().is_some());
        assert_eq!(ledger.terminal().unwrap(), &OperationEvent::Cancelled {
            attempt_id: "cancel-1".into(),
            phase: OperationPhase::Fill,
        });
    }

    #[test]
    fn abbruch_in_jeder_phase_ist_terminal() {
        for phase in [
            OperationPhase::AcquireProfile,
            OperationPhase::Navigate,
            OperationPhase::Ready,
            OperationPhase::Fill,
            OperationPhase::VerifyInput,
            OperationPhase::Submit,
            OperationPhase::VerifySubmit,
            OperationPhase::Observe,
        ] {
            let mut ledger = OperationLedger::new();
            let _ = ledger.record(OperationEvent::Started {
                attempt_id: "c".into(),
                origin: Origin::Swarm,
                phase,
            });
            assert!(ledger.record(OperationEvent::Cancelled {
                attempt_id: "c".into(),
                phase,
            })
            .is_ok());
            assert!(ledger.is_terminal(), "Abbruch in {phase:?} muss terminal sein");
            assert!(
                ledger.record(OperationEvent::Terminal {
                    attempt_id: "c".into(),
                    phase: OperationPhase::Finished,
                    outcome: SurfaceOutcome::Content,
                })
                .is_err(),
                "nach Abbruch kein doppelter Abschluss"
            );
        }
    }

    #[test]
    fn surface_zai_html_bleibt_rohbeleg() {
        let raw = "No response, Please try again later.\nSyntaxError: Unexpected token '<', \"<!doctypeh\"... is not valid JSON";
        let outcome = classify_surface_text(raw);
        assert!(!outcome.is_content(), "Zai-HTML darf nie Inhalt sein");
        assert_eq!(outcome.label(), "ui_glitch");
    }

    #[test]
    fn surface_unterscheidet_inhalt_von_glitch_und_limit_und_login() {
        assert!(classify_surface_text("WEBAGENT/1 MESSAGE\ntext: fertig").is_content());
        assert_eq!(
            classify_surface_text("No response, Please try again later.").label(),
            "ui_glitch"
        );
        assert_eq!(
            classify_surface_text("You have reached the daily usage limit. Please wait 2 hours.")
                .label(),
            "limit"
        );
        assert_eq!(classify_surface_text("Sie sind nicht angemeldet.").label(), "login");
    }

    #[test]
    fn blocked_driver_erzeugt_started_heartbeat_timeout() {
        let mut ledger = OperationLedger::new();
        let _ = ledger.record(OperationEvent::Started {
            attempt_id: "blocked-1".into(),
            origin: Origin::Probe,
            phase: OperationPhase::Observe,
        });
        // Treiber antwortet nie -> poll() bleibt Err; timeout 60ms.
        let result = poll_with_heartbeat(
            &mut ledger,
            "blocked-1",
            OperationPhase::Observe,
            10,
            60,
            || Err(()),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err, OperationEvent::Terminal {
            attempt_id: "blocked-1".into(),
            phase: OperationPhase::Failed,
            outcome: SurfaceOutcome::Transient,
        });
        let kinds: Vec<&str> = ledger.events().iter().map(|e| match e {
            OperationEvent::Started { .. } => "started",
            OperationEvent::Heartbeat { .. } => "heartbeat",
            OperationEvent::Timeout { .. } => "timeout",
            OperationEvent::Terminal { .. } => "terminal",
            OperationEvent::Retry { .. } => "retry",
            OperationEvent::Cancelled { .. } => "cancelled",
        }).collect();
        assert_eq!(kinds[0], "started");
        assert!(kinds.contains(&"heartbeat"), "blockierter Treiber muss heartbeats zeigen");
        assert!(kinds.contains(&"timeout"));
        assert!(kinds.contains(&"terminal"));
        assert!(ledger.is_terminal());
    }

    #[test]
    fn poll_erfolgreich_beendet_ohne_terminal() {
        let mut ledger = OperationLedger::new();
        let _ = ledger.record(OperationEvent::Started {
            attempt_id: "ok-1".into(),
            origin: Origin::Controller,
            phase: OperationPhase::Observe,
        });
        let result = poll_with_heartbeat(&mut ledger, "ok-1", OperationPhase::Observe, 5, 1000, || {
            Ok(())
        });
        assert!(result.is_ok());
        assert!(!ledger.is_terminal(), "Erfolg ist kein Terminal in poll_with_heartbeat");
    }

    #[test]
    fn operation_traegt_alle_vertragsfelder() {
        let op = Operation::new(
            "a-7",
            "run-x",
            "claude",
            Origin::Api,
            3,
            42,
            OperationPhase::Observe,
        );
        assert_eq!(op.attempt_id, "a-7");
        assert_eq!(op.run_id, "run-x");
        assert_eq!(op.brain_id, "claude");
        assert_eq!(op.origin, Origin::Api);
        assert_eq!(op.lease_generation, 3);
        assert_eq!(op.revision, 42);
        assert_eq!(op.phase, OperationPhase::Observe);
        assert!(op.created_at_ms > 0);
    }

    #[test]
    fn origin_labels_sind_eindeutig() {
        let labels: Vec<&str> = [
            Origin::Controller,
            Origin::Repl,
            Origin::Relay,
            Origin::WebUi,
            Origin::Api,
            Origin::Swarm,
            Origin::Fake,
            Origin::Probe,
        ]
        .iter()
        .map(|o| o.label())
        .collect();
        let unique = labels.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), labels.len(), "Origin-Labels muessen eindeutig sein");
    }

    #[test]
    fn serde_roundtrip_operation_event() {
        for event in [
            OperationEvent::Started {
                attempt_id: "a".into(),
                origin: Origin::Relay,
                phase: OperationPhase::Ready,
            },
            OperationEvent::Terminal {
                attempt_id: "a".into(),
                phase: OperationPhase::Finished,
                outcome: SurfaceOutcome::Content,
            },
        ] {
            let text = serde_json::to_string(&event).unwrap();
            let back: OperationEvent = serde_json::from_str(&text).unwrap();
            assert_eq!(back, event);
        }
    }
}

/// Klassifiziert einen Rohtext einer Oberflaechenantwort in een [`SurfaceOutcome`].
///
/// Reihenfolge ist wichtig:
/// 1. Protokoll-Nutzlast (`WEBAGENT/1 …`) ist immer Inhalt.
/// 2. Klassische UI-Glitch-Profile (leer, „No response", „Unexpected token",
///    HTML-Dokumente) sind `UiGlitch` — blitzt auch neben echtem Inhalt nicht
///    als Antwort durch.
/// 3. Provider-Block (usage limit etc.) ist `Limit`.
/// 4. Anmelde-/Login-Phrasen sind `Login`.
/// 5. Alles andere mit Inhalt ist `Content`.
///
/// Die zwei Kernel-Mechaniken sind schnell und ohne Browser: [`crate::brain::
/// is_retryable_empty_response`] kennt die Glitch-/Limit-Profile,
/// [`crate::browser::block_phrase_in_text`] die Provider-Block-Phrasen.
pub fn classify_surface_text(raw: &str) -> SurfaceOutcome {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return SurfaceOutcome::UiGlitch {
            raw: raw.to_string(),
        };
    }
    if crate::browser::has_protocol_payload(trimmed) {
        return SurfaceOutcome::Content;
    }
    if crate::brain::is_retryable_empty_response(trimmed) {
        // Zwischen Glitch und Provider-Limit unterscheiden, damit ein
        // Einstiegspunkt Limits nicht als transienten Repair-Versuch behandelt.
        if let Some(_phrase) = crate::browser::block_phrase_in_text(trimmed) {
            return SurfaceOutcome::Limit;
        }
        return SurfaceOutcome::UiGlitch {
            raw: raw.to_string(),
        };
    }
    let low = trimmed.to_lowercase();
    if low.contains("anmelden")
        || low.contains("log in")
        || low.contains("sign in")
        || low.contains("nicht angemeldet")
        || low.contains("login erforderlich")
    {
        return SurfaceOutcome::Login;
    }
    SurfaceOutcome::Content
}