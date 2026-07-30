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
        let norm = line
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
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
/// `true`, wenn eine Zeile als echter Verbesserungsvorschlag durchgeht.
///
/// Filtert den Müll heraus, der sonst in den Katalog rutscht und sogar die
/// Abstimmung gewinnen kann. Real beobachtet 2026-07-21: in zwei von drei
/// Benchmark-Runden lautete der Sieger
/// `SyntaxError: Unexpected token '<', "<!doctypeh"... is not valid JSON`
/// — eine JavaScript-Fehlermeldung aus einer Brain-Oberfläche. Beide Runden
/// waren dadurch wertlos. Ebenso landete „Thought Process" schon als Vorschlag
/// auf Platz 9 einer Rangliste.
pub fn is_plausible_suggestion(line: &str) -> bool {
    let t = line.trim();
    // Zu kurz für einen sinnvollen Vorschlag, oder absurd lang (Antwortblock).
    let n = t.chars().count();
    if !(20..=400).contains(&n) {
        return false;
    }
    let low = t.to_lowercase();
    // Technische Fehler-/UI-Artefakte statt Inhalt.
    const JUNK: &[&str] = &[
        "syntaxerror",
        "typeerror",
        "referenceerror",
        "<!doctype",
        "<html",
        "is not valid json",
        "unexpected token",
        "stack trace",
        "traceback",
        "http error",
        "err_",
        "thought process",
        "thinking…",
        "reasoning:",
        // Ausfall-/Wartemeldungen der Brain-Oberflaechen. "No response, Please
        // try again later." gewann am 2026-07-21 eine echte Abstimmung: 38
        // Zeichen, reiner Fliesstext — die Laengen- und Buchstabenpruefung
        // laesst so etwas anstandslos durch, nur die Wortliste faengt es.
        "no response",
        "try again later",
        "please try again",
        "something went wrong",
        "service unavailable",
        "too many requests",
        "failed to fetch",
        "network error",
    ];
    if JUNK.iter().any(|j| low.contains(j)) {
        return false;
    }
    // Muss überwiegend Fließtext sein (Fehlermeldungen/HTML sind zeichenlastig).
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    letters * 10 >= n * 6
}

pub fn parse_suggestions(response: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in response.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let cleaned = strip_list_marker(line);
        if is_plausible_suggestion(cleaned) {
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
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
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
    let mut facts = build_facts(&readme, &progress, &modules, max_chars);

    // Vorhandene oeffentliche API anhaengen. Ohne sie schlug der Schwarm
    // wiederholt Dinge vor, die es LAENGST gibt (striktes Schema, error_code,
    // format_audit_line) — die Brains antworteten dann korrekt "ist bereits
    // implementiert", taten nichts, und wurden als Fehlschlag gewertet
    // (22/22 did_change=false, Messung 2026-07-21).
    let api = collect_public_api(&root.join("src"));
    if !api.is_empty() {
        facts.push_str(
            "\n\nBEREITS VORHANDENE oeffentliche Funktionen (NICHT erneut vorschlagen): ",
        );
        facts.push_str(crate::char_prefix(&api.join(", "), 900));
    }
    facts
}

/// Namen aller `pub fn` unter `src/` — sortiert und dedupliziert.
/// Dient als "das gibt es schon"-Signal fuer Abstimmung und Verfeinerung.
pub fn collect_public_api(src: &Path) -> Vec<String> {
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(src) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("pub fn ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    names.insert(name);
                }
            }
        }
    }
    names.into_iter().collect()
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
/// Befragt mehrere Brains NEBENLÄUFIG mit demselben Prompt, höchstens `limit`
/// gleichzeitig. Ergebnisse kommen in Eingabereihenfolge zurück, unabhängig
/// davon, wer zuerst fertig wird.
///
/// Sammeln und Abstimmen sind reine Lesephasen — die Brains teilen sich nichts
/// als den Prompt, jede Abfrage bekommt ohnehin ihre eigene Browser-Profilkopie.
/// Sequenziell summierte sich das auf `Anzahl Brains × Antwortzeit`, obwohl die
/// Wartezeit fast vollständig aus dem Warten auf die Antwort besteht.
///
/// `limit` ist bewusst begrenzbar: jede Abfrage startet einen eigenen
/// WebView2-Browser, acht davon gleichzeitig kosten spürbar RAM.
pub fn query_parallel<Q>(
    brains: &[String],
    prompt: &str,
    limit: usize,
    on_done: &(dyn Fn(usize, usize) + Sync),
    query: &Q,
) -> Vec<(String, Result<String, String>)>
where
    Q: Fn(&str, &str) -> Result<String, String> + Sync,
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let total = brains.len();
    let slots: Vec<Mutex<Option<Result<String, String>>>> =
        (0..total).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let workers = limit.clamp(1, total.max(1));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= total {
                    break;
                }
                let r = query(&brains[i], prompt);
                if let Ok(mut g) = slots[i].lock() {
                    *g = Some(r);
                }
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                on_done(d, total);
            });
        }
    });

    brains
        .iter()
        .zip(slots)
        .map(|(b, slot)| {
            let r = slot
                .into_inner()
                .ok()
                .flatten()
                .unwrap_or_else(|| Err("kein Ergebnis".to_string()));
            (b.clone(), r)
        })
        .collect()
}

