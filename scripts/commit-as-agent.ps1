# Commit mit expliziter Agent-Identitaet.
#
# Jeder Agent committet mit eigener Git-Autor-/Committer-Identitaet, damit aus
# dem Commit ersichtlich ist, wer ihn erzeugt hat. Diese Datei fasst KEINE
# Secrets an — der Push nutzt denselben System-Credential-Store wie ueblich.
#
# Wichtig: Die Repo-Git-Config hat nur EINEN user.name/user.email. Um pro Agent
# korrekt zu sein, setzt dieses Skript die Identitaet NUR fuer den jeweiligen
# Commit via -c / GIT_AUTHOR_* / GIT_COMMITTER_* — ohne umzusaet das globale Set.
#
# Verwendung (im Repo-Root):
#   pwsh -File scripts/commit-as-agent.ps1 -Agent claude-code -Message "mein commit"
#   pwsh -File scripts/commit-as-agent.ps1 -Agent grok-agent -Message "grok tut x"
#
# Agent-Namen und E-Mails: siehe Doku docs/GIT_AGENTS.md (Schema @webagent.local).
<#.SYNOPSIS
    Committet mit expliziter Agent-Identitaet (Autor/Committer).
.DESCRIPTION
    Fuehrt `git commit` mit gesetzten GIT_AUTHOR_*/GIT_COMMITTER_* aus, sodass
    der Commit den uebergebenen Agent als Urheber traegt, ohne die repo-weite
    Git-Config zu veraendern. Gedacht fuer autonome Agents, die auf `master`
    arbeiten (Gates gruen).
.PARAMETER Agent
    Erkennbarer Agent-Schluessel (opencode, claude-code, chatgpt-codex, grok-agent, manus).
.PARAMETER Message
    Commit-Message.
.PARAMETER Paths
    Optionale Pfad(e) zum stagen (Standard: -A).
.EXAMPLE
    pwsh -File scripts/commit-as-agent.ps1 -Agent grok-agent -Message "T-102: tools registry"
#>

param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('opencode','claude-code','chatgpt-codex','grok-agent','manus')]
    [string]$Agent,
    [Parameter(Mandatory = $true)]
    [string]$Message,
    [string[]]$Paths
)

$ErrorActionPreference = 'Stop'

# Identitaets-Mapping (Quelle der Wahrheit bleibt docs/GIT_AGENTS.md).
$Identity = @{
    'opencode'      = @{ name = 'opencode';      email = 'opencode@webagent.local' }
    'claude-code'   = @{ name = 'claude-code';   email = 'claude-code@webagent.local' }
    'chatgpt-codex' = @{ name = 'chatgpt-codex'; email = 'chatgpt-codex@webagent.local' }
    'grok-agent'    = @{ name = 'grok-agent';    email = 'grok-agent@webagent.local' }
    'manus'         = @{ name = 'manus';         email = 'manus@webagent.local' }
}
$id = $Identity[$Agent]

# Commit nur auf master (Schirm gegen Nebenzweig-Verlust).
$current = git branch --show-current
if ($LASTEXITCODE -ne 0 -or $current -ne 'master') {
    throw "Commit nur auf master erlaubt (aktuell: $current). Siehe START_HERE Grundmodell."
}

# Dateien, falls angegeben; sonst alle Aenderungen.
if ($Paths) { git add -- $Paths } else { git add -A }
if ($LASTEXITCODE -ne 0) { throw 'git add schlug fehl' }

$env:GIT_AUTHOR_NAME = $id.name
$env:GIT_AUTHOR_EMAIL = $id.email
$env:GIT_COMMITTER_NAME = $id.name
$env:GIT_COMMITTER_EMAIL = $id.email

try {
    git commit -m $Message
    if ($LASTEXITCODE -ne 0) { throw 'git commit schlug fehl' }
    Write-Host ("Committet als {0} <{1}>" -f $id.name, $id.email)
}
finally {
    Remove-Item Env:GIT_AUTHOR_NAME, Env:GIT_AUTHOR_EMAIL,
                Env:GIT_COMMITTER_NAME, Env:GIT_COMMITTER_EMAIL -ErrorAction SilentlyContinue
}
# Hinweis: push separat (oder direkt git push) durchfuehren.