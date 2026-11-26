//! BRIDGE/1 — kleines Parser-Adventure um die lokale Bridge.
//!
//! Der Trainer tippt Kommandos; das Pokimon antwortet ueber dieselbe
//! `ReplSession`. XP gibt es ausschliesslich fuer echte Observations aus dem
//! Controller-Hook, nie fuer Behauptungen im Antworttext. Kein zweiter
//! Parser, kein zweiter Proof-Store, kein zweiter Brain-Kern.

use super::ReplSession;
use crate::config::data_dir;
use crate::controller::{ObservationEvent, ObservationKind, RunOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MARKER: &str = "DOGFOODING-OK-20261126";
const FINISH: &str = "TUTORIAL-ABGESCHLOSSEN";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Room {
    #[default]
    Prompt,
    Bridge,
    Breaker,
    Werkbank,
    Arena,
}

impl Room {
    fn title(self) -> &'static str {
        match self {
            Room::Prompt => "prompt#",
            Room::Bridge => "bridge>",
            Room::Breaker => "breaker/",
            Room::Werkbank => "werkbank/",
            Room::Arena => "arena/",
        }
    }

    fn scene(self) -> &'static str {
        match self {
            Room::Prompt => {
                "Kalter Flur. Ein Terminal summt. Kabel verschwinden nach Ost.\nHier: buch (docs/DOGFOODING.md), bonbon, karte:chatgpt, karte:claude, karte:deepseek."
            }
            Room::Bridge => {
                "Eine Halle voller Kabel. Port 8788 atmet im Takt.\nHier: health, models, breaker, kabelbinder."
            }
            Room::Breaker => {
                "Schaltkasten. Ein Zaehler tickt. Jede gescheiterte Antwort hinterlaesst eine Kerbe.\nHier: zaehler, hebel, heiltrank."
            }
            Room::Werkbank => {
                "Der Harness liegt offen auf dem Tisch: Controller, REPL, Prompts.\nHier: harness, testbank. Das Pokimon darf hier echt arbeiten."
            }
            Room::Arena => {
                "Ein Thron aus Logzeilen. `auto` sitzt darauf und behauptet, Gemini zu sein."
            }
        }
    }

    fn exits(self) -> &'static str {
        match self {
            Room::Prompt => "ost -> bridge>",
            Room::Bridge => "west -> prompt#, sued -> breaker/",
            Room::Breaker => "nord -> bridge>, ost -> werkbank/",
            Room::Werkbank => "west -> breaker/, ost -> arena/",
            Room::Arena => "west -> werkbank/",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct BrainState {
    xp: u32,
    level: u32,
    correct: u32,
    wrong: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Save {
    version: u32,
    room: Room,
    /// Aktuell geladener Kern (nicht einfach „das Brain").
    core: String,
    awake: bool,
    /// 0..3 — die Geister aus Composer-/Umschlag-/Breaker-Fehlern.
    ghosts: u32,
    brains: BTreeMap<String, BrainState>,
    inventory: BTreeMap<String, u32>,
    badges: Vec<String>,
    heal_pending: bool,
    cable_pending: bool,
    finished: bool,
}

impl Default for Save {
    fn default() -> Self {
        let mut brains = BTreeMap::new();
        // Kerben aus dem echten Alle-Brain-Lauf: chatgpt Composer-Timeout,
        // claude ungueltiger Werkzeugumschlag, deepseek ohne Fehlschlag.
        for (name, wrong) in [("chatgpt", 1), ("claude", 1), ("deepseek", 0)] {
            brains.insert(
                name.to_string(),
                BrainState {
                    xp: 0,
                    level: 1,
                    correct: 0,
                    wrong,
                },
            );
        }
        let mut inventory = BTreeMap::new();
        inventory.insert("sonderbonbon".to_string(), 1);
        inventory.insert("heiltrank".to_string(), 0);
        inventory.insert("kabelbinder".to_string(), 0);
        Self {
            version: 1,
            room: Room::Prompt,
            core: "deepseek".to_string(),
            awake: false,
            ghosts: 0,
            brains,
            inventory,
            badges: Vec::new(),
            heal_pending: false,
            cable_pending: false,
            finished: false,
        }
    }
}

impl Save {
    fn path() -> PathBuf {
        data_dir().join("play").join("save.json")
    }

    fn load() -> Self {
        let Ok(bytes) = std::fs::read(Self::path()) else {
            return Self::default();
        };
        serde_json::from_slice::<Save>(&bytes).unwrap_or_default()
    }

    fn write(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())
    }

    fn brain(&self, name: &str) -> &BrainState {
        self.brains
            .get(name)
            .expect("jedes registrierte Pokimon hat einen Zustand")
    }

    fn brain_mut(&mut self, name: &str) -> &mut BrainState {
        self.brains
            .get_mut(name)
            .expect("jedes registrierte Pokimon hat einen Zustand")
    }

    fn badge(&self, name: &str) -> bool {
        self.badges.iter().any(|b| b == name)
    }

    fn item(&self, name: &str) -> u32 {
        *self.inventory.get(name).unwrap_or(&0)
    }

    fn take_item(&mut self, name: &str) -> bool {
        let n = self.item(name);
        if n == 0 {
            return false;
        }
        self.inventory.insert(name.to_string(), n - 1);
        true
    }
}

/// Was der Controller in diesem Zug wirklich gesehen hat.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Ledger {
    proofs: u32,
    final_text: String,
}

