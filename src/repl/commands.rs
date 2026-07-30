//! Slash-Befehle der REPL: Grammatik und Parser.
//!
//! Kindmodul von `repl`. Reine Textverarbeitung ohne Browser, Zustand oder
//! Seiteneffekte — der am leichtesten pruefbare Teil der REPL und deshalb der
//! erste, der aus der 2090-Zeilen-Datei herausgeloest wird.

/// Ergebnis der Zeilenverarbeitung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplAction {
    Continue,
    Exit,
}

/// Slash-Befehl und optionales Argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Exit,
    Help,
    New,
    Memory {
        query: Option<String>,
    },
    Remember {
        text: String,
    },
    Forget {
        id: u64,
    },
    Switch {
        target: Option<String>,
    },
    Login,
    Chat {
        message: String,
    },
    Whoami,
    Brains,
    /// Leistungsindex-Tabelle (Reliability aus echten swarm/relay-Aufrufen).
    Score,
    /// Canary-Health-Tabelle (`/canary`).
    Canary,
    /// Einheitliches Login für alle Brains (sequenziell), schreibt profiles/<brain>.
    LoginAll,
    /// Stehendes Ziel setzen/anzeigen/löschen (fließt in autonome Aufgaben ein).
    Goal {
        arg: Option<String>,
    },
    /// Multi-Brain-Swarm: alle antworten, dann führt ein Orchestrator zusammen.
    /// `orchestrator = Some(n)` wählt Brain n (1-basiert) fest; `None` = Konsens.
    Swarm {
        orchestrator: Option<usize>,
        prompt: String,
    },
    /// Worker-Pool-TUI aus dem Chat heraus starten (`/pool [n]`, n = active).
    Pool {
        active: Option<usize>,
    },
    /// Git-Änderungen im Arbeitsverzeichnis zeigen (`/diff`).
    Diff,
    /// Autoresearch mit dem aktiven Session-Brain: `/autoresearch <eval-cmd> :: <goal>`.
    /// Leere Felder = fehlender ` :: `-Trenner → Usage-Hinweis im Handler.
    Autoresearch {
        eval_cmd: String,
        goal: String,
    },
    /// Swarm-Selbstbewertung: `/autoresearch.self [N] [--top K]` — der Pool
    /// bewertet die eigenen nächsten Verbesserungen. `None` = Default (N=10, K=10).
    AutoresearchSelf {
        suggestions: Option<usize>,
        top: Option<usize>,
    },
    /// Wiki-Memory: `/wiki` (Index), `/wiki <suchbegriff>` (Suche),
    /// `/wiki lint` (mechanischer Lint-Report).
    Wiki {
        arg: Option<String>,
    },
    Unknown {
        raw: String,
    },
}

