> **Referenz.** Echter Live-Nachweis für T-926: der erste `VerifiedFree`-Adapter
> läuft über eine nutzergehörige **WebView-OAuth-Session**, ohne API-Key, ohne
> Credit-Pfad. Kein Tag, kein Release. Operativer Stand:
> `docs/CURRENT_WORK.md`.

# T-926 — VerifiedFree-Adapter über WebView-OAuth

## Freigabe und Rahmen

- Der Eigentümer hat am 2026-11-21 ausdrücklich vorgegeben: **„keine api keys.
  nur oauth."** und anschließend **„Ja, WebView-Session (Empfohlen)"**
  bestätigt. Key-basierte Anbieter (Groq, Gemini-API-Key, Cloudflare, NVIDIA,
  OpenRouter) wurden deshalb **nicht** angebunden.
- Es wird kein Token, kein Cookie und keine Profildatei gelesen oder
  weitergegeben. Die Sitzung liegt im browsergehörigen Nutzerprofil; der
  Adapter kennt nur die `brain`-ID. CLI und Code haben **kein** Key-Argument.
- Datum: 2026-11-21, ca. 05:55–06:00 lokale Zeit.
- Binary der drei Live-Läufe: `target/debug/webagent.exe`, SHA256
  `97E4B4DB555346BB95D06097B11C3D320BF761552D940EFD80CCD71E68B611FB`.
  Der finale, committete Stand hat SHA256
  `D39AC37E7C82F68636C5E4542CFE25F8D402D7717A41574C51117A67F8667678`;
  der einzige Unterschied ist die Registry-Aufnahme mit Verifikationsdatum —
  Adapter, Transport und CLI-Verhalten sind unverändert.
- Kein HTTP-Port im Spiel (T-926 nutzt die WebView direkt); Port `8788` und der
  fremde Port `8787` blieben unberührt.
