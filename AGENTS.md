# Regeln fuer jede opencode-Session in diesem Repo

Diese Regeln existieren, weil Fehler wiederholt auftraten (Stand 2026-08-12). Sie
sind verbindlich, keine Absichtserklaerung.

## 1. Bestand zuerst pruefen, nie parallel bauen

Bevor ein neues Modul, ein neuer Store, eine neue Datei, ein neues CLI-Flag oder
ein neuer Mechanismus entsteht: mit `rg` pruefen, was bereits existiert. Der
Repo-Kern hat fuer viele Dinge schon eine Loesung:

- Beweise/Level-Gate: `capability_proof` (`capability/proofs.jsonl`, `ProofState`,
  Selektor-Hash, TTL). Es gibt **genau einen** Beweis-Gate.
- Messung am Brain: `brain_probe` (`Verdict`), `mock_page`, `page_driver`.
- Zustand/Laeufe: `run_store`, `config`/`data_dir`, `circuit_breaker`.

Beim Portieren aus einem anderen Branch oder Diff (z. B. `4b58dd2.diff`): den
Entwurf ZUERST gegen den aktuellen Main-Stand lesen, nicht den Entwurf als
Spezifikation behandeln. Das Main-Repo entwickelt Designs weiter; ein Referenz-
Diff ist Geschichte, kein Auftrag. Wenn der Diff etwas einfuehrt, das der
Main-Stand bereits erweitert hat, ist der Main-Stand massgeblich.

## 2. Ein einziger Beleg-Gate

Neue Belege werden ausschliesslich via `capability_proof::record_measurement`
(oder `record_proof`) geschrieben und ueber `level_from_selectors_with` vom
Level gelesen. Keinen zweiten Beleg-Speicher anlegen. Route→Faehigkeit ueber
`capability::capability_for_route` aufloesen. Wer einen Beweis-Gate umgeht,
baut ein Versprechen statt eines Koennens.

## 3. Kein stummes Untersuchen

Eine Lesephase muss in einem Edit, einem Testlauf oder einer schriftlichen
Entscheidungsfrage (Kanal `to_claude.jsonl` bzw. Nutzer) enden. Lange stille
`read`-Serien ohne sichtbares Ergebnis gelten als Stillstand. Design-Konflikte
sofort aussprechen, sobald sie auftauchen — nicht stumm verrechnen.

## 4. Sichtbare Checkpoints

Nach jedem abgeschlossenen Stueck: Build/Tests/`clippy` gruen UND eine kurze
Fortschrittszeile (Terminal oder Kanal). Uncommittete Arbeit gehoert ueber
Session-Grenzen in `STATUS_LIVE.md`.

## 5. Commit nur auf Ansage

Niemals committen, pushen oder einen PR bauen, ausser der Nutzer fordert es
ausdruecklich.

## 6. Benchmark-Monitoring & TUI

### Live-Log (bench_events)
Der Benchmark schreibt Events in `brain_score/events.jsonl` (JSON-Lines).
Der in-memory Ringpuffer (bench_events.rs) ist prozessglobal und nicht
auslesbar — das File ist die einzige persisted Quelle.

```bash
# Letzte 10 Events
Get-Content "C:\Users\storax\AppData\Local\webagent\data\brain_score\events.jsonl" -Tail 10

# Circuit-Breaker-Status (welche Brains sind gesperrt)
Get-Content "C:\Users\storax\AppData\Local\webagent\data\circuit_breaker\state.json" | ConvertFrom-Json | ConvertTo-Json -Depth 3

# Prozesse
Get-Process webagent -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Output "PID=$($_.Id) cpu=$([math]::Round($_.CPU,1))s threads=$($_.Threads.Count)"
}

# Alles auf einmal
powershell -File bench-monitor.ps1
```

### TUI starten
Die TUI braucht ein Terminal mit raw mode. Aus einer Pipe (opencode, viele
Agenten-CLIs) ist `stdout().is_terminal()` false — dann `--force-tui`.
Fenster mit Titel, `cmd /c` (nicht `/k`), sonst bleiben tote Konsolen:

```bat
cmd /c start "webagent TUI" cmd /c "cd /d <repo> && target\debug\webagent.exe tui --force-tui --view=bench --benchmark=--brains chatgpt,claude,deepseek,kimi,mistral,perplexity,qwen,zai --rounds 1 --headless"
```

### Working Tree muss sauber sein
Der Benchmark prueft `is_working_tree_clean()`. Auch untracked Files (`??`)
zaehlen. `git status --short` muss leer sein.

### Desktop: Terminal unten, Kacheln oben
`brain_grid::split_areas` teilt den Arbeitsbereich (ohne Taskleiste):
obere 70 % Brain-Fenster, untere 30 % TUI. Die Wall startet an, `w` schaltet.

- Fensterzaehler: Titel `webagent · <brain>`, PID im TUI-Baum (TUI oder Kind).
- Was nicht in 320×240 passt, liegt auf `-32000` (geparkt, nicht minimiert).
- Sichtbare Kacheln: `HWND_TOPMOST` + kein Fokus. Ohne TOPMOST liegen sie
  hinter einem Vollbild, obwohl das Raster stimmt.
- Nach Minimize/Win+D: `needs_relayout` (IsIconic), nicht nur neue HWNDs.
  TUI-Minimize parkt die Kacheln; Restore legt neu.

`--force-tui` heisst nicht „keine Kacheln“. Es verhindert nur, dass ein
fehlendes Konsolenfenster die Wall hart abbricht. Das Host-Fenster (oft
Windows Terminal mit Titel `webagent TUI`) ist die TUI, nicht ein Brain-Tab.

### OCR
Nur wenn ein Fensterstand belegt werden muss: Windows PowerShell (nicht pwsh),
`screenshot_ocr.ps1`. Keine Sichtbarkeitsbehauptung ohne HWND+Rect oder OCR.
