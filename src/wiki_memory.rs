//! Markdown-Wiki als Langzeitgedächtnis (Karpathy-LLM-Wiki, Kern-Pattern).
//!
//! "RAG retrieves and forgets, a wiki accumulates and compounds": kleine
//! Markdown-Seiten unter `data/memory/wiki/`, verlinkt per `[[slug]]`,
//! katalogisiert über `index.md`. Der Index fließt als Kontextblock in
//! autonome Runs ein; Brains pflegen Seiten über die normalen
//! write/edit-Actions. Siehe docs/WIKI_MEMORY_PLAN.md.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static TOKEN_RE: OnceLock<Regex> = OnceLock::new();

fn token_regex() -> &'static Regex {
    // Gleiche Token-Definition wie memory.rs (Token-Overlap-Heuristik).
    TOKEN_RE.get_or_init(|| Regex::new(r"[A-Za-zÄÖÜäöüß0-9_-]{3,}").unwrap())
}

/// Stop-Wörter wie in memory.rs — häufige Füllwörter zählen nicht als Treffer.
static STOP_WORDS: &[&str] = &[
    "aber", "alle", "auch", "dass", "eine", "einen", "einer", "eines", "fuer", "für", "haben",
    "hier", "nicht", "oder", "soll", "und", "von", "werden", "with", "from", "that", "this", "the",
    "webagent",
];

fn tokens(text: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for m in token_regex().find_iter(text) {
        let full = m.as_str().to_lowercase();
        // Bindestrich/Unterstrich gehören zur Token-Zeichenklasse (memory.rs-
        // Erbe) — dadurch wäre "Deploy-Regeln" EIN Token und die Suche nach
        // "deploy" fände nichts. In einem Wiki voller Kebab-Slugs wäre das
        // fatal: deshalb zusätzlich die Bestandteile aufnehmen.
        for part in full.split(['-', '_']) {
            if part.len() >= 3 && !STOP_WORDS.contains(&part) {
                set.insert(part.to_string());
            }
        }
        if !STOP_WORDS.contains(&full.as_str()) {
            set.insert(full);
        }
    }
    set
}

/// Konvertiert einen Titel in einen URL-freundlichen Slug (Kebab-Case).
/// - Nur Kleinbuchstaben, Zahlen und Bindestriche.
/// - Umlaute werden transliteriert (ä/Ä -> ae, ö/Ö -> oe, ü/Ü -> ue, ß -> ss).
/// - Mehrfache Bindestriche werden zusammengefasst.
/// - Führende und abschließende Bindestriche werden entfernt.
/// - Ein leeres Ergebnis wird zu "seite".
///
/// slugify/extract_links von der webagent-Flotte (qwen) vorimplementiert.
pub fn slugify(title: &str) -> String {
    let mut result = String::new();
    for c in title.chars() {
        match c {
            'ä' | 'Ä' => result.push_str("ae"),
            'ö' | 'Ö' => result.push_str("oe"),
            'ü' | 'Ü' => result.push_str("ue"),
            'ß' => result.push_str("ss"),
            _ if c.is_ascii_alphanumeric() => result.push(c.to_ascii_lowercase()),
            _ => result.push('-'),
        }
    }

    let mut collapsed = String::new();
    let mut last_was_hyphen = false;
    for c in result.chars() {
        if c == '-' {
            if !last_was_hyphen {
                collapsed.push(c);
                last_was_hyphen = true;
            }
        } else {
            collapsed.push(c);
            last_was_hyphen = false;
        }
    }

    let trimmed = collapsed.trim_matches('-');
    if trimmed.is_empty() {
        "seite".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extrahiert alle Wiki-Links im Format [[slug]] aus dem Text.
/// - Links dürfen NICHT über Zeilengrenzen gehen.
/// - Verschachtelte oder kaputte Klammern werden ignoriert.
/// - Die Ergebnisliste ist in der Reihenfolge des Auftretens und dedupliziert.
///
/// slugify/extract_links von der webagent-Flotte (qwen) vorimplementiert.
pub fn extract_links(body: &str) -> Vec<String> {
    let mut links = Vec::new();
    for line in body.lines() {
        let mut i = 0;
        let bytes = line.as_bytes();
        while i + 3 < bytes.len() {
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                let start = i + 2;
                if let Some(end) = line[start..].find("]]") {
                    let slug = &line[start..start + end];
                    // Ignoriere kaputte/verschachtelte Klammern und leere Slugs
                    if !slug.contains('[') && !slug.contains(']') && !slug.trim().is_empty() {
                        let slug_str = slug.to_string();
                        if !links.contains(&slug_str) {
                            links.push(slug_str);
                        }
                    }
                    i = start + end + 2;
                    continue;
                } else {
                    i += 2;
                }
            } else {
                i += 1;
            }
        }
    }
    links
}

/// Eine Wiki-Seite: Datei `<slug>.md`, erste Zeile `# Titel`, danach Markdown-Body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiPage {
    pub slug: String,
    pub title: String,
    pub body: String,
}

