//! `/autoresearch.self` — Swarm-Selbstbewertung (Prioritätsfindung durch den Pool).
//!
//! Vier Phasen (siehe docs/SELF_RESEARCH_PLAN.md):
//! 1. **Sammeln:** jedes Brain liefert genau N nummerierte Verbesserungsvorschläge.
//! 2. **Konsolidieren:** EIN Orchestrator-Brain (Reliability-Auswahl) dedupliziert
//!    den Pool zu einem nummerierten Katalog distinkter Vorschläge. Schlägt das
//!    fehl → roher [`dedupe_pool`]-Fallback (die Runde scheitert nie ganz).
//! 3. **Abstimmen:** der Katalog geht an jedes Brain, das die K wichtigsten
//!    Nummern in absteigender Priorität zurückgibt ([`parse_vote_line`]).
//! 4. **Auszählen:** gewichtete Borda-Zählung ([`tally`]) plus Zustimmungshäufigkeit.
//!
//! Die reinen Helfer ([`parse_vote_line`], [`tally`], [`dedupe_pool`],
//! [`build_facts`]) sind unit-getestet; der Browser-Teil wird über die
//! `query`-Closure (in REPL/CLI aus `repl::isolated_query`) eingespeist — kein
//! echtes Brain im Test.

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Ein gerankter Vorschlag im Endergebnis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedSuggestion {
    /// 1-basierte Katalognummer.
    pub index: usize,
    /// Klartext des Vorschlags aus dem Katalog.
    pub text: String,
    /// Borda-Punkte (Platz 1 = K … Platz K = 1, aufsummiert über alle Stimmen).
    pub points: u32,
    /// Zustimmungshäufigkeit (wie viele Brains ihn überhaupt nannten).
    pub approvals: u32,
}

/// Vollständiges Ergebnis eines Self-Research-Laufs.
#[derive(Debug, Clone)]
pub struct SelfResearchReport {
    /// Nummerierter Katalog distinkter Vorschläge (1-basiert über den Index).
    pub catalog: Vec<String>,
    /// Gerankte Top-K.
    pub ranked: Vec<RankedSuggestion>,
    /// Orchestrator-Brain der Konsolidierung; `None` = Fallback-Dedupe.
    pub consolidated_by: Option<String>,
    /// Brains, die in Phase 1 Vorschläge lieferten.
    pub collected: usize,
    /// Brains, die in Phase 3 eine gültige Stimme abgaben.
    pub voters: usize,
    /// Insgesamt befragte Brains.
    pub brains_total: usize,
}

/// Extrahiert 1-basierte Nummern aus einer Stimm-Zeile, in Reihenfolge des
/// ersten Auftretens, dedupliziert und auf `1..=catalog_len` beschränkt.
///
/// Robust gegen Prosa drumherum (`«2) dann 5)»` → `[2, 5]`), Out-of-Range und
/// leere Antworten (→ leerer Vektor).
pub fn parse_vote_line(line: &str, catalog_len: usize) -> Vec<usize> {
    // Zeilenbasiert statt Blob-Scan: Brains liefern oft eine Vorrede
    // ("Thought Process", Begründungen) und erst am Ende die eigentliche
    // Nummernzeile. Ein Scan über den ganzen Text zieht die Zahlen aus der
    // Vorrede mit hinein und erzeugt einen verrauschten, zu kurzen Stimmzettel
    // (real beobachtet 2026-07-21: Rangliste mit 3 statt 10 Einträgen).
    // Deshalb: bevorzugt eine „reine" Nummernzeile verwenden, die letzte
    // gewinnt (Modelle wiederholen die finale Antwort am Schluss).
    let mut best: Option<Vec<usize>> = None;
    for raw in line.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Führendes Label abschneiden ("Meine Wahl: 4, 1, …"), damit kurze
        // Nummernzeilen nicht am Wortanteil des Labels scheitern. Nur wenn der
        // Teil vor dem ':' selbst ziffernfrei und kurz ist (echtes Label).
        let trimmed = match trimmed.split_once(':') {
            Some((head, tail))
                if head.chars().count() <= 30
                    && !head.chars().any(|c| c.is_ascii_digit())
                    && !tail.trim().is_empty() =>
            {
                tail.trim()
            }
            _ => trimmed,
        };
        let nums = extract_numbers(trimmed, catalog_len);
        if nums.len() < 3 {
            continue;
        }
        // Anteil „Stimmzettel-Zeichen" (Ziffern + übliche Trenner) an der Zeile.
        // Eine echte Nummernzeile besteht fast nur daraus; ein Listenpunkt wie
        // „3. Sandbox einführen" besteht überwiegend aus Buchstaben.
        let total = trimmed.chars().count().max(1);
        let ballotish = trimmed
            .chars()
            .filter(|c| c.is_ascii_digit() || " ,;.:|-–—>[]()\t".contains(*c))
            .count();
        if ballotish * 10 >= total * 7 {
            best = Some(nums);
        }
    }
    if let Some(v) = best {
        return v;
    }
    // Fallback: ganze Antwort scannen (altes Verhalten) — besser ein
    // verrauschter Stimmzettel als gar keiner.
    extract_numbers(line, catalog_len)
}

