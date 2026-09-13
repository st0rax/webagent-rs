<!-- **Referenz: Beleg der T-910-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-910 Handoff — Auth- und Fehler-Boundary

- Task: T-910
- Owner: grok-agent
- Branch: `refactor/T-910-api-bridge-boundary`
- Claim: `origin/master` `cc8121c` (2026-09-13)

## Dateiliste

| Datei | Inhalt |
|---|---|
| `src/api_bridge/boundary.rs` | `authorize`, `constant_time_equal`, `api_error`, `api_error_code`, `api_error_with` |
| `src/api_bridge.rs` | unveraendert in der Absicht des Slots; T-913 verdrahtet `mod boundary` |

## Security-Invarianten

- Keine neuen Auth-Arten. Nur Bearer-Prefix und `x-api-key`.
- Tokenvergleich timing-sicher, Laengenmismatch ist sofort `false`.
- 401-Text unveraendert: `Ungueltiger oder fehlender API-Token.`
- OpenAI- vs Anthropic-Fehlerform unveraendert (`error` object vs `type: error`).
- Keine gelockerten Vergleiche, kein Logging des Tokens.

## Gates

Siehe `docs/proofs/T-910/gates.txt`.
