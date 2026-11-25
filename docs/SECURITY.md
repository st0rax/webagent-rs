# Sicherheitsmodell

> **Referenz:** Beschreibung des implementierten Modells, keine unabhängige
> Sicherheitsprüfung und keine Zertifizierung.
>
> **Stand:** 2026-11-25, abgeleitet aus `AGENTS.md`, `src/shell_policy.rs`,
> `src/file_actions.rs`, `src/api_bridge/boundary.rs`,
> `src/network_observation.rs`, `src/capability_proof.rs`, `src/run_store.rs`
> und `.gitignore`. Dies ist eine Beschreibung des implementierten Modells,
> **keine** unabhängige Sicherheitsprüfung und keine Zertifizierung.

## Bedrohungsmodell

Der Agent ist ein **Single-User-, lokaler** Harness. Die Shell ist nach Design
offen: Der Agent braucht generische Shell-Macht, keine Mandanten-Trennung. Das
Schutzziel ist nicht „kein Shell", sondern (1) versehentlich destruktive oder
injektionsgetriebene Befehle zu erkennen und (2) jede Entscheidung auditierbar
zu machen. Siehe `src/shell_policy.rs:1-8`.

Jede Modellausgabe ist potenziell feindlich: Text kann aus Webseiten, Tool-
Output oder einer Brain-Antwort stammen. Vertrauensgrenzen:

1. **WebView/Brain-Inhalt** → Parser (`src/protocol/`) → Controller. Der Parser
   führt nichts aus und lehnt Prosa, unbekannte Felder und doppelte IDs ab.
2. **Controller** → Policy → Werkzeuge (Shell/Datei). Die Policy sitzt vor
   `executor.execute`, nicht dahinter.
3. **Lokale API** → Bridge → Browser-Turn. Die Bridge ist ein zweiter Einstieg,
   kein Ersatz für die Shell-Policy.

## Shell

- Denylist und destruktive Muster in `src/shell_policy.rs`: rekursives
  `Remove-Item`, `rm -rf` auf Root/Home/Wildcard, `rd/rmdir /s`, `format`,
  `mkfs`, `diskpart`/`clear-disk`/`remove-partition`, `dd of=/dev/`, HKLM-Registry-Löschung,
  Fork-Bomb, `curl|sh` und `irm|iex`.
- Die Prüfung läuft **je Anweisung**, nicht über die ganze Kette; ein
  destruktives Statement blockiert die Kette. Zitierte Literale und `echo`
  werden nicht als Befehl gewertet (`Select-String "rm -rf /"` bleibt erlaubt).
- `evaluate_command_policy(...)` verlangt `user_confirmed` für Treffer;
  ohne Bestätigung → `Deny`, mit Bestätigung →auditierter `Allow`.
- Optionaler Strict-Modus (`WEBAGENT_SHELL_STRICT=1`): nur eine feste Liste
  lesender Kommandos ist erlaubt; alles andere wird verweigert.
- Umgehungsversuche werden über `RECENT_DENIALS` innerhalb von 60 Sekunden
  erkannt und mitgeloggt; das ist Erkennung, kein perfekter Schutz.
- Jede Entscheidung wird per `format_audit_line` in die Audit-Kette geschrieben.

## Dateien und Workspace

- `src/file_actions.rs` kanonisiert den Workspace-Root, lehnt `..`-Traversal ab
  und prüft Symlinks: ein Link darf nicht aus dem Root hinauszeigen.
- `edit` verlangt einen exakt und eindeutig vorkommenden Anker; `edit_batch`
  validiert alle Einträge vor dem ersten Schreibvorgang (atomar).
- `write` lehnt eine bereits vorhandene Zieldatei ab; bestehende Dateien
  werden ausschließlich über `edit` geändert.
- Ohne expliziten Workspace gilt der konfigurierte Root; `AgentController`
  kann ihn für isolierte Läufe binden (`set_workspace_root`).
- `/data`, `/profiles`, `.env*` und `runtime-workers/` sind in `.gitignore`
  ausgeschlossen. Die App liest keine Credential-Dateien; Anmeldung, CAPTCHA,
  2FA und Einmalcodes gehören dem Menschen (`login --help`, `AGENTS.md`).

## Lokale API