/// Alle gültigen Katalog-Nummern eines Textes in Vorkommensreihenfolge,
/// dedupliziert (1..=`catalog_len`).
fn extract_numbers(text: &str, catalog_len: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    let mut cur = String::new();
    // Ein abschließendes Nicht-Ziffer-Zeichen erzwingt den letzten Flush.
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(n) = cur.parse::<usize>() {
                if n >= 1 && n <= catalog_len && seen.insert(n) {
                    out.push(n);
                }
            }
            cur.clear();
        }
    }
    out
}

/// Gewichtete Borda-Auszählung: pro Stimmzettel bekommt Platz `i` (0-basiert)
/// `top_k - i` Punkte (Platz 1 = `top_k` … Platz `top_k` = 1), Einträge jenseits
/// von `top_k` zählen nicht. Rückgabe `(nummer, punkte, zustimmungen)`, sortiert
/// nach Punkten absteigend, dann Zustimmungen absteigend, dann Nummer aufsteigend
/// (letzteres nur für stabile, deterministische Reihenfolge); auf `top_k` gekürzt.
pub fn tally(votes: &[Vec<usize>], top_k: usize) -> Vec<(usize, u32, u32)> {
    let mut points: HashMap<usize, u32> = HashMap::new();
    let mut approvals: HashMap<usize, u32> = HashMap::new();
    for ballot in votes {
        for (i, &num) in ballot.iter().take(top_k).enumerate() {
            *points.entry(num).or_insert(0) += (top_k - i) as u32;
            *approvals.entry(num).or_insert(0) += 1;
        }
    }
    let mut items: Vec<(usize, u32, u32)> = points
        .iter()
        .map(|(&num, &p)| (num, p, approvals.get(&num).copied().unwrap_or(0)))
        .collect();
    items.sort_by(|a, b| {
        b.1.cmp(&a.1) // Punkte desc
            .then(b.2.cmp(&a.2)) // Zustimmungen desc
            .then(a.0.cmp(&b.0)) // Nummer asc (Stabilität)
    });
    items.truncate(top_k);
    items
}

/// Entfernt exakte Duplikate aus einem rohen Vorschlags-Pool. Der Vergleich
/// normalisiert Whitespace (kollabiert) und Groß-/Kleinschreibung; der
/// Originaltext (getrimmt) des ersten Auftretens bleibt erhalten. Leere Zeilen
/// werden übersprungen.
pub fn dedupe_pool(lines: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let norm = line.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if norm.is_empty() {
            continue;
        }
        if seen.insert(norm) {
            out.push(line.trim().to_string());
        }
    }
    out
}

/// Bündelt Projektfakten für den Sammel-Prompt: README-Auszug + neuester
/// PROGRESS.md-Abschnitt + Modulliste (`src/*.rs` mit Zeilenzahl), zeichensicher
/// auf `max_chars` gekürzt. So bewertet der Schwarm den AKTUELLEN Stand.
pub fn build_facts(
    readme: &str,
    progress: &str,
    modules: &[(String, usize)],
    max_chars: usize,
) -> String {
    let readme_excerpt: String = readme
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");
    let mut section = first_progress_section(progress);
    if section.is_empty() {
        // Kein `## `-Abschnitt gefunden → einfach die ersten Zeilen nehmen.
        section = progress
            .lines()
            .take(15)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
    }
    let module_list: String = modules
        .iter()
        .map(|(name, count)| format!("- {name} ({count})"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = String::from("# Projektfakten webagent-rs\n\n");
    if !readme_excerpt.is_empty() {
        out.push_str("## README (Auszug)\n");
        out.push_str(&readme_excerpt);
        out.push_str("\n\n");
    }
    if !section.is_empty() {
        out.push_str("## Fortschritt (neuester Abschnitt)\n");
        out.push_str(&section);
        out.push_str("\n\n");
    }
    if !module_list.is_empty() {
        out.push_str("## Module (src/*.rs, Zeilen)\n");
        out.push_str(&module_list);
        out.push('\n');
    }
    let out = out.trim_end().to_string();
    crate::char_prefix(&out, max_chars).to_string()
}

/// Erster `## `-Abschnitt der PROGRESS.md (der neueste — die Datei wird oben
/// erweitert). Inklusive Überschrift, bis zur nächsten `## `-Zeile oder Dateiende.
fn first_progress_section(progress: &str) -> String {
    let mut section: Vec<&str> = Vec::new();
    let mut in_section = false;
    for line in progress.lines() {
        if line.starts_with("## ") {
            if in_section {
                break;
            }
            in_section = true;
            section.push(line);
        } else if in_section {
            section.push(line);
        }
    }
    section.join("\n").trim().to_string()
}

/// Zerlegt eine Brain-Antwort in einzelne Vorschlags-Zeilen: nicht-leere Zeilen,
/// ohne führenden Listen-Marker (`1.`, `2)`, `-`, `*`, `•`).
pub fn parse_suggestions(response: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in response.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let cleaned = strip_list_marker(line);
        if !cleaned.is_empty() {
            out.push(cleaned.to_string());
        }
    }
    out
}

/// Entfernt einen führenden Listen-Marker (`12. `, `3) `, `- `, `* `, `• `).
fn strip_list_marker(line: &str) -> &str {
    let t = line.trim_start();
    let bytes = t.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b')') {
        return t[i + 1..].trim_start();
    }
    t.trim_start_matches(['-', '*', '•']).trim_start()
}

/// Modulliste aus einem `src/`-Verzeichnis: `*.rs`-Dateien mit Zeilenzahl,
/// alphabetisch. I/O-behaftet, daher nicht unit-getestet (der Orchestrator übt
/// es e2e). Fehlt das Verzeichnis, kommt eine leere Liste zurück.
pub fn collect_modules(src_dir: &Path) -> Vec<(String, usize)> {
    let mut mods: Vec<(String, usize)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(src_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    mods.push((name, text.lines().count()));
                }
            }
        }
    }
    mods.sort_by(|a, b| a.0.cmp(&b.0));
    mods
}

/// Liest README.md + PROGRESS.md + `src/*.rs` unter `root` und baut daraus die
/// Projektfakten ([`build_facts`]). Fehlende Dateien → leerer Beitrag.
pub fn gather_facts(root: &Path, max_chars: usize) -> String {
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap_or_default();
    let progress = std::fs::read_to_string(root.join("PROGRESS.md")).unwrap_or_default();
    let modules = collect_modules(&root.join("src"));
    build_facts(&readme, &progress, &modules, max_chars)
}

