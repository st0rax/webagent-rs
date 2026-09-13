<!-- **Referenz: Modulkarte der API-Bridge (Phase 10–12); Betrieb bleibt docs/API_BRIDGE.md.** -->
# API-Bridge — Architektur und Agenten-Einstieg

Dieses Dokument beschreibt die **Modulgrenzen** der lokalen Provider-Bridge.
Laufzeitvertrag (Bindung, Token, Endpunkte) steht in [`API_BRIDGE.md`](API_BRIDGE.md).
Keine unbelegten Fähigkeitsaussagen: Live-Matrix bleibt [`CAPABILITY_MATRIX.json`](CAPABILITY_MATRIX.json).

**Regel:** Eine Datei unter `src/api_bridge/*.rs` ohne passende `mod`-Zeile in
`src/api_bridge.rs` ist **orphan / unwired**. Unwired ist nicht gleich
produktionswirksam. T-915–T-918 haben extrahiert; die Verdrahtung ist
T-922 / T-923 / T-931 / T-933.

## Aktuelle Dateien (`src/api_bridge/`)

| Pfad | Slot | `mod` in Root? | Verantwortung |
|---|---|---|---|
| `src/api_bridge.rs` | Root | — | Orchestrierung, Typen, Medien, Inference, noch Duplikate der Orphans |
| `routing.rs` | T-907 | ja (T-913) | `classify`, Streaming-Policy |
| `provider_handlers.rs` | T-908 | ja (T-913) | OpenAI-Chat, Anthropic, Responses |
| `transport.rs` | T-909 | ja (T-913) | HTTP-Request-Parsing |
| `wire.rs` | T-909 | ja (T-913) | Antwortheader, SSE-Frames, `sse_data` |
| `boundary.rs` | T-910 | ja (T-913) | Auth, timing-sicherer Vergleich, Fehlerkörper |
| `tests.rs` | T-911 | ja (T-913) | 50 Unit-Tests |
| `store.rs` | T-915 | **nein** → T-922 | Mandanten-Store, Lifecycle |
| `content.rs` | T-916 | **nein** → T-923 | Prompts, Tools, unsupported fields |
| `catalog.rs` | T-917 | **nein** → T-931 | Katalog, Auto-Router |
| `response_protocol.rs` | T-918 | **nein** → T-933 | JSON/SSE-Antwortkörper |

Nicht als Kindmodul extrahiert (User-Skip, bleiben in der Root-Datei):

| Thema | Slot | Status |
|---|---|---|
| Bild/Audio/Multipart | T-914 | deferred, nicht claimen |
| `run_task_blocking` / streaming | T-919 | deferred, nicht claimen |

## Erlaubte Abhängigkeitsrichtung

```
transport           -> HttpRequest-Typen (Root)
wire                -> HttpResponse, completion_id (Root)
boundary            -> Auth-Typen, HttpResponse
routing             -> Methode/Pfad/Body (keine Handler, kein Browser)
provider_handlers   -> Parent-Helfer; kein Selektor, kein Profil
store / content / catalog / response_protocol
                    -> Parent-Typen und -Helfer, bis T-922/923/931/933 verdrahten
tests               -> Parent
api_bridge.rs       -> verdrahtete Kindmodule
```

Kindmodule dürfen **nicht** auf Browser-Selektoren oder Providerprofile zugreifen.
Kein HTTP-Routing in `catalog.rs` (das bleibt `routing.rs`).
`response_protocol` ändert keine Header; `sequence_number` bleibt `wire::sse_data`.

## Nicht-Ziele

- Keine neuen Endpunkte, keine JSON-Semantik-Änderung, keine Timeout- oder Headervertragsänderung.
- Keine neuen Auth-Arten.
- Keine Live-Behauptungen über Brains; das ist die Capability-Matrix.

## Gates

Kind-Extrakt (wie T-915–T-918):

```
cargo fmt --all -- --check
cargo clippy --features webview --all-targets -- -D warnings
cargo test --features webview --lib api_bridge::tests::
git diff --check
```

Verdrahtung (T-913, T-921, T-922, T-923, T-931, T-933): zusätzlich
`cargo test --features webview --lib`.

## Claim-Regeln

Quelle: `docs/TASKBOARD.json`. Ein Entwickler, eine Aufgabe.

- T-914 und T-919 nicht claimen (deferred).
- Orphan-Dateien (`store`, `content`, `catalog`, `response_protocol`) nicht
  als fertige Produktion behandeln; Nachzug ist T-922 / T-923 / T-931 / T-933.
- T-921 verdrahtet den Rest, ist aber `depends_on` T-915, T-916, T-917, T-918,
  T-920, T-922, T-923, T-931, T-933 — nicht claimen, solange Vorgänger offen sind.
- Typen (`HttpRequest`, DTOs, `BridgeConfig`) bleiben in der Root-Datei.

## Verbleibende Risiken

- Drift Root vs. Orphan, solange `mod` fehlt: Clippy sieht die Orphans nicht.
- Medien und Inference bleiben in der Root-Datei (T-914/T-919 deferred).
- `docs/API_BRIDGE.md` ist der Betriebsvertrag; dieses Dokument ist die Modulkarte.
- Phase-12-Nachzug (T-922+) ist der Weg, Orphans zu schließen — nicht ein
  zweites paralleles Extrakt ohne `mod`.

## Agenten-Einstieg

1. `START_HERE.md` und `docs/WORK_CONTRACT.md`.
2. `git pull origin master`.
3. Freien Task in `docs/TASKBOARD.json` claimen, Branch nach `docs/GIT_GLOSSAR.md`.
4. Nur den Scope der Aufgabe anfassen. T-914/T-919 überspringen.
5. Gates grün, Beleg unter `docs/proofs/T-…/`, Merge nach `master`.
