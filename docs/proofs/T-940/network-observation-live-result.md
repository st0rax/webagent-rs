> **Referenz.** Echter Live-Nachweis für T-940: ausschließlich **lesende**
> CDP-Netzwerkbeobachtung während eines Provider-Turns über die bestehende
> WebView-OAuth-Session. Kein Intercept, kein Token-Zugriff, kein Tag, kein
> Release. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-940 — Lesende CDP-Netzwerkbeobachtung

## Freigabe und Rahmen

- T-940 ergänzt den in T-926 freigegebenen WebView-OAuth-Pfad um einen
  **opt-in**-Sensor. Es gilt weiterhin: **keine api keys, nur oauth**. Der
  Adapter kennt nur die `brain`-ID; es gibt kein Key- oder Token-Argument.
- Es wird kein Token, kein Cookie und keine Profildatei gelesen oder
  weitergegeben. Es werden keine Anfragen oder Antworten manipuliert.
- Datum: 2026-11-21, ca. 06:10–06:25 lokale Zeit.
- Binary Lauf 1: `target/debug/webagent.exe`, SHA256
  `7ECA822805A46193C7B873BD358EF67A84D5D26BFB05781B29DD2354039A7BC6`.
- Binary Lauf 2 (final, mit korrigierter Auswahlheuristik): SHA256
  `A0B5D0D1E87D4BCD6F6DF641BB23FAEBD0940F7185DE90DFECB47AD3493306CD`.
- Kein HTTP-Port im Spiel; Port `8788` und der fremde Port `8787` blieben
  unberührt. Während der Läufe wurden keine Repo-Dateien geändert. Rohdaten
  liegen außerhalb des Repos unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T940-network-observation-20261121\`.

## Implementierung

| Element | Ort |
|---|---|
| Datenmodell, Redaktion, Auswahlheuristik, Global-Log | `src/network_observation.rs` |
| Registrierung des Moduls | `src/lib.rs` |
| Drei lesende CDP-Receiver + `Network.enable` | `src/webview_runtime.rs` (`attach_network_observation`) |
| Opt-in `--net-trace`, Ausgabe auch bei Fehlschlag | `src/cli.rs`, `src/main.rs` |

Der Sensor registriert genau drei Events — `Network.requestWillBeSent`,
`Network.responseReceived`, `Network.loadingFinished` — und ruft nur
`Network.enable` auf. Es gibt **kein** `Network.setRequestInterception`, kein
`Fetch.enable` und keine Port-/WebSocket-Verbindung. Die Receiver werden beim
Tab-Ende über den `Drop` des `PageSlot` wieder abgemeldet.

Schutzregeln im Log:

- URLs werden auf `scheme://host` reduziert. Query, Fragment, Userinfo und
  Pfad — also genau die Stellen, in denen Tokens stehen könnten — werden
  **nie** gespeichert. Test `t940_redacts_tokens_userinfo_fragment_and_port`
  sichert das ab.
- Es werden nur Zähler, Status, MIME-Typ, Ressourcentyp und Zeitstempel
  gesammelt.
- Jede Liste ist auf 512 Einträge begrenzt; verworfene Ereignisse werden in
  `dropped_events` gezählt, damit ein Beleg nicht stillschweigend unvollständig
  ist. Test `t940_bounded_log_counts_dropped_events` sichert das ab.
- Ohne `--net-trace` bzw. `WEBAGENT_NETWORK_OBSERVE` werden **keine** CDP-Receiver
  registriert.

## Red/Green

Sechs neue Unit-Tests (`cargo test --lib t940_`) sind grün:

```
test network_observation::tests::t940_redacts_tokens_userinfo_fragment_and_port ... ok
test network_observation::tests::t940_no_api_request_means_fill_error ... ok
test network_observation::tests::t940_request_without_response_means_response_error ... ok
test network_observation::tests::t940_response_reports_status_and_finish_duration ... ok
test network_observation::tests::t940_prefers_sse_provider_call_over_later_static_fetch ... ok
test network_observation::tests::t940_bounded_log_counts_dropped_events ... ok
test result: ok. 6 passed; 0 failed
```

Der Test `t940_prefers_sse_provider_call_over_later_static_fetch` bildet den
echten Fehler aus Lauf 1 ab: ein später 4-ms-Static-Fetch darf nicht gegen den
früheren SSE-Provider-Call gewinnen.

## Live-Ergebnisse

Befehl je Lauf:
`webagent cloud web-view-stream --brain deepseek --prompt "Antworte genau mit <NONCE> und nichts anderem." --headless --net-trace`

| Lauf | Nonce | Exit | Dauer | Token-Antwort | `verdict` |
|---:|---|---:|---:|---|---|
| 1 | `T940-EB0F8B8395` | 0 | 15,9 s | exakt `T940-EB0F8B8395` | ❌ `fe-static.deepseek.com`, 4 ms — **falsch gewählt** |
| 2 | `T940-A58B1F5DC0` | 0 | 14,9 s | exakt `T940-A58B1F5DC0` | ✅ `chat.deepseek.com`, `text/event-stream`, 200, 1101 ms |

