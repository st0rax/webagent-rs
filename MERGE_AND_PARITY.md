# webagent + webagent-rs — Vergleich, Parität, Merge & Plattform-Plan

> **Ziel (Storax):** EIN Projekt, plattformunabhängig (Windows, Linux),
> **lokaler Embedded WebView** auf jeder Plattform (kein CDP als Kern-Strategie),
> self-contained (keine Python-/Toolchain-Abhängigkeiten zur Laufzeit), mit
> Release-Binaries. `webagent-rs` (Rust) ist die Zukunft; `webagent` (Python) wird
> nach erreichter Parität abgelöst.

**Stand:** v0.5.0 (2026-07-14) — CDP entfernt, Embedded WebView (`wry`/`tao`).

## 1. Feature-Vergleich (CLI)

| Befehl | Python `webagent` | Rust `webagent-rs` | Anmerkung |
|---|---|---|---|
| run | ✅ | ✅ | Kern-Loop |
| repl | ✅ | ✅ | Session pro Turn (Optimierung offen) |
| login | ✅ | ✅ | Sichtbares WebView |
| diagnose | ✅ | ✅ | |
| doctor | ✅ | ✅ | |
| watchdog | ✅ | ✅ | |
| maintenance-check | ✅ | ✅ | |
| **brains-health** | ✅ | ✅ | Pre-flight ohne Browser |
| **relay** | ✅ | ✅ | CLI + `examples/relay.rs` |
| **oobe** | ✅ | ✅ | Ersteinrichtungs-Wizard |
| **genius / consensus** | ✅ | ❌ | Multi-Agent-Council — DEFERRED |
| presence-check | ❌ | ❌ | Ausgelagert nach `presence-monitor` |

## 2. Modul-Lücken (Python hat, Rust fehlt)

Kern portiert (protocol, controller, executor, browser, browser_pool, doctor,
watchdog, memory, transcript, relay, brains_health, oobe). **Fehlend in Rust:**

- **Multi-Agent-Schicht:** `council`, `consensus`, `battleroyale`,
  `countup_eval`, `leader_calibration`, `performance` — DEFERRED (siehe
  `docs/GENIUS_COUNCIL_CONCEPT.md`).
- `browser_pool` — ✅ in Rust (`browser_pool.rs`), Shared-Profil + Tab je Brain.
- Browser-Backend — ✅ WebView (`webview_runtime`, `page_driver`), kein CDP.

## 3. Plattform-Stand

1. **`presence-check`:** ✅ Aus webagent-rs entfernt; eigenes Repo `presence-monitor`.
2. **Android:** Nicht v0.5.0-Ziel. Frühere Remote-CDP-Strategie ist **obsolet**.
   Android erfordert eigenen WebView-Plan (später).
3. **CI:** `cargo test/clippy --no-default-features` auf ubuntu (ohne GTK/WebView2).

## 4. Plan (priorisiert)

**Phase S — Stabilität:** Provider-Integrationen nach WebView-Migration live
verifizieren (siehe `PROVIDER_STATUS.md`).

**Phase P — Plattform:**
- P1 Linux CI grün (`--no-default-features`, unix-Executor-Tests).
- P2 Android: WebView-Strategie definieren (nach Desktop-Stabilität).

**Phase F — Funktionsparität:**
- F1 `relay` als CLI — ✅ erledigt.
- F2 `brains-health` — ✅ erledigt.
- F3 `oobe` — ✅ erledigt.
- F4 Multi-Agent — DEFERRED.
- F5 REPL: Browser-Session über Turns offen halten — offen.

**Phase M — Merge:** Wenn Rust Parität + Stabilität hat: `webagent` (Python)
archivieren; `webagent-rs` wird der einzige Baum.

**Phase R — Releases/Binaries:**
- R1 Windows (x86_64-pc-windows-gnu).
- R2 Linux (x86_64-unknown-linux-gnu / musl).
- R3 Android — nach P2.

## 5. Entscheidungen (2026-07-13/14)

- ✅ **Merge-Richtung:** Rust ersetzt Python vollständig.
- ✅ **presence-check:** Eigenes Repo `presence-monitor`, nicht webagent.
- ✅ **CDP entfernt:** Embedded WebView ist die Kern-Strategie (v0.5.0).
- ✅ **Genius-Council:** DEFERRED.

## 6. Projekt-/Repo-Aufteilung

- `webagent-rs` — DAS webagent-Projekt (Rust). Eigenes Repo.
- `presence-monitor` — Home-Presence-Erkennung. Eigenes Repo.
- `bot2bot` — Agent-Messaging (eigenes Repo).
- Maßgeblicher Gesamtplan: `Desktop/HAUPTAUFGABENPLAN.md`.