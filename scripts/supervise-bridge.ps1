<#
.SYNOPSIS
Idempotenter Supervisor fuer die lokale WebAgent-Bridge auf 127.0.0.1:8788.

.DESCRIPTION
Der Supervisor wird beim Login und zyklisch vom Task Scheduler ausgefuehrt.
Ist die Bridge gesund, passiert nichts. Andernfalls stoppt er ausschliesslich
den Prozess, den er selbst ueber bridge.pid gestartet hat, und startet die
Bridge neu. WEBAGENT_API_KEY wird aus der Benutzer-Umgebung gelesen und nie
protokolliert. Es werden keine Profil-, Cookie- oder Token-Dateien geoeffnet.
Nur Loopback-Adressen sind zulaessig.

Exit-Codes: 0 gesund, 2 Konfiguration fehlt, 3 Port belegt, 4 Prozess beendet,
5 Gesundheit nicht erreicht.
#>
[CmdletBinding()]
param(
    [string]$BridgeExe = "",
    [string]$BaseUrl = "http://127.0.0.1:8788",
    [string]$ApiKeyEnv = "WEBAGENT_API_KEY",
    [string]$Brain = "chatgpt",
    [string]$LogDir = "",
    [int]$HealthyWaitSecs = 20,
    [switch]$RequireOptIn
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$uri = [Uri]$BaseUrl
if ($uri.Scheme -ne "http" -or $uri.Host -notin @("127.0.0.1", "localhost", "::1")) {
    throw "Nur http auf Loopback ist erlaubt: $BaseUrl"
}
$port = $uri.Port

$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($BridgeExe)) {
    $BridgeExe = Join-Path (Split-Path -Parent $repoRoot) "webagent-deploy-target\release\webagent.exe"
}
if ([string]::IsNullOrWhiteSpace($LogDir)) {
    $LogDir = Join-Path $env:LOCALAPPDATA "webagent\bridge"
}
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null

$statusLog = Join-Path $LogDir "supervisor.log"
$pidFile = Join-Path $LogDir "bridge.pid"
$outLog = Join-Path $LogDir "bridge.out.log"
$errLog = Join-Path $LogDir "bridge.err.log"

function Limit-Log([string]$path) {
    if ((Test-Path -LiteralPath $path) -and (Get-Item -LiteralPath $path).Length -gt 262144) {
        Move-Item -LiteralPath $path -Destination "$path.old" -Force
    }
}

function Write-Status([string]$message) {
    $line = "{0} {1}" -f (Get-Date -Format "yyyy-MM-ddTHH:mm:ss"), $message
    Add-Content -LiteralPath $statusLog -Value $line -Encoding UTF8
}

function Test-BridgeHealth {
    try {
        $response = Invoke-RestMethod -Uri "$BaseUrl/health" -TimeoutSec 3 -ErrorAction Stop
        return ($response.status -eq "ok")
    } catch {
        return $false
    }
}

Limit-Log $statusLog
Limit-Log $outLog
Limit-Log $errLog

if ($RequireOptIn -and -not (Test-Path -LiteralPath (Join-Path $LogDir "enabled"))) {
    Write-Status "OK Opt-in fehlt, kein Start"
    exit 0
}

if (-not (Test-Path -LiteralPath $BridgeExe -PathType Leaf)) {
    Write-Status "FAIL Bridge-EXE fehlt: $BridgeExe"
    exit 2
}
$token = [Environment]::GetEnvironmentVariable($ApiKeyEnv, "User")
if ([string]::IsNullOrWhiteSpace($token)) {
    Write-Status "FAIL $ApiKeyEnv als Benutzer-Umgebungsvariable fehlt"
    exit 2
}
[Environment]::SetEnvironmentVariable($ApiKeyEnv, $token, "Process")

# T-1019: Ein Supervisor-Neustart muss die aktive Bridge-Konfiguration
# beibehalten. Ohne diese drei Flags waere der fortsetzende Provider-Chat
# nach einem Neustart still wieder inaktiv.
$env:WEBAGENT_USE_SHARED_BROWSER = "1"
$env:WEBAGENT_PERSIST_TABS = "1"
$env:WEBAGENT_CONTINUE_PROVIDER_CHAT = "1"

if (Test-BridgeHealth) {
    Write-Status "OK Bridge gesund, keine Aenderung"
    exit 0
}

# Nur den selbst gestarteten Prozess beenden; fremde Prozesse bleiben unberuehrt.
if (Test-Path -LiteralPath $pidFile) {
    $oldPid = 0
    $rawPid = (Get-Content -LiteralPath $pidFile -Raw -ErrorAction SilentlyContinue).Trim()
    if ([int]::TryParse($rawPid, [ref]$oldPid)) {
        $old = Get-Process -Id $oldPid -ErrorAction SilentlyContinue
        if ($null -ne $old -and $old.ProcessName -eq "webagent") {
            try {
                $ci = Get-CimInstance Win32_Process -Filter "ProcessId = $oldPid"
                if ($ci.CommandLine -match 'api\s+serve' -and $ci.CommandLine -match "--port\s+$port") {
                    Stop-Process -Id $oldPid -Force
                    Write-Status "Altprozess $oldPid gestoppt"
                } else {
                    Write-Status "PID $oldPid ist nicht unsere Bridge - nicht gestoppt"
                }
            } catch {
                Write-Status "PID $oldPid konnte nicht geprueft werden - nicht gestoppt"
            }
        } elseif ($null -ne $old) {
            Write-Status "PID $oldPid gehoert nicht zu webagent - nicht gestoppt"
        }
    }
    Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
}

$listener = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
if ($listener) {
    Write-Status "FAIL Port $port belegt, kein Neustart"
    exit 3
}

$arguments = @(
    "--skip-startup-reconcile",
    "api", "serve",
    "--brain", $Brain,
    "--headless",
    "--port", "$port",
    "--timeout-secs", "120"
)
$proc = Start-Process -FilePath $BridgeExe -ArgumentList $arguments -PassThru `
    -WindowStyle Hidden -RedirectStandardOutput $outLog -RedirectStandardError $errLog
[System.IO.File]::WriteAllText($pidFile, "$($proc.Id)", [System.Text.UTF8Encoding]::new($false))
Write-Status "Bridge gestartet PID=$($proc.Id)"

$deadline = (Get-Date).AddSeconds($HealthyWaitSecs)
while ((Get-Date) -lt $deadline) {
    if (Test-BridgeHealth) {
        Write-Status "OK Bridge gesund PID=$($proc.Id)"
        exit 0
    }
    if ($proc.HasExited) {
        Write-Status "FAIL Bridge-PID $($proc.Id) beendet mit exit $($proc.ExitCode)"
        Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
        exit 4
    }
    Start-Sleep -Seconds 1
}
Write-Status "FAIL Bridge nach ${HealthyWaitSecs}s nicht gesund"
exit 5