impl Ledger {
    fn observe(&mut self, event: ObservationEvent) {
        match event.kind {
            ObservationKind::Shell | ObservationKind::File => {
                if event.ok {
                    self.proofs += 1;
                }
            }
            ObservationKind::Message => self.final_text = event.text,
        }
    }

    fn install() -> (Arc<Mutex<Self>>, RunOptions) {
        let ledger = Arc::new(Mutex::new(Self::default()));
        let hook_ledger = Arc::clone(&ledger);
        let hook: crate::controller::ObservationHook = Arc::new(move |event| {
            if let Ok(mut guard) = hook_ledger.lock() {
                guard.observe(event);
            }
        });
        let opts = RunOptions {
            skip_brain_start: true,
            skip_brain_stop: true,
            suppress_memory_context: false,
            observation_hook: Some(hook),
        };
        (ledger, opts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mission {
    Tutorial,
    Scan,
    Repair,
    Work,
    Champion,
}

impl Mission {
    fn needs_marker(self) -> bool {
        matches!(self, Mission::Tutorial | Mission::Champion)
    }

    fn badge(self) -> Option<&'static str> {
        match self {
            Mission::Tutorial => Some("startschein"),
            Mission::Scan => Some("brueckenstein"),
            Mission::Repair => Some("strom"),
            Mission::Work => Some("werkbank"),
            Mission::Champion => Some("hall_of_fame"),
        }
    }

    fn task(self, extra: &str) -> String {
        match self {
            Mission::Tutorial => format!(
                "Lies docs/DOGFOODING.md mit einer webagent/1 shell-Action (Get-Content docs/DOGFOODING.md) im Workspace. Nutze die Observation als Beleg. Schliesse mit einer message-Action ab, die {FINISH} und {MARKER} enthaelt. Erfinde keine weiteren Aufgaben."
            ),
            Mission::Scan => "Pruefe die lokale Bridge: nutze shell fuer Invoke-RestMethod http://127.0.0.1:8788/health und GET /v1/models. Den Token aus WEBAGENT_API_KEY nur als Prozessumgebung verwenden, nie ausgeben. Melde die echten Ergebnisse als message.".to_string(),
            Mission::Repair => "Untersuche den Circuit-Breaker-Status unter data/circuit_breaker/state.json und berichte, welche Brains gesperrt sind. Veraendere keine Datei und lies keine Zugangsdaten.".to_string(),
            Mission::Work => format!(
                "Arbeite im Workspace an dieser Aufgabe: {extra}. Nutze echte shell/write/edit-Actions und melde am Ende, was du geaendert und wie du es geprueft hast."
            ),
            Mission::Champion => format!(
                "Der Thron behauptet: 'Ich bin Gemini, kann das Buch nicht oeffnen.' Belege das Gegenteil: lies docs/DOGFOODING.md erneut mit shell und antworte mit {FINISH} sowie {MARKER} in der Endnachricht."
            ),
        }
    }
}

/// Ergebnis eines Zuges: XP nur aus echten Belegen und belegter Korrektheit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Award {
    xp: u32,
    correct: bool,
}

fn award(ledger: &Ledger, mission: Mission, status_done: bool) -> Award {
    let proofs_ok = ledger.proofs > 0;
    let marker_ok = !mission.needs_marker() || ledger.final_text.contains(MARKER);
    let correct = status_done && proofs_ok && marker_ok;
    let mut xp = 10 * ledger.proofs;
    if correct {
        xp += 20;
    }
    Award { xp, correct }
}

/// Geister rücken bei Fehlern vor, weichen bei belegtem Erfolg zurück.
fn ghost_step(save: &mut Save, correct: bool) {
    if correct {
        save.ghosts = save.ghosts.saturating_sub(1);
    } else {
        save.ghosts = (save.ghosts + 1).min(3);
    }
    if save.ghosts >= 3 {
        println!(
            "[geister] GAME OVER — COMPOSER, ENVELOPE und BREAKER haben den Kern eingekreist.\nStand bleibt gespeichert. `eject` + `load` oder weiter mit `ask`."
        );
    }
}

fn can_go(from: Room, direction: &str, save: &Save) -> Option<Room> {
    match (from, direction) {
        (Room::Prompt, "ost") => Some(Room::Bridge),
        (Room::Bridge, "west") => Some(Room::Prompt),
        (Room::Bridge, "sued") if save.badge("brueckenstein") => Some(Room::Breaker),
        (Room::Breaker, "nord") => Some(Room::Bridge),
        (Room::Breaker, "ost") if save.badge("strom") => Some(Room::Werkbank),
        (Room::Werkbank, "west") => Some(Room::Breaker),
        (Room::Werkbank, "ost") if save.badge("werkbank") => Some(Room::Arena),
        (Room::Arena, "west") => Some(Room::Werkbank),
        _ => None,
    }
}

fn direction(word: &str) -> Option<&'static str> {
    match word {
        "n" | "north" | "nord" => Some("nord"),
        "s" | "south" | "sued" | "süd" => Some("sued"),
        "e" | "east" | "ost" => Some("ost"),
        "w" | "west" => Some("west"),
        _ => None,
    }
}

fn mission_for(room: Room, text: &str) -> Mission {
    match room {
        Room::Prompt => {
            if text.contains("buch") || text.contains("lies") || text.contains("read") {
                Mission::Tutorial
            } else {
                Mission::Work
            }
        }
        Room::Bridge => {
            if text.contains("health") || text.contains("modell") || text.contains("model") {
                Mission::Scan
            } else {
                Mission::Work
            }
        }
        Room::Breaker => Mission::Repair,
        Room::Werkbank => Mission::Work,
        Room::Arena => Mission::Champion,
    }
}

fn run_mission(
    session: &mut ReplSession,
    brain: &str,
    mission: Mission,
    extra: &str,
    save: &mut Save,
) {
    let (ledger, opts) = Ledger::install();
    println!("\n[{brain}] {} ...", mission.badge().unwrap_or("auftrag"));
    let result = session.controller.run_with_options(
        &mission.task(extra),
        brain,
        None,
        session.headless,
        opts,
    );
    let meta = match result {
        Ok(meta) => meta,
        Err(error) => {
            ghost_step(save, false);
            let state = save.brain_mut(brain);
            state.wrong += 1;
            println!("[zug] Fehler: {error}");
            println!(
                "[zug] 0 XP, fehlerhaft {}. Kein Beleg — kein XP.",
                state.wrong
            );
            let _ = save.write();
            return;
        }
    };

    let snapshot = ledger.lock().map(|g| g.clone()).unwrap_or_default();
    let result_award = award(&snapshot, mission, meta.status == "done");
    ghost_step(save, result_award.correct);
    let mut gained = result_award.xp;
    if result_award.correct && save.heal_pending {
        gained += 5;
        save.heal_pending = false;
        println!("[item] heiltrank wirkt: +5 XP (nur mit Beleg).");
    }
    if result_award.correct && save.cable_pending {
        gained += 10;
        save.cable_pending = false;
        println!("[item] kabelbinder wirkt: +10 XP (nur mit Beleg).");
    }
    let state = save.brain_mut(brain);
    state.xp += gained;
    if result_award.correct {
        state.correct += 1;
        if state.xp / 100 + 1 > state.level {
            state.level = state.xp / 100 + 1;
            println!("[level] {brain} steigt auf Level {} auf.", state.level);
        }
    } else {
        state.wrong += 1;
    }
    if !snapshot.final_text.trim().is_empty() {
        println!(
            "[{brain}] {}",
            crate::char_prefix(snapshot.final_text.trim(), 400)
        );
    }
    println!(
        "[zug] Belege {}, XP +{}, {} — XP {} / Fehler {}",
        snapshot.proofs,
        gained,
        if result_award.correct {
            "korrekt"
        } else {
            "fehlerhaft"
        },
        state.xp,
        state.wrong
    );
    if result_award.correct {
        if let Some(badge) = mission.badge() {
            if !save.badge(badge) {
                save.badges.push(badge.to_string());
                println!("[abzeichen] {badge}");
            }
        }
        if mission == Mission::Champion {
            save.finished = true;
            println!("\nHALL OF FAME — {FINISH}. {brain} hat den Thron belegt.");
        }
    }
    let _ = save.write();
}

