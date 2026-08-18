<#
.SYNOPSIS
  Qualifies an OHA-SSS worker/reviewer profile pair through repeated isolated harness runs.

.DESCRIPTION
  The runner invokes the existing evidence-producing OHA-SSS harness serially.
  It writes only one aggregated qualification.json under TEMP and returns success
  only if every requested run is a full PASS with worker and reviewer evidence.
  Any missing evidence, non-PASS verdict, browser failure, or leftover process is
  treated as NOT_QUALIFIED.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$WebAgentExe,

    [ValidatePattern('^[A-Za-z0-9_-]+$')]
    [string]$Worker,

    [ValidatePattern('^[A-Za-z0-9_-]+$')]
    [string]$Reviewer,

    [ValidateRange(2, 5)]
    [int]$QualificationRuns = 2,

    [ValidateRange(15, 600)]
    [int]$TimeoutSeconds = 180,

    [ValidateNotNullOrEmpty()]
    [string]$Task = 'Return exactly this text through a structured WebAgent final result action: OHA-SSS-QUALIFICATION-PASS. The action text must consist only of OHA-SSS-QUALIFICATION-PASS, with no labels, commentary, Markdown, punctuation, or additional text. Do not use shell, browser, Git, or local-file actions.',

    [ValidateNotNullOrEmpty()]
    [string[]]$Criteria = @(
        'The candidate is exactly OHA-SSS-QUALIFICATION-PASS.',
        'The candidate contains no label, commentary, Markdown, punctuation, or additional text.',
        'The candidate requires no shell, browser, Git, or local-file action.'
    )
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path, [string]$Content) {
    [System.IO.Directory]::CreateDirectory((Split-Path -Parent $Path)) | Out-Null
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Stop-WebAgentProcesses {
    Get-Process webagent -ErrorAction SilentlyContinue | Stop-Process -Force
}

function Get-OptionalProperty([object]$Object, [string]$Name) {
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Get-ParsedReply([object]$Evidence, [string]$ReplyName) {
    $reply = Get-OptionalProperty $Evidence $ReplyName
    return Get-OptionalProperty $reply 'parsed'
}

$harnessPath = Join-Path $PSScriptRoot 'run-oha-sss-harness.ps1'
if (-not (Test-Path -LiteralPath $harnessPath -PathType Leaf)) {
    throw "OHA-SSS-Harness fehlt neben diesem Runner: $harnessPath"
}

$root = Join-Path ([System.IO.Path]::GetTempPath()) ("oha-sss-qualification-{0}" -f (Get-Date -Format 'yyyyMMdd_HHmmss_fff'))
$evidencePath = Join-Path $root 'qualification.json'
[System.IO.Directory]::CreateDirectory($root) | Out-Null

$summary = [ordered]@{
    contract = 'oha-sss-qualification-v1'
    started_at = (Get-Date).ToUniversalTime().ToString('o')
    worker = $Worker
    reviewer = $Reviewer
    qualification_runs_required = $QualificationRuns
    timeout_seconds = $TimeoutSeconds
    task = $Task
    criteria = @($Criteria)
    runs = @()
    status = 'RUNNING'
    error = ''
    active_webagent_processes_after_cleanup = $null
}

try {
    $criteriaText = $Criteria -join ' | '
    for ($runNumber = 1; $runNumber -le $QualificationRuns; $runNumber++) {
        Stop-WebAgentProcesses
        $before = @(
            Get-ChildItem -LiteralPath $env:TEMP -Directory -Filter 'oha-sss-harness-*' -ErrorAction SilentlyContinue |
                ForEach-Object { $_.FullName }
        )

        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $harnessPath `
            -WebAgentExe $WebAgentExe `
            -Worker $Worker `
            -Reviewer $Reviewer `
            -Task $Task `
            -Criteria $criteriaText `
            -TimeoutSeconds $TimeoutSeconds
        $harnessExit = $LASTEXITCODE

        $runDirectory = Get-ChildItem -LiteralPath $env:TEMP -Directory -Filter 'oha-sss-harness-*' -ErrorAction SilentlyContinue |
            Where-Object { $before -notcontains $_.FullName } |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -First 1

        $record = [ordered]@{
            run = $runNumber
            harness_exit = $harnessExit
            evidence_path = ''
            status = 'NOT_ASSESSABLE'
            verdict = 'NOT_RUN'
            error = ''
            worker = $null
            reviewer = $null
        }

        if ($null -eq $runDirectory) {
            $record.error = 'Kein neues Harness-Evidence-Verzeichnis gefunden.'
        }
        else {
            $singleEvidencePath = Join-Path $runDirectory.FullName 'evidence.json'
            $record.evidence_path = $singleEvidencePath
            if (-not (Test-Path -LiteralPath $singleEvidencePath -PathType Leaf)) {
                $record.error = 'Harness-Evidence-Datei fehlt.'
            }
            else {
                $single = Get-Content -LiteralPath $singleEvidencePath -Raw | ConvertFrom-Json
                $reportedStatus = Get-OptionalProperty $single 'status'
                $reportedVerdict = Get-OptionalProperty $single 'verdict'
                $reportedError = Get-OptionalProperty $single 'error'
                if (-not [string]::IsNullOrWhiteSpace([string]$reportedStatus)) {
                    $record.status = [string]$reportedStatus
                }
                if (-not [string]::IsNullOrWhiteSpace([string]$reportedVerdict)) {
                    $record.verdict = [string]$reportedVerdict
                }
                $record.error = [string]$reportedError
                $record.worker = Get-ParsedReply $single 'worker_reply'
                $record.reviewer = Get-ParsedReply $single 'reviewer_reply'
                if ($null -eq $record.worker -or $null -eq $record.reviewer) {
                    $record.error = 'Harness-Evidence enthält keine vollständigen strukturierten Worker- und Reviewer-Ergebnisse.'
                }
            }
        }

        $summary.runs += [pscustomobject]$record
        Stop-WebAgentProcesses

        if ($record.status -ne 'PASS' -or $record.verdict -ne 'PASS' -or $record.harness_exit -ne 0 -or -not [string]::IsNullOrWhiteSpace($record.error) -or $null -eq $record.worker -or $null -eq $record.reviewer) {
            $summary.status = 'NOT_QUALIFIED'
            break
        }
    }

    if ($summary.status -eq 'RUNNING') {
        if ($summary.runs.Count -eq $QualificationRuns) {
            $summary.status = 'QUALIFIED'
        }
        else {
            $summary.status = 'NOT_QUALIFIED'
            $summary.error = 'Nicht alle geforderten Qualifikationsläufe wurden ausgeführt.'
        }
    }
}
catch {
    $summary.status = 'NOT_QUALIFIED'
    $summary.error = $_.Exception.Message
}
finally {
    Stop-WebAgentProcesses
    $summary.active_webagent_processes_after_cleanup = @(
        Get-Process webagent -ErrorAction SilentlyContinue
    ).Count
    if ($summary.active_webagent_processes_after_cleanup -ne 0) {
        $cleanupFailure = "Cleanup unvollstÃ¤ndig: $($summary.active_webagent_processes_after_cleanup) webagent-Prozess(e) laufen noch."
        if ($summary.status -eq 'QUALIFIED') {
            $summary.status = 'NOT_QUALIFIED'
        }
        if ([string]::IsNullOrWhiteSpace($summary.error)) {
            $summary.error = $cleanupFailure
        }
        else {
            $summary.error = "$($summary.error) $cleanupFailure"
        }
    }
    $summary.finished_at = (Get-Date).ToUniversalTime().ToString('o')
    Write-Utf8NoBom $evidencePath ($summary | ConvertTo-Json -Depth 10)
    Write-Host "OHA-SSS-Qualifikation: $evidencePath"
}

if ($summary.status -eq 'QUALIFIED') { exit 0 }
exit 1
