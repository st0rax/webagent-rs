<#
.SYNOPSIS
Live-Smoke-Test fuer eine laufende WebAgent-Bridge mit einer lokalen Pi-Installation.

.DESCRIPTION
Der Test kopiert bewusst examples/pi/models.json in ein temporaeres,
isoliertes PI_CODING_AGENT_DIR. Er veraendert weder die regulaere
Pi-Konfiguration unter ~/.pi noch das Repository.
Zuerst wird ein reiner Textturn geprueft, danach ein echter Pi-read-Tool-Loop
gegen eine zufaellige Datei in einem temporaeren Verzeichnis.
#>

param(
    [string]$PiCommand = "pi",
    [string]$BaseUrl = "http://127.0.0.1:8787",
    [string]$ApiKeyEnv = "WEBAGENT_API_KEY"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$piConfigTemplate = Join-Path $repoRoot "examples\pi\models.json"
$token = [Environment]::GetEnvironmentVariable($ApiKeyEnv, "Process")
if ([string]::IsNullOrWhiteSpace($token)) {
    throw "Die Prozess-Umgebungsvariable $ApiKeyEnv fehlt. Sie muss denselben Token wie webagent api serve enthalten."
}
if ($BaseUrl.TrimEnd('/') -ne "http://127.0.0.1:8787") {
    throw "Der isolierte Beispielprovider ist auf http://127.0.0.1:8787 festgelegt. Fuer einen anderen Port examples/pi/models.json kopieren und baseUrl anpassen."
}

$pi = Get-Command $PiCommand -ErrorAction Stop
$health = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get
if ($health.status -ne "ok") {
    throw "Bridge meldet keinen gesunden Zustand: $($health | ConvertTo-Json -Compress)"
}
$headers = @{ Authorization = "Bearer $token" }
$models = Invoke-RestMethod -Uri "$BaseUrl/v1/models" -Headers $headers -Method Get
if (-not ($models.data.id -contains "webagent/chatgpt")) {
    throw "Modell webagent/chatgpt fehlt im Modellkatalog."
}

$oldPiDir = [Environment]::GetEnvironmentVariable("PI_CODING_AGENT_DIR", "Process")
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("webagent-pi-smoke-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
$piConfig = Join-Path $tempRoot "pi-config"
New-Item -ItemType Directory -Path $piConfig | Out-Null
[System.IO.File]::Copy($piConfigTemplate, (Join-Path $piConfig "models.json"))
$nonce = "PI_TOOL_" + [guid]::NewGuid().ToString("N")
[System.IO.File]::WriteAllText((Join-Path $tempRoot "nonce.txt"), $nonce)

try {
    [Environment]::SetEnvironmentVariable("PI_CODING_AGENT_DIR", $piConfig, "Process")

    Write-Host "[pi-smoke] Textturn..."
    $textOutput = & $pi.Source --offline --provider webagent --model "webagent/chatgpt" `
        --no-tools --no-session --no-context-files --no-extensions --no-skills `
        --no-prompt-templates --mode json -p "Antworte exakt mit PI_BRIDGE_OK." 2>&1
    if ($LASTEXITCODE -ne 0 -or ($textOutput -join "`n") -notmatch "PI_BRIDGE_OK") {
        throw "Pi-Textturn fehlgeschlagen:`n$($textOutput -join "`n")"
    }

    Write-Host "[pi-smoke] read-Tool-Loop..."
    Push-Location -LiteralPath $tempRoot
    try {
        $toolOutput = & $pi.Source --offline --provider webagent --model "webagent/chatgpt" `
            --tools read --no-session --no-context-files --no-extensions --no-skills `
            --no-prompt-templates --mode json -p `
            "Du musst zuerst das read-Werkzeug fuer nonce.txt aufrufen. Antworte danach ausschliesslich mit dem exakten Dateiinhalt." 2>&1
    }
    finally {
        Pop-Location
    }
    $toolTranscript = $toolOutput -join "`n"
    if ($LASTEXITCODE -ne 0 -or $toolTranscript -notmatch [regex]::Escape($nonce) -or $toolTranscript -notmatch '"type":"tool_execution_start"') {
        throw "Pi-Tool-Loop fehlgeschlagen oder wurde ohne Tool beantwortet:`n$toolTranscript"
    }

    Write-Host "[pi-smoke] PASS: Textturn und echter Pi-read-Tool-Loop sind gruen."
}
finally {
    [Environment]::SetEnvironmentVariable("PI_CODING_AGENT_DIR", $oldPiDir, "Process")
    $resolvedTemp = (Resolve-Path -LiteralPath $tempRoot -ErrorAction SilentlyContinue).Path
    $expectedParent = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\')
    if ($resolvedTemp -and $resolvedTemp.StartsWith($expectedParent + "\webagent-pi-smoke-", [System.StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
    }
}
