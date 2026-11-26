<#
.SYNOPSIS
Installiert oder entfernt den lokalen WebAgent-Bridge-Dauerbetrieb.

.DESCRIPTION
Installiert eine sichtbare Abfrage im Autostart-Ordner (beim Anmelden) und eine
zyklische Health-Pruefung als Aufgabe des aktuellen Benutzers. Die Pruefung
greift nur, wenn der Anmeldedialog mit "J" beantwortet wurde. Alle Aenderungen
sind lokal, nur fuer den aktuellen Benutzer und mit -Remove vollstaendig
rueckbaubar. Es werden keine Zugangsdaten gelesen oder gespeichert.
#>
[CmdletBinding()]
param([switch]$Remove)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$promptScript = Join-Path $PSScriptRoot "bridge-start-prompt.ps1"
$supervisorScript = Join-Path $PSScriptRoot "supervise-bridge.ps1"
$taskName = "WebAgent Bridge Health (5min)"
$startupDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Startup"
$shortcut = Join-Path $startupDir "WebAgent Bridge Start.lnk"
$logDir = Join-Path $env:LOCALAPPDATA "webagent\bridge"
$flag = Join-Path $logDir "enabled"
$pidFile = Join-Path $logDir "bridge.pid"
$taskPrincipal = "$env:USERDOMAIN\$env:USERNAME"

foreach ($required in @($promptScript, $supervisorScript)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Skript fehlt: $required"
    }
}

if ($Remove) {
    $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if ($task) {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
        Write-Host "Aufgabe entfernt: $taskName"
    }
    if (Test-Path -LiteralPath $shortcut) {
        Remove-Item -LiteralPath $shortcut -Force
        Write-Host "Autostart-Verknuepfung entfernt: $shortcut"
    }
    Remove-Item -LiteralPath $flag -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $pidFile) {
        $oldPid = 0
        if ([int]::TryParse((Get-Content -LiteralPath $pidFile -Raw -ErrorAction SilentlyContinue).Trim(), [ref]$oldPid)) {
            $old = Get-Process -Id $oldPid -ErrorAction SilentlyContinue
            if ($null -ne $old -and $old.ProcessName -eq "webagent") {
                Stop-Process -Id $oldPid -Force
                Write-Host "Bridge $oldPid gestoppt"
            }
        }
        Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Rueckweg abgeschlossen. Logdateien unter $logDir bleiben als Beleg erhalten."
    exit 0
}

New-Item -ItemType Directory -Path $startupDir -Force | Out-Null
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
$link.TargetPath = "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
$link.Arguments = "-NoProfile -ExecutionPolicy RemoteSigned -File `"$promptScript`""
$link.WorkingDirectory = $repoRoot
$link.Description = "Fragt beim Anmelden, ob die lokale WebAgent-Bridge gestartet werden soll."
$link.Save()
Write-Host "Autostart-Verknuepfung: $shortcut"

$existing = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
if (-not $existing) {
    $taskArguments = "-NoProfile -ExecutionPolicy RemoteSigned -File `"$supervisorScript`" -RequireOptIn"
    $action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument $taskArguments -WorkingDirectory $repoRoot
    $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes 5)
    $principal = New-ScheduledTaskPrincipal -UserId $taskPrincipal -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 2) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger `
        -Principal $principal -Settings $settings `
        -Description "Zyklische Health-Pruefung der lokalen WebAgent-Bridge; nur nach Opt-in aus dem Anmeldefenster aktiv." | Out-Null
    Write-Host "Aufgabe registriert: $taskName"
} else {
    Write-Host "Aufgabe bereits vorhanden, unveraendert: $taskName"
}
Write-Host "Hinweis: Die Bridge startet erst nach 'J' im Anmeldefenster."
