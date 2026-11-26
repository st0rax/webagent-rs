<#
.SYNOPSIS
Sichtbares Anmeldefenster mit Ja/Nein-Abfrage fuer die lokale WebAgent-Bridge.

.DESCRIPTION
Wird beim Anmelden ueber eine Verknuepfung im Autostart-Ordner geoeffnet.
Bei "J" wird die Zustimmung vermerkt und der Supervisor einmal ausgefuehrt;
die zyklische Gesundheitspruefung bleibt danach aktiv. Bei "N" bleibt die
Bridge aus und die zyklische Pruefung tut nichts. Es werden keine Zugangsdaten
abgefragt und keine Profil-, Cookie- oder Token-Dateien geoeffnet.
#>
[CmdletBinding()]
param(
    [string]$SupervisorScript = "",
    [int]$AutoCloseSecs = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($SupervisorScript)) {
    $SupervisorScript = Join-Path $PSScriptRoot "supervise-bridge.ps1"
}
$logDir = Join-Path $env:LOCALAPPDATA "webagent\bridge"
$flag = Join-Path $logDir "enabled"
$promptLog = Join-Path $logDir "prompt.log"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

try { $host.UI.RawUI.WindowTitle = "WebAgent Bridge Start" } catch { }

Write-Host ""
Write-Host "WebAgent Bridge - lokaler Provider auf 127.0.0.1:8788"
Write-Host "Nur Loopback. Kein Login, keine Zugangsdaten werden abgefragt."
$answer = (Read-Host "Bridge jetzt starten? [J/N]").Trim()
$start = $answer -match '^(j|y|ja|yes)$'
$decision = if ($start) { "start" } else { "aus" }
Add-Content -LiteralPath $promptLog -Value ("{0} Antwort={1}" -f (Get-Date -Format "yyyy-MM-ddTHH:mm:ss"), $decision) -Encoding UTF8

if ($start) {
    [System.IO.File]::WriteAllText($flag, "1", [System.Text.UTF8Encoding]::new($false))
    & powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $SupervisorScript
    $code = $LASTEXITCODE
    if ($code -eq 0) {
        Write-Host "Bridge ist gestartet und gesund."
    } else {
        Write-Host "Start fehlgeschlagen (exit $code). Log: $logDir\supervisor.log"
    }
} else {
    Remove-Item -LiteralPath $flag -Force -ErrorAction SilentlyContinue
    $pidFile = Join-Path $logDir "bridge.pid"
    if (Test-Path -LiteralPath $pidFile) {
        $oldPid = 0
        if ([int]::TryParse((Get-Content -LiteralPath $pidFile -Raw -ErrorAction SilentlyContinue).Trim(), [ref]$oldPid)) {
            $old = Get-Process -Id $oldPid -ErrorAction SilentlyContinue
            if ($null -ne $old -and $old.ProcessName -eq "webagent") {
                Stop-Process -Id $oldPid -Force
                Write-Host "Laufende Bridge $oldPid gestoppt."
            }
        }
        Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Bridge bleibt aus; die zyklische Pruefung tut nichts."
}

if ($AutoCloseSecs -gt 0) {
    Write-Host "Dieses Fenster schliesst sich in $AutoCloseSecs Sekunden."
    Start-Sleep -Seconds $AutoCloseSecs
}
