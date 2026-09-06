# T-501: Modellwahl-Belege korrigieren

> **Referenz.** Datierter Audit- und Verifikationsbeleg. Operativer Arbeitsstand:
> `docs/CURRENT_WORK.md`; aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

Datum: 2026-09-07. Basis: `28e9f1c`, Arbeitsbranch `fix/T-501-model-proof`.
Eigentuemer: `chatgpt-codex`. Unabhaengige Code-/Belegpruefung durch Reviewer.

## Befund und Wirkung

`verify --cap model_switch` nutzte denselben Trigger-Roundtrip wie ein
Umschalter. Ein Wechsel von `aria-expanded=false` zu `true` bestand den Test,
obwohl kein Modell gewaehlt wurde. `probe --verify` konnte denselben Beleg
erzeugen. Alte Selektorhashes blieben trotz geaenderter Verifikationssemantik
gueltig. Ein positives Menue-Ergebnis ist kein Nachweis fuer die Modellwahl
aus dem Produktvertrag.

## Korrektur

- Der eigene Modelltest liest das aktuelle Modell und die Laufzeitliste,
  waehlt eine andere eindeutige Option und liest das Ergebnis unabhaengig
  von der Rueckgabe des Klickbefehls nach. PASS verlangt zusaetzlich den
  nachgelesenen Rueckweg zum Ausgangsmodell in derselben Sitzung.
- Drift beim Listenlesen, `bereits aktiv`, unveraenderte Auswahl, fehlende
  Alternativen oder misslungener Restore bestehen nicht. Nach einem Fehler
  mit beobachteter Zustandsaenderung wird die Wiederherstellung versucht.
- Der vorhandene UI-Auswahlpfad unterstuetzt Teiltexte. Deshalb muessen
  Ausgangs- und Zielmodell gegen die gesamte Laufzeitliste eindeutig sein;
  mehrdeutige Kombinationen wie `Pro`, `Pro Max`, `Lite` werden vor einem
  Eingriff verworfen. Eine kuenftige exakte Auswahl-API kann diese Grenze
  erweitern. Generische Menuebeschriftungen sind ebenfalls keine Auswahl.
- Der generische Oberflaechen-Probe bestaetigt `model_switch` nicht mehr.
  Einweg-Befehle bleiben bedienbar, erzeugen aber ueber `record_route_proof`
  keinen Modell-Roundtrip-Beleg. Die Messung nutzt den bestehenden Proof-Store.
- Nur `model_switch` erhaelt die Hash-Version `model-selection-roundtrip-v2`.
  Historische Trigger-Belege werden dadurch ungueltig; andere Faehigkeiten
  behalten ihre Hashes. Keine lokalen Belegdateien werden geloescht.

## Matrix-Audit

| Brain | Bisheriger Teilbeleg | Neue Bewertung |
|---|---|---|
| claude | `live_caps_2026-09-03.jsonl`, Zeile 10: generischer Trigger | Modellwahl neu pruefen |
| qwen | dieselbe Datei, Zeile 20: generischer Trigger | Modellwahl neu pruefen |
| perplexity | dieselbe Datei, Zeile 24: Trigger, auch via echtem Mausklick | Modellwahl neu pruefen |
| zai | dieselbe Datei, Zeile 29: generischer Trigger | Modellwahl neu pruefen |
| gemini | `model_switch_gemini_2026-09-06.json`: aria-expanded; zusaetzliche Auswahl nur narrativ beschrieben | strukturierter Auswahl-/Restore-Beleg fehlt |
| kimi | `model_switch_kimi_2026-09-06.json`: Auswahl beschrieben; Rueckweg als `bereits aktiv` in separatem CLI-Aufruf | Roundtrip in derselben Sitzung fehlt |

Diese sechs Zellen wechseln von `passed` auf `not_run` fuer die neue Abnahme.
Historische Artefakte und Notizen bleiben erhalten. Gesamtstand nach Audit:
**103 passed, 10 failed, 4 unreachable, 13 not_run = 130 Zellen**.
Dies dokumentiert strengere Evidenz, keinen gemessenen Provider-Ausfall.
Die zehn roten Managed-Tool-Zellen und die restliche T-501-Abnahme bleiben offen.

## Login-Gegenprobe

Mit der bereits vorhandenen Binary im urspruenglichen Checkout wurde seriell
`webagent diagnose --brain <id> --headless` fuer alle neun Brains ausgefuehrt:
chatgpt, claude, deepseek, gemini, kimi, mistral, perplexity, qwen, zai.
Alle lieferten `session_state: Ready`, `logged_in: true`,
`login_button: false`, `composer: ok`, `cloudflare: false`.
Es wurde keine Chatnachricht gesendet und kein neuer Login benoetigt.
Der historische Doctor-Befund `chatgpt: login_required` wurde durch diese
Live-Gegenprobe widerlegt. Dies ist eine Login-/Composer-Pruefung, keine
aktuelle Modellwahl- oder Antwort-Abnahme und kein Test der geaenderten Binary.

## Verifikation

Die fokussierten Modelltests waren gruen (16 passed). Die abschliessende
vollstaendige Bibliothekssuite war ebenfalls gruen: **1331 passed, 1 ignored**.
Zusaetzlich bestanden `cargo clippy --locked --all-targets -- -D warnings`,
`cargo check --locked --no-default-features` und `cargo check --locked
--features tui`. Die Harvest-Test-Isolation wurde dabei als eigenstaendiger
Regressionstest aufgenommen. Kein Releasebeleg und keine Live-Rezertifizierung
der Modellwahl.