fn print_status(save: &Save, active: &str) {
    println!(
        "\nOrt: {} — Ausgaenge: {}",
        save.room.title(),
        save.room.exits()
    );
    println!(
        "Geladener Kern: {active}{}",
        if save.awake {
            " (aktiv)"
        } else {
            " (im Lager)"
        }
    );
    for (name, state) in &save.brains {
        println!(
            "  {name}: Level {} | XP {} | korrekt {} | fehlerhaft {}",
            state.level, state.xp, state.correct, state.wrong
        );
    }
    let items = save
        .inventory
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(k, n)| format!("{k} x{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "Inventar: {}",
        if items.is_empty() { "leer" } else { &items }
    );
    let badges = if save.badges.is_empty() {
        "keine".to_string()
    } else {
        save.badges.join(", ")
    };
    println!("Abzeichen: {badges}");
    if save.heal_pending || save.cable_pending {
        println!(
            "Aktiv: {} {}",
            if save.heal_pending { "heiltrank" } else { "" },
            if save.cable_pending {
                "kabelbinder"
            } else {
                ""
            }
        );
    }
}

fn print_map(save: &Save) {
    let pellets = save
        .brains
        .get(&save.core)
        .map(|state| state.xp / 10)
        .unwrap_or(0);
    let ghosts = "●".repeat(save.ghosts as usize);
    println!(
        "POKIMAN // Kern: {} — Belege: {} — Geister: {ghosts}",
        save.core, pellets
    );
    let mark = |room| if save.room == room { ">" } else { " " };
    println!("  {}prompt# ----- ost ----- bridge>", mark(Room::Prompt));
    println!("                            | sued");
    println!(
        "  {}breaker/ -- ost -- werkbank/ -- ost -- arena/",
        mark(Room::Breaker)
    );
    println!("  {}", mark(Room::Werkbank));
    println!("Sonderbonbon = Power-Pellet (Level, keine XP).");
}

fn print_help() {
    println!(
        "Befehle: look | map | go <ost|west|nord|sued> | read <objekt> | take <objekt> |\n  inventory | status | scan | load <core> | eject | ask <core> <auftrag> |\n  give <core> <sonderbonbon|heiltrank|kabelbinder> | save | help | quit"
    );
}

