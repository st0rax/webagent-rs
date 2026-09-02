# Agent-Git-Identitaeten

> **Referenz.** Verzeichnis der Git-Autor-Identitaeten lokaler Agents. Damit ist
> aus jedem Commit ersichtlich, welcher Agent ihn erzeugt hat (Redundanz +
> Nachvollziehbarkeit). Der Push nutzt den gleichen System-Credential-Store —
> dieses Dokument fasst keine Secrets an.

## Prinzip

Die Repo-Git-Config hat **eine einzige** `user.name`/`user.email`. Um pro Agent
korrekt zu bleiben, setzt jeder Agent seine Identität **nur für seinen Commit**
via `GIT_AUTHOR_*` / `GIT_COMMITTER_*` — nicht global. Genau das macht
`scripts/commit-as-agent.ps1`.

## Identitäts-Tabelle (Quelle der Wahrheit)

| Agent | user.name | user.email |
|---|---|---|
| opencode (lokal) | `opencode` | `opencode@webagent.local` |
| claude-code | `claude-code` | `claude-code@webagent.local` |
| chatgpt-codex | `chatgpt-codex` | `chatgpt-codex@webagent.local` |
| grok-agent | `grok-agent` | `grok-agent@webagent.local` |
| manus | `manus` | `manus@webagent.local` |

Domain-Suffix `@webagent.local` → nie verwechselbar mit einer echten externen
Adresse; kein Agent nutzt die Adresse eines anderen.

## So committest du als Agent

Ablauf und Zweigmodell: **`START_HERE.md`** (bei Widerspruch gilt START_HERE).

```pwsh
# auf master ODER feature|fix|docs|chore|refactor|test/...
pwsh -File scripts/commit-as-agent.ps1 -Agent claude-code -Message "T-102: tools registry"

pwsh -File scripts/commit-as-agent.ps1 -Agent grok-agent -Message "fix rollback" -Paths scripts/a.ps1 src/b.rs
```

Danach den in START_HERE beschriebenen Merge nach `master` und **ein** `git push origin master`.

## Regeln

- Commit auf `master` oder einem benannten Arbeits-Zweig (Schirm im Skript, Schema `docs/GIT_GLOSSAR.md`). Sichtbare Abgabe ist immer der Stamm.
- **Vor dem Commit: Gates grün** (`cargo fmt --all -- --check`, `cargo test --lib` u. ä.).
- **Nie die Identität eines anderen Agents verwenden.**
- Dieses Skript/Doku fasst keine Credentials an — Geheimnisse bleiben im
  geteilten Credential-Store.