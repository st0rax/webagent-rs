# start-workers.ps1 — bringt eine Flotte autonomer webagent-Brain-Worker online.
#
# Jeder Worker pollt seine bot2bot-Inbox (agents/<brain>/inbox), arbeitet Tasks
# ueber den Controller-Loop autonom ab (Shell + Dateien, shell_policy-geschuetzt)
# und schreibt das Ergebnis an den Absender zurueck — grok-Aequivalent.
# Jeder Prozess isoliert sein Browser-Profil selbst (Q5-copy aus profiles/<brain>),
# daher laufen N Worker parallel ohne SingletonLock.
#
# WICHTIG: RAM ist der Limiter. Jeder Worker faehrt ein eigenes headless-Chromium
# (~300-500 MB). Realistisch ~4-6 parallel stabil. Starte nur so viele wie noetig.
#
# Aufgabe an einen Worker geben: eine .msg.txt in agents/<brain>/inbox/ ablegen
# (z.B. via bot2bot send.ps1 -To <brain> -From claude -Message "<task>").

param(
    # Welche Brains als Worker starten (Default: die mit eingeloggtem Profil).
    [string[]]$Brains = @('deepseek','chatgpt','gemini','kimi'),
    # Poll-Intervall in Sekunden.
    [int]$PollSecs = 30,
    # Max Controller-Zyklen pro Task.
    [int]$MaxCycles = 100,
    # Pfade (an diese Maschine angepasst; bei Bedarf ueberschreiben).
    [string]$Exe = "C:\Users\storax\Desktop\webagent\webagent-rs\target\release\webagent.exe",
    [string]$Bot2BotRoot = "C:\Users\storax\Desktop\bot2bot"
)

if (-not (Test-Path $Exe)) { Write-Error "webagent.exe nicht gefunden: $Exe"; exit 1 }
$env:WEBAGENT_BOT2BOT_ROOT = $Bot2BotRoot

Write-Host "Starte $($Brains.Count) Worker (poll=${PollSecs}s, bot2bot=$Bot2BotRoot)..."
foreach ($b in $Brains) {
    $args = @('bot2bot-worker','--brain',$b,'--headless','--poll-secs',"$PollSecs",'--max-cycles',"$MaxCycles")
    Start-Process -FilePath $Exe -ArgumentList $args -WindowStyle Hidden
    Write-Host "  [$b] Worker gestartet."
    Start-Sleep -Milliseconds 800   # gestaffelt, damit die Profil-Kopien sich nicht draengeln
}
Write-Host "Fertig. Stoppen: Get-Process webagent | Stop-Process"