/// Ergebnis eines mechanischen Lint-Laufs über das Wiki.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    /// (seite, ziel-slug): Link auf eine Seite, die nicht existiert.
    pub broken_links: Vec<(String, String)>,
    /// Seiten, die nirgends verlinkt sind (auch nicht im index.md).
    pub orphan_pages: Vec<String>,
    /// Seiten, die nur einen Titel und keinen Body haben.
    pub empty_pages: Vec<String>,
    /// Seiten, die existieren, aber im index.md fehlen.
    pub index_missing: Vec<String>,
}

impl LintReport {
    /// True, wenn der Lint keinerlei Befunde hat.
    pub fn is_clean(&self) -> bool {
        self.broken_links.is_empty()
            && self.orphan_pages.is_empty()
            && self.empty_pages.is_empty()
            && self.index_missing.is_empty()
    }
}

/// Markdown-Wiki unter einem Wurzelverzeichnis (`data/memory/wiki`).
pub struct WikiMemory {
    root: PathBuf,
}

impl WikiMemory {
    /// Erzeugt eine WikiMemory-Instanz ohne Dateisystem-Zugriff.
    /// Das Layout entsteht erst beim ersten Zugriff (ensure_layout).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Pfad zum Katalog `index.md`.
    fn index_path(&self) -> PathBuf {
        self.root.join("index.md")
    }

    /// Pfad zur Seiten-Datei eines Slugs.
    fn page_path(&self, slug: &str) -> PathBuf {
        self.root.join(format!("{slug}.md"))
    }

