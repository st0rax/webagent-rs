#Requires -Version 5.1
<#
.SYNOPSIS
  Release-Build + WebView2Loader neben die Binary (ein Befehl, kein Warten).
#>
[CmdletBinding()]
param(
    [switch]$DebugBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$profile = if ($DebugBuild) { "debug" } else { "release" }
if ($DebugBuild) {
    cargo build
} else {
    cargo build --release
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& (Join-Path $PSScriptRoot "copy-webview2-loader.ps1") -Profile $profile
$exe = Join-Path $root "target\$profile\webagent.exe"
if (Test-Path $exe) {
    Write-Host "[ok] $exe"
    Get-Item $exe | Format-List FullName, Length, LastWriteTime
} else {
    Write-Error "Binary fehlt: $exe"
    exit 1
}
