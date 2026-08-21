//! Autonome Abläufe der REPL: Swarm, Autoresearch, Self-Research und
//! autonome Aufgaben gegen den AgentController.
//!
//! Aus `repl::mod` extrahiert (Schritt 8) — reine Moves, keine Logikänderung.
//! Der `impl ReplSession`-Block erreicht die privaten Felder/Methoden des
//! Elternmoduls als Kindmodul (gleiche Technik wie `pool.rs`).

use super::{isolated_query, ReplSession};
use crate::config::{available_brain_ids, data_dir};
use crate::controller::RunOptions;

impl ReplSession {
    /// Ein voller Frage-Zyklus gegen ein frisches Brain-Backend: start → ensure_ready
    /// → new_chat → send → wait_response → stop. Für den Swarm, wo jedes Brain der
    /// Reihe nach befragt wird. `profile_override` erlaubt ein isoliertes
    /// Laufzeit-Profil (Swarm-Teilkopie) statt des Shared-Profils.
    fn swarm_query(
        &self,
        brain_id: &str,
        prompt: &str,
        profile_override: Option<std::path::PathBuf>,
    ) -> Result<String, String> {
        isolated_query(brain_id, prompt, self.headless, profile_override)
    }

    /// `/swarm [n] <prompt>` — Multi-Brain-Swarm (schlüssiger Ablauf).
    ///
    /// **Ablauf**
    /// 1. **Antworten:** jedes verfügbare Brain bekommt denselben Prompt, jeweils
    ///    in einem **isolierten** Profil (`prepare_swarm_profile` → Kopie aus
    ///    `reference/<brain>` oder `profiles/<brain>`). Kein Shared-Pool.
    /// 2. **Orchestrator wählen** (wer synthetisiert):
    ///    - `/swarm N …` → Brain N (1-basiert), falls es in Phase 1 geantwortet hat
    ///    - sonst: **Reliability** unter den Antwortenden (`brain_score`, kein
    ///      zusätzlicher Browser-Roundtrip)
    ///    - optional teuer: `WEBAGENT_SWARM_VOTE=1` → jedes antwortende Brain
    ///      stimmt ab (sieht Antwort-Kurzfassungen im Prompt)
    /// 3. **Synthese:** nur der Orchestrator bekommt alle Antworten und liefert final.
    /// 4. Swarm-Profile aufräumen, REPL-Brain wieder starten.
    ///
    /// Früher Phase-2-„Konsens“: jedes Brain nochmal voll befragen *ohne* die
    /// Antworten zu sehen → teuer und inhaltlich blind. Default ist jetzt Score.
    pub(crate) fn run_swarm(&mut self, orchestrator: Option<usize>, prompt: &str) {
        if prompt.trim().is_empty() {
            println!("[swarm] Nutzung: /swarm <prompt>         — Orchestrator per Reliability");
            println!("[swarm]         /swarm <1-8> <prompt>  — Orchestrator fest");
            println!("[swarm]         WEBAGENT_SWARM_VOTE=1  — teure Live-Abstimmung");
            return;
        }
        let targets = available_brain_ids();
        if let Some(n) = orchestrator {
            if !(1..=targets.len()).contains(&n) {
                println!(
                    "[swarm] Ungültiger Orchestrator-Index {n} (1-{}).",
                    targets.len()
                );
                return;
            }
        }
        if let Err(error) = self.stop_brain() {
            eprintln!("[swarm] Aktives Brain konnte nicht gestoppt werden: {error}");
            return;
        }

        let run_id = crate::now_run_stamp();
        let mut profiles = Vec::with_capacity(targets.len());
        for brain in &targets {
            match crate::config::prepare_swarm_profile(&run_id, brain) {
                Ok(lease) => profiles.push(lease),
                Err(error) => {
                    eprintln!(
                        "[swarm] profile preparation failed for run={run_id} brain={brain}: {error}"
                    );
                    let _ = self.start_brain();
                    return;
                }
            }
        }
        let profile_of = |brain: &str| -> Option<std::path::PathBuf> {
            profiles
                .iter()
                .find(|lease| lease.brain_id() == brain)
                .map(|lease| lease.profile_dir().to_path_buf())
        };

        // Stehendes /goal analog zu run_autonomous voranstellen (leer wenn keins).
        let goal_ctx = match &self.goal {
            Some(g) => format!("Übergeordnetes Ziel: {g}\n\n"),
            None => String::new(),
        };
        let framed_prompt = format!("{goal_ctx}{prompt}");

        // ---- Phase 1: Antworten (isoliert, sequenziell) ----
        println!(
            "[swarm] Phase 1/3 — {} Brains antworten (isolierte Profile)…",
            targets.len()
        );
        let mut answers: Vec<(String, String)> = Vec::new();
        for (i, tb) in targets.iter().enumerate() {
            let prof = profile_of(tb);
            match self.swarm_query(tb, &framed_prompt, prof) {
                Ok(a) => {
                    let preview: String = a.chars().take(200).collect();
                    println!(
                        "[swarm {}/{}] \x1b[1m{tb}\x1b[0m: {preview}{}",
                        i + 1,
                        targets.len(),
                        if a.chars().count() > 200 { "…" } else { "" }
                    );
                    answers.push((tb.clone(), a));
                }
                Err(e) => println!("[swarm {}/{}] {tb}: — {e}", i + 1, targets.len()),
            }
        }
        let cards = crate::transcript::session_turns_from_swarm(&framed_prompt, &answers);
        for turn in &cards {
            if turn.kind == crate::transcript::SessionTurnKind::Brain {
                println!("[swarm-karte] {}", turn.body);
            }
        }
        if answers.is_empty() {
            println!("[swarm] Keine Antworten — Abbruch.");
            let _ = self.start_brain();
            return;
        }
        if answers.len() == 1 {
            println!(
                "[swarm] Nur eine Antwort ({}) — überspringe Synthese.",
                answers[0].0
            );
            println!("\n[swarm ⇒ final]\n{}\n", answers[0].1);
            let _ = self.start_brain();
            return;
        }
        let names: Vec<String> = answers.iter().map(|(b, _)| b.clone()).collect();

        // ---- Phase 2: Orchestrator ----
        let live_vote = matches!(
            std::env::var("WEBAGENT_SWARM_VOTE")
                .unwrap_or_default()
                .to_lowercase()
                .as_str(),
            "1" | "true" | "yes" | "on"
        );
        let orch = match orchestrator {
            Some(n) => {
                let chosen = targets[n - 1].clone();
                if !names.contains(&chosen) {
                    println!(
                        "[swarm] Phase 2/3 — {chosen} hat nicht geantwortet → Fallback {}",
                        names[0]
                    );
                    names[0].clone()
                } else {
                    println!("[swarm] Phase 2/3 — Orchestrator (fest): \x1b[1m{chosen}\x1b[0m");
                    chosen
                }
            }
            None if live_vote => {
                // Teuer: jeder Antwortende stimmt ab — mit Kurzfassungen der Antworten.
                println!("[swarm] Phase 2/3 — Live-Abstimmung (WEBAGENT_SWARM_VOTE=1)…");
                let mut snippets = String::new();
                for (b, a) in &answers {
                    let snip: String = a.chars().take(280).collect();
                    snippets.push_str(&format!("\n### {b}\n{snip}\n"));
                }
                let vote_prompt = format!(
                    "{goal_ctx}Aufgabe: «{prompt}».\n\
                     Folgende Modelle haben geantwortet (Kurzfassung):{snippets}\n\
                     Welches EINE Modell aus der Liste [{list}] soll die finale Synthese machen?\n\
                     Antworte NUR mit genau einem Namen aus der Liste.",
                    list = names.join(", ")
                );
                let mut votes: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for voter in &names {
                    let prof = profile_of(voter);
                    if let Ok(v) = self.swarm_query(voter, &vote_prompt, prof) {
                        let low = v.to_lowercase();
                        if let Some(pick) = names.iter().find(|n| low.contains(&n.to_lowercase())) {
                            *votes.entry(pick.clone()).or_insert(0) += 1;
                            println!("[swarm vote] {voter} → {pick}");
                        } else {
                            println!("[swarm vote] {voter} → (keine klare Nennung: {v})");
                        }
                    }
                }
                let winner = names
                    .iter()
                    .max_by_key(|n| votes.get(*n).copied().unwrap_or(0))
                    .cloned()
                    .unwrap_or_else(|| names[0].clone());
                let wv = votes.get(&winner).copied().unwrap_or(0);
                println!("[swarm] Phase 2/3 — Abstimmung: \x1b[1m{winner}\x1b[0m ({wv} Stimme(n))");
                winner
            }
            None => {
                // Default: Reliability unter den Antwortenden — kein extra Browser-Round.
                let board = crate::brain_score::leaderboard();
                let score_of = |id: &str| -> f64 {
                    board
                        .iter()
                        .find(|s| s.brain_id == id)
                        .map(|s| s.reliability)
                        .unwrap_or(0.5)
                };
                let winner = names
                    .iter()
                    .max_by(|a, b| {
                        score_of(a)
                            .partial_cmp(&score_of(b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .cloned()
                    .unwrap_or_else(|| names[0].clone());
                let sc = score_of(&winner);
                println!(
                    "[swarm] Phase 2/3 — Orchestrator per Reliability: \x1b[1m{winner}\x1b[0m (score={sc:.2})"
                );
                println!("[swarm]         (Live-Vote: WEBAGENT_SWARM_VOTE=1)");
                winner
            }
        };

        // ---- Phase 3: Synthese (nur Orchestrator) ----
        println!("[swarm] Phase 3/3 — {orch} synthetisiert…");
        let joined: String = answers
            .iter()
            .map(|(b, a)| format!("### {b}\n{a}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let synth_prompt = format!(
            "{goal_ctx}Aufgabe: «{prompt}».\n\nDie beteiligten Modelle haben so geantwortet:\n\n{joined}\n\n\
             Führe diese Antworten zu einer einzigen, besten finalen Antwort zusammen. \
             Nenne Widersprüche, wenn es welche gibt. Du bist der Orchestrator ({orch}).",
        );
        match self.swarm_query(&orch, &synth_prompt, profile_of(&orch)) {
            Ok(final_answer) => {
                self.stats.chars_out += final_answer.chars().count();
                self.stats.brains_used.insert(orch.clone());
                println!("\n[swarm ⇒ final via \x1b[1m{orch}\x1b[0m]\n{final_answer}\n");
            }
            Err(e) => {
                println!("[swarm] Synthese durch {orch} fehlgeschlagen: {e}");
                // Fallback: längste/erste Antwort zeigen statt totaler Leere
                if let Some((b, a)) = answers.first() {
                    println!("[swarm] Fallback — erste Antwort ({b}):\n{a}\n");
                }
            }
        }

        // Lease-Drops räumen Profile; REPL-Brain wieder starten.
        let _ = self.start_brain();
        println!("[swarm] fertig. Aktiv weiterhin: {}", self.brain_id);
    }

    /// `/autoresearch <eval-cmd> :: <goal>` — Autoresearch mit dem aktiven
    /// Session-Brain und kleinen Defaults fürs interaktive Ausprobieren
    /// (max_iterations=3). Fortschritt druckt die Kernschleife live:
    /// `[autoresearch i/N] metrik X -> Y (behalten|verworfen)`.
    pub(crate) fn run_autoresearch(&mut self, eval_cmd: &str, goal: &str) {
        if eval_cmd.trim().is_empty() || goal.trim().is_empty() {
            println!("[autoresearch] Nutzung: /autoresearch <eval-cmd> :: <goal>");
            println!(
                "[autoresearch] Beispiel: /autoresearch cargo build 2>&1 | grep -c warning :: Compiler-Warnings reduzieren"
            );
            return;
        }
        let workdir = match crate::autoresearch::resolve_project_root() {
            Ok(root) => root,
            Err(e) => {
                eprintln!("[autoresearch] {e}");
                return;
            }
        };
        // Der Modify-Schritt öffnet ein eigenes Backend auf demselben Profil —
        // das Session-Brain vorher freigeben, danach wieder starten (wie /pool).
        if let Err(error) = self.stop_brain() {
            eprintln!("[autoresearch] Aktives Brain konnte nicht gestoppt werden: {error}");
            return;
        }
        let config = crate::autoresearch::AutoResearchConfig {
            brain_id: self.brain_id.clone(),
            goal: goal.to_string(),
            eval_cmd: eval_cmd.to_string(),
            direction: crate::autoresearch::Direction::HigherIsBetter,
            max_iterations: 3,
            no_improve_abort: 3,
            headless: self.headless,
            workdir,
            eval_timeout_secs: 300,
        };
        match crate::autoresearch::run(config) {
            Ok(report) => {
                let kept = report.iterations.iter().filter(|i| i.kept).count();
                println!(
                    "[autoresearch] fertig: branch={} stop={} iterationen={} behalten={} final_metric={}",
                    report.branch,
                    report.stopped_reason,
                    report.iterations.len(),
                    kept,
                    report.final_metric
                );
            }
            Err(e) => eprintln!("[autoresearch] Fehler: {e}"),
        }
        if let Err(e) = self.start_brain() {
            eprintln!("[autoresearch] Brain-Neustart fehlgeschlagen: {e}");
        }
    }

    /// `/autoresearch.self [N] [--top K]` — Swarm-Selbstbewertung: der ganze Pool
    /// bewertet in vier Phasen die wichtigsten nächsten Verbesserungen. Nutzt
    /// isolierte Profile wie `/swarm` und legt das Ergebnis zusätzlich als
    /// Wiki-Seite `self-research-<stamp>` ab.
    pub(crate) fn run_self_research(&mut self, suggestions: Option<usize>, top: Option<usize>) {
        let n = suggestions.unwrap_or(10);
        let k = top.unwrap_or(10);
        let targets = available_brain_ids();
        if targets.is_empty() {
            println!("[self-research] keine Brains registriert.");
            return;
        }
        self.stats.swarms += 1;
        if let Err(error) = self.stop_brain() {
            eprintln!("[self-research] Aktives Brain konnte nicht gestoppt werden: {error}");
            return;
        }

        let run_id = crate::now_run_stamp();
        let mut profiles = Vec::with_capacity(targets.len());
        for brain in &targets {
            match crate::config::prepare_swarm_profile(&run_id, brain) {
                Ok(lease) => profiles.push(lease),
                Err(error) => {
                    eprintln!(
                        "[self-research] profile preparation failed for run={run_id} brain={brain}: {error}"
                    );
                    let _ = self.start_brain();
                    return;
                }
            }
        }
        let profile_of = |brain: &str| -> Option<std::path::PathBuf> {
            profiles
                .iter()
                .find(|lease| lease.brain_id() == brain)
                .map(|lease| lease.profile_dir().to_path_buf())
        };

        // Projektfakten aus dem Repo-Root (Fallback: aktuelles Verzeichnis).
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        let root = crate::autoresearch::git_repo_root(&cwd).unwrap_or(cwd);
        let facts = crate::self_research::gather_facts(&root, 1200);
        let headless = self.headless;

        let report = crate::self_research::run_self_research(&targets, &facts, n, k, 4, |b, p| {
            isolated_query(b, p, headless, profile_of(b))
        });

        // Ergebnis als Wiki-Seite ablegen (Dogfooding der Wiki-Memory).
        if !report.catalog.is_empty() {
            let wiki = crate::wiki_memory::WikiMemory::new(data_dir().join("memory").join("wiki"));
            let title = format!("self-research-{run_id}");
            let body = crate::self_research::format_report(&report);
            match wiki.write_page(&title, &body) {
                Ok(slug) => println!("[self-research] Ergebnis abgelegt als [[{slug}]]."),
                Err(e) => eprintln!("[self-research] Wiki-Ablage fehlgeschlagen: {e}"),
            }
        }

        // Lease-Drops räumen die Profile; REPL-Brain wieder starten.
        let _ = self.start_brain();
        println!("[self-research] fertig. Aktiv weiterhin: {}", self.brain_id);
    }

    /// `/diff` — was hat sich im Arbeitsverzeichnis (git) geändert?
    pub(crate) fn print_diff(&self) {
        let run_git = |args: &[&str]| -> Option<String> {
            std::process::Command::new("git")
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        };
        let Some(status) = run_git(&["status", "--short"]) else {
            println!("[diff] Kein git-Repository im Arbeitsverzeichnis (oder git fehlt).");
            return;
        };
        if status.is_empty() {
            println!("[diff] Arbeitsverzeichnis sauber — keine Änderungen.");
            return;
        }
        println!("[diff] git status --short:\n{status}");
        if let Some(stat) = run_git(&["diff", "--stat"]) {
            if !stat.is_empty() {
                println!("\n[diff] git diff --stat:\n{stat}");
            }
        }
        println!("\n[diff] Details: git diff <datei> im Terminal.");
    }

    pub(crate) fn run_autonomous(&mut self, task: &str) {
        let _ = self.start_brain();
        // Stehendes Ziel als Kontext voranstellen.
        let effective = match &self.goal {
            Some(g) => format!("Übergeordnetes Ziel: {g}\n\nAktuelle Aufgabe: {task}"),
            None => task.to_string(),
        };
        self.stats.tasks += 1;
        self.stats.chars_in += task.chars().count();
        self.stats.brains_used.insert(self.brain_id.clone());
        let opts = RunOptions {
            skip_brain_start: true,
            skip_brain_stop: true,
            suppress_memory_context: false,
        };
        match self.controller.run_with_options(
            &effective,
            &self.brain_id,
            self.resume.as_deref(),
            self.headless,
            opts,
        ) {
            Ok(meta) => {
                self.resume = Some(meta.run_id.clone());
                if meta.status == "done" {
                    self.stats.tasks_ok += 1;
                } else {
                    self.stats.tasks_failed += 1;
                }
                self.stats.cycles += meta.cycles;
                println!(
                    "[repl] status={} run_id={} cycles={}",
                    meta.status, meta.run_id, meta.cycles
                );
            }
            Err(e) => {
                self.stats.tasks_failed += 1;
                eprintln!("[repl] Fehler: {e}");
            }
        }
    }
}
