**Referenz.** Betriebsanleitung fuer den lokalen WebAgent-Bridge-Dauerbetrieb auf diesem Windows-Rechner (Stand 2026-11-26). Nur Loopback, kein Fernzugriff, kein Login durch den Agenten.

# Lokaler Bridge-Dauerbetrieb

Die Bridge ist kein Windows-Dienst im Session-0-Kontext. Sie laeuft als Prozess des angemeldeten Benutzers, damit eine bestehende Browser-Anmeldung nutzbar bleibt. Beim Anmelden erscheint ein sichtbares Terminalfenster mit der Frage, ob die Bridge gestartet werden soll.

| Datei | Zweck |
|---|---|
| `scripts/bridge-start-prompt.ps1` | Sichtbares Ja/Nein-Fenster beim Anmelden. `J` setzt das Opt-in und startet einmal; `N` lässt die Bridge aus. |
| `scripts/supervise-bridge.ps1` | Idempotenter Health-Check auf `http://127.0.0.1:8788/health`. Startet die Bridge nur, wenn sie nicht gesund ist. |
| `scripts/setup-bridge-autostart.ps1` | Installiert oder entfernt Verknuepfung und zyklische Aufgabe. |

## Installation

```powershell
pwsh -NoProfile -ExecutionPolicy RemoteSigned -File scripts/setup-bridge-autostart.ps1
```

Das Skript legt eine Verknuepfung unter `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WebAgent Bridge Start.lnk` an und registriert die Aufgabe `WebAgent Bridge Health (5min)` fuer den aktuellen Benutzer (Interactive, Limited, alle 5 Minuten). Es braucht keine Administratorrechte und ueberschreibt keine fremden Tasks.

## Bedienung

Beim Anmelden fragt das Fenster `Bridge jetzt starten? [J/N]`:

- **J** – das Opt-in `%LOCALAPPDATA%\webagent\bridge\enabled` wird gesetzt und der Supervisor gestartet. Die zyklische Aufgabe haelt die Bridge danach gesund.
- **N** – das Opt-in wird entfernt, eine noch laufende eigene Bridge gestoppt und die zyklische Aufgabe tut nichts.

Manuell:

```powershell
pwsh -NoProfile -ExecutionPolicy RemoteSigned -File scripts/supervise-bridge.ps1
pwsh -NoProfile -ExecutionPolicy RemoteSigned -File scripts/supervise-bridge.ps1 -RequireOptIn
```

`-RequireOptIn` ist der Modus der zyklischen Aufgabe: ohne `enabled`-Datei passiert nichts. Ohne diesen Schalter startet der Supervisor die Bridge unabhängig vom Opt-in.

Logs, PID-Datei und Opt-in liegen unter `%LOCALAPPDATA%\webagent\bridge\`: `prompt.log`, `supervisor.log`, `bridge.out.log`, `bridge.err.log`, `bridge.pid`, `enabled`. Der Token `WEBAGENT_API_KEY` wird aus der Benutzer-Umgebung gelesen und nie in eine Datei oder Ausgabe geschrieben.

Exit-Codes des Supervisors: `0` gesund, `2` Konfiguration fehlt, `3` Port belegt, `4` Prozess beendet, `5` Gesundheit nicht erreicht.

## Rückweg

```powershell
pwsh -NoProfile -ExecutionPolicy RemoteSigned -File scripts/setup-bridge-autostart.ps1 -Remove
```

Das entfernt Verknuepfung, Aufgabe, Opt-in und die vom Supervisor gestartete Bridge. Logdateien bleiben als Beleg erhalten und koennen separat geloescht werden.

## Grenzen

- Der Anmeldedialog wurde anhand von Verknuepfung und Skript geprueft, nicht durch einen echten Reboot/Re-Login.
- Nur `127.0.0.1:8788` und nur der Benutzerkontext. Kein Fernzugriff, kein Dienst, kein Session-0-Browser, keine Rechteaenderung.
- Standard-Brain ist `chatgpt`; die Bridge-fremden Brains sind nicht Teil dieser Scheibe.
- Kein Last-, Dauerlast- oder Mono-/Multi-Brain-Paralleltest, kein Monitoring ausser dem 5-Minuten-Health-Check.
- Der Supervisor stoppt ausschliesslich die eigene, per `bridge.pid` und Prozessname verifizierte PID. Fremde Prozesse auf Port 8788 fuehren zu Exit 3 statt zu einem Kill.
