# T-967: fail-closed Pruefung, dass ein Release-Tag zur Version des Baums passt.
#
# Exit-Codes:
#   0 = Tag, Cargo.toml und Cargo.lock stimmen ueberein
#   1 = Mismatch oder unlesbarer/unvollstaendiger Baum
#   2 = Tag-Format ungueltig (erwartet vX.Y.Z)
#
# Kein Netzwerk, keine Geheimnisse, keine Repo-Aenderung. Der bestehende Tag
# v0.11.4 wird nicht bewegt; ein erneuter Lauf gegen ihn scheitert absichtlich.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ManifestVersion {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Datei fehlt: $Path"
    }

    $inPackage = $false
    foreach ($line in [System.IO.File]::ReadAllLines($Path)) {
        if ($line -match '^\s*\[([^\]]+)\]\s*$') {
            $inPackage = ($Matches[1] -eq 'package')
            continue
        }
        if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"\s*$') {
            return $Matches[1]
        }
    }
    throw "Keine [package]-Version in $Path gefunden"
}

function Get-LockVersion {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Datei fehlt: $Path"
    }

    $isWebagent = $false
    $inBlock = $false
    foreach ($line in [System.IO.File]::ReadAllLines($Path)) {
        if ($line -match '^\s*\[\[package\]\]\s*$') {
            $inBlock = $true
            $isWebagent = $false
            continue
        }
        if ($line -match '^\s*\[\s*[^\[].*\]\s*$') {
            $inBlock = $false
            $isWebagent = $false
            continue
        }
        if (-not $inBlock) {
            continue
        }
        if ($line -match '^\s*name\s*=\s*"([^"]+)"\s*$') {
            $isWebagent = ($Matches[1] -eq 'webagent')
            continue
        }
        if ($isWebagent -and $line -match '^\s*version\s*=\s*"([^"]+)"\s*$') {
            return $Matches[1]
        }
    }
    throw "Kein webagent-Eintrag mit Version in $Path gefunden"
}

if ($Tag -notmatch '^v(\d+\.\d+\.\d+)$') {
    [Console]::Error.WriteLine("Ungueltiges Tag-Format '$Tag' (erwartet vX.Y.Z).")
    exit 2
}
$wanted = $Matches[1]
$root = Split-Path -Parent $PSScriptRoot

try {
    $manifest = Get-ManifestVersion -Path (Join-Path $root 'Cargo.toml')
    $lock = Get-LockVersion -Path (Join-Path $root 'Cargo.lock')
}
catch {
    [Console]::Error.WriteLine("Versionsdateien nicht lesbar: $($_.Exception.Message)")
    exit 1
}

if ($manifest -ne $wanted -or $lock -ne $wanted) {
    [Console]::Error.WriteLine(
        "Versionsmismatch: Tag '$Tag' erwartet $wanted, Cargo.toml=$manifest, Cargo.lock=$lock. " +
        'Bitte aus einem Commit taggen, dessen Manifest und Lockfile uebereinstimmen.'
    )
    exit 1
}

Write-Host "Release-Version geprueft: $Tag = Cargo.toml $manifest = Cargo.lock $lock"
exit 0
