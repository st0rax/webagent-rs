<!-- **Referenz: Modulkarte der API-Bridge (Phase 10); Betrieb bleibt docs/API_BRIDGE.md.** -->
# API-Bridge — Architektur und Agenten-Einstieg

Dieses Dokument beschreibt die **Modulgrenzen** der lokalen Provider-Bridge.
Laufzeitvertrag (Bindung, Token, Endpunkte) steht in [`API_BRIDGE.md`](API_BRIDGE.md).
Keine unbelegten Fähigkeitsaussagen: Live-Matrix bleibt [`CAPABILITY_MATRIX.json`](CAPABILITY_MATRIX.json).

## Aktuelle Dateien

| Pfad | Slot | Verantwortung |
|---|---|---|
| `src/api_bridge.rs` | Root, T-913 | Orchestrierung, Typen, noch die alten Funktionen bis zur Verdrahtung |
| `src/api_bridge/routing.rs` | T-907 | Routenentscheidung `classify`, Streaming-Policy |
| `src/api_bridge/provider_handlers.rs` | T-908 | OpenAI-Chat, Anthropic, Responses (gepuffert + SSE) |
| `src/api_bridge/transport.rs` | T-909 | HTTP-Request-Parsing, `find_bytes` |
| `src/api_bridge/wire.rs` | T-909 | Antwortheader, SSE-Frames |
| `src/api_bridge/boundary.rs` | T-910 | Auth, timing-sicherer Vergleich, Fehlerkörper |
| `src/api_bridge/tests.rs` | T-911 | 50 Unit-Tests, thematische Abschnitte |

Bis T-913 sind die Kindmodule **vorbereitet, nicht eingebunden**. Die Root-Datei bleibt die laufende Implementierung. T-913 setzt `mod routing` / `mod provider_handlers` / `mod transport` / `mod wire` / `mod boundary` / `#[cfg(test)] mod tests` und entfernt doppelte Logik.

## Erlaubte Abhängigkeitsrichtung

```
transport  ->  (HttpRequest-Typen)
wire       ->  HttpResponse, completion_id
boundary   ->  Auth-Typen, HttpResponse
routing    ->  Methode/Pfad/Body  (keine Handler, kein Browser)
provider_handlers -> boundary/wire/store/prompt (Parent-Helfer), kein Selektor, kein Profil
tests      ->  Parent
api_bridge.rs (nach T-913) -> alle Kindmodule
```

Kindmodule dürfen **nicht** auf Browser-Selektoren, Circuit-Breaker-Politik oder Providerprofile zugreifen. Keine umgekehrte Abhängigkeit Root ← Kind für neue Fachlogik.

## Nicht-Ziele

- Keine neuen Endpunkte, keine JSON-Semantik-Änderung, keine Timeout- oder Headervertragsänderung.
- Keine neuen Auth-Arten.
- Keine Live-Behauptungen über Brains; das ist die Capability-Matrix.

## Gates (Phase 10)

```
cargo fmt --all -- --check
cargo clippy --features webview --all-targets -- -D warnings
cargo test --features webview --lib api_bridge::tests::
git diff --check
```

T-911/T-913 zusätzlich: `cargo test --features webview --lib`.

## Claim-Regeln

Quelle: `docs/TASKBOARD.json`. Ein Entwickler, eine Aufgabe. Kind-Slots (T-907–T-912) ändern **nicht** `src/api_bridge.rs`. T-913 ist blockiert, bis T-907–T-912 `done` sind.

## Verbleibende Risiken

- Doppelte Logik Root vs. Kindmodul bis T-913: Drift möglich, wenn jemand die Root-Datei und das Kind parallel ändert.
- Kindmodule kompilieren erst nach `mod …` in der Root-Datei (Clippy sieht sie vorher nicht).
- Image/Audio/Lifecycle-Handler bleiben bewusst in der Root-Datei (nicht T-908).
- `docs/API_BRIDGE.md` ist der Betriebsvertrag; dieses Dokument ist die Modulkarte.

## Agenten-Einstieg

1. `START_HERE.md` und `docs/WORK_CONTRACT.md`.
2. `git pull origin master`.
3. Freien Task in `docs/TASKBOARD.json` claimen, Branch nach `docs/GIT_GLOSSAR.md`.
4. Nur den Scope der Aufgabe anfassen.
5. Gates grün, Beleg unter `docs/proofs/T-…/`, Merge nach `master`.
