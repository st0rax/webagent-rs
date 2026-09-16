<!-- **Referenz: Beleg der T-924-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-924 Handoff - CI-Gate: api_bridge Kindmodule verdrahtet

- Task: T-924
- Owner: local/opencode
- Branch: `refactor/T-924-ci-gate-wired-modules` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `168da7d` (Status `claimed`, owner `local/opencode`)

## Gate-Pfad

`src/api_bridge/tests.rs`: Test `kindmodul_dateien_sind_im_root_verdrahtet`
(api_bridge::tests). Regel: jede `*.rs`-Datei unter `src/api_bridge/`
(ausser `tests.rs`/`mod.rs`) braucht in `src/api_bridge.rs` eine passende
`mod <stem>;`-Zeile. Fehlt sie, failt der Test mit Namen der Orphan-Datei.

CI-Anbindung ohne fremden Umbau: laeuft automatisch im bestehenden Job
"Test (volle Features)" in `.github/workflows/ci.yml` (`cargo test --lib`).
Deckt store/content/catalog/response_protocol und kuenftige Kindmodule ab.

## Beispiel-Fail (reproduzierbar)

Temporaere Orphan-Probe `src/api_bridge/zz_gate_probe.rs` angelegt, dann
`cargo test --features webview --lib kindmodul`:

    thread 'api_bridge::tests::kindmodul_dateien_sind_im_root_verdrahtet'
      panicked at src\api_bridge\tests.rs:1472:5:
    Orphan-Kindmodule ohne mod-Zeile in api_bridge.rs: zz_gate_probe
    test result: FAILED. 0 passed; 1 failed
    error: test failed, exit code 101

Probe entfernt, Re-Run gruen (1 passed). Vollständige Ausgabe siehe
`docs/proofs/T-924/gates.txt`.

## Diff-Scope

- `src/api_bridge/tests.rs`: +37 Zeilen (Gate-Test, Abschnittskommentar).
- `docs/proofs/T-924/`: `gates.txt`, `handoff.md` (dieses).

Keine fachliche Refaktorierung; kein CI-Job ausserhalb des Gates geaendert
(Non-Goals eingehalten).

## Branch / Commit

- Branch: `refactor/T-924-ci-gate-wired-modules`
- Commit: T-924 Gate-Test (als opencode), Basis `168da7d` (Claim) -- siehe
  Integrationsprotokoll `docs/proofs/*/T-924`-Praxis der vorherigen Tasks.