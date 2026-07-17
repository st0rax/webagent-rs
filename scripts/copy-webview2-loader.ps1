#Requires -Version 5.1
<#
.SYNOPSIS
  Kopiert WebView2Loader.dll neben webagent.exe (debug/release).

.DESCRIPTION
  Nach `cargo build` / `cargo build --release` liegt die DLL oft nur unter
  target/*/build/webview2-com-sys-*/out/x64/ — ohne Kopie startet die Binary
  auf manchen Maschinen mit 0xC0000135 (DLL not found).

  Nutzung:
    pwsh -File scripts/copy-webview2-loader.ps1
    pwsh -File scripts/copy-webview2-loader.ps1 -Profile release
#>
[CmdletBinding()]
param(
    [ValidateSet("debug", "release", "both")]
    [string]$Profile = "both"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent  # webagent-rs/

function Find-Loader([string]$targetDir) {
    $build = Join-Path $targetDir "build"
    if (-not (Test-Path $build)) { return $null }
    $hit = Get-ChildItem $build -Directory -Filter "webview2-com-sys-*" -ErrorAction SilentlyContinue |
        ForEach-Object {
            $x64 = Join-Path $_.FullName "out\x64\WebView2Loader.dll"
            if (Test-Path $x64) { $x64 }
        } |
        Select-Object -First 1
    return $hit
}

function Copy-ForProfile([string]$name) {
    $dir = Join-Path $root "target\$name"
    $exe = Join-Path $dir "webagent.exe"
    if (-not (Test-Path $exe)) {
        Write-Host "[skip] ${name}: webagent.exe fehlt ($dir)"
        return
    }
    $src = Find-Loader $dir
    if (-not $src) {
        Write-Host "[warn] ${name}: WebView2Loader.dll nicht unter build/webview2-com-sys-*/out/x64 gefunden"
        return
    }
    $dst = Join-Path $dir "WebView2Loader.dll"
    Copy-Item -Force -LiteralPath $src -Destination $dst
    Write-Host "[ok] ${name}: $(Split-Path $src -Leaf) -> $dst"
}

$profiles = if ($Profile -eq "both") { @("debug", "release") } else { @($Profile) }
foreach ($p in $profiles) { Copy-ForProfile $p }