pub fn run_self_research<Q>(
    brains: &[String],
    facts: &str,
    suggestions: usize,
    top: usize,
    parallel: usize,
    query: Q,
) -> SelfResearchReport
where
    Q: Fn(&str, &str) -> Result<String, String> + Sync,
{
    let n = suggestions.max(1);
    let k = top.max(1);
    let total = brains.len();

    // ---- Phase 1: Sammeln ----
    crate::bench_events::info_line(&format!(
        "[self-research] Phase 1/4 — {total} Brains sammeln je {n} Vorschläge…"
    ));
    // Zuschnitt der Vorschläge: klein genug, um sie auch zu BAUEN.
    //
    // Gemessen am 30.07.2026: der Schwarm wählte Brocken wie „zentraler
    // JSON-Sanitizer mit Codefence-Strip, BOM-Check, UTF-8-Recovery,
    // Truncation-Guard, Rohantwort-Logging und strukturiertem Fehlertyp" in
    // einer Datei mit 1567 Zeilen. In neun Läufen gab es daraufhin NULL
    // Datei-Aktionen: zai verbrauchte alle 15 Zyklen damit, die Datei
    // scheibchenweise zu lesen (`Get-Content`/`Select-String`, Schritt 1 bis 8),
    // und kam nie zum Editieren.
    //
    // Die Planungsphase verlangt längst „genau EINE Zieldatei"; der Brocken
    // entsteht also früher, hier. Ein Vorschlag, der nicht in wenige Schritte
    // passt, ist im Benchmark kein Vorschlag, sondern eine Sackgasse — er kostet
    // eine ganze Runde und liefert keinen Messpunkt.
    let collect_prompt = format!(
        "{facts}\n\nBewerte das Projekt oben. Nenne GENAU {n} konkrete, umsetzbare \
         Verbesserungen für den nächsten Schritt — als nummerierte Liste (1. … {n}. …), \
         ein Vorschlag pro Zeile, knapp und konkret. Keine Einleitung, kein Nachwort.\n\n\
         ZUSCHNITT (wichtig): Jeder Vorschlag muss in EINER Datei umsetzbar sein und \
         in wenigen Schritten fertig werden — eine Funktion, ein Gate, ein Test, eine \
         Fehlermeldung. KEINE Sammelpakete (A, B, C und D in einem Zug), keine Umbauten \
         quer über mehrere Module, keine neuen Subsysteme. Wer mehr will, nennt den \
         kleinsten Teilschritt, der für sich allein baut und testbar ist."
    );
    let mut pool: Vec<String> = Vec::new();
    let mut answered: Vec<String> = Vec::new();
    {
        let t = crate::StageTimer::start(format!("sammeln — {total} Brains, {parallel} parallel"));
        let note = t.note_handle();
        let results = query_parallel(
            brains,
            &collect_prompt,
            parallel,
            &|done, tot| note.set(&format!("{done}/{tot} geantwortet")),
            &query,
        );
        t.finish(&format!("{total} Antworten"));
        for (b, r) in results {
            match r {
                Ok(resp) => {
                    let items = parse_suggestions(&resp);
                    let count_ok = items.len() == n;
                    let compliance = if count_ok {
                        format!("Vorschlagsanzahl erfüllt ({}/{n})", items.len())
                    } else {
                        format!("Vorschlagsanzahl abweichend: {}/{}", items.len(), n)
                    };
                    // Die verlangte Anzahl ist ein klarer, günstiger Test für
                    // Instruktionsbefolgung. Als einzelnes Reliability-Event
                    // wirkt die Abweichung nur leicht, bleibt aber dauerhaft
                    // nachvollziehbar und kann technische Qualität nicht
                    // überstimmen.
                    crate::brain_score::record_event(
                        &b,
                        count_ok,
                        Some(&format!("self_research: {compliance}")),
                        0,
                        resp.len(),
                    );
                    crate::bench_events::emit(
                        if count_ok {
                            crate::bench_events::Level::Pass
                        } else {
                            crate::bench_events::Level::Warn
                        },
                        Some(&b),
                        &compliance,
                    );
                    crate::bench_events::info_line(&format!(
                        "[self-research] sammeln — {b}: {} Vorschläge",
                        items.len()
                    ));
                    if !items.is_empty() {
                        answered.push(b);
                        pool.extend(items);
                    }
                }
                Err(e) => {
                    crate::bench_events::info_line(&format!("[self-research] sammeln — {b}: — {e}"))
                }
            }
        }
    }
    if pool.is_empty() {
        crate::bench_events::info_line("[self-research] keine Vorschläge gesammelt — Abbruch.");
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
    let _tc = crate::StageTimer::start(format!("konsolidieren via {orch}"));
    let __rc = query(&orch, &consolidate_prompt);
    _tc.finish("Antwort da");
    let (catalog, consolidated_by) = match __rc {
        Ok(resp) => {
            let cat = parse_suggestions(&resp);
            if cat.is_empty() {
                crate::bench_events::info_line(&format!(
                    "[self-research] Konsolidierung via {orch} leer → roher Pool (dedupe)."
                ));
                (dedupe_pool(&pool), None)
            } else {
                crate::bench_events::info_line(&format!(
                    "[self-research] konsolidieren via {orch} … {} distinkte Vorschläge",
                    cat.len()
                ));
                (cat, Some(orch.clone()))
            }
        }
        Err(e) => {
            crate::bench_events::info_line(&format!(
                "[self-research] Konsolidierung via {orch} fehlgeschlagen ({e}) → roher Pool (dedupe)."
            ));
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
    crate::bench_events::info_line(&format!(
        "[self-research] Phase 3/4 — {total} Brains stimmen ab (Katalog: {} Einträge)…",
        catalog.len()
    ));
    let vote_prompt = format!(
        "Katalog von Verbesserungsvorschlägen:\n\n{cat}\n\n\
         Wähle die {k} WICHTIGSTEN. Antworte NUR mit den Nummern in absteigender \
         Priorität (wichtigste zuerst), z.B. «3, 1, 7». Keine Begründung.",
        cat = number_list(&catalog)
    );
    let mut ballots: Vec<Vec<usize>> = Vec::new();
    let mut voters = 0usize;
    {
        let t =
            crate::StageTimer::start(format!("abstimmen — {total} Brains, {parallel} parallel"));
        let note = t.note_handle();
        let results = query_parallel(
            brains,
            &vote_prompt,
            parallel,
            &|done, tot| note.set(&format!("{done}/{tot} abgestimmt")),
            &query,
        );
        t.finish(&format!("{total} Stimmzettel"));
        for (b, r) in results {
            match r {
                Ok(resp) => {
                    let ballot = parse_vote_line(&resp, catalog.len());
                    if !ballot.is_empty() {
                        voters += 1;
                    }
                    crate::bench_events::info_line(&format!(
                        "[self-research] abstimmen — {b}: {} Stimmen",
                        ballot.len()
                    ));
                    ballots.push(ballot);
                }
                Err(e) => crate::bench_events::info_line(&format!(
                    "[self-research] abstimmen — {b}: — {e}"
                )),
            }
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
    crate::bench_events::info_line(&format!(
        "[self-research] Phase 4/4 — Rangliste (Top {}):",
        ranked.len()
    ));
    for (rank, r) in ranked.iter().enumerate() {
        crate::bench_events::info_line(&format!(
            "   {}. {} Pkt · {} Stimmen — {}",
            rank + 1,
            r.points,
            r.approvals,
            r.text
        ));
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

pub fn borda_aggregate(phases: Vec<Vec<Vec<String>>>) -> Vec<(String, i64)> {
    use std::collections::HashMap;

    let mut scores: HashMap<String, i64> = HashMap::new();

    for phase in phases {
        for ranking in phase {
            let n = ranking.len() as i64;
            for (rank_index, proposal) in ranking.into_iter().enumerate() {
                let points = n - rank_index as i64;
                *scores.entry(proposal).or_insert(0) += points;
            }
        }
    }

    let mut result: Vec<(String, i64)> = scores.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_borda_aggregate_single_ranking() {
        let result = borda_aggregate(vec![vec![vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
        ]]]);
        assert_eq!(
            result,
            vec![
                ("A".to_string(), 3),
                ("B".to_string(), 2),
                ("C".to_string(), 1)
            ]
        );
    }

    #[test]
    fn test_borda_aggregate_tie_sorted_lexicographically() {
        let result = borda_aggregate(vec![vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["B".to_string(), "A".to_string()],
        ]]);
        assert_eq!(result, vec![("A".to_string(), 3), ("B".to_string(), 3)]);
    }

    #[test]
    fn test_borda_aggregate_multiple_phases_sum_scores() {
        let result = borda_aggregate(vec![
            vec![vec!["A".to_string(), "B".to_string()]],
            vec![vec!["B".to_string(), "A".to_string()]],
        ]);
        assert_eq!(result, vec![("A".to_string(), 3), ("B".to_string(), 3)]);
    }

    #[test]
    fn test_borda_aggregate_different_ranking_lengths() {
        let result = borda_aggregate(vec![vec![
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            vec!["A".to_string(), "B".to_string()],
        ]]);
        assert_eq!(result[0], ("A".to_string(), 5));
        assert_eq!(result[1], ("B".to_string(), 3));
        assert_eq!(result[2], ("C".to_string(), 1));
    }

    #[test]
    fn test_borda_aggregate_empty_input() {
        let result = borda_aggregate(vec![vec![], vec![]]);
        assert!(result.is_empty());
    }

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
        assert_eq!(
            parse_vote_line("Priorität: 2) dann 5) dann 2)", 8),
            vec![2, 5]
        );
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
    fn plausible_suggestion_filtert_muell() {
        // Der reale Uebeltaeter: eine JS-Fehlermeldung gewann zwei Abstimmungen.
        assert!(!is_plausible_suggestion(
            "SyntaxError: Unexpected token '<', \"<!doctypeh\"... is not valid JSON"
        ));
        assert!(!is_plausible_suggestion("Thought Process"));
        assert!(!is_plausible_suggestion(
            "<html><body>Error 502</body></html>"
        ));
        assert!(!is_plausible_suggestion("kurz")); // zu kurz
        assert!(!is_plausible_suggestion(&"x".repeat(500))); // absurd lang
                                                             // Echte Vorschlaege muessen durchkommen.
        assert!(is_plausible_suggestion(
            "Zentrale thiserror-Fehlerhierarchie einfuehren statt verstreuter String-Fehler"
        ));
        assert!(is_plausible_suggestion(
            "stdout-Logs durch tracing mit strukturierten Spans pro Run ersetzen"
        ));
    }

    #[test]
    fn parse_suggestions_filtert_muell_aus_der_liste() {
        let resp = "1. Zentrale Fehlerhierarchie mit thiserror einfuehren\n\
                    2. SyntaxError: Unexpected token '<' is not valid JSON\n\
                    3. Strukturiertes Logging via tracing-Crate ergaenzen\n\
                    Thought Process";
        let got = parse_suggestions(resp);
        assert_eq!(got.len(), 2, "Muell muss raus: {got:?}");
        assert!(got[0].contains("thiserror"));
        assert!(got[1].contains("tracing"));
    }

    #[test]
    fn parse_suggestions_strips_markers() {
        // Realistische Laengen: parse_suggestions filtert seit 2026-07-21 auch
        // Muell (zu kurz / Fehlermeldungen); Spielzeugstrings wie "Erstes" waeren
        // kein plausibler Vorschlag mehr.
        let resp = "1. Fehlerhierarchie mit thiserror einfuehren\n\
                    2) Strukturiertes Logging via tracing ergaenzen\n\
                    - Protokoll-Schema strikt validieren beim Parsen\n\
                    * Worker-Pool um Prioritaets-Queue erweitern\n\
                    • Wiki-Suche auf semantisches Retrieval umstellen\n\n   ";
        assert_eq!(
            parse_suggestions(resp),
            vec![
                "Fehlerhierarchie mit thiserror einfuehren",
                "Strukturiertes Logging via tracing ergaenzen",
                "Protokoll-Schema strikt validieren beim Parsen",
                "Worker-Pool um Prioritaets-Queue erweitern",
                "Wiki-Suche auf semantisches Retrieval umstellen"
            ]
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
            // Realistische Laengen: parse_suggestions filtert seit 2026-07-21
            // implausible Zeilen (zu kurz / Fehlermeldungen) heraus.
            if prompt.contains("distinkte") {
                Ok("1. Fehlerhierarchie mit thiserror einfuehren\n\
                    2. Strukturiertes Logging via tracing ergaenzen\n\
                    3. Protokoll strikt validieren beim Parsen"
                    .to_string())
            } else if prompt.contains("WICHTIGSTEN") {
                Ok("2, 1, 3".to_string())
            } else {
                Ok("1. Fehlerhierarchie mit thiserror einfuehren\n\
                    2. Strukturiertes Logging via tracing ergaenzen"
                    .to_string())
            }
        };
        let report = run_self_research(&brains, "# facts", 2, 3, 1, query);
        assert_eq!(
            report.catalog,
            vec![
                "Fehlerhierarchie mit thiserror einfuehren",
                "Strukturiertes Logging via tracing ergaenzen",
                "Protokoll strikt validieren beim Parsen"
            ]
        );
        assert_eq!(report.collected, 2);
        assert_eq!(report.voters, 2);
        assert!(report.consolidated_by.is_some());
        // Beide Stimmzettel [2,1,3], top_k=3: num2=3+3=6, num1=2+2=4, num3=1+1=2.
        assert_eq!(report.ranked[0].index, 2);
        assert_eq!(report.ranked[0].points, 6);
        assert_eq!(
            report.ranked[0].text,
            "Strukturiertes Logging via tracing ergaenzen"
        );
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
                // Identisch bis auf Case/Whitespace (dedupe_pool normalisiert),
                // aber realistisch lang — sonst greift der Plausibilitaetsfilter.
                Ok("1. Fehlerhierarchie mit thiserror einfuehren\n\
                    2. fehlerhierarchie MIT thiserror einfuehren  "
                    .to_string())
            }
        };
        let report = run_self_research(&brains, "f", 2, 2, 1, query);
        // Fallback greift: Katalog aus dedupe_pool (Case/Whitespace normalisiert).
        assert!(report.consolidated_by.is_none());
        assert_eq!(
            report.catalog,
            vec!["Fehlerhierarchie mit thiserror einfuehren"]
        );
        assert_eq!(report.ranked[0].index, 1);
        assert_eq!(
            report.ranked[0].text,
            "Fehlerhierarchie mit thiserror einfuehren"
        );
    }

    #[test]
    fn no_suggestions_aborts_cleanly() {
        let brains = vec!["a".to_string()];
        let query = |_b: &str, _p: &str| -> Result<String, String> { Err("blockiert".to_string()) };
        let report = run_self_research(&brains, "f", 3, 3, 1, query);
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

    #[test]
    fn ui_failure_messages_are_not_suggestions() {
        // Real beobachtet: "No response, Please try again later." gewann eine
        // Abstimmung und machte die Runde wertlos.
        assert!(!is_plausible_suggestion(
            "No response, Please try again later."
        ));
        assert!(!is_plausible_suggestion(
            "Something went wrong. Please try again."
        ));
        assert!(!is_plausible_suggestion(
            "Too many requests - service unavailable"
        ));
    }

    #[test]
    fn rate_limiting_as_a_topic_stays_a_valid_suggestion() {
        // Abgrenzung: "Rate-Limit" im Fachsinn darf NICHT als Ausfallmeldung
        // gelten, sonst filtert der Schutz echte Vorschlaege weg.
        assert!(is_plausible_suggestion(
            "Rate-Limiting fuer die Brain-Abfragen einfuehren, damit Anbieter-Limits nicht gerissen werden"
        ));
    }

    #[test]
    fn query_parallel_keeps_input_order_regardless_of_completion() {
        // Die Reihenfolge der Ergebnisse muss der Eingabe folgen, nicht der
        // Fertigstellung — sonst wandert eine Antwort dem falschen Brain zu.
        let brains: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let q = |b: &str, _p: &str| -> Result<String, String> {
            // Frueh gestartete kuenstlich verlangsamen, damit die Fertigstellung
            // garantiert von der Eingabereihenfolge abweicht.
            let ms = match b {
                "a" => 60,
                "b" => 40,
                "c" => 20,
                _ => 0,
            };
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Ok(format!("antwort-{b}"))
        };
        let got = query_parallel(&brains, "prompt", 4, &|_, _| {}, &q);
        let names: Vec<&str> = got.iter().map(|(b, _)| b.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
        assert_eq!(got[0].1.as_deref(), Ok("antwort-a"));
        assert_eq!(got[3].1.as_deref(), Ok("antwort-d"));
    }

    #[test]
    fn query_parallel_reports_progress_once_per_brain() {
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let seen = std::sync::Mutex::new(Vec::<usize>::new());
        let q = |_b: &str, _p: &str| -> Result<String, String> { Ok("x".to_string()) };
        let _ = query_parallel(
            &brains,
            "p",
            2,
            &|done, total| {
                assert_eq!(total, 3);
                seen.lock().unwrap().push(done);
            },
            &q,
        );
        let mut v = seen.into_inner().unwrap();
        v.sort_unstable();
        assert_eq!(v, vec![1, 2, 3], "jeder Abschluss genau einmal gemeldet");
    }

    #[test]
    fn query_parallel_keeps_errors_with_their_brain() {
        let brains: Vec<String> = ["ok1", "boom", "ok2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let q = |b: &str, _p: &str| -> Result<String, String> {
            if b == "boom" {
                Err("kaputt".to_string())
            } else {
                Ok(b.to_string())
            }
        };
        let got = query_parallel(&brains, "p", 3, &|_, _| {}, &q);
        assert_eq!(got[1].0, "boom");
        assert!(got[1].1.is_err());
        assert!(got[0].1.is_ok() && got[2].1.is_ok());
    }

    #[test]
    fn query_parallel_serialises_when_limit_is_one() {
        let brains: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let live = std::sync::atomic::AtomicUsize::new(0);
        let peak = std::sync::atomic::AtomicUsize::new(0);
        let q = |_b: &str, _p: &str| -> Result<String, String> {
            use std::sync::atomic::Ordering;
            let n = live.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(n, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(20));
            live.fetch_sub(1, Ordering::SeqCst);
            Ok("x".to_string())
        };
        let _ = query_parallel(&brains, "p", 1, &|_, _| {}, &q);
        assert_eq!(peak.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
