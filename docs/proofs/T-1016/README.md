# T-1016 — Selektor-I/O-Fehler auf zweitem PC bricht Backend ab

> **Referenz.** Task proof für T-1016. Die Änderung macht die eingebetteten
> Selektoren gegen einen unerreichbaren Build-Pfad robust; sie schaltet nichts
> live und beschreibt keinen Deploy.

- Task: T-1016
- Owner: local/opencode
- Branch: `fix/T-1016-selector-io-fallback`
- Status: umgesetzt und geprüft; Gates siehe `gates.txt`

## Befund (Meldung des Eigentuemers, 2026-11-26)

Der Eigentuemer testete die Binary auf einem zweiten PC und meldete:
„backend fehler selektronre nicht ladbar das gerät ist nicht bereit os error 21".

Die einzige Stelle mit diesem Fehlertext ist
`crates/webagent-transport/src/browser/mod.rs:118`:

```rust
let selectors = webagent_core::config::load_selectors(brain_id)
    .map(selectors::Selectors::from_value)
    .map_err(|e| format!("Selektoren nicht ladbar: {e}"))?;
```

`load_selectors` liest zwei Dateien über `read_selector_file`:

1. `shipped_selectors()` → `selectors_dir().join("{brain}.json")`.
   `selectors_dir()` ist `env!("CARGO_MANIFEST_DIR")/selectors`, also ein
   **Compile-Zeit-Pfad des Build-Rechners**.
2. `user_selectors()` → `<stable_root>/selectors/{brain}.json`.

`read_selector_file` behandelte vorher nur `NotFound` als „Datei fehlt"; jeder
andere I/O-Fehler wurde als harter Fehler nach oben gegeben. `os error 21`
(`ERROR_NOT_READY`, „Das Gerät ist nicht bereit") tritt genau dann auf, wenn
der Pfad auf ein nicht bereites Laufwerk zeigt — z. B. wenn die auf dem
Build-PC erzeugte EXE auf einem fremden PC läuft und `C:\Users\storax\…`
dort auf eine nicht verbundene Netzwerk-/Wechselmedium-Ressource zeigt.
Die Selektoren sind zusätzlich per `include_str!` in der Binary enthalten;
der Backend-Abbruch war also unnötig.

## Änderung

`crates/webagent-core/src/config/selectors.rs`, `read_selector_file`:

- `NotFound` bleibt `None` (wie zuvor).
- **Jeder weitere I/O-Fehler** wird ebenfalls als `None` behandelt und mit
  einer sichtbaren Warnung `[selectors] <Pfad> nicht lesbar (<Fehler>); Datei
  wird als nicht vorhanden behandelt.` protokolliert. Damit greift bei den
  mitgelieferten Selektoren die eingebettete Kopie und bei der Nutzerdatei
  die Basis ohne Overlay — das Backend startet.
- Kaputtes JSON bleibt `InvalidData` und damit ein harter Fehler: eine
  verunglückte Reparatur darf nicht still auf die Basis zurückfallen.
- Keine Änderung an `merge_selectors`, an Selektor-Inhalten oder an den
  Pfad-Prioritäten.

Neue Regressionstests in `config::selectors::tests`:

- `unlesbare_selektor_datei_gilt_als_nicht_vorhanden` — ein Verzeichnis an
  der Dateistelle erzeugt einen echten I/O-Fehler (kein `NotFound`); das
  Ergebnis muss `Ok(None)` sein.
- `kaputtes_selektor_json_bleibt_ein_fehler` — ungültiges JSON bleibt
  `InvalidData`.

## Erwartung / Grenze

- Eine auf einem fremden PC gestartete EXE mit nicht erreichbarem
  Build-Pfad startet das Backend mit den eingebetteten Selektoren und
  meldet die Ursache sichtbar, statt `Selektoren nicht ladbar` zu liefern.
- Die Warnung ist bewusst nicht still: wenn eine Nutzerdatei tatsächlich
  nicht lesbar ist, fehlt ihr Overlay und die Ursache steht im Log.
- Nicht gelöst: eine **inhaltlich** falsche oder kaputte JSON-Datei bleibt
  ein Fehler; das ist beabsichtigt. Ein bloß anderer Pfad ohne I/O-Fehler
  war bereits vorher über `NotFound` abgedeckt.
- Der lokale Bridge-Prozess wurde für T-1016 **nicht** angefasst; ein
  erneuter EXE-Tausch braucht eine neue ausdrückliche Freigabe. Kein Tag,
  GitHub-Release, Versionssprung oder Login. Keine Zugangsdaten gelesen
  oder ausgegeben.
