param(
    [string]$Binary = "$PSScriptRoot\..\target\x86_64-pc-windows-gnu\release\webagent.exe",
    [string[]]$Brains = @('deepseek','gemini','mistral','perplexity','qwen','zai','kimi','claude','chatgpt'),
    [int]$TimeoutSeconds = 300,
    [string]$OutputDirectory = "$PSScriptRoot\..\artifacts\repl-matrix"
)

$ErrorActionPreference = 'Stop'
$env:WEBAGENT_FULL_LOG = '1'
# Vollstaendige Rohdaten bleiben in stdout.log/Transkript; fuer den naechsten
# Brain-Turn reichen Kopf und Ende einer grossen Shell-Ausgabe. Das verhindert,
# dass ein Brain komplette Branchlisten oder JSON-Dateien in jeder Runde erneut
# als Kontext verarbeitet.
$env:WEBAGENT_MAX_OBSERVATION_CHARS = '1500'

function Read-SharedText([string]$Path) {
    $fs = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        $reader = [IO.StreamReader]::new($fs)
        try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $fs.Dispose() }
}

$Binary = (Resolve-Path -LiteralPath $Binary).Path
$Workspace = (Resolve-Path -LiteralPath (Join-Path (Split-Path $Binary) 'source\webagent-rs')).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$task = @'
Read START_HERE.md from the current GitHub checkout. Then perform exactly two bounded commands: (1) `Get-Content docs/TASKBOARD.json -Raw | ConvertFrom-Json | ForEach-Object { $_.tasks | Where-Object status -eq free | Select-Object id,title } | ConvertTo-Json -Compress` and (2) `git status --short --branch`. Do not run any other command, do not follow linked documents, do not dump complete files or branch lists, do not modify files, and do not claim a task. Report those two command results and terminate immediately with a valid WEBAGENT/1 finish action. If START_HERE.md or docs/TASKBOARD.json cannot be read, report that as a terminal blocker instead of guessing.
When the inspection is complete, terminate the run with a valid WEBAGENT/1 finish action; a prose-only final report is not a terminal signal.
'@

foreach ($brain in $Brains) {
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $dir = Join-Path $OutputDirectory "$stamp-$brain"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $input = Join-Path $dir 'input.txt'
    $stdout = Join-Path $dir 'stdout.log'
    $stderr = Join-Path $dir 'stderr.log'
    $result = [ordered]@{ brain=$brain; started=(Get-Date).ToUniversalTime().ToString('o'); timeout_seconds=$TimeoutSeconds; status='started' }
    # Kein /exit vorab senden: das würde die REPL beenden, bevor der
    # Providerturn abgeschlossen ist, und einen falschen exit_0 vortäuschen.
    Set-Content -LiteralPath $input -Value ($task.Trim() + "`r`n") -Encoding utf8
    # Start-Process nimmt ein Argument-Array nicht als argv entgegen, sondern
    # setzt es auf Windows zu einer Kommandozeile zusammen. Der Task muss als
    # ein einziges, quotiertes Argument übergeben werden.
    $taskArg = '"' + $task.Trim().Replace('"', '\"') + '"'
    # Matrix-Tests starten bewusst aus einem leeren Kontext: kein Memory, kein
    # Wiki und keine alten Run-Episoden dürfen die Provider vergleichen.
    $args = "run --brain `"$brain`" --task $taskArg --no-memory"
    $p = Start-Process -FilePath $Binary -ArgumentList $args -WorkingDirectory $Workspace -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
    # Start-Process kann stdout/stderr zwar sicher in Dateien schreiben, zeigt
    # sie mit RedirectStandard* aber nicht im sichtbaren Fenster. Die Dateien
    # werden deshalb waehrend des Laufs inkrementell gespiegelt; so bleiben
    # Transkript und Live-Diagnose identisch nachvollziehbar.
    $outSeen = 0
    $errSeen = 0
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (-not $p.HasExited -and [DateTime]::UtcNow -lt $deadline) {
        foreach ($stream in @(@{ Path=$stdout; Prefix="[$brain stdout]"; Seen=[ref]$outSeen }, @{ Path=$stderr; Prefix="[$brain stderr]"; Seen=[ref]$errSeen })) {
            if (Test-Path -LiteralPath $stream.Path) {
                $raw = Read-SharedText $stream.Path
                if ($raw.Length -gt $stream.Seen.Value) {
                    $delta = $raw.Substring($stream.Seen.Value)
                    $stream.Seen.Value = $raw.Length
                    foreach ($line in ($delta -split "`r?`n")) {
                        if ($line.Length -gt 0) { Write-Host ("{0} {1}" -f $stream.Prefix,$line) }
                    }
                }
            }
        }
        Start-Sleep -Milliseconds 250
    }
    if (-not $p.HasExited) {
        $result.status = 'timeout'
        taskkill.exe /PID $p.Id /T /F | Out-Null
        $p.WaitForExit(5000)
    } else {
        $result.status = if ($p.ExitCode -eq 0) { 'exit_0' } else { 'exit_nonzero' }
        $result.exit_code = $p.ExitCode
    }
    $result.finished = (Get-Date).ToUniversalTime().ToString('o')
    $result.stdout = $stdout
    $result.stderr = $stderr
    $result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $dir 'result.json') -Encoding utf8
    Write-Host ("[{0}] {1} -> {2}" -f $brain,$result.status,$dir)
}