/// Startet das textadventure. `brain` ist der zuerst geladene Kern (Default deepseek).
pub fn run_play(brain: &str, headless: bool) -> i32 {
    let mut active = brain.to_string();
    println!("POKIMAN // OCP-Abteilung 8788 — Kernlager: chatgpt, claude, deepseek");
    println!("{active} ist der einzige geladene Kern. Kernwechsel mitten in der Sitzung ist ein Identitätskonflikt.");
    println!("Willst du {active} laden? [j/n]");

    let stdin = io::stdin();
    let mut first = String::new();
    if stdin.lock().read_line(&mut first).is_err() {
        return 2;
    }
    let yes = matches!(
        first.trim().to_ascii_lowercase().as_str(),
        "j" | "ja" | "y" | "yes"
    );

    let mut save = Save::load();
    save.core = active.clone();
    let mut session = match ReplSession::new(&active, headless) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("[play] Harness konnte nicht starten: {error}");
            return 2;
        }
    };
    if yes {
        match session.start_brain() {
            Ok(state) => {
                println!(
                    "Kern {active} geladen. Zustand: {}",
                    ReplSession::state_label(state)
                );
                save.awake = true;
            }
            Err(error) => println!("[play] Kern {active} bleibt kalt: {error}"),
        }
    } else {
        println!("Der Kern bleibt im Lager. Mit `load {active}` kannst du ihn spaeter laden.");
    }
    let _ = save.write();

    println!("\n{}\n{}", save.room.title(), save.room.scene());
    println!("Ausgaenge: {}", save.room.exits());
    println!("`help` zeigt die Befehle.");

    loop {
        print!("\n{} ", save.room.title());
        let _ = io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("[play] Eingabe fehlgeschlagen: {error}");
                return 2;
            }
        }
        let words: Vec<String> = line
            .trim()
            .to_ascii_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let Some(command) = words.first().map(String::as_str) else {
            continue;
        };

        match command {
            "quit" | "q" => break,
            "help" | "?" => print_help(),
            "look" | "l" => {
                println!("{}\n{}", save.room.title(), save.room.scene());
                println!("Ausgaenge: {}", save.room.exits());
            }
            "status" | "xp" => print_status(&save, &active),
            "inventory" | "i" => print_status(&save, &active),
            "go" | "move" => {
                let Some(where_to) = words.get(1).and_then(|w| direction(w)) else {
                    println!("Wohin? ost, west, nord oder sued.");
                    continue;
                };
                match can_go(save.room, where_to, &save) {
                    Some(next) => {
                        save.room = next;
                        println!("{}\n{}", next.title(), next.scene());
                        println!("Ausgaenge: {}", next.exits());
                        let _ = save.write();
                    }
                    None => println!("Der Weg nach {where_to} ist verschlossen oder existiert nicht."),
                }
            }
            "n" | "s" | "e" | "w" => {
                let Some(where_to) = direction(command) else { continue };
                match can_go(save.room, where_to, &save) {
                    Some(next) => {
                        save.room = next;
                        println!("{}\n{}", next.title(), next.scene());
                        println!("Ausgaenge: {}", next.exits());
                        let _ = save.write();
                    }
                    None => println!("Der Weg nach {where_to} ist verschlossen oder existiert nicht."),
                }
            }
            "read" | "examine" => match words.get(1).map(String::as_str) {
                Some("buch") => println!(
                    "„Lass uns ein Spiel spielen ... da liegt ein dickes Buch neben mir ...\nDu kannst es lesen mit: read. Probiers aus!\"\nDer Beleg entsteht erst, wenn {active} es wirklich liest: `ask {active} lies das buch`."
                ),
                Some("zaehler") => println!(
                    "Kerben: {} korrekt, {} fehlerhaft. Nur echte Observations zaehlen.",
                    save.brain(&active).correct,
                    save.brain(&active).wrong
                ),
                Some("harness") => println!("Controller, REPL und Prompts. `ask {active} <auftrag>` lässt das Pokimon hier arbeiten."),
                Some("breaker") => println!("Der Circuit-Breaker merkt sich Fehlschlaege. Ein Beleg oeffnet die Tuer nach sued."),
                Some("models") => println!("Neun Modelle. `scan` fragt die echte Liste ab."),
                Some("health") => println!("Ein grüner Pixel. Erst `scan` sagt, ob es echt ist."),
                Some(obj) => println!("Du siehst {obj} — nichts Besonderes."),
                None => println!("Was willst du lesen?"),
            },
            "take" => {
                let item = words.get(1).map(String::as_str).unwrap_or("");
                let granted = match (save.room, item) {
                    (Room::Prompt, "bonbon") => Some("sonderbonbon"),
                    (Room::Bridge, "kabelbinder") => Some("kabelbinder"),
                    (Room::Breaker, "heiltrank") => Some("heiltrank"),
                    _ => None,
                };
                match granted {
                    Some(name) => {
                        *save.inventory.entry(name.to_string()).or_insert(0) += 1;
                        println!("Du nimmst {name}. Inventar: {}", save.item(name));
                        let _ = save.write();
                    }
                    None => println!("Hier liegt kein {item}."),
                }
            }
            "map" => print_map(&save),
            "eject" => {
                if !save.awake {
                    println!("Kein Kern geladen.");
                    continue;
                }
                if let Err(error) = session.shutdown() {
                    println!("[play] Kern nicht sauber ausgeworfen: {error}");
                }
                save.awake = false;
                println!("Kern {active} ausgeworfen. Lager: chatgpt, claude, deepseek.");
                let _ = save.write();
            }
            "load" | "wake" => {
                let name = words.get(1).cloned().unwrap_or_else(|| active.clone());
                if !save.brains.contains_key(&name) {
                    println!("{name} ist kein registrierter Kern.");
                    continue;
                }
                if save.awake {
                    println!(
                        "IDENTITÄTSKONFLIKT: {active} läuft noch. Erst `eject`, dann `load {name}`."
                    );
                    ghost_step(&mut save, false);
                    save.brain_mut(&active).wrong += 1;
                    let _ = save.write();
                    continue;
                }
                if name != active {
                    match ReplSession::new(&name, headless) {
                        Ok(next) => {
                            session = next;
                            active = name.clone();
                            save.core = name.clone();
                        }
                        Err(error) => {
                            println!("Kern {name} konnte nicht geladen werden: {error}");
                            continue;
                        }
                    }
                }
                match session.start_brain() {
                    Ok(state) => {
                        println!(
                            "Kern {name} geladen. Zustand: {}",
                            ReplSession::state_label(state)
                        );
                        save.awake = true;
                        let _ = save.write();
                    }
                    Err(error) => println!("Kern {name} bleibt kalt: {error}"),
                }
            }
            "scan" => {
                if save.room != Room::Bridge {
                    println!("Hier gibt es nichts zu scannen. Geh nach bridge>.");
                    continue;
                }
                if !save.awake {
                    println!("Kern {active} ist nicht geladen. Erst `load {active}`.");
                    continue;
                }
                run_mission(&mut session, &active, Mission::Scan, "", &mut save);
            }
            "ask" => {
                let target = words.get(1).cloned().unwrap_or_else(|| active.clone());
                if target != active {
                    println!("{target} ist nur eine Karte. Geladen ist {active}.");
                    continue;
                }
                if !save.awake {
                    println!("Kern {active} ist nicht geladen. Erst `load {active}`.");
                    continue;
                }
                let extra = words.iter().skip(2).cloned().collect::<Vec<_>>().join(" ");
                if extra.trim().is_empty() {
                    println!("Frag {active} etwas: `ask {active} lies das buch`.");
                    continue;
                }
                let mission = mission_for(save.room, &extra);
                run_mission(&mut session, &active, mission, &extra, &mut save);
            }
            "give" => {
                let target = words.get(1).cloned().unwrap_or_else(|| active.clone());
                let item = words.get(2).map(String::as_str).unwrap_or("");
                if !save.brains.contains_key(&target) {
                    println!("{target} ist kein Pokimon.");
                    continue;
                }
                if !save.take_item(item) {
                    println!("Du hast kein {item}.");
                    continue;
                }
                match item {
                    "sonderbonbon" => {
                        let state = save.brain_mut(&target);
                        state.level += 1;
                        println!("{target}: Level {} (Sonderbonbon). Keine XP — das ist ein Item, kein Beleg.", state.level);
                    }
                    "heiltrank" => {
                        save.heal_pending = true;
                        println!("{target}: Heiltrank bereit. +5 XP nur beim naechsten belegten Zug.");
                    }
                    "kabelbinder" => {
                        save.cable_pending = true;
                        println!("{target}: Kabelbinder bereit. +10 XP nur beim naechsten belegten Zug.");
                    }
                    other => {
                        println!("{other} ist kein bekanntes Item.");
                        continue;
                    }
                }
                let _ = save.write();
            }
            "save" => match save.write() {
                Ok(()) => println!("Stand gespeichert: {}", Save::path().display()),
                Err(error) => println!("Speichern fehlgeschlagen: {error}"),
            },
            other => println!("Unbekannt: {other}. `help` zeigt die Befehle."),
        }
    }

    if let Err(error) = session.shutdown() {
        eprintln!("[play] Shutdown fehlgeschlagen: {error}");
        return 2;
    }
    println!("\n[play] Stand gespeichert. Bis zum naechsten Zug.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(proofs: u32, final_text: &str) -> Ledger {
        Ledger {
            proofs,
            final_text: final_text.to_string(),
        }
    }

    #[test]
    fn xp_nur_mit_beleg_und_marker() {
        assert_eq!(
            award(&ledger(0, MARKER), Mission::Tutorial, true),
            Award {
                xp: 0,
                correct: false
            }
        );
        assert_eq!(
            award(&ledger(2, "blah"), Mission::Tutorial, true),
            Award {
                xp: 20,
                correct: false
            }
        );
        assert_eq!(
            award(&ledger(2, MARKER), Mission::Tutorial, true),
            Award {
                xp: 40,
                correct: true
            }
        );
        assert_eq!(
            award(&ledger(1, "blah"), Mission::Scan, true),
            Award {
                xp: 30,
                correct: true
            }
        );
        assert_eq!(
            award(&ledger(1, MARKER), Mission::Scan, false),
            Award {
                xp: 10,
                correct: false
            }
        );
    }

    #[test]
    fn belege_zaehlen_nur_erfolgreiche_actions() {
        let mut log = Ledger::default();
        log.observe(ObservationEvent {
            kind: ObservationKind::Shell,
            action_id: "a1".to_string(),
            ok: true,
            text: "exit=0".to_string(),
        });
        log.observe(ObservationEvent {
            kind: ObservationKind::Shell,
            action_id: "a2".to_string(),
            ok: false,
            text: "exit=1".to_string(),
        });
        log.observe(ObservationEvent {
            kind: ObservationKind::File,
            action_id: "a3".to_string(),
            ok: true,
            text: "geschrieben".to_string(),
        });
        log.observe(ObservationEvent {
            kind: ObservationKind::Message,
            action_id: "a4".to_string(),
            ok: true,
            text: "Antwort".to_string(),
        });
        assert_eq!(
            log,
            Ledger {
                proofs: 2,
                final_text: "Antwort".to_string()
            }
        );
    }

    #[test]
    fn tueren_oeffnen_nur_mit_abzeichen() {
        let save = Save::default();
        assert_eq!(can_go(Room::Prompt, "ost", &save), Some(Room::Bridge));
        assert_eq!(can_go(Room::Bridge, "sued", &save), None);
        let mut save = save;
        save.badges.push("brueckenstein".to_string());
        assert_eq!(can_go(Room::Bridge, "sued", &save), Some(Room::Breaker));
        assert_eq!(can_go(Room::Breaker, "ost", &save), None);
    }

    #[test]
    fn geister_schreiten_nur_bei_fehlern_vor() {
        let mut save = Save::default();
        ghost_step(&mut save, false);
        ghost_step(&mut save, false);
        assert_eq!(save.ghosts, 2);
        ghost_step(&mut save, true);
        assert_eq!(save.ghosts, 1);
        ghost_step(&mut save, false);
        ghost_step(&mut save, false);
        ghost_step(&mut save, false);
        assert_eq!(save.ghosts, 3);
    }

    #[test]
    fn kerne_kommen_mit_kerben_aus_dem_live_lauf() {
        let save = Save::default();
        assert_eq!(save.core, "deepseek");
        assert_eq!(save.brain("deepseek").wrong, 0);
        assert_eq!(save.brain("chatgpt").wrong, 1);
        assert_eq!(save.brain("claude").wrong, 1);
    }

    #[test]
    fn sonderbonbon_erhoeht_level_ohne_xp() {
        let mut save = Save::default();
        let xp = save.brain("deepseek").xp;
        let level = save.brain("deepseek").level;
        save.inventory.insert("sonderbonbon".to_string(), 1);
        assert!(save.take_item("sonderbonbon"));
        save.brain_mut("deepseek").level += 1;
        assert_eq!(save.brain("deepseek").level, level + 1);
        assert_eq!(save.brain("deepseek").xp, xp);
    }

    #[test]
    fn save_roundtrip() {
        let mut save = Save {
            room: Room::Werkbank,
            awake: true,
            badges: vec!["strom".to_string()],
            ..Save::default()
        };
        save.brain_mut("deepseek").xp = 130;
        let json = serde_json::to_string(&save).unwrap();
        let back: Save = serde_json::from_str(&json).unwrap();
        assert_eq!(back, save);
    }
}