- Die Bridge bindet ausschließlich an Loopback; `src/api_bridge.rs:213`
  verweigert einen nicht-Loopback-Bind.
- `Authorization: Bearer <token>` oder `x-api-key` werden gegen den
  konfigurierten Token in `src/api_bridge/boundary.rs` verglichen
  (`constant_time_equal`). Fehlender/falscher Token → 401 mit dem festen Text
  `Ungueltiger oder fehlender API-Token.`
- Endpunkte: `/v1/models`, `/v1/chat/completions`, `/v1/responses`,
  `/v1/messages`. Streaming und Non-Streaming sind in
  `src/api_bridge/wire.rs` getrennt; Antworten sind `no-store`.
- Die API spricht keine Credentials aus und ersetzt keine WebView-Sitzung; sie
  vermittelt einen Turn an das angemeldete Brain.

## Redaktion und Beobachtung

- `src/network_observation.rs` ist **opt-in** (`WEBAGENT_NETWORK_OBSERVE`) und
  ausschließlich lesend: nur `Network.enable` plus drei Sensor-Events, kein
  Interception, keine Mutation, kein Port-/WebSocket-Kanal.
- URLs werden auf `scheme://host` reduziert; Query, Fragment und Userinfo —
  die Stellen mit möglichen Tokens — werden nie gespeichert.
- Listen sind auf `MAX_EVENTS = 512` begrenzt; verworfene Ereignisse werden
  gezählt, damit ein Beleg nicht still unvollständig ist.

## Persistenz und Belege

- `src/run_store.rs` schreibt `meta.json` über temporäre Datei + `rename`,
  hält `events.jsonl` unter einem prozessübergreifenden `events.lock` mit
  `fsync` und quarantinetert abgeschnittene Tails statt sie zu erraten.
- Es gibt **genau einen** Beleg-Gate: `src/capability_proof.rs` schreibt nach
  `data_dir()/capability/proofs.jsonl` über `record_measurement` /
  `record_proof` / `record_route_proof`.
- `ProofState` unterscheidet `Proven`, `Expired`, `Failed`, `Never`. Ein Beleg
  verfällt durch TTL (`WEBAGENT_PROOF_TTL_DAYS`) oder wenn die
  `needs`-Selektoren sich ändern (`selector_hash`). Kein zweiter Belegspeicher
  ist erlaubt.
- Live-Fähigkeiten brauchen Datum, Binary-Hash und Rohbeleg
  (`docs/CAPABILITY_MATRIX.json`, `docs/proofs/`); ein grüner Exit-Code allein
  ist kein Beleg.

## Bekannte Grenzen (Stand 2026-11-25)

- **Parallelität:** `src/brain_score.rs` hält `PENDING_TURN`,
  `PENDING_WEB_SEARCH` und `PENDING_SOURCE_HINTS` als prozessglobale Mutexe.
  Ein Restzustand zwischen Läufen ist nicht strukturell ausgeschlossen;
  T-970 ist dafür geclaimt und noch offen.
- **`VerifiedFree`:** `AccessMode::VerifiedFree` in
  `src/free_cloud_chat.rs` ist eine Registry-Kennzeichnung mit datiertem
  Markdown-Beleg (`docs/proofs/T-926/`), kein gate-gestützter Eintrag in
  `proofs.jsonl`. T-971 ist dafür geclaimt und noch offen.
- **Panics:** user-nahe Pfade sind nicht als panic-frei zertifiziert; `unwrap`
  kommt in Hilfs- und Testpfaden vor. Ein Security-Review mit Fuzzing/Property-
  Tests steht aus.
- **Produktivbetrieb:** kein definiertes Ziel, kein Live-WebView2-Turn im
  Release-Pfad, keine schriftliche Restrisiko-Akzeptanz
  (`docs/PRODUCTION_READINESS.md`). Diese Datei ändert daran nichts.

## Pflichten für Agenten

Keine Secrets lesen, kopieren oder loggen. Keine Force-Pushes. Keine
Anmeldung, CAPTCHA- oder 2FA-Bedienung durch Agenten. Keine destruktive
Aktion ohne ausdrückliche Freigabe. `done` erst nach Merge. Details:
`AGENTS.md`, `docs/WORK_CONTRACT.md`.
