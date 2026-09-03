//! Grok-Bot-Gruppenmodus (T-701): 2-6 Brains, Runden, `@Brain`, Leader-Synthese.
//!
//! UI-neutral: der Runner schreibt auf [`crate::session::SessionService`] /
//! [`crate::session::EventStream`]. Antworten kommen aus einem injizierten
//! Callback (echte Brains oder Fake). `@Brain`-Handoff laeuft ueber die
//! bestehende HandoffQueue.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::benchmark::HandoffQueue;
use crate::session::{SessionEvent, SessionHandle, SessionService};
use serde::Serialize;

/// Untere Grenze: eine Gruppe ist kein Einzelchat.
pub const MIN_GROUP_BRAINS: usize = 2;
/// Obere Grenze laut Phase-7-Spec.
pub const MAX_GROUP_BRAINS: usize = 6;

static GROUP_RUN_SEQ: AtomicU64 = AtomicU64::new(1);

/// Persistente Gruppe aus 2-6 eindeutigen Brain-Ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupSpec {
    pub id: String,
    pub name: String,
    pub brains: Vec<String>,
}

impl GroupSpec {
    /// Lehnt leere Ids, Duplikate und Anzahlen ausserhalb 2-6 ab.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        brains: Vec<String>,
    ) -> Result<Self, String> {
        let id = id.into().trim().to_string();
        let name = name.into().trim().to_string();
        if id.is_empty() {
            return Err("Gruppen-Id darf nicht leer sein".to_string());
        }
        if name.is_empty() {
            return Err("Gruppenname darf nicht leer sein".to_string());
        }
        let cleaned: Vec<String> = brains.into_iter().map(|b| b.trim().to_string()).collect();
        if cleaned.iter().any(|b| b.is_empty()) {
            return Err("Brain-Ids duerfen nicht leer sein".to_string());
        }
        if cleaned.len() < MIN_GROUP_BRAINS || cleaned.len() > MAX_GROUP_BRAINS {
            return Err(format!(
                "Gruppe braucht {MIN_GROUP_BRAINS}-{MAX_GROUP_BRAINS} Brains, nicht {}",
                cleaned.len()
            ));
        }
        let mut seen = HashSet::new();
        for brain in &cleaned {
            let key = brain.to_ascii_lowercase();
            if !seen.insert(key) {
                return Err(format!("Brain {brain:?} ist doppelt"));
            }
        }
        Ok(Self {
            id,
            name,
            brains: cleaned,
        })
    }

    pub fn contains_brain(&self, id: &str) -> bool {
        self.brains.iter().any(|b| b.eq_ignore_ascii_case(id))
    }

    pub fn canonical_brain(&self, id: &str) -> Option<&str> {
        self.brains
            .iter()
            .find(|b| b.eq_ignore_ascii_case(id))
            .map(String::as_str)
    }

    /// Label fuer [`SessionEvent::Started::brain`].
    pub fn session_brain(&self) -> String {
        format!("group:{}", self.id)
    }
}

/// In-Memory-Register lebender Gruppen (eine Instanz je UI-Prozess).
#[derive(Debug, Clone, Default)]
pub struct GroupRegistry {
    groups: Arc<Mutex<HashMap<String, GroupSpec>>>,
}

impl GroupRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, spec: GroupSpec) -> Result<GroupSpec, String> {
        let mut groups = self.groups.lock().unwrap();
        if groups.contains_key(&spec.id) {
            return Err(format!("Gruppe {:?} ist bereits registriert", spec.id));
        }
        groups.insert(spec.id.clone(), spec.clone());
        Ok(spec)
    }

    pub fn get(&self, id: &str) -> Option<GroupSpec> {
        self.groups.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<GroupSpec> {
        let mut out: Vec<GroupSpec> = self.groups.lock().unwrap().values().cloned().collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn len(&self) -> usize {
        self.groups.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.lock().unwrap().is_empty()
    }
}

/// Stub-Responder fuer die HTTP-Schicht (kein Live-Brain).
pub fn stub_respond(brain: &str, prompt: &str) -> String {
    if prompt.contains("[LEADER-SYNTHESIS]") {
        format!("Synthese ({brain}).")
    } else {
        format!("{brain}: verstanden.")
    }
}

/// Wortgrenzen-`@Brain` gegen die Mitgliederliste (case-insensitive).
pub fn parse_at_mention(text: &str, members: &[String]) -> Option<String> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].1 != '@' {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !is_id_char(chars[i - 1].1);
        let token_start = i + 1;
        let mut token_end = token_start;
        while token_end < chars.len() && is_id_char(chars[token_end].1) {
            token_end += 1;
        }
        if token_start < token_end {
            let after_ok = token_end == chars.len() || !is_id_char(chars[token_end].1);
            if before_ok && after_ok {
                let start_byte = chars[token_start].0;
                let end_byte = if token_end < chars.len() {
                    chars[token_end].0
                } else {
                    text.len()
                };
                let token = &text[start_byte..end_byte];
                if let Some(member) = members.iter().find(|b| b.eq_ignore_ascii_case(token)) {
                    return Some(member.clone());
                }
            }
        }
        i = token_end.max(i + 1);
    }
    None
}

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn turn_prompt(
    round: u32,
    rounds: u32,
    brain: &str,
    task: &str,
    answers: &[(String, String)],
) -> String {
    let mut out = format!("Runde {round}/{rounds}. Du bist {brain}. Aufgabe: {task}\n");
    if !answers.is_empty() {
        out.push_str("\nBisher:\n");
        for (who, text) in answers {
            out.push_str(&format!("[{who}] {text}\n"));
        }
    }
    out.push_str(
        "\nAntworte kurz. Mit @Brain kannst du das Wort an ein anderes Gruppenmitglied weitergeben.",
    );
    out
}