/// Parst eine REPL-Zeile in einen Slash-Befehl oder `None` (autonomer Task).
pub fn parse_slash_command(line: &str) -> Option<SlashCommand> {
    let trimmed = line.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    if trimmed == "/help" || trimmed == "/?" {
        return Some(SlashCommand::Help);
    }
    if trimmed == "/exit" || trimmed == "/quit" {
        return Some(SlashCommand::Exit);
    }
    if trimmed == "/new" {
        return Some(SlashCommand::New);
    }
    if trimmed == "/memory" {
        return Some(SlashCommand::Memory { query: None });
    }
    if let Some(rest) = trimmed.strip_prefix("/memory ") {
        return Some(SlashCommand::Memory {
            query: Some(rest.trim().to_string()),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/remember ") {
        return Some(SlashCommand::Remember {
            text: rest.trim().to_string(),
        });
    }
    if trimmed == "/remember" {
        return Some(SlashCommand::Remember {
            text: String::new(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/forget ") {
        if let Ok(id) = rest.trim().parse::<u64>() {
            return Some(SlashCommand::Forget { id });
        }
        return Some(SlashCommand::Forget { id: 0 });
    }
    if trimmed == "/forget" {
        return Some(SlashCommand::Forget { id: 0 });
    }
    // /switch und /model sind synonym — ein „Brain" ist das Modell/der Provider,
    // wie der /model-Parameter bei anderen Agenten.
    if trimmed == "/switch" || trimmed == "/model" {
        return Some(SlashCommand::Switch { target: None });
    }
    if let Some(rest) = trimmed
        .strip_prefix("/switch ")
        .or_else(|| trimmed.strip_prefix("/model "))
    {
        return Some(SlashCommand::Switch {
            target: Some(rest.trim().to_lowercase()),
        });
    }
    if trimmed == "/goal" {
        return Some(SlashCommand::Goal { arg: None });
    }
    if let Some(rest) = trimmed.strip_prefix("/goal ") {
        return Some(SlashCommand::Goal {
            arg: Some(rest.trim().to_string()),
        });
    }
    if trimmed == "/swarm" {
        return Some(SlashCommand::Swarm {
            orchestrator: None,
            prompt: String::new(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/swarm ") {
        let rest = rest.trim();
        // Optionaler Orchestrator-Index: „/swarm 3 <prompt>" (1-8). Nur wenn das
        // erste Token eine Zahl 1-8 ist UND ein Prompt folgt — sonst ganzer Rest = Prompt.
        if let Some((head, tail)) = rest.split_once(char::is_whitespace) {
            if let Ok(n) = head.parse::<usize>() {
                if (1..=8).contains(&n) && !tail.trim().is_empty() {
                    return Some(SlashCommand::Swarm {
                        orchestrator: Some(n),
                        prompt: tail.trim().to_string(),
                    });
                }
            }
        }
        return Some(SlashCommand::Swarm {
            orchestrator: None,
            prompt: rest.to_string(),
        });
    }
    if trimmed == "/diff" {
        return Some(SlashCommand::Diff);
    }
    // WICHTIG: vor dem `/autoresearch`-Zweig prüfen, sonst schluckt der (bzw. der
    // Unknown-Fallback) den `.self`-Befehl. Syntax: /autoresearch.self [N] [--top K].
    if trimmed == "/autoresearch.self" || trimmed.starts_with("/autoresearch.self ") {
        let rest = trimmed
            .strip_prefix("/autoresearch.self")
            .unwrap_or("")
            .trim();
        let mut suggestions: Option<usize> = None;
        let mut top: Option<usize> = None;
        let mut it = rest.split_whitespace();
        while let Some(tok) = it.next() {
            if tok == "--top" {
                top = it
                    .next()
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|n| *n >= 1);
            } else if let Some(v) = tok.strip_prefix("--top=") {
                top = v.parse::<usize>().ok().filter(|n| *n >= 1);
            } else if suggestions.is_none() {
                // Erstes freies Token als N (Vorschläge je Brain), nur wenn Zahl >= 1.
                suggestions = tok.parse::<usize>().ok().filter(|n| *n >= 1);
            }
        }
        return Some(SlashCommand::AutoresearchSelf { suggestions, top });
    }
    if trimmed == "/autoresearch" || trimmed.starts_with("/autoresearch ") {
        // Syntax: /autoresearch <eval-cmd> :: <goal> — der Trenner " :: " macht
        // Eval-Befehle mit Leerzeichen/Pipes eindeutig vom Ziel unterscheidbar.
        let rest = trimmed.strip_prefix("/autoresearch").unwrap_or("").trim();
        if let Some((cmd, goal)) = rest.split_once(" :: ") {
            let (cmd, goal) = (cmd.trim(), goal.trim());
            if !cmd.is_empty() && !goal.is_empty() {
                return Some(SlashCommand::Autoresearch {
                    eval_cmd: cmd.to_string(),
                    goal: goal.to_string(),
                });
            }
        }
        // Fehlender Trenner oder leere Teile → Usage-Pfad.
        return Some(SlashCommand::Autoresearch {
            eval_cmd: String::new(),
            goal: String::new(),
        });
    }
    if trimmed == "/wiki" {
        return Some(SlashCommand::Wiki { arg: None });
    }
    if let Some(rest) = trimmed.strip_prefix("/wiki ") {
        return Some(SlashCommand::Wiki {
            arg: Some(rest.trim().to_string()),
        });
    }
    if trimmed == "/pool" || trimmed == "/tui" || trimmed == "/workers" {
        return Some(SlashCommand::Pool { active: None });
    }
    if let Some(rest) = trimmed
        .strip_prefix("/pool ")
        .or_else(|| trimmed.strip_prefix("/tui "))
        .or_else(|| trimmed.strip_prefix("/workers "))
    {
        let active = rest.trim().parse::<usize>().ok().filter(|n| *n >= 1);
        return Some(SlashCommand::Pool { active });
    }
    if trimmed == "/login" {
        return Some(SlashCommand::Login);
    }
    if trimmed == "/whoami" {
        return Some(SlashCommand::Whoami);
    }
    if trimmed == "/brains" || trimmed == "/modules" {
        return Some(SlashCommand::Brains);
    }
    if trimmed == "/score" || trimmed == "/leaderboard" {
        return Some(SlashCommand::Score);
    }
    if trimmed == "/canary" {
        return Some(SlashCommand::Canary);
    }
    if trimmed == "/login-all" {
        return Some(SlashCommand::LoginAll);
    }
    if let Some(rest) = trimmed.strip_prefix("/chat ") {
        return Some(SlashCommand::Chat {
            message: rest.trim().to_string(),
        });
    }
    Some(SlashCommand::Unknown {
        raw: trimmed.to_string(),
    })
}