Beide Läufe lieferten `adapter: webview_oauth`, `api_key: false`, `oauth_session:
true` und die Events `started → token → completed`. Die Nonces wurden je Lauf
frisch per GUID erzeugt und waren vorher nirgends gespeichert.

### Was Lauf 1 gezeigt hat — und die Korrektur

Lauf 1 war ein ehrlicher Negativbefund, kein Beleg: Die damalige Heuristik
„jüngster Request mit Antwort gewinnt" wählte einen statischen Fetch von
`https://fe-static.deepseek.com` mit 4 ms, obwohl der eigentliche Provider-Call
(`request_id 5568.202`, POST `https://chat.deepseek.com`,
`mime_type text/event-stream`, start 3709 ms, response 4048 ms, finish 5385 ms =
1676 ms) im selben Log stand. Der Token-Output war korrekt, die **Klassifikation**
war falsch.

Daraufhin wurde die Auswahl gewichtet:

- `text/event-stream` → +8 (stärkstes Signal für den Antwortstrom),
- `POST` → +3,
- Origin der Dokumentseite → +2,
- 2xx-Status → +1,
- bei Gleichstand gewinnt die längere Dauer.

Das Urteil trägt jetzt `request_id`, `mime_type` und `selection_reason`, damit
die Wahl nachvollziehbar ist. Lauf 2 wurde mit dem neuen Binary wiederholt.

### Verifikation Lauf 2 (Rohereignisse)

Gewählter Request `7812.198`, direkt aus `live-deepseek-nettrace-2.stdout.json`:

```
request  {"at_ms":3198,"method":"POST","origin":"https://chat.deepseek.com","request_id":"7812.198","resource":"xhr"}
response {"at_ms":3539,"mime_type":"text/event-stream","origin":"https://chat.deepseek.com","request_id":"7812.198","status":200}
finish   {"at_ms":4299,"encoded_bytes":3065,"request_id":"7812.198"}
```

Daraus ergibt sich `duration_ms = 4299 − 3198 = 1101`, exakt der gemeldete Wert.
Das `verdict` lautet:

```json
{
  "provider_response": {
    "request_id": "7812.198",
    "origin": "https://chat.deepseek.com",
    "status": 200,
    "mime_type": "text/event-stream",
    "duration_ms": 1101,
    "finished": true,
    "selection_reason": "SSE-Antwort (text/event-stream), POST, Origin der Dokumentseite, 2xx"
  }
}
```

Zähler des Laufs: `total_requests=74`, `total_responses=54`,
`total_finished=54`, `dropped_events=0`.

## Ehrliche Einschränkungen

- Es wurden **zwei** Läufe ausgewertet, beide mit `deepseek`. Das genügt für den
  Nachweis „die Beobachtung erkennt den Provider-Call in einem echten Turn",
  nicht für eine Stabilitätsgarantie über Brains oder Tage.
- Die Auswahl ist eine **begründete Heuristik**, kein Beweis, dass ein bestimmter
  Request „der" Chat-Call war. Gestützt wird sie im konkreten Lauf durch
  SSE-MIME, POST, Provider-Origin und den gleichzeitigen Token-Output. Bei
  einem anderen Provider mit anderer Antwortform ist die Heuristik erneut zu
  prüfen.
- Die gemessene Dauer schwankt naturgemäß (Lauf 2: 1101 ms; der fälschlich
  gewählte Static-Fetch in Lauf 1: 4 ms). Als Beleg dient die Kombination aus
  Origin, MIME-Typ und Request-ID — nicht eine einzelne Zahl.
- `dropped_events=0` in beiden Läufen; die Begrenzung auf 512 Einträge wurde
  daher nicht wirksam, sondern nur unit-getestet.

Rohdatei-Hashes (SHA256) und Zuordnung:

```
live-deepseek-nettrace-1.stdout.json  36DB8A29901989BD217A83931EE4939CA62E7B1C55FC18D264FE123CCF83D415  (Lauf 1, 33665 Byte)
live-deepseek-nettrace-1.stderr.log   E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
live-deepseek-nettrace-2.stdout.json  8549AE948F6C36136B4F5F40242781FBC0D14D825AAFF173F2F76BC0F644BD7F  (Lauf 2, 33330 Byte)
live-deepseek-nettrace-2.stderr.log   E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
```

## Gates (finaler Stand)

| Gate | Ergebnis |
|---|---|
| `cargo test --lib t940_` | 6 passed / 0 failed |
| `cargo test --lib` | 1498 passed / 0 failed / 1 ignored |
| `cargo fmt --all` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo build` | grün (Binary `A0B5D0D1…3306CD`) |
| `git diff --check` | grün |

## Einordnung

T-940 ist technisch erfüllt: lesende Beobachtung, Redaktion, begrenztes Log,
nachvollziehbares Urteil und ein echter Livebeleg mit korrekt gewähltem
Provider-Call. **Kein Tag, kein GitHub-Release.** Die Heuristik bleibt an das
Verifikationsdatum `2026-11-21` gebunden; bei einem anderen Provider- oder
Antwortformat ist sie erneut gegen einen Livebeleg zu prüfen.
