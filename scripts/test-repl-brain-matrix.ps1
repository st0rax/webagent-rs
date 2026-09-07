param(
    [string]$Binary = "$PSScriptRoot\..\target\x86_64-pc-windows-gnu\release\webagent.exe",
    [string[]]$Brains = @('chatgpt','claude','deepseek','gemini','kimi','mistral','perplexity','qwen','zai'),
    [int]$TimeoutSeconds = 300,
    [string]$OutputDirectory = "$PSScriptRoot\..\artifacts\repl-matrix"
)

$ErrorActionPreference = 'Stop'
$env:WEBAGENT_FULL_LOG = '1'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$Workspace = (Resolve-Path -LiteralPath (Join-Path (Split-Path $Binary) 'source\webagent-rs')).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$task = @'
Begin by reading START_HERE.md from the current GitHub checkout, then follow its exact links and order. Reconstruct the current live repository state from the checkout before acting. Claim the currently free task only if docs/TASKBOARD.json proves it is free. Then perform one small, real repository inspection and report the result. Finish only when the work is actually complete. Do not invent ownership, branches, tests, or repository state. If START_HERE.md or the repository cannot be read, report that as a terminal blocker instead of guessing.
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
    if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
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