/// Nummerierte Liste (`1. …`) für Prompts.
fn number_list(items: &[String]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Orchestrator-Auswahl per Reliability unter den Antwortenden (wie `run_swarm`);
/// unbekannte Brains gelten als 0.5, Fallback ist das erste antwortende Brain.
fn pick_orchestrator(answered: &[String]) -> String {
    let board = crate::brain_score::leaderboard();
    let score = |id: &str| -> f64 {
        board
            .iter()
            .find(|s| s.brain_id == id)
            .map(|s| s.reliability)
            .unwrap_or(0.5)
    };
    answered
        .iter()
        .max_by(|a, b| {
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
        .unwrap_or_default()
}

/// Fährt die vier Phasen und liefert den [`SelfResearchReport`]. Der Browser-Teil
/// steckt in `query(brain, prompt) -> Result<antwort, fehler>` — in REPL/CLI die
/// isolierte Swarm-Abfrage (`repl::isolated_query`). Fortschritt druckt live.
pub fn run_self_research<Q>(
    brains: &[String],
    facts: &str,
    suggestions: usize,
    top: usize,
    query: Q,
) -> SelfResearchReport
where
    Q: Fn(&str, &str) -> Result<String, String>,
{
    let n = suggestions.max(1);
    let k = top.max(1);
    let total = brains.len();

    // ---- Phase 1: Sammeln ----
    println!("[self-research] Phase 1/4 — {total} Brains sammeln je {n} Vorschläge…");
    let collect_prompt = format!(
        "{facts}\n\nBewerte das Projekt oben. Nenne GENAU {n} konkrete, umsetzbare \
         Verbesserungen für den nächsten Schritt — als nummerierte Liste (1. … {n}. …), \
         ein Vorschlag pro Zeile, knapp und konkret. Keine Einleitung, kein Nachwort."
    );
    let mut pool: Vec<String> = Vec::new();
    let mut answered: Vec<String> = Vec::new();
    for (i, b) in brains.iter().enumerate() {
        match query(b, &collect_prompt) {
            Ok(resp) => {
                let items = parse_suggestions(&resp);
                println!(
                    "[self-research] sammeln {}/{total} — {b}: {} Vorschläge",
                    i + 1,
                    items.len()
                );
                if !items.is_empty() {
                    answered.push(b.clone());
                    pool.extend(items);
                }
            }
            Err(e) => println!("[self-research] sammeln {}/{total} — {b}: — {e}", i + 1),
        }
    }
    if pool.is_empty() {
        println!("[self-research] keine Vorschläge gesammelt — Abbruch.");
        return SelfResearchReport {
            catalog: Vec::new(),
            ranked: Vec::new(),
            consolidated_by: None,
            collected: 0,
            voters: 0,
            brains_total: total,
        };
    }

    // ---- Phase 2: Konsolidieren (mit Fallback) ----
    let orch = pick_orchestrator(&answered);
    let consolidate_prompt = format!(
        "Hier sind gesammelte Verbesserungsvorschläge (teils Dubletten):\n\n{pool}\n\n\
         Fasse Duplikate zusammen und gib EINE nummerierte Liste distinkter, klar \
         formulierter Vorschläge (1. … pro Zeile). Keine Einleitung, kein Nachwort.",
        pool = number_list(&pool)
    );
    let (catalog, consolidated_by) = match query(&orch, &consolidate_prompt) {
        Ok(resp) => {
            let cat = parse_suggestions(&resp);
            if cat.is_empty() {
                println!("[self-research] Konsolidierung via {orch} leer → roher Pool (dedupe).");
                (dedupe_pool(&pool), None)
            } else {
                println!(
                    "[self-research] konsolidieren via {orch} … {} distinkte Vorschläge",
                    cat.len()
                );
                (cat, Some(orch.clone()))
            }
        }
        Err(e) => {
            println!(
                "[self-research] Konsolidierung via {orch} fehlgeschlagen ({e}) → roher Pool (dedupe)."
            );
            (dedupe_pool(&pool), None)
        }
    };
    if catalog.is_empty() {
        return SelfResearchReport {
            catalog,
            ranked: Vec::new(),
            consolidated_by,
            collected: answered.len(),
            voters: 0,
            brains_total: total,
        };
    }

    // ---- Phase 3: Abstimmen ----
    println!("[self-research] Phase 3/4 — {total} Brains stimmen ab (Katalog: {} Einträge)…", catalog.len());
    let vote_prompt = format!(
        "Katalog von Verbesserungsvorschlägen:\n\n{cat}\n\n\
         Wähle die {k} WICHTIGSTEN. Antworte NUR mit den Nummern in absteigender \
         Priorität (wichtigste zuerst), z.B. «3, 1, 7». Keine Begründung.",
        cat = number_list(&catalog)
    );
    let mut ballots: Vec<Vec<usize>> = Vec::new();
    let mut voters = 0usize;
    for (i, b) in brains.iter().enumerate() {
        match query(b, &vote_prompt) {
            Ok(resp) => {
                let ballot = parse_vote_line(&resp, catalog.len());
                if !ballot.is_empty() {
                    voters += 1;
                }
                println!(
                    "[self-research] abstimmen {}/{total} — {b}: {} Stimmen",
                    i + 1,
                    ballot.len()
                );
                ballots.push(ballot);
            }
            Err(e) => println!("[self-research] abstimmen {}/{total} — {b}: — {e}", i + 1),
        }
    }

    // ---- Phase 4: Auszählen ----
    let tallied = tally(&ballots, k);
    let ranked: Vec<RankedSuggestion> = tallied
        .iter()
        .map(|&(num, pts, appr)| RankedSuggestion {
            index: num,
            text: catalog.get(num - 1).cloned().unwrap_or_default(),
            points: pts,
            approvals: appr,
        })
        .collect();
    println!("[self-research] Phase 4/4 — Rangliste (Top {}):", ranked.len());
    for (rank, r) in ranked.iter().enumerate() {
        println!(
            "   {}. {} Pkt · {} Stimmen — {}",
            rank + 1,
            r.points,
            r.approvals,
            r.text
        );
    }

    SelfResearchReport {
        catalog,
        ranked,
        consolidated_by,
        collected: answered.len(),
        voters,
        brains_total: total,
    }
}

/// Markdown-Body für die Wiki-Ablage (`self-research-<stamp>`).
pub fn format_report(report: &SelfResearchReport) -> String {
    let src = match &report.consolidated_by {
        Some(b) => format!("konsolidiert via {b}"),
        None => "roher Pool (Fallback-Dedupe)".to_string(),
    };
    let mut out = format!(
        "Swarm-Selbstbewertung: {}/{} Brains lieferten Vorschläge, {} stimmten ab; {} ({} Katalog-Einträge).\n\n",
        report.collected, report.brains_total, report.voters, src, report.catalog.len()
    );
    out.push_str("## Rangliste\n");
    if report.ranked.is_empty() {
        out.push_str("(keine Stimmen)\n");
    } else {
        for (rank, r) in report.ranked.iter().enumerate() {
            out.push_str(&format!(
                "{}. [{} Punkte, {} Stimmen] {}\n",
                rank + 1,
                r.points,
                r.approvals,
                r.text
            ));
        }
    }
    out.push_str("\n## Katalog\n");
    for (i, c) in report.catalog.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, c));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vote_line_cases() {
        // Ziffern in Reihenfolge.
        assert_eq!(parse_vote_line("3, 1, 7", 10), vec![3, 1, 7]);
        // Duplikate raus, Reihenfolge des ersten Auftretens.
        assert_eq!(parse_vote_line("1. Foo 1. Foo 2. Bar", 10), vec![1, 2]);
        // Out-of-Range (12 > 5) fällt weg.
        assert_eq!(parse_vote_line("ich wähle 12 und 3", 5), vec![3]);
        // Prosa ohne Zahlen → leer.
        assert_eq!(parse_vote_line("keine zahlen hier", 5), Vec::<usize>::new());
        // Leere Antwort → leer.
        assert_eq!(parse_vote_line("", 5), Vec::<usize>::new());
        // Klammer-Marker und Prosa drumherum.
        assert_eq!(parse_vote_line("Priorität: 2) dann 5) dann 2)", 8), vec![2, 5]);
        // catalog_len 0 akzeptiert nichts.
        assert_eq!(parse_vote_line("1 2 3", 0), Vec::<usize>::new());
    }

    #[test]
    fn parse_vote_line_prefers_pure_number_line_over_prose() {
        // Realer Fehlerfall 2026-07-21: Vorrede + Begründungen mit Zahlen
        // verrauschten den Stimmzettel. Die reine Nummernzeile muss gewinnen.
        let answer = "Thought Process\n\
                      Ich halte Punkt 9 fuer wichtig, ausserdem spricht 2026 dafuer.\n\
                      Meine Wahl: 4, 1, 6, 10, 2, 16, 5, 8, 12, 7";
        assert_eq!(
            parse_vote_line(answer, 22),
            vec![4, 1, 6, 10, 2, 16, 5, 8, 12, 7]
        );
    }

    #[test]
    fn parse_vote_line_ignores_markdown_enumeration_lines() {
        // "3. Sandbox einfuehren" ist ein Listenpunkt, KEIN Stimmzettel:
        // solche Zeilen duerfen die reine Nummernzeile nicht verdraengen.
        let answer = "1. Sandbox fuer Shell-Aktionen einfuehren\n\
                      2. Protokoll versionieren und validieren\n\
                      3. Brain-Trait fuer Tests definieren\n\
                      7, 3, 1";
        assert_eq!(parse_vote_line(answer, 10), vec![7, 3, 1]);
    }

    #[test]
    fn parse_vote_line_last_number_line_wins() {
        // Modelle wiederholen die finale Antwort haeufig am Schluss.
        let answer = "Entwurf: 1, 2, 3\nKorrigiert: 5, 6, 7";
        assert_eq!(parse_vote_line(answer, 10), vec![5, 6, 7]);
    }

    #[test]
    fn parse_vote_line_falls_back_when_no_clean_line() {
        // Kein sauberer Stimmzettel vorhanden → altes Blob-Verhalten als
        // Rueckfallebene (lieber verrauscht als gar nichts).
        assert_eq!(parse_vote_line("ich nehme 4 und dann 9", 10), vec![4, 9]);
    }

    #[test]
    fn tally_borda_and_tiebreak() {
        // Borda + Gleichstand: 1 und 2 je 5 Punkte / 2 Zustimmungen → Nummer asc.
        let votes = vec![vec![1, 2, 3], vec![2, 1, 3]];
        assert_eq!(tally(&votes, 3), vec![(1, 5, 2), (2, 5, 2), (3, 2, 2)]);

        // Zustimmungs-Tiebreaker greift bei Punktgleichstand nicht nötig, aber
        // Ranking bleibt korrekt: 1 (3 Pkt, 2 Zust.) vor 2 (2 Pkt, 1 Zust.).
        let votes2 = vec![vec![1], vec![2, 1]];
        assert_eq!(tally(&votes2, 2), vec![(1, 3, 2), (2, 2, 1)]);

        // Einträge jenseits top_k zählen nicht.
        let votes3 = vec![vec![1, 2, 3, 4]];
        assert_eq!(tally(&votes3, 2), vec![(1, 2, 1), (2, 1, 1)]);

        // Leere Eingabe → leeres Ergebnis.
        assert!(tally(&[], 3).is_empty());
    }

    #[test]
    fn dedupe_pool_normalizes() {
        let lines = vec![
            "Fix the bug".to_string(),
            "fix   the BUG".to_string(), // Dublette via Case/Whitespace
            "  Add tests ".to_string(),
            "".to_string(),
            "Add tests".to_string(),
        ];
        assert_eq!(
            dedupe_pool(&lines),
            vec!["Fix the bug".to_string(), "Add tests".to_string()]
        );
    }

    #[test]
    fn parse_suggestions_strips_markers() {
        let resp = "1. Erstes\n2) Zweites\n- Drittes\n* Viertes\n• Fünftes\n\n   ";
        assert_eq!(
            parse_suggestions(resp),
            vec!["Erstes", "Zweites", "Drittes", "Viertes", "Fünftes"]
        );
    }

    #[test]
    fn build_facts_bundles_and_caps() {
        let readme = "# webagent\nZeile2\n\nZeile3";
        let progress = "# P\n\n## 2026-07-21 neu\nA\nB\n\n## 2026-07-20 alt\nX";
        let modules = vec![("brain.rs".to_string(), 100), ("repl.rs".to_string(), 200)];
        let facts = build_facts(readme, progress, &modules, 10_000);
        assert!(facts.contains("# webagent"), "readme fehlt: {facts}");
        assert!(facts.contains("2026-07-21 neu"), "neuester Abschnitt fehlt");
        assert!(
            !facts.contains("2026-07-20 alt"),
            "nur der neueste Abschnitt gehört rein: {facts}"
        );
        assert!(facts.contains("brain.rs (100)"), "modul fehlt");
        // Cap greift zeichensicher.
        let capped = build_facts(readme, progress, &modules, 20);
        assert!(capped.chars().count() <= 20, "cap verletzt: {capped}");
    }

    #[test]
    fn first_progress_section_takes_newest() {
        let progress = "# T\n\n## neu\nA\n## alt\nB";
        assert_eq!(first_progress_section(progress), "## neu\nA");
        assert_eq!(first_progress_section("keine sektion"), "");
    }

    #[test]
    fn orchestration_end_to_end_with_mock() {
        // Kein echtes Brain: die Closure antwortet je nach Phase (am Prompt erkannt).
        let brains = vec!["a".to_string(), "b".to_string()];
        let query = |_b: &str, prompt: &str| -> Result<String, String> {
            if prompt.contains("distinkte") {
                Ok("1. Alpha\n2. Beta\n3. Gamma".to_string())
            } else if prompt.contains("WICHTIGSTEN") {
                Ok("2, 1, 3".to_string())
            } else {
                Ok("1. Alpha\n2. Beta".to_string())
            }
        };
        let report = run_self_research(&brains, "# facts", 2, 3, query);
        assert_eq!(report.catalog, vec!["Alpha", "Beta", "Gamma"]);
        assert_eq!(report.collected, 2);
        assert_eq!(report.voters, 2);
        assert!(report.consolidated_by.is_some());
        // Beide Stimmzettel [2,1,3], top_k=3: num2=3+3=6, num1=2+2=4, num3=1+1=2.
        assert_eq!(report.ranked[0].index, 2);
        assert_eq!(report.ranked[0].points, 6);
        assert_eq!(report.ranked[0].text, "Beta");
        assert_eq!(report.ranked[1].index, 1);
        assert_eq!(report.ranked[2].index, 3);
    }

    #[test]
    fn consolidation_failure_falls_back_to_dedupe() {
        let brains = vec!["a".to_string(), "b".to_string()];
        let query = |_b: &str, prompt: &str| -> Result<String, String> {
            if prompt.contains("distinkte") {
                Err("boom".to_string()) // Konsolidierung scheitert
            } else if prompt.contains("WICHTIGSTEN") {
                Ok("1".to_string())
            } else {
                Ok("1. Same\n2. same  ".to_string()) // beide Brains identisch
            }
        };
        let report = run_self_research(&brains, "f", 2, 2, query);
        // Fallback greift: Katalog aus dedupe_pool (Case/Whitespace normalisiert).
        assert!(report.consolidated_by.is_none());
        assert_eq!(report.catalog, vec!["Same"]);
        assert_eq!(report.ranked[0].index, 1);
        assert_eq!(report.ranked[0].text, "Same");
    }

    #[test]
    fn no_suggestions_aborts_cleanly() {
        let brains = vec!["a".to_string()];
        let query = |_b: &str, _p: &str| -> Result<String, String> { Err("blockiert".to_string()) };
        let report = run_self_research(&brains, "f", 3, 3, query);
        assert!(report.catalog.is_empty());
        assert!(report.ranked.is_empty());
        assert_eq!(report.collected, 0);
    }

    #[test]
    fn format_report_shows_ranking_and_catalog() {
        let report = SelfResearchReport {
            catalog: vec!["Alpha".to_string(), "Beta".to_string()],
            ranked: vec![RankedSuggestion {
                index: 2,
                text: "Beta".to_string(),
                points: 6,
                approvals: 2,
            }],
            consolidated_by: Some("claude".to_string()),
            collected: 2,
            voters: 2,
            brains_total: 3,
        };
        let body = format_report(&report);
        assert!(body.contains("konsolidiert via claude"));
        assert!(body.contains("[6 Punkte, 2 Stimmen] Beta"));
        assert!(body.contains("## Katalog"));
        assert!(body.contains("2. Beta"));
    }
}