    /// Legt `wiki/` samt leerem `index.md` an, falls noch nicht vorhanden.
    /// Idempotent; wird von allen Zugriffs-Methoden aufgerufen (nie beim Start).
    pub fn ensure_layout(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root)
            .map_err(|e| format!("Wiki-Verzeichnis {:?} nicht anlegbar: {e}", self.root))?;
        let index = self.index_path();
        if !index.exists() {
            fs::write(&index, "")
                .map_err(|e| format!("index.md {:?} nicht anlegbar: {e}", index))?;
        }
        Ok(())
    }

    /// Listet alle Seiten-Slugs (ohne index.md), alphabetisch sortiert.
    pub fn list_pages(&self) -> Result<Vec<String>, String> {
        self.ensure_layout()?;
        let mut slugs = Vec::new();
        let entries = fs::read_dir(&self.root)
            .map_err(|e| format!("Wiki-Verzeichnis {:?} nicht lesbar: {e}", self.root))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(slug) = name.strip_suffix(".md") {
                if slug != "index" && !slug.is_empty() {
                    slugs.push(slug.to_string());
                }
            }
        }
        slugs.sort();
        Ok(slugs)
    }

    /// Liest eine Seite. Erste Zeile `# Titel`, Rest ist Body (getrimmt).
    pub fn read_page(&self, slug: &str) -> Result<WikiPage, String> {
        self.ensure_layout()?;
        let path = self.page_path(slug);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Wiki-Seite {:?} nicht lesbar: {e}", path))?;
        let mut lines = content.lines();
        let first = lines.next().unwrap_or("");
        let title = first.trim_start_matches('#').trim().to_string();
        let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
        Ok(WikiPage {
            slug: slug.to_string(),
            title,
            body,
        })
    }

    /// Schreibt (oder überschreibt) eine Seite und hält `index.md` aktuell:
    /// bestehende Index-Zeile des Slugs wird ersetzt, sonst eine neue angehängt.
    /// Handschriftliche Index-Zeilen anderer Seiten bleiben erhalten.
    /// Rückgabe: der Slug der Seite.
    pub fn write_page(&self, title: &str, body: &str) -> Result<String, String> {
        self.ensure_layout()?;
        let title = title.trim();
        if title.is_empty() {
            return Err("Wiki-Seite braucht einen Titel".to_string());
        }
        let slug = slugify(title);
        let body = body.trim();
        let path = self.page_path(&slug);
        let content = if body.is_empty() {
            format!("# {title}\n")
        } else {
            format!("# {title}\n\n{body}\n")
        };
        fs::write(&path, content)
            .map_err(|e| format!("Wiki-Seite {:?} nicht schreibbar: {e}", path))?;
        self.update_index_line(&slug, title, body)?;
        Ok(slug)
    }

    /// Ersetzt bzw. ergänzt die Katalogzeile `- [[slug]] — Kurzhook` im index.md.
    fn update_index_line(&self, slug: &str, title: &str, body: &str) -> Result<(), String> {
        // Kurzhook: erste nicht-leere Body-Zeile (gekürzt), sonst der Titel.
        let hook_raw = body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or(title);
        let hook = if hook_raw.chars().count() > 80 {
            format!("{}…", crate::char_prefix(hook_raw, 79))
        } else {
            hook_raw.to_string()
        };
        let new_line = format!("- [[{slug}]] — {hook}");

        let index = self.index_path();
        let existing = fs::read_to_string(&index).unwrap_or_default();
        // Anker am Zeilenanfang ("- [[slug]]"), NICHT contains: der Kurzhook
        // anderer Zeilen darf denselben Link enthalten, ohne ersetzt zu werden.
        let marker = format!("- [[{slug}]]");
        let mut lines: Vec<String> = existing
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect();
        let mut replaced = false;
        for line in lines.iter_mut() {
            // Zeilenformat: "- [[slug]] — ..." — handschriftliche Zeilen mit
            // gleichem Format zählen ebenfalls als Treffer.
            if !replaced && line.trim_start().starts_with(&marker) {
                *line = new_line.clone();
                replaced = true;
            }
        }
        if !replaced {
            lines.push(new_line);
        }
        let mut out = lines.join("\n");
        out.push('\n');
        fs::write(&index, out).map_err(|e| format!("index.md {:?} nicht schreibbar: {e}", index))
    }

    /// Sucht Seiten per Token-Overlap (gleiche Heuristik wie memory.rs);
    /// Treffer im Titel zählen doppelt. Kein Embedding, kein Index-Neubau.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<WikiPage>, String> {
        let query_tokens = tokens(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }
        let mut scored: Vec<(f64, WikiPage)> = Vec::new();
        for slug in self.list_pages()? {
            let Ok(page) = self.read_page(&slug) else {
                continue;
            };
            let title_overlap = query_tokens.intersection(&tokens(&page.title)).count();
            let body_overlap = query_tokens.intersection(&tokens(&page.body)).count();
            let score = (title_overlap as f64) * 2.0 + body_overlap as f64;
            if score > 0.0 {
                scored.push((score, page));
            }
        }
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.slug.cmp(&b.1.slug))
        });
        Ok(scored
            .into_iter()
            .take(limit.max(1))
            .map(|(_, p)| p)
            .collect())
    }

    /// Mechanischer Lint: kaputte Links, Orphans, leere Seiten, Index-Lücken.
    pub fn lint(&self) -> Result<LintReport, String> {
        let slugs = self.list_pages()?;
        let slug_set: HashSet<&str> = slugs.iter().map(String::as_str).collect();
        let index_content = fs::read_to_string(self.index_path()).unwrap_or_default();
        let index_links: HashSet<String> = extract_links(&index_content).into_iter().collect();

        let mut report = LintReport::default();
        let mut page_links: HashMap<String, Vec<String>> = HashMap::new();
        for slug in &slugs {
            let page = self.read_page(slug)?;
            if page.body.is_empty() {
                report.empty_pages.push(slug.clone());
            }
            let links = extract_links(&page.body);
            for target in &links {
                if !slug_set.contains(target.as_str()) {
                    report.broken_links.push((slug.clone(), target.clone()));
                }
            }
            page_links.insert(slug.clone(), links);
        }
        for slug in &slugs {
            let linked_from_pages = page_links
                .iter()
                .any(|(from, links)| from != slug && links.iter().any(|l| l == slug));
            if !linked_from_pages && !index_links.contains(slug.as_str()) {
                report.orphan_pages.push(slug.clone());
            }
            if !index_links.contains(slug.as_str()) {
                report.index_missing.push(slug.clone());
            }
        }
        Ok(report)
    }

    /// Inhalt von index.md, zeichensicher hart auf `max_chars` gekürzt —
    /// dieser Block wird in Prompts injiziert.
    pub fn context_block(&self, max_chars: usize) -> Result<String, String> {
        self.ensure_layout()?;
        let content = fs::read_to_string(self.index_path())
            .map_err(|e| format!("index.md nicht lesbar: {e}"))?;
        let trimmed = content.trim();
        Ok(crate::char_prefix(trimmed, max_chars).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Eindeutiges Tempdir pro Test (Muster: memory.rs/file_actions.rs-Tests).
    fn unique_wiki() -> WikiMemory {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "test_wiki_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ));
        WikiMemory::new(root)
    }

    // === slugify/extract_links von der webagent-Flotte (qwen) vorimplementiert ===

    #[test]
    fn test_slugify_umlaute() {
        assert_eq!(slugify("Äpfel & Birnen"), "aepfel-birnen");
    }

    #[test]
    fn test_slugify_sonderzeichen() {
        assert_eq!(slugify("Hallo Welt!"), "hallo-welt");
    }

    #[test]
    fn test_slugify_mehrfach_bindestriche() {
        assert_eq!(slugify("a---b"), "a-b");
    }

    #[test]
    fn test_slugify_leer() {
        assert_eq!(slugify(""), "seite");
    }

    #[test]
    fn test_slugify_nur_sonderzeichen() {
        assert_eq!(slugify("!@#"), "seite");
    }

    #[test]
    fn test_slugify_gross_kleinschreibung() {
        assert_eq!(slugify("TestCASE"), "testcase");
    }

    #[test]
    fn test_slugify_fuehrende_abschliessende_bindestriche() {
        assert_eq!(slugify("---test---"), "test");
    }

    #[test]
    fn test_slugify_sz() {
        assert_eq!(slugify("Straße"), "strasse");
    }

    #[test]
    fn test_extract_links_normal() {
        assert_eq!(
            extract_links("Hallo [[welt]] und [[erde]]"),
            vec!["welt".to_string(), "erde".to_string()]
        );
    }

    #[test]
    fn test_extract_links_duplikate() {
        assert_eq!(extract_links("[[a]] und [[a]]"), vec!["a".to_string()]);
    }

    #[test]
    fn test_extract_links_ueber_zeilengrenze() {
        assert_eq!(extract_links("[[link\nüber\nzeilen]]"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_links_kaputte_klammern_verschachtelt() {
        assert_eq!(extract_links("[[a[b]]]"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_links_kaputte_klammern_fehlend() {
        assert_eq!(extract_links("[[a]"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_links_leer_whitespace() {
        assert_eq!(extract_links("[[ ]]"), Vec::<String>::new());
    }

    #[test]
    fn test_extract_links_mehrere_in_einer_zeile() {
        assert_eq!(
            extract_links("[[x]][[y]]"),
            vec!["x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn test_extract_links_kein_link() {
        assert_eq!(extract_links("nur text"), Vec::<String>::new());
    }

    // === WikiMemory ===

    #[test]
    fn test_ensure_layout_idempotent() {
        let wiki = unique_wiki();
        wiki.ensure_layout().unwrap();
        wiki.ensure_layout().unwrap();
        assert_eq!(wiki.list_pages().unwrap(), Vec::<String>::new());
        assert_eq!(wiki.context_block(1000).unwrap(), "");
    }

    #[test]
    fn test_write_page_creates_file_and_index_line() {
        let wiki = unique_wiki();
        let slug = wiki
            .write_page("Äpfel & Birnen", "Obst-Notizen.\nZweite Zeile.")
            .unwrap();
        assert_eq!(slug, "aepfel-birnen");

        let page = wiki.read_page(&slug).unwrap();
        assert_eq!(page.title, "Äpfel & Birnen");
        assert!(page.body.starts_with("Obst-Notizen."));

        let index = wiki.context_block(1000).unwrap();
        assert!(
            index.contains("- [[aepfel-birnen]] — Obst-Notizen."),
            "Index-Zeile fehlt: {index}"
        );
        assert_eq!(wiki.list_pages().unwrap(), vec!["aepfel-birnen".to_string()]);
    }

    #[test]
    fn test_write_page_twice_replaces_body_and_keeps_single_index_line() {
        let wiki = unique_wiki();
        wiki.write_page("Notiz", "Alter Inhalt").unwrap();
        let slug = wiki.write_page("Notiz", "Neuer Inhalt").unwrap();
        assert_eq!(slug, "notiz");

        let page = wiki.read_page("notiz").unwrap();
        assert_eq!(page.body, "Neuer Inhalt");
        assert!(!page.body.contains("Alter Inhalt"));

        let index = wiki.context_block(1000).unwrap();
        let hits = index.matches("[[notiz]]").count();
        assert_eq!(hits, 1, "Index-Zeile muss einmalig bleiben: {index}");
        assert!(index.contains("Neuer Inhalt"));
    }

    #[test]
    fn test_write_page_preserves_handwritten_index_lines() {
        let wiki = unique_wiki();
        wiki.ensure_layout().unwrap();
        std::fs::write(
            wiki.index_path(),
            "- [[handarbeit]] — von Hand gepflegte Zeile\n",
        )
        .unwrap();
        wiki.write_page("Neu", "Inhalt").unwrap();
        let index = wiki.context_block(1000).unwrap();
        assert!(index.contains("[[handarbeit]]"), "Handzeile weg: {index}");
        assert!(index.contains("[[neu]]"));
    }

    #[test]
    fn test_search_title_hit_beats_body_hit() {
        let wiki = unique_wiki();
        wiki.write_page("Deployment Ablauf", "Schritte zum Ausrollen.")
            .unwrap();
        wiki.write_page("Sonstiges", "Hinweise zum Deployment im Body.")
            .unwrap();

        let hits = wiki.search("Deployment", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].slug, "deployment-ablauf", "Titel-Treffer zuerst");
        assert_eq!(hits[1].slug, "sonstiges");
    }

    #[test]
    fn test_search_no_overlap_is_empty() {
        let wiki = unique_wiki();
        wiki.write_page("Deployment", "Ausrollen.").unwrap();
        assert!(wiki.search("Quantencomputer", 10).unwrap().is_empty());
    }

    #[test]
    fn test_lint_reports_all_categories() {
        let wiki = unique_wiki();
        // Seite mit kaputtem Link (Ziel existiert nicht):
        wiki.write_page("Start", "Siehe [[fehlt]] und [[leer]].")
            .unwrap();
        // Leere Seite (nur Titel), von "start" verlinkt:
        wiki.write_page("Leer", "").unwrap();
        // Orphan: nirgends verlinkt — Index-Zeile von Hand entfernen,
        // damit sie auch index_missing ist.
        wiki.write_page("Waise", "Inhalt ohne Verlinkung.").unwrap();
        let index = std::fs::read_to_string(wiki.index_path()).unwrap();
        let filtered: String = index
            .lines()
            .filter(|l| !l.contains("[[waise]]"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(wiki.index_path(), filtered).unwrap();

        let report = wiki.lint().unwrap();
        assert_eq!(
            report.broken_links,
            vec![("start".to_string(), "fehlt".to_string())]
        );
        assert_eq!(report.empty_pages, vec!["leer".to_string()]);
        assert_eq!(report.orphan_pages, vec!["waise".to_string()]);
        assert_eq!(report.index_missing, vec!["waise".to_string()]);
        assert!(!report.is_clean());
    }

    #[test]
    fn test_lint_clean_wiki() {
        let wiki = unique_wiki();
        wiki.write_page("Solo", "Inhalt.").unwrap();
        // write_page hat die Index-Zeile gepflegt → weder Orphan noch index_missing.
        let report = wiki.lint().unwrap();
        assert!(report.is_clean(), "unerwartete Befunde: {report:?}");
    }

    #[test]
    fn test_search_findet_bindestrich_titel_ueber_teilwort() {
        // Live-Fund 2026-07-20: '-' steckt in der Token-Zeichenklasse, dadurch
        // war "Deploy-Regeln" EIN Token und /wiki deploy fand nichts.
        let wiki = unique_wiki();
        wiki.write_page("Deploy-Regeln", "Flotten-Kopie nur via Skript.")
            .unwrap();
        let hits = wiki.search("deploy", 10).unwrap();
        assert_eq!(hits.len(), 1, "Teilwort eines Bindestrich-Titels muss treffen");
        assert_eq!(hits[0].slug, "deploy-regeln");
        // Der volle Kebab-Begriff trifft weiterhin.
        assert_eq!(wiki.search("deploy-regeln", 10).unwrap().len(), 1);
    }

    #[test]
    fn test_context_block_truncates_char_safe() {
        let wiki = unique_wiki();
        wiki.write_page("Ärger", "Übermäßig lange Zeile für die Kürzung.")
            .unwrap();
        let full = wiki.context_block(10_000).unwrap();
        assert!(full.chars().count() > 10);
        let cut = wiki.context_block(10).unwrap();
        assert_eq!(cut.chars().count(), 10);
        assert!(full.starts_with(&cut));
    }
}
