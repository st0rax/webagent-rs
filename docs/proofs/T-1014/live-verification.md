# T-1014 live verification on the restarted bridge — 2026-11-26

> **Referenz.** Zwei echte Provider-Turns durch den lokal neu gestarteten Bridge-Prozess PID `4484` (`webagent 0.11.8 (84ba99fb5)`). Damit ist der Fix nicht nur gebaut und deployt, sondern am lebenden Browserpfad gemessen. Kein Login, keine Zugangsdatenausgabe, kein Tag/Release.

## Rahmen

- Ziel: `http://127.0.0.1:8788/v1/chat/completions`, `stream=false`, Authorization-Header aus der vorhandenen `WEBAGENT_API_KEY`-Umgebungsvariable. Der Wert wurde weder angezeigt noch gespeichert.
- Vorher: `/health` → `{"service":"webagent-provider-bridge","status":"ok"}`; `/v1/models` → 9 Modelle, `webagent/gemini` mit `context_window=12000`, `max_tokens=4000`.
- Beide Requests liefen nacheinander; die Bridge serialisiert Turns desselben Brains, hier waren es zwei verschiedene Brains.

## 1. Gemini — langer Prompt, ehemaliger Zeichen-für-Zeichen-Fallback

- Prompt: 2.041 Zeichen aus wiederholter Aufforderung plus Schlusszeile `Antworte jetzt exakt mit T1014-GEMINI-OK.`; damit bewusst über der alten 256-Zeichen-Fallback-Schwelle und unter dem 24.000-Zeichen-Cap.
- Ergebnis: HTTP 200, `id=chatcmpl-18ebad8d26060bfc`, `model=webagent/gemini`, `finish_reason=stop`, Antwort exakt `T1014-GEMINI-OK`, Gesamtlatenz 18.582 ms clientseitig (Event: 18.406 ms).
- Event `2026-11-26T19:01:38.175360+00:00`: `success=true`, `reason=null`, `prompt_chars=2210`, `pasted_chars=2210`, `expected_chars=2210`, `sends=1`, `phase=content_check`.
- Aussage: Der Composer hat den gesamten Text in einem Paste gehalten; es gab keinen `type_text_char_by_char`-Pfad und keinen `prompt_zu_lang`-Abbruch. Frühere Gemini-Turns mit 38.561–150.325 Zeichen brauchten 69–667 s bei `pasted_chars=0`, `sends=0`.

## 2. ChatGPT — 48.041 Zeichen, ehemaliger `content_check`-Fehler

- Prompt: 48.041 Zeichen Fülltext plus Schlusszeile `Antworte am Ende exakt mit T1014-LANG-OK.`; JSON-Body 48.177 Byte. Vergleich zum alten Fehlerevent um 18:50:51 UTC: 45.601 Zeichen, `pasted_chars=45.433`, `sends=0`, `reason=send_phase=content_check: Composer-Feld nicht gefunden (Timeout)`.
- Ergebnis: HTTP 200, `id=chatcmpl-18ebad9761b98228`, `model=webagent/chatgpt`, `finish_reason=stop`, Antwort `chatgpt  T1014-LANG-OK` (22 Zeichen), Gesamtlatenz 35.515 ms clientseitig (Event: 25.510 ms).
- Event `2026-11-26T19:02:22.126891+00:00`: `success=true`, `reason=null`, `prompt_chars=48213`, `pasted_chars=48216`, `expected_chars=48213`, `sends=1`, `phase=settle`, `focus_arrived=true`, `focus_tries=3`.
- Aussage: Die tolerante `composer_holds_text`-Prüfung trägt auch bei ~48k Zeichen; der Turn sendet genau einmal und schließt sauber ab. Die leichte Längenabweichung (3 Zeichen) liegt innerhalb der erlaubten 10-%-Bandbreite und ist kein truncierter Text.

## 3. Over-Cap-Fail-fast — Pose ohne Browserstart

- Prompt: 25.000 Zeichen `x ` an `webagent/gemini`, `stream=false`. Erhalten wurden 25.169 Zeichen inklusive Prompt-Rahmen; das Cap liegt bei 24.000.
- Ergebnis: HTTP 502, `type=invalid_request_error`, Nachricht `prompt_zu_lang: gemini nimmt maximal 24000 Zeichen an (gemessen), erhalten 25169; bitte Kontext verkleinern`. Latenz 177 ms — vor jeder Breaker-, Backend- oder Browserarbeit.
- Event `2026-11-26T19:37:07.165545+00:00`: `success=false`, `reason=prompt_zu_lang…`, `latency_ms=0`, `prompt_chars=25169`, **kein** `turn`-Objekt, `sends` nicht gesetzt. Der Breaker-Zustand blieb unverändert (kein `gemini`-Eintrag neu).
- Aussage: Der ueberlange Prompt wird verworfen, nicht still gekuerzt und nicht an einen Browser übergeben. Damit ist auch der dritte T-1014-Pfad live belegt, nicht nur per Unit-Test.

## Betrieb nach dem Restart

Zwischen 19:22 und 19:35 UTC liefen elf weitere echte Turns durch die Bridge (zehn `chatgpt`, einer `gemini` aus Abschnitt 1), alle `success=true`, alle mit `sends=1`, `reason=null`. Im stderr erscheinen mehrfach `[T-961] semantische Werkzeugverweigerung; ein Frisch-Chat-Retry mit dem ursprünglichen Auftrag.` Das ist die dokumentierte, genau einmalige Eskalation aus T-961 (Task `done`), kein Fehler; `Retry erschoepft` kam nicht vor. `/health` bleibt `ok`, `/v1/models` liefert neun Modelle, `/v1/models` ohne Token liefert 401, PID `4484` und Port `8788` blieben stabil.

## Logbild nach den Turns

`bridge.err.restart-20261126.log` enthält nur die bekannten, sachlichen Hinweise:

```
[api] chat model=chatgpt stream=true tools=12
[relay] chatgpt: Websuche nicht isolierbar (not_isolatable)
[api] chat model=webagent/gemini stream=false tools=0
[relay] chatgpt: Websuche nicht isolierbar (not_isolatable)
[relay] gemini: Websuche nicht isolierbar (not_isolatable)
[api] chat model=webagent/chatgpt stream=false tools=0
[relay] chatgpt: Websuche nicht isolierbar (not_isolatable)
```

Bis auf den bewusst ausgelosten Over-Cap-Abbruch gab es kein `prompt_zu_lang`, keinen Composer-Timeout, keinen Browserabsturz und kein Reconnect. Der Prozess PID `4484` und Port `8788` blieben durchgehend aktiv.

## Grenze

Drei kontrollierte Turne/Abbrüche, zwei Brains, bestehende Sitzungen, lokale Loopback-Brücke, dazu Beobachtung der anschliessenden Regelturns. Kein Dauerbetrieb, kein Fern-Deployment. Keine Zugangsdaten gelesen oder ausgegeben.
