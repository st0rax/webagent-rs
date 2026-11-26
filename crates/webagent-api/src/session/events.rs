//! UI-neutrale, geordnete Session-Events mit monotonen Sequenznummern.
//!
//! Diese Schicht kennt weder Webview noch TUI noch einen Brain-Adapter. Der
//! Kern legt Ereignisse mit [`EventStream::push`] ab, Konsumenten lesen ab
//! einer Sequenznummer. Die Nummern steigen streng monoton und werden auch
//! nach dem Kuerzen der Retentionsgrenze nie wiederverwendet — eine Luecke
//! bleibt dadurch erkennbar statt still zu verschwinden.

use serde::{Deserialize, Serialize};

/// Retentionsgrenze je Lauf-Strom: aelteste Eintraege werden ab dieser Menge
/// gekuerzt, ausgegebene Sequenznummern bleiben stabil.
pub const MAX_STREAM_EVENTS: usize = 4096;

/// Geordnetes Ereignis eines Session-Laufs.
///
/// Die Varianten sind bewusst klein und textlastig: sie decken Chat und
/// Agenten-Loop ab, ohne den Brain-Adapter zu kennen. Transport (SSE/JSON)
/// laeuft ueber serde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionEvent {
    /// Lauf wurde angelegt.
    Started {
        run_id: String,
        brain: String,
        task: String,
    },
    /// Teilstueck einer Modellantwort (Streaming).
    TextDelta { text: String },
    /// Text-Phase eines Turns abgeschlossen.
    TextComplete,
    /// Aufruf eines Werkzeugs (read/bash/edit/write).
    ToolStart { tool: String, id: String },
    /// Ergebnis eines Werkzeug-Aufrufs.
    ToolResult {
        id: String,
        ok: bool,
        summary: String,
    },
    /// Laufstatuswechsel (running/login_required/cloudflare/...).
    Status { state: String },
    /// Endgueltiger Fehler.
    Error { message: String },
    /// Lauf abgeschlossen. Terminal: danach akzeptiert der Strom keine
    /// weiteren Events.
    Done { status: String },
}

/// Ein Event samt zugehoeriger monotoner Sequenznummer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StampedEvent {
    pub seq: u64,
    pub event: SessionEvent,
}

/// Ergebnis von [`EventStream::events_since`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Since {
    /// Alle Events ab `seq` sind lueckenlos vorhanden.
    Exact { events: Vec<StampedEvent> },
    /// `seq` liegt vor der Retentionsgrenze; fruehere Events sind gekuerzt.
    /// Die gelieferten Events sind die neueren ab der Grenze — der Konsument
    /// kann die Luecke sehen und z. B. die Stream-Sicht neu aufbauen.
    Gap { events: Vec<StampedEvent> },
}

impl Since {
    /// Gehaltene Events (bei `Exact` wie bei `Gap` die wirklich gelieferten).
    pub fn events(&self) -> &[StampedEvent] {
        match self {
            Since::Exact { events } | Since::Gap { events } => events,
        }
    }
}

/// Geordneter Event-Strom eines Laufs mit monotonen Sequenznummern.
#[derive(Debug, Clone)]
pub struct EventStream {
    run_id: String,
    next_seq: u64,
    events: Vec<StampedEvent>,
    /// Hoehester gekuerzter Sequenzbereich (`0` = bisher nichts gekuerzt).
    compacted_to: u64,
    done: bool,
}

impl EventStream {
    /// Neuer leerer Strom fuer einen Lauf. Die erste Sequenznummer ist 1.
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            next_seq: 1,
            events: Vec::new(),
            compacted_to: 0,
            done: false,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Naechste freie Sequenznummer (erst nach [`EventStream::push`] belegt).
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Hoeheste ausgegebene Sequenznummer (0 bei leerem, nie gekuerztem Strom).
    pub fn last_seq(&self) -> u64 {
        self.events
            .last()
            .map(|e| e.seq)
            .unwrap_or(self.compacted_to)
    }

