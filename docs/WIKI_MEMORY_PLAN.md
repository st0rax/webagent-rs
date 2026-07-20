# Wiki-Memory — Implementierungsplan (Karpathy-LLM-Wiki, Kern-Pattern)

> **STATUS: SPEZIFIZIERT 2026-07-20, Umsetzung delegiert.** Grundlage:
> Karpathys "LLM Wiki"-Pattern (raw sources → wiki → schema; ingest/query/lint).
> Bewusst NUR das Kern-Pattern — keine v2-Erweiterungen (Konsolidierungs-Tiers,
> Confidence-Decay, Supersession-Versionierung), gemäß Projektlinie
> "keine vorzeitige Abstraktion". Das bestehende `memory.rs` (JSONL) bleibt
> unangetastet als Kurzzeit-/Quellen-Schicht bestehen.

## 1. Idee in einem Satz

"RAG retrieves and forgets, a wiki accumulates and compounds": statt nur
flacher JSONL-Einträge bekommt webagent ein kleines Markdown-Wiki
(`data/memory/wiki/`), dessen Seiten sich per `[[wikilink]]` verlinken, über
`index.md` katalogisiert sind und dessen Wissen in autonome Runs einfließt.
Brains können Seiten mit den vorhandenen edit/write-Actions selbst pflegen.

## 2. Layout auf der Platte

```
data/memory/
  memory.jsonl          # bestehend, unveraendert (Kurzzeit/Quellen)
  wiki/
    index.md            # Katalog: eine Zeile pro Seite: - [[slug]] — Kurzhook
    <slug>.md           # Seiten: erste Zeile "# Titel", dann Markdown-Body
```

- Slugs: kebab-case, `[a-z0-9-]+`, aus dem Titel abgeleitet.
- Links im Body: `[[slug]]` (nur Slug, keine Pfade).
- `index.md` wird von `write_page` automatisch aktuell gehalten (Zeile
  ergänzen/ersetzen). Handschriftliche Index-Änderungen bleiben erhalten,
  solange das Zeilenformat stimmt.

## 3. Neues Modul: `src/wiki_memory.rs`

```rust
pub struct WikiMemory { root: PathBuf }           // root = data/memory/wiki

pub struct WikiPage { pub slug: String, pub title: String, pub body: String }

pub struct LintReport {
    pub broken_links: Vec<(String, String)>,      // (seite, ziel-slug)
    pub orphan_pages: Vec<String>,                // nirgends verlinkt (auch nicht im index)
    pub empty_pages: Vec<String>,                 // nur Titel, kein Body
    pub index_missing: Vec<String>,               // Seite existiert, fehlt im index.md
}

impl WikiMemory {
    pub fn new(root: impl Into<PathBuf>) -> Self;
    pub fn ensure_layout(&self) -> Result<(), String>;         // wiki/ + leeres index.md
    pub fn list_pages(&self) -> Result<Vec<String>, String>;   // slugs
    pub fn read_page(&self, slug: &str) -> Result<WikiPage, String>;
    pub fn write_page(&self, title: &str, body: &str) -> Result<String, String>; // -> slug; pflegt index.md
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<WikiPage>, String>;
    pub fn lint(&self) -> Result<LintReport, String>;
    pub fn context_block(&self, max_chars: usize) -> Result<String, String>;
}

pub fn slugify(title: &str) -> String;
pub fn extract_links(body: &str) -> Vec<String>;   // [[slug]]-Vorkommen, dedupliziert
```

- `search`: gleiche einfache Token-Overlap-Heuristik wie `memory.rs` (Treffer
  in Titel doppelt gewichten). Kein Embedding, kein Index-Neubau.
- `context_block`: Inhalt von `index.md`, hart auf `max_chars` gekürzt —
  das ist der Teil, der in Prompts injiziert wird.
- Alles `std` + vorhandene Deps; keine neuen Crates.

## 4. Integration (v1, bewusst klein)

1. **Autonome Runs** (`controller.rs`): dort, wo heute `memory_context` aus
   `MemoryStore.search` gebaut wird, zusätzlich `WikiMemory::context_block(1500)`
   anhängen (eigener Abschnitt "Wiki-Index (Langzeitwissen; Seiten unter
   data/memory/wiki/, per edit/write-Action pflegbar):"). Fehler → leerer
   String, niemals den Run blockieren.
2. **REPL** (`repl.rs`):
   - `/wiki` — index.md anzeigen (oder "leer" + Hinweis).
   - `/wiki <suchbegriff>` — `search`, Treffer mit Slug + erster Body-Zeile.
   - `/wiki lint` — `LintReport` menschenlesbar ausgeben.
   Banner-Zeile um `/wiki` ergänzen.
3. **Ingest = vorhandene Mechanik:** Brains schreiben/ändern Seiten über die
   normalen write/edit-Actions (Pfad steht im injizierten Kontext). KEIN
   eigener Ingest-Befehl in v1.

## 5. Teststrategie (Muster: `file_actions.rs`-Tests, Tempdir)

- slugify: Umlaute/Sonderzeichen/Mehrfach-Bindestriche.
- extract_links: mehrere Links, Duplikate, `[[...]]` über Zeilengrenzen NICHT
  matchen, kaputte Klammern ignorieren.
- write_page: legt Datei + Index-Zeile an; zweiter write auf gleichen Titel
  ersetzt Body + laesst Index-Zeile einmalig.
- lint: broken link, Orphan, leere Seite, Seite-fehlt-im-Index — je ein Fall.
- context_block: Kürzung auf max_chars.
- search: Titel-Treffer schlägt Body-Treffer.

## 6. Außerhalb des Scopes für v1

- LLM-getriebene Lint-/Konsolidierungsläufe ("dreaming") — erst wenn v1 sich
  in der Praxis füllt. Mechanischer `lint` reicht als Fundament.
- Schema-Schicht (AGENTS.md-artig) — Kandidat für v2.
- Migration bestehender memory.jsonl-Einträge ins Wiki — manuell/spaeter.