fn synthesis_prompt(leader: &str, task: &str, answers: &[(String, String)]) -> String {
    let joined: String = answers
        .iter()
        .map(|(brain, text)| format!("### {brain}\n{text}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "[LEADER-SYNTHESIS]\nDu bist der Orchestrator ({leader}).\nAufgabe: «{task}».\n\n\
         Die beteiligten Modelle haben so geantwortet:\n\n{joined}\n\n\
         Fuehre diese Antworten zu einer einzigen, besten finalen Antwort zusammen. \
         Nenne Widersprueche, wenn es welche gibt."
    )
}

fn next_run_id(group_id: &str) -> String {
    let seq = GROUP_RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("group-{}-{}-{seq}", group_id, crate::now_run_stamp())
}

/// Fuehrt die Gruppe als normale Session: ein `run_id`, ein EventStream.
///
/// `respond(brain, prompt) -> text` ist injiziert (Fake in Tests, Stub in der API).
pub fn run_group<F>(
    service: &SessionService,
    spec: &GroupSpec,
    task: &str,
    rounds: u32,
    leader: &str,
    mut respond: F,
) -> Result<SessionHandle, String>
where
    F: FnMut(&str, &str) -> String,
{
    let task = task.trim();
    if task.is_empty() {
        return Err("Aufgabe darf nicht leer sein".to_string());
    }
    if rounds == 0 {
        return Err("Runden muessen >= 1 sein".to_string());
    }
    let leader = spec
        .canonical_brain(leader)
        .ok_or_else(|| format!("Leader {leader:?} gehoert nicht zur Gruppe"))?
        .to_string();

    let run_id = next_run_id(&spec.id);
    let brain_label = spec.session_brain();
    let handle = service.start(&run_id, &brain_label, task)?;
    handle.push(SessionEvent::Started {
        run_id: run_id.clone(),
        brain: brain_label,
        task: task.to_string(),
    })?;

    let mut answers: Vec<(String, String)> = Vec::new();
    for round in 1..=rounds {
        if handle.is_done() {
            return Ok(handle);
        }
        let round_task = format!("{task} [Runde {round}/{rounds}]");
        let plan: Vec<(String, String)> = spec
            .brains
            .iter()
            .map(|brain| (brain.clone(), round_task.clone()))
            .collect();
        let mut queue = HandoffQueue::new(&plan, &spec.brains, spec.brains.len().max(1));
        while let Some((brain, _effective, _handoff)) = queue.next() {
            if handle.is_done() {
                return Ok(handle);
            }
            let prompt = turn_prompt(round, rounds, &brain, task, &answers);
            let reply = respond(&brain, &prompt);
            if handle.is_done() {
                return Ok(handle);
            }
            handle.push(SessionEvent::Status {
                state: format!("round:{round}:{brain}"),
            })?;
            handle.push(SessionEvent::TextDelta {
                text: format!("[{brain}] {reply}"),
            })?;
            handle.push(SessionEvent::TextComplete)?;
            if let Some(mentioned) = parse_at_mention(&reply, &spec.brains) {
                if !mentioned.eq_ignore_ascii_case(&brain) {
                    let _ = queue.target_next_speaker(&mentioned);
                }
            }
            answers.push((brain, reply));
        }
    }

    if handle.is_done() {
        return Ok(handle);
    }
    let synth_prompt = synthesis_prompt(&leader, task, &answers);
    let synthesis = respond(&leader, &synth_prompt);
    if handle.is_done() {
        return Ok(handle);
    }
    handle.push(SessionEvent::Status {
        state: format!("synthesis:{leader}"),
    })?;
    handle.push(SessionEvent::TextDelta { text: synthesis })?;
    handle.push(SessionEvent::TextComplete)?;
    handle.push(SessionEvent::Done {
        status: "done".to_string(),
    })?;
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionEvent, Since};

    fn two() -> GroupSpec {
        GroupSpec::new("demo", "Demo 2er-Gruppe", vec!["A".into(), "B".into()]).unwrap()
    }

    #[test]
    fn group_spec_rejects_count_duplicates_and_empty_ids() {
        assert!(GroupSpec::new("g", "n", vec!["A".into()]).is_err());
        assert!(GroupSpec::new(
            "g",
            "n",
            vec!["1", "2", "3", "4", "5", "6", "7"]
                .into_iter()
                .map(str::to_string)
                .collect()
        )
        .is_err());
        assert!(GroupSpec::new("g", "n", vec!["A".into(), "a".into()]).is_err());
        assert!(GroupSpec::new("g", "n", vec!["A".into(), " ".into()]).is_err());
        assert!(GroupSpec::new(" ", "n", vec!["A".into(), "B".into()]).is_err());
        assert!(GroupSpec::new("g", "  ", vec!["A".into(), "B".into()]).is_err());
        let ok = two();
        assert_eq!(ok.brains, vec!["A", "B"]);
        assert_eq!(ok.session_brain(), "group:demo");
    }

    #[test]
    fn parse_at_mention_is_word_boundary_and_case_insensitive() {
        let members = vec!["A".into(), "B".into()];
        assert_eq!(
            parse_at_mention("bitte @B pruefen", &members).as_deref(),
            Some("B")
        );
        assert_eq!(parse_at_mention("ping @b.", &members).as_deref(), Some("B"));
        assert_eq!(parse_at_mention("@Bfoo bleibt A", &members), None);
        assert_eq!(parse_at_mention("mailA@B.com", &members), None);
        assert_eq!(parse_at_mention("kein mention", &members), None);
        assert_eq!(parse_at_mention("@C fremd", &members), None);
    }

    #[test]
    fn fake_group_run_is_one_eventstream_with_mention_and_leader_synthesis() {
        let spec = two();
        let service = SessionService::new();
        let mut a_turns = 0u32;
        let handle = run_group(&service, &spec, "summiere 2+2", 1, "A", |brain, prompt| {
            if prompt.contains("[LEADER-SYNTHESIS]") {
                "SYNTHESIS-OK".to_string()
            } else if brain == "A" {
                a_turns += 1;
                "A sieht @B als naechstes.".to_string()
            } else {
                "B sagt vier.".to_string()
            }
        })
        .unwrap();

        assert_eq!(a_turns, 1);
        assert_eq!(handle.brain(), "group:demo");
        assert!(handle.is_done());
        assert_eq!(handle.status(), "done");

        let since = service.events_since(&handle.run_id(), 0).unwrap();
        let Since::Exact { events } = since else {
            panic!("events_since(0) muss Exact sein, war Gap");
        };
        assert!(!events.is_empty());
        let mut last_seq = 0u64;
        for stamped in &events {
            assert!(
                stamped.seq > last_seq,
                "Sequenz nicht monoton: {} nach {}",
                stamped.seq,
                last_seq
            );
            last_seq = stamped.seq;
        }
        let texts: Vec<String> = events
            .iter()
            .filter_map(|e| match &e.event {
                SessionEvent::TextDelta { text } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("A sieht @B")),
            "A-Text fehlt: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("B sagt vier")),
            "B-Text fehlt: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "SYNTHESIS-OK"),
            "Leader-Synthese fehlt: {texts:?}"
        );
        assert!(matches!(
            events.last().map(|e| &e.event),
            Some(SessionEvent::Done { status }) if status == "done"
        ));
        assert!(matches!(
            &events[0].event,
            SessionEvent::Started { brain, .. } if brain == "group:demo"
        ));

        let other = SessionService::new();
        assert!(other.events_since(&handle.run_id(), 0).is_none());
        assert!(other.is_empty());
        assert_eq!(service.len(), 1);
    }

    #[test]
    fn at_mention_promotes_target_over_round_robin() {
        let spec =
            GroupSpec::new("three", "A C B", vec!["A".into(), "C".into(), "B".into()]).unwrap();
        let service = SessionService::new();
        let handle = run_group(&service, &spec, "reihenfolge", 1, "A", |brain, prompt| {
            if prompt.contains("[LEADER-SYNTHESIS]") {
                "SYNTHESIS-OK".to_string()
            } else if brain == "A" {
                "A reicht an @B weiter.".to_string()
            } else {
                format!("{brain}-ok")
            }
        })
        .unwrap();
        let since = service.events_since(&handle.run_id(), 0).unwrap();
        let speaking: Vec<String> = since
            .events()
            .iter()
            .filter_map(|e| match &e.event {
                SessionEvent::Status { state } if state.starts_with("round:") => {
                    Some(state.rsplit(':').next().unwrap().to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            speaking,
            vec!["A", "B", "C"],
            "@B muss vor C sprechen: {speaking:?}"
        );
    }

    #[test]
    fn group_registry_is_isolated_per_instance() {
        let a = GroupRegistry::new();
        let b = GroupRegistry::new();
        a.insert(two()).unwrap();
        assert_eq!(a.len(), 1);
        assert!(b.is_empty());
        assert!(a.insert(two()).unwrap_err().contains("bereits"));
    }

    #[test]
    fn stop_mid_run_keeps_done_terminal() {
        let spec = two();
        let service = SessionService::new();
        let svc = service.clone();
        let handle = run_group(&service, &spec, "stopp", 2, "B", move |brain, prompt| {
            if prompt.contains("[LEADER-SYNTHESIS]") {
                "should-not-run".to_string()
            } else {
                if let Some(snap) = svc.list().first() {
                    let _ = svc.push(
                        &snap.run_id,
                        SessionEvent::Done {
                            status: "cancelled".to_string(),
                        },
                    );
                }
                format!("{brain}-x")
            }
        })
        .unwrap();
        assert!(handle.is_done());
        assert_eq!(handle.status(), "cancelled");
        let since = service.events_since(&handle.run_id(), 0).unwrap();
        assert!(matches!(
            since.events().last().map(|e| &e.event),
            Some(SessionEvent::Done { status }) if status == "cancelled"
        ));
        assert!(!since.events().iter().any(
            |e| matches!(&e.event, SessionEvent::TextDelta { text } if text == "should-not-run")
        ));
    }
}
