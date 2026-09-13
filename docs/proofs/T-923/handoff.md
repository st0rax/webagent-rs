<!-- **Referenz: Beleg der T-923-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-923 Handoff — Content echt verdrahten

- Task: T-923
- Owner: grok-agent
- Branch: `feature/T-923-wire-content`
- Depends on: T-916 (orphan content.rs)

## Was getan

1. `require_clean_text_tools` in `src/api_bridge/content.rs` repariert (Rumpf aus Root).
2. `mod content;` in `src/api_bridge.rs` verdrahtet.
3. Root-Duplikate der Prompt/Tool/`reject_unsupported_*`-Blöcke entfernt.
4. `pub(crate) use content::{...}` (+ `#[cfg(test)]` für Test-Helfer).
5. Betroffene Parent-Typen `pub(crate)` (private-interfaces).

## Kopplung

- Content nutzt Parent-Typen: `OpenAiRequest`, `AnthropicRequest`, `ResponsesRequest`,
  `ConversationMessage`, `PromptBundle`, `OpenAiTool`, `HttpResponse`, Assistant-Tool-Typen,
  `audio_mime`, `api_error_code`.
- Call-Sites (`provider_handlers`, `store`, Tests) unverändert über Parent-Reexports.

## Nicht angefasst

- Store / T-915 / T-922
- Inference / T-919
- T-914 / T-931 / T-932
- SSE-Renderer (T-918)

## Beweis

Siehe `docs/proofs/T-923/gates.txt`.