    /// Terminal geschlossen (ein [`SessionEvent::Done`] wurde gepusht)?
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Legt ein Event ab und liefert seine monotone Sequenznummer.
    ///
    /// Schlaegt fehl, wenn der Lauf bereits [`SessionEvent::Done`] gesehen hat.
    pub fn push(&mut self, event: SessionEvent) -> Result<u64, String> {
        if self.done {
            return Err(format!(
                "Lauf {:?} ist bereits abgeschlossen (Done gilt als terminal)",
                self.run_id
            ));
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let is_done = matches!(&event, SessionEvent::Done { .. });
        self.events.push(StampedEvent { seq, event });
        if self.events.len() > MAX_STREAM_EVENTS {
            let overflow = self.events.len() - MAX_STREAM_EVENTS;
            self.events.drain(0..overflow);
            self.compacted_to = self.compacted_to.saturating_add(overflow as u64);
        }
        if is_done {
            self.done = true;
        }
        Ok(seq)
    }

    /// Alle Events strikt ab `seq` (exklusiv). Siehe [`Since`] fuer die
    /// Lueckenhervorhebung.
    pub fn events_since(&self, seq: u64) -> Since {
        let events = self
            .events
            .iter()
            .filter(|e| e.seq > seq)
            .cloned()
            .collect();
        if seq < self.compacted_to {
            Since::Gap { events }
        } else {
            Since::Exact { events }
        }
    }

    /// Kompakt-Blick auf den ganzen Bestand: (kuerzungsgrenze, gehaltene Events).
    /// Das erste gehaltene Event hat die Sequenznummer `kuerzungsgrenze + 1`.
    pub fn all_events(&self) -> (u64, &[StampedEvent]) {
        (self.compacted_to, &self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(text: &str) -> SessionEvent {
        SessionEvent::TextDelta {
            text: text.to_string(),
        }
    }

    #[test]
    fn sequenznummern_steigen_streng_monoton_ab_eins() {
        let mut stream = EventStream::new("run-a");
        assert_eq!(stream.next_seq(), 1);
        let mut last = 0;
        for i in 0..10 {
            let seq = stream.push(delta(&format!("t{i}"))).unwrap();
            assert_eq!(
                seq,
                last + 1,
                "Sequenznummern muessen lueckenlos aufsteigen"
            );
            last = seq;
        }
        assert_eq!(stream.last_seq(), 10);
        assert_eq!(stream.next_seq(), 11);
        assert!(!stream.is_done());
    }

    #[test]
    fn events_since_ist_strikt_jenseits_der_sequenz() {
        let mut stream = EventStream::new("run-b");
        let mut seqs = Vec::new();
        for i in 0..3 {
            seqs.push(stream.push(delta(&format!("t{i}"))).unwrap());
        }
        let Since::Exact { events } = stream.events_since(seqs[0]) else {
            panic!("keine Luecke, aber Gap gemeldet");
        };
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, seqs[1]);
        assert_eq!(events[1].seq, seqs[2]);
        assert!(stream.events_since(seqs[2]).events().is_empty());
    }

    #[test]
    fn leerer_strom_liefert_nichts() {
        let stream = EventStream::new("run-c");
        let Since::Exact { events } = &stream.events_since(0) else {
            panic!("Gap auf leerem Strom?");
        };
        assert_eq!(*events, vec![]);
    }

    #[test]
    fn kuertzen_hinterlaesst_erkennbare_luecke_und_stabile_nummern() {
        let mut stream = EventStream::new("run-d");
        let nr = 5 * MAX_STREAM_EVENTS / 2;
        for i in 0..nr {
            stream.push(delta(&format!("t{i}"))).unwrap();
        }
        let (compacted_to, retained) = stream.all_events();
        assert_eq!(retained.len(), MAX_STREAM_EVENTS);
        assert!(compacted_to > 0);
        assert_eq!(
            retained[0].seq,
            compacted_to + 1,
            "erstes gehaltenes Event muss direkt hinter der Kuerzungsgrenze liegen"
        );
        // Nummern bleiben global monoton und eindeutig.
        let tail = retained.last().unwrap().seq;
        assert_eq!(tail, compacted_to + MAX_STREAM_EVENTS as u64);
        assert_eq!(stream.next_seq(), tail + 1);

        // Vor der Grenze: Gap (Luecke sichtbar). An der Grenze: Exact.
        assert!(matches!(
            stream.events_since(compacted_to - 1),
            Since::Gap { .. }
        ));
        let Since::Exact { events: boundary } = stream.events_since(compacted_to) else {
            panic!("an der Grenze darf keine Luecke gemeldet werden");
        };
        assert_eq!(boundary.len(), MAX_STREAM_EVENTS);
    }

    #[test]
    fn done_ist_terminal_fuer_weitere_events() {
        let mut stream = EventStream::new("run-e");
        stream.push(delta("arbeit")).unwrap();
        let done_seq = stream
            .push(SessionEvent::Done {
                status: "done".to_string(),
            })
            .unwrap();
        assert!(stream.is_done());
        assert_eq!(stream.last_seq(), done_seq);
        let err = stream.push(delta("spaet")).unwrap_err();
        assert!(err.contains("termin"));
        let err2 = stream
            .push(SessionEvent::Error {
                message: "verspaetet".to_string(),
            })
            .unwrap_err();
        assert!(err2.contains("termin"));
    }

    #[test]
    fn events_ueberleben_done_im_lese_weg() {
        let mut stream = EventStream::new("run-f");
        let first = stream.push(delta("eins")).unwrap();
        stream
            .push(SessionEvent::Done {
                status: "done".to_string(),
            })
            .unwrap();
        let Since::Exact { events } = stream.events_since(first) else {
            panic!("keine Luecke");
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0].event, SessionEvent::Done { .. }));
    }

    #[test]
    fn session_events_serialisieren_wiederherstellbar() {
        let start = SessionEvent::Started {
            run_id: "run-g".to_string(),
            brain: "claude".to_string(),
            task: "t".to_string(),
        };
        let tool = SessionEvent::ToolResult {
            id: "a1".to_string(),
            ok: true,
            summary: "ok".to_string(),
        };
        for event in [start, delta("x"), tool] {
            let text = serde_json::to_string(&event).unwrap();
            let back: SessionEvent = serde_json::from_str(&text).unwrap();
            assert_eq!(back, event);
        }
        let stamped = StampedEvent {
            seq: 7,
            event: delta("s"),
        };
        let text = serde_json::to_string(&stamped).unwrap();
        assert_eq!(
            serde_json::from_str::<StampedEvent>(&text).unwrap(),
            stamped
        );
    }
}
