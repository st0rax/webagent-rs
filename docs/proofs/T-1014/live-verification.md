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

Kein `prompt_zu_lang`, kein Composer-Timeout, kein Browserabsturz, kein Reconnect. Der Prozess PID `4484` und Port `8788` blieben nach beiden Turns unverändert aktiv.

## Grenze

Zwei Turne, zwei Brains, bestehende Sitzungen, lokale Loopback-Brücke. Kein Dauerbetrieb, kein Fern-Deployment, kein Cap-Grenzfall jenseits von 24.000 Zeichen in diesem Lauf (der Fail-fast-Pfad ist in `docs/proofs/T-1014/gates.txt` per Unit-Test belegt). Keine Zugangsdaten gelesen oder ausgegeben.