- Während der Läufe wurden keine Repo-Dateien geändert. Rohdaten liegen außerhalb
  des Repos unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T926-webview-oauth-20261121\`.

## Implementierung

| Element | Ort |
|---|---|
| `WEBVIEW_OAUTH_MODEL_ID = "webagent/webview-oauth-session"` | `src/free_cloud_chat.rs` |
| `webview_oauth_model()` — `VerifiedFree`, `adapter_compatible`, Datum `2026-11-21` | `src/free_cloud_chat.rs` |
| `WebViewSessionTransport` — injizierbare Nahtstelle, **kein** Key-/Token-Parameter | `src/free_cloud_chat.rs` |
| `BrowserInferenceWebViewTransport` — ruft `browser_inference::complete()` mit `tools: &[]`, `tool_choice: None` | `src/free_cloud_chat.rs` |
| `WebViewSessionAdapter<T>` — Free-only-Gate → Modellbindung → Transport → `Started/Token/Completed` | `src/free_cloud_chat.rs` |
| CLI `cloud web-view-stream --brain <brain> --prompt <prompt> [--headless]` | `src/cli.rs`, `src/main.rs` |

Der Adapter prüft in dieser Reihenfolge: nichtleerer Prompt, `decide_route`
muss `Auto` ergeben (sonst `RouteDenied`, Transport bleibt unberührt), die
Modellkennung muss genau `webagent/webview-oauth-session` sein (sonst
`InvalidRequest`), erst dann wird die Sitzung angesprochen. Leere Antworten und
Transportfehler werden als `AdapterFailure` ohne Teilereignisse gemeldet.

## Red/Green

Sechs neue Tests wurden gegen den absichtlich leeren Adapterrumpf ausgeführt
(**rot**): 5 `FAILED`, 1 `passed` (nur die Metadatenprüfung, die den Rumpf nicht
berührt). Danach grün:

```
running 6 tests
test free_cloud_chat::tests::t926_webview_adapter_emits_events_from_session_text ... ok
test free_cloud_chat::tests::t926_transport_never_called_on_denied_route ... ok
test free_cloud_chat::tests::t926_webview_adapter_rejects_foreign_model ... ok
test free_cloud_chat::tests::t926_webview_adapter_maps_transport_failure ... ok
test free_cloud_chat::tests::t926_webview_adapter_rejects_empty_reply ... ok
test free_cloud_chat::tests::t926_webview_model_is_verified_free_and_adapter_compatible ... ok
test free_cloud_chat::tests::t926_registry_has_exactly_one_auto_routable_verified_free_entry ... ok
test result: ok. 7 passed; 0 failed
```

(Ein siebter Test — `t926_registry_has_exactly_one_auto_routable_verified_free_entry`
— kam nach dem Live-Nachweis hinzu und sichert die Registry-Invariante ab.)

## Live-Ergebnisse

Befehl je Lauf:
`webagent cloud web-view-stream --brain <brain> --prompt "Antworte genau mit <NONCE> und nichts anderem." --headless`

| Lauf | Brain | Nonce | Exit | Dauer | Token-Antwort |
|---:|---|---|---:|---:|---|
| 1 | gemini | `T926-0A8327B448` | 0 | 17,6 s | `T926-0A8327B448` + nachgelagerter UI-Text („Möchtest du mehr über meine Funktionsweise erfahren? Ja") |
| 2 | gemini | `T926-38A1C3E14F` | 0 | 15,8 s | exakt `T926-38A1C3E14F` |
| 3 | deepseek | `T926-AD4B110890` | 0 | 13,2 s | exakt `T926-AD4B110890` |

Alle drei Läufe lieferten `adapter: webview_oauth`, `oauth_session: true`,
`api_key: false` und die Events `started → token → completed` mit der
Modellkennung `webagent/webview-oauth-session`. Die Nonces wurden je Lauf frisch
per GUID erzeugt und waren vorher nirgends gespeichert — eine erfundene oder
zwischengespeicherte Antwort ist damit ausgeschlossen.

**Ehrliche Einschränkungen:** Lauf 1 enthielt zusätzlich Oberflächentext der
Chat-UI; die Läufe 2 und 3 waren exakt. Lauf 3 meldete auf stderr einen
harmlosen WebView2-Teardown-Hinweis
(`Failed to unregister class Chrome_WidgetWin_0. Error = 1412`); die Antwort
war dennoch vollständig. Es wurden **drei** Läufe ausgewertet, nicht mehr; das
genügt für den Nachweis „Adapter funktioniert mit OAuth-Session", nicht für
eine Stabilitätsgarantie über viele Tage.

Rohdatei-Hashes (SHA256) und Zuordnung:

```
live-gemini-1.stdout.json   E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (0 Byte, Erstversuch)
live-gemini-1.stderr.log    D0A79851EB869C1104B261B7E0DEBB1EC093C51ADACBF80AD69A217496B6B498  (falscher Subcommand-Name)
live-gemini-2.stdout.json   53F613F3DC69619FEDF3710A3115D09AC7E1F9083814BAE195178BC727B305D1  (Lauf 1)
live-gemini-2.stderr.log    E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
live-gemini-3.stdout.json   0AE60FB3EB8E6B3AC58E2F575FA06BA88E774E1EE0E239AD54B23E0E999C78E3  (Lauf 2)
live-gemini-3.stderr.log    319B89808B16B29040A8935EBC087ED89621753BA4720B36DA3E91532D2CA170  (WebView2-Teardown-Hinweis)
live-deepseek-3.stdout.json D6EF17B0D8DF8A48AF0A3A1284687F46906195673A5F406189420EB9725C2BD6  (Lauf 3)
live-deepseek-3.stderr.log  E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
```

`live-gemini-1.*` stammt aus dem fehlgeschlagenen Erstversuch mit dem falschen
Subcommand-Namen (`webview-stream` statt `web-view-stream`) und zählt **nicht**
als Live-Beleg. Der Hash der leeren stdout-Datei ist der SHA256 der leeren
Eingabe (`E3B0C442…`).

## Registry- und CLI-Nachweis

Nach dem Live-Nachweis wurde `webview_oauth_model()` als **genau ein**
`VerifiedFree`-Eintrag in `default_registry()` aufgenommen. Der Test
`t926_registry_has_exactly_one_auto_routable_verified_free_entry` prüft, dass
im `free_only`-Modus ausschließlich diese Kennung zu `RouteDecision::Auto`
führt. `huggingface/inference-providers` bleibt `ExplicitCredits` und wird
weiterhin als `manual_only` ausgewiesen:

```
$ webagent cloud decide --model-id huggingface/inference-providers
{
  "decision": {
    "decision": "manual_only",
    "model_id": "huggingface/inference-providers",
    "reason": "Free-only-Modus erlaubt keine automatische Ausführung: nur nach ausdrücklicher Kredit-/Token-Freigabe."
  },
  "free_only": true
}
```

## Gates

| Gate | Ergebnis |
|---|---|
| `cargo test --lib t926_` | 7 passed / 0 failed |
| `cargo test --lib` | 1491 passed / 0 failed / 1 ignored |
| `cargo fmt --all --check` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo build` | grün (Binary `97E4B4DB…B611FB`) |
| `git diff --check` | grün |

## Einordnung

T-926 erfüllt seine DoD: Adapter-Test, `decide` nur für `VerifiedFree` im
Auto-Pfad, aktualisiertes Statusdokument und dieser Beleg. **Kein Tag, kein
GitHub-Release.** Die Registry-Aufnahme bleibt an das Verifikationsdatum
`2026-11-21` gebunden; bei einer späteren Sitzungsunverträglichkeit ist der
Eintrag erneut zu prüfen, nicht zuversichtlich zu belassen.
