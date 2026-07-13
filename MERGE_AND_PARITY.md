# webagent + webagent-rs — Vergleich, Parität, Merge & Plattform-Plan

> **Ziel (Storax):** EIN Projekt, plattformunabhängig (Windows, Linux, Android),
> **lokaler Browser auf jeder Plattform** (KEINE Remote-CDP-Verbindung zu einem
> Desktop-Chrome als Kern-Strategie), self-contained (keine Python-/Toolchain-
> Abhängigkeiten zur Laufzeit), mit Release-Binaries. `webagent-rs` (Rust) ist die
> Zukunft; `webagent` (Python) wird nach erreichter Parität abgelöst.

## 1. Feature-Vergleich (CLI)

| Befehl | Python `webagent` | Rust `webagent-rs` | Anmerkung |
|---|---|---|---|
| run | ✅ | ✅ | Kern-Loop, in Rust live verifiziert (6/8 Provider) |
| repl | ✅ | ✅ | |
| login | ✅ | ✅ | |
| diagnose | ✅ | ✅ | |
| doctor | ✅ | ✅ | |
| watchdog | ✅ | ✅ | |
| maintenance-check | ✅ | ✅ | |
| **brains-health** | ✅ | ❌ | Pre-flight ohne Browser — FEHLT in Rust |
| **relay** | ✅ | ⚠️ nur `examples/relay.rs` | Kein CLI-Befehl — FEHLT |
| **oobe** | ✅ | ❌ | Ersteinrichtungs-Wizard — FEHLT |
| **genius / consensus** | ✅ | ❌ | Multi-Agent-Council — FEHLT |
| presence-check | ❌ | ⚠️ **kaputt** | Rust-Neuzugang, siehe §3 |

## 2. Modul-Lücken (Python hat, Rust fehlt)

Kern portiert (protocol, controller, executor, browser, doctor, watchdog, memory,
transcript, timeouts, loop_guard, observer, prompts, config). **Fehlend in Rust:**

- **Multi-Agent-Schicht:** `council`, `consensus`, `battleroyale`,
  `countup_eval`, `leader_calibration`, `performance` — die gesamte Genius-Council-
  /Bewertungs-Logik.
- **`oobe`** (Ersteinrichtung), **`relay`** (als Modul/Befehl), **`agent`**
  (AgentManager — in Rust durch controller/brain ersetzt, funktional ok).
- `browser_pool`/`browser_launch` — in Rust durch `cdp`/`browser` ersetzt (ok).
- `terminal_status` — in Rust inline (ok).

## 3. Plattform-Verstöße im aktuellen Rust-Stand (MÜSSEN behoben werden)

Diese verletzen das „self-contained, plattformunabhängig, lokaler Browser"-Ziel:

1. ~~**`presence-check` ist ein Windows-/Python-Shim.**~~ ✅ **ERLEDIGT (entfernt).**
   presence-check ist Qwens Personal-Assistant-Feature (erkennen ob Storax zuhause
   ist) und gehört NICHT in webagent (Storax-Entscheidung). Aus webagent-rs entfernt;
   Qwens `presence_check.py` bleibt in Qwens eigenem Bereich.
2. **Android-Strategie = Remote-CDP zu Desktop-Chrome.** `WEBAGENT_CDP_ENDPOINT`
   lässt Android auf einen entfernten Desktop-Chrome zeigen (README/`cdp.rs`).
   **Das ist nicht das Ziel** — auf Android soll der **lokale** Browser (das Chrome
   des Geräts, per lokalem DevTools-Socket) genutzt werden. Remote-CDP darf nur ein
   **optionaler** Escape-Hatch bleiben, nicht die Kern-Strategie.
3. **Android-Build über `cargo zigbuild`/`zig`.** Scheitert auf Termux („zig
   unbekannt"). Besser: **nativer Build auf dem Gerät** (`pkg install rust; cargo
   build --release`) — keine Cross-Toolchain, kein zig.

## 4. Plan (priorisiert; Stabilität bleibt Prio 1)

**Phase S — Stabilität fertig (läuft):** die 2 harten Integrationen
(`webagent/gemini`, `webagent/qwen`) live fixen. 6/8 laufen bereits.

**Phase P — Plattform-Bereinigung (§3):**
- P1 `presence-check` nativ in Rust (kein Python/cmd/hardcoded path) — oder raus.
- P2 Android: lokaler Browser (Android-Chrome via lokalem CDP-Socket); Remote-CDP
  auf „optional" zurückstufen; README/Default korrigieren.
- P3 Termux: nativer `cargo build` dokumentieren; zig-Weg entfernen/optional.

**Phase F — Funktionsparität (§1/§2):**
- F1 `relay` als CLI-Befehl (Logik existiert schon in `examples/relay.rs`).
- F2 `brains-health` (Pre-flight ohne Browser).
- F3 `oobe` (Wizard, nativ).
- F4 Multi-Agent: `genius/council/consensus` nach Rust (größter Brocken).

**Phase M — Merge:** Wenn Rust Parität + Stabilität hat: `webagent` (Python)
archivieren; `webagent-rs` wird der einzige Baum (ggf. in `webagent/` umziehen).

**Phase R — Releases/Binaries:**
- R1 Windows (x86_64-pc-windows-gnu) — Release-Build steht schon lokal.
- R2 Linux (x86_64-unknown-linux-gnu / musl).
- R3 Android (aarch64) — **erst nach P2/P3** (lokaler Browser + nativer Build).
- GitHub-Actions Release-Workflow, der die drei Artefakte an ein Tag hängt.

## 5. Entscheidungen, die ich von Storax brauche

- **Merge-Richtung:** Rust ersetzt Python vollständig (empfohlen), Python nur
  archivieren? Oder Python parallel halten?
- ~~**presence-check:** Zweck?~~ ✅ GEKLÄRT: Qwens Personal-Assistant-Feature, nicht
  webagent. Aus webagent-rs entfernt.
- **Remote-CDP:** als optionalen Escape-Hatch behalten (empfohlen) oder ganz raus?
- **Genius-Council-Parität** wirklich nötig für v1, oder später? (Größter Aufwand.)
