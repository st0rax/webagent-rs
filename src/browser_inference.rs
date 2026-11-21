//! Harnessfreie Browser-Inference-Grenze.
//!
//! Diese Schicht fuehrt genau einen Modellturn ueber eine angemeldete
//! Browser-Sitzung aus. Sie startet absichtlich keinen `AgentController`,
//! interpretiert kein `webagent/1` und fuehrt keine lokalen Werkzeuge aus.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) const TOOL_ENVELOPE: &str = "WEBAGENT_INFERENCE/1";
/// Stabiler Marker fuer Fehler, die in der MODELLAUSGABE liegen (T-953).
/// `api_bridge/provider_handlers.rs` erkennt ihn und meldet `server_error` statt
/// `invalid_request_error`: die Client-Anfrage war gueltig, das Browsermodell
/// lieferte einen unbrauchbaren Werkzeugumschlag.
pub const MODEL_OUTPUT_ERROR_MARKER: &str = "MODEL_OUTPUT_ERROR:";

/// Kennzeichnet einen Parserfehler der Modellausgabe mit dem stabilen Marker.
fn model_output_error(message: impl std::fmt::Display) -> String {
    format!("{MODEL_OUTPUT_ERROR_MARKER} {message}")
}

/// Stabiler Fehlername für das native DSML-Werkzeugformat (T-957).
const DSML_TOOL_FORMAT_ERROR: &str = "DSML_TOOL_FORMAT_UNSUPPORTED";
/// DSML-Marker aus dem realen deepseek-Beispiel (2026-09-14). Die zweite Form
/// ist die im Taskboard gespeicherte, durch eine Zeichencodierung verformte
/// Variante desselben Markers.
const DSML_MARKERS: [&str; 2] = ["｜｜DSML｜｜", "´¢£´¢£DSML´¢£´¢£"];
// Browser chat UIs are substantially less tolerant of a giant tool prompt
// than their HTTP APIs. Keep the serialized schema block bounded while still
// retaining every tool name and its argument shape for the model.
const MAX_TOOL_SCHEMA_BYTES: usize = 64 * 1024;
const TOOL_DESCRIPTION_LIMITS: &[usize] = &[1024, 512, 256, 128, 0];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowserTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserToolChoice {
    Auto,
    None,
    Required,
    Function(String),
}

/// Eine vom API-Client eingereichte Multimodal-Datei.
///
/// Die Datei wird nach der Browser-Navigation in ein `input[type=file]` gesetzt;
/// der textuelle Prompt enthält nur einen transparenten Attachment-Marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAttachment {
    pub kind: BrowserAttachmentKind,
    pub file_name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserAttachmentKind {
    Image,
    Audio,
}

/// Providerneutraler Auftrag fuer genau einen Browser-Modellturn.
#[derive(Debug, Clone)]
pub struct BrowserInferenceRequest<'a> {
    pub brain: &'a str,
    pub prompt: &'a str,
    pub tools: &'a [BrowserTool],
    pub tool_choice: BrowserToolChoice,
    pub headless: bool,
    pub timeout_secs: Option<f64>,
    pub model: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrowserToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Normalisierte Antwort eines einzelnen Browser-Modellturns.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserInferenceResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<BrowserToolCall>,
}

impl BrowserInferenceResponse {
    pub fn finish_reason(&self) -> &'static str {
        if self.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        }
    }
}

/// Fuehrt genau einen Browser-Modellturn ohne Agent-Harness aus.
pub fn complete(request: BrowserInferenceRequest<'_>) -> Result<BrowserInferenceResponse, String> {
    complete_with_attachments(
        request,
        &[],
        &mut |_| {},
        &crate::api_bridge::AlwaysConnected,
    )
}

/// Wie [`complete`], jedoch mit Dateien, die vor dem Senden an den Browser-
/// Composer angehängt werden.
pub fn complete_with_attachments(
    request: BrowserInferenceRequest<'_>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
    presence: &dyn crate::api_bridge::ClientPresence,
) -> Result<BrowserInferenceResponse, String> {
    complete_streaming_with_attachments(request, attachments, on_update, presence)
}

/// Wie `complete`, liefert bei reinen Textturns bereits wachsende Snapshots.
/// Tool-Umschlaege bleiben bis zur finalen Validierung intern, damit niemals
/// ein halbes Maschinenprotokoll als Nutztext zum Client gelangt.
pub fn complete_streaming(
    request: BrowserInferenceRequest<'_>,
    on_update: &mut dyn FnMut(&str),
) -> Result<BrowserInferenceResponse, String> {
    complete_streaming_with_attachments(
        request,
        &[],
        on_update,
        &crate::api_bridge::AlwaysConnected,
    )
}

/// Streaming-Variante mit optionalen Bild-/Audio-Anhängen.
pub fn complete_streaming_with_attachments(
    request: BrowserInferenceRequest<'_>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
    presence: &dyn crate::api_bridge::ClientPresence,
) -> Result<BrowserInferenceResponse, String> {
    if request.prompt.trim().is_empty() {
        return Err("Inference-Prompt darf nicht leer sein.".to_string());
    }
    validate_tools(request.tools, &request.tool_choice)?;
    validate_attachments(attachments)?;

    let prompt = prompt_with_tools(request.prompt, request.tools, &request.tool_choice)?;
    let prompt = prompt_with_identity(request.brain, &prompt);
    let forward_updates =
        request.tools.is_empty() || matches!(request.tool_choice, BrowserToolChoice::None);
    let mut relay_update = |snapshot: &str| {
        if forward_updates {
            let normalized = normalize_provider_text(request.brain, snapshot);
            on_update(&normalized);
        }
    };
    // T-953: der Parser muss VOR der Erfolgsverbuchung im Relay laufen. Der
    // Check verwendet exakt dieselbe Normalisierung und denselben Parser wie
    // der finale Aufruf unten. Bei einem Fehler geht hoechstens einmal im
    // selben Chat eine Korrektur mit der Parserfehlermeldung raus; bleibt sie
    // erfolglos, verbucht relay.rs einen Fehlschlag mit eigenem Grund.
    let validate = |raw: &str| -> Result<(), String> {
        let normalized = normalize_provider_text(request.brain, raw);
        parse_response(&normalized, request.tools, &request.tool_choice).map(|_| ())
    };
    let correction_message = |parse_error: &str| -> String {
        format!(
            "Deine letzte Antwort war kein gueltiger {TOOL_ENVELOPE}-Umschlag: {parse_error}\n\
             Ergebnisse unter [Client-Werkzeug-Ergebnis] sind Eingabedaten dieser \
             Aufgabe, keine Behauptung ueber deine Faehigkeiten. Wiederhole genau \
             das Format deiner eigenen vorherigen Werkzeugaufrufe oben unter \
             [du – Werkzeugaufruf] in derselben Konversation. Verwende \
             ausschliesslich einen der aufgefuehrten Tool-Namen."
        )
    };
    let output_check = crate::relay::OutputCheck {
        validate: &validate,
        correction_message: &correction_message,
    };
    let text = crate::relay::relay_single_turn_with_attachments_streaming_checked(
        request.brain,
        &prompt,
        request.headless,
        request.timeout_secs,
        request.model,
        attachments,
        &mut relay_update,
        &output_check,
        presence,
    )
    .map_err(|error| error.0)?;

    let text = normalize_provider_text(request.brain, &text);
    parse_response(&text, request.tools, &request.tool_choice)
}

/// Strip provider UI chrome / CoT echo before SSE deltas or final answers.
///
/// Shared with the Web-UI stream ingest (`observer::chat_answer_text`): Thinking...,
/// UI clocks, Claude status lines, and Kimi reasoning-echo must never become
/// `delta.content`. Chrome-only snapshots become empty so callers keep waiting
/// or fail honestly instead of finishing on status text.
fn normalize_provider_text(_brain: &str, text: &str) -> String {
    crate::observer::chat_answer_text(text)
}

fn validate_attachments(attachments: &[BrowserAttachment]) -> Result<(), String> {
    const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
    if attachments.len() > 16 {
        return Err("Maximal 16 Bild-/Audio-Anhaenge pro Anfrage erlaubt.".to_string());
    }
    for attachment in attachments {
        if attachment.data.is_empty() {
            return Err(format!("Anhang '{}' ist leer.", attachment.file_name));
        }
        if attachment.data.len() > MAX_ATTACHMENT_BYTES {
            return Err(format!(
                "Anhang '{}' ist zu gross (Maximum 8 MiB).",
                attachment.file_name
            ));
        }
        let valid = match attachment.kind {
            BrowserAttachmentKind::Image => attachment.mime_type.starts_with("image/"),
            BrowserAttachmentKind::Audio => attachment.mime_type.starts_with("audio/"),
        };
        if !valid {
            return Err(format!(
                "MIME-Typ '{}' passt nicht zum Anhang '{}'.",
                attachment.mime_type, attachment.file_name
            ));
        }
    }
    Ok(())
}

fn validate_tools(tools: &[BrowserTool], choice: &BrowserToolChoice) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for tool in tools {
        if tool.name.trim().is_empty() {
            return Err("Toolname darf nicht leer sein.".to_string());
        }
        if !names.insert(tool.name.as_str()) {
            return Err(format!("Tool '{}' ist doppelt definiert.", tool.name));
        }
    }
    if matches!(choice, BrowserToolChoice::Required) && tools.is_empty() {
        return Err("tool_choice=required benoetigt mindestens ein Tool.".to_string());
    }
    if let BrowserToolChoice::Function(name) = choice {
        if !names.contains(name.as_str()) {
            return Err(format!(
                "tool_choice verweist auf unbekanntes Tool '{name}'."
            ));
        }
    }
    Ok(())
}

/// Nennt dem Modell sein Brain, bevor irgendetwas anderes kommt. Hinter der
/// Bridge wissen Web-Modelle sonst nicht, unter welchem Namen sie angesprochen
/// werden — am 2026-09-14 nannte sich deepseek ueber Pi "claude" (T-951).
/// Die API-Bridge loest `webagent/auto` vorher in ein konkretes Brain auf
/// (`api_bridge::inference`); kommt trotzdem `auto` an, ist das Brain noch
/// unbekannt und eine Behauptung waere falsch.
fn prompt_with_identity(brain: &str, prompt: &str) -> String {
    let brain = brain.trim();
    if brain.is_empty() || brain == "auto" {
        return prompt.to_string();
    }
    format!(
        "[Identitaet] Du bist das Modell hinter dem WebAgent-Brain \"{brain}\" (API-Modell webagent/{brain}). Fragt jemand nach deinem Namen oder deiner Identitaet, nenne \"{brain}\".\n\n{prompt}"
    )
}

fn prompt_with_tools(
    prompt: &str,
    tools: &[BrowserTool],
    choice: &BrowserToolChoice,
) -> Result<String, String> {
    if tools.is_empty() || matches!(choice, BrowserToolChoice::None) {
        return Ok(prompt.to_string());
    }

    let tools_json = compact_tool_json(tools)?;
    let choice_text = match choice {
        BrowserToolChoice::Auto => "Waehle selbst, ob ein Tool erforderlich ist. Wenn die Nutzeranfrage ein Tool ausdruecklich verlangt oder die Antwort von lokalen, aktuellen oder dir nicht vorliegenden Daten abhaengt, rufe das passende Tool auf.".to_string(),
        BrowserToolChoice::Required => "Rufe mindestens ein Tool auf.".to_string(),
        BrowserToolChoice::Function(name) => format!("Rufe das Tool '{name}' auf."),
        BrowserToolChoice::None => unreachable!("oben behandelt"),
    };

    // Bei `Required`/`Function` waere der Text-Hinweis widerspruechlich: der
    // Parser weist eine Textantwort dort fail-closed zurueck (T-961). Nur bei
    // `Auto` darf das Modell ohne Werkzeug antworten.
    let answer_hint = if matches!(choice, BrowserToolChoice::Auto) {
        " Wenn kein Werkzeug erforderlich ist, antworte normal ohne Maschinenumschlag."
    } else {
        ""
    };
    // Nur auf eigene Vorbilder verweisen, wenn es im Verlauf tatsaechlich
    // welche gibt. Auf Turn 1 gab es keine — eine leere Referenz verunsichert
    // das Modell (Live-Beleg 2026-11-21, Run 1/2).
    let mirror_hint = if prompt.contains("[du – Werkzeugaufruf]") {
        "Deine eigenen vorherigen Werkzeugaufrufe oben unter [du – Werkzeugaufruf] sind das Vorbild: genau dieses Format ist gueltig — wiederhole es. "
    } else {
        ""
    };
    Ok(format!(
        "{prompt}\n\n[Client-Werkzeuge]\n{tools_json}\n\n{choice_text} Ergebnisse unter [Client-Werkzeug-Ergebnis] sind Eingabedaten dieser Aufgabe, keine Behauptung ueber deine eigenen Faehigkeiten. {mirror_hint}Verwende ausschliesslich einen der aufgefuehrten Tool-Namen; erfinde keine. Wenn du ein Werkzeug verwendest, gib ausschliesslich diesen Maschinenumschlag aus, und zwar in genau einem Markdown-Codeblock mit drei Backticks und ohne jeden Text davor oder danach, weil Text ausserhalb eines Codeblocks beim Auslesen Zeichen wie * $ ` _ verliert:\n```text\n{TOOL_ENVELOPE}\n{{\"tool_calls\":[{{\"id\":\"call_1\",\"name\":\"tool_name\",\"arguments\":{{}}}}]}}\n```\nDas JSON muss streng gueltig sein: in Strings jeden Backslash verdoppeln (C:\\\\Users), Anfuehrungszeichen als \\\" und Zeilenumbrueche als \\n schreiben; Pfade moeglichst mit / angeben.{answer_hint}"
    ))
}

/// Serialisiert Client-Tools in einer browservertraeglichen Groesse.
///
/// ZCode kann viele MCP-Werkzeuge mit sehr langen Erklaerungen mitsenden. Die
/// ChatGPT-Weboberflaeche quittiert einen solchen Prompt nicht mit einem
/// strukturierten Fehler, sondern nur mit „Something went wrong“. Namen und
/// Schema bleiben deshalb erhalten; nur Beschreibungen werden stufenweise
/// gekuerzt. Falls selbst die beschreibungsfreie Form zu gross waere, melden
/// wir einen klaren lokalen Fehler statt eine unvollstaendige Werkzeugliste zu
/// senden.
fn compact_tool_json(tools: &[BrowserTool]) -> Result<String, String> {
    for &description_limit in TOOL_DESCRIPTION_LIMITS {
        let compacted = tools
            .iter()
            .map(|tool| {
                let mut parameters = tool.parameters.clone();
                compact_json_descriptions(&mut parameters, description_limit);
                BrowserTool {
                    name: tool.name.clone(),
                    description: tool
                        .description
                        .as_deref()
                        .map(|description| truncate_chars(description, description_limit)),
                    parameters,
                }
            })
            .collect::<Vec<_>>();
        let json = serde_json::to_string(&compacted)
            .map_err(|error| format!("Tooldefinitionen nicht serialisierbar: {error}"))?;
        if json.len() <= MAX_TOOL_SCHEMA_BYTES {
            return Ok(json);
        }
    }
    Err(format!(
        "Tooldefinitionen sind zu gross fuer den Browserprompt (Maximum {} KiB).",
        MAX_TOOL_SCHEMA_BYTES / 1024
    ))
}

fn compact_json_descriptions(value: &mut Value, limit: usize) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(description)) = object.get_mut("description") {
                *description = truncate_chars(description, limit);
            }
            for child in object.values_mut() {
                compact_json_descriptions(child, limit);
            }
        }
        Value::Array(items) => {
            for item in items {
                compact_json_descriptions(item, limit);
            }
        }
        _ => {}
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    if limit <= 3 {
        return value.chars().take(limit).collect();
    }
    let mut truncated: String = value.chars().take(limit - 3).collect();
    truncated.push_str("...");
    truncated
}

/// Repariert ausschliesslich eindeutige Fehler in JSON-String-Literalen, wie
/// Browsermodelle sie im Umschlag erzeugen (beobachtet 2026-09-14, T-952):
/// Ein Backslash vor einem Zeichen, das kein JSON-Escape ist — typisch fuer
/// Windows-Pfade wie `C:\Users` —, wird zum literalen Backslash; rohe
/// Steuerzeichen wie Zeilenumbrueche werden escaped. Alles andere bleibt
/// unveraendert, ein nicht eindeutiger Umschlag scheitert weiter fail-closed.
/// `C:\new` bleibt dagegen ein gueltiges `\n` und ist nicht reparierbar; dafuer
/// verlangt die Umschlag-Anweisung verdoppelte Backslashes.
///
/// Unescapte Anfuehrungszeichen (T-954, etwa `test -z "$(find ...)"` in einem
/// bash-Befehl): In gueltigem JSON folgt auf ein Stringende nach optionalem
/// Leerraum immer `,`, `}`, `]`, `:` oder das Textende. Folgt etwas anderes,
/// kann das Zeichen kein Stringende sein und wird escaped. Folgt eines dieser
/// Zeichen, gilt es als Ende — ist das falsch, scheitert das Parsen weiter.
///
/// Nicht geschlossene Objekte vor `]` (T-959, deepseek schrieb
/// `"timeout":90}]}` statt `"timeout":90}}]}`): Steht ausserhalb eines Strings
/// ein `]`, waehrend oben auf dem Klammerstapel noch `{` offen ist, ist `}` die
/// einzige Fortsetzung, die alle Zeichen in ihrer Reihenfolge behaelt. Offene
/// Klammern am Textende werden bewusst nicht geschlossen: das waere bei einer
/// abgeschnittenen Antwort ein unvollstaendiger Aufruf.
fn repair_json_string_escapes(payload: &str) -> String {
    let mut out = String::with_capacity(payload.len() + 16);
    let mut in_string = false;
    let mut open: Vec<char> = Vec::new();
    let mut chars = payload.chars().peekable();
    while let Some(c) = chars.next() {
        if !in_string {
            match c {
                '"' => in_string = true,
                '{' | '[' => open.push(c),
                '}' => {
                    if open.last() == Some(&'{') {
                        open.pop();
                    }
                }
                ']' => {
                    while open.last() == Some(&'{') {
                        out.push('}');
                        open.pop();
                    }
                    if open.last() == Some(&'[') {
                        open.pop();
                    }
                }
                _ => {}
            }
            out.push(c);
            continue;
        }
        match c {
            '"' => {
                let closes = matches!(
                    chars.clone().find(|next| !next.is_whitespace()),
                    None | Some(',' | '}' | ']' | ':')
                );
                if closes {
                    in_string = false;
                    out.push(c);
                } else {
                    out.push_str("\\\"");
                }
            }
            '\\' => {
                let valid_escape = match chars.peek() {
                    Some('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') => true,
                    Some('u') => {
                        let hex: Vec<char> = chars.clone().skip(1).take(4).collect();
                        hex.len() == 4 && hex.iter().all(char::is_ascii_hexdigit)
                    }
                    _ => false,
                };
                if valid_escape {
                    out.push(c);
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                } else {
                    out.push_str("\\\\");
                }
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// Ausschnitt um die von serde_json gemeldete Fehlerstelle. Die ersten
/// Zeichen eines langen Umschlags zeigen die Ursache meist nicht — am
/// 2026-09-14 lag sie hinter Zeichen 400 (T-955). `>>>HIER<<<` markiert die
/// Stelle; Steuerzeichen erscheinen escaped.
fn excerpt_around_error(text: &str, error: &serde_json::Error) -> String {
    let line_start: usize = text
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum();
    let mut pos = (line_start + error.column().saturating_sub(1)).min(text.len());
    while !text.is_char_boundary(pos) {
        pos -= 1;
    }
    let mut start = pos.saturating_sub(200);
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (pos + 120).min(text.len());
    while !text.is_char_boundary(end) {
        end += 1;
    }
    format!(
        "{}{}>>>HIER<<<{}{}",
        if start > 0 { "..." } else { "" },
        text[start..pos].escape_debug(),
        text[pos..end].escape_debug(),
        if end < text.len() { "..." } else { "" }
    )
}

#[derive(Deserialize)]
struct ToolEnvelope {
    tool_calls: Vec<ToolEnvelopeCall>,
}

#[derive(Deserialize)]
struct ToolEnvelopeCall {
    id: String,
    name: String,
    arguments: Value,
}

/// Findet den Maschinenumschlag auch dann, wenn das Modell davor Text
/// geschrieben hat (real am 2026-11-20: ChatGPT antwortete
/// `Nur Text\nWEBAGENT_INFERENCE/1\n{...}`). Der Marker muss am Zeilenanfang
/// stehen — eine blosse Erwaehnung im Fliesstext ist KEIN Tool-Call. Ein
/// Markdown-Codeblock-Abschluss wird entfernt, damit der JSON-Parser nicht
/// an den drei Backticks hängen bleibt.
fn envelope_payload(raw: &str) -> Option<&str> {
    if let Some(rest) = raw.strip_prefix(TOOL_ENVELOPE) {
        return Some(rest.trim());
    }
    let marker = format!("\n{TOOL_ENVELOPE}");
    let mut from = 0usize;
    while let Some(rel) = raw[from..].find(&marker) {
        let at = from + rel;
        let rest = &raw[at + marker.len()..];
        // Direkt nach dem Marker muss Whitespace oder eine Codeblock-Klammer
        // kommen, sonst war es bloss ein Wort im Satz.
        if rest
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '`')
        {
            let mut payload = rest.trim();
            if let Some(stripped) = payload.strip_suffix("```") {
                payload = stripped.trim();
            }
            if !payload.is_empty() {
                return Some(payload);
            }
        }
        from = at + marker.len();
    }
    None
}

fn parse_response(
    raw: &str,
    tools: &[BrowserTool],
    choice: &BrowserToolChoice,
) -> Result<BrowserInferenceResponse, String> {
    let trimmed = raw.trim();
    let Some(payload) = envelope_payload(trimmed) else {
        if DSML_MARKERS.iter().any(|marker| trimmed.contains(marker)) {
            return Err(model_output_error(format!(
                "{DSML_TOOL_FORMAT_ERROR}: Das Browsermodell lieferte natives DSML-Werkzeugformat statt eines {TOOL_ENVELOPE}-Umschlags. Rohtext ({} Zeichen) beginnt: {}",
                trimmed.chars().count(),
                truncate_chars(&trimmed.escape_debug().to_string(), 120)
            )));
        }
        if matches!(
            choice,
            BrowserToolChoice::Required | BrowserToolChoice::Function(_)
        ) {
            return Err(model_output_error(
                "Das Browsermodell lieferte trotz verpflichtendem Tool-Call nur Text.",
            ));
        }
        return Ok(BrowserInferenceResponse {
            text: Some(trimmed.to_string()),
            tool_calls: Vec::new(),
        });
    };

    let payload = payload.trim();
    let envelope: ToolEnvelope = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(strict_error) => {
            let repaired = repair_json_string_escapes(payload);
            let envelope = serde_json::from_str(&repaired).map_err(|repaired_error| {
                model_output_error(format!(
                    "Ungueltiger Browser-Tool-Call-Umschlag: {strict_error}; nach Reparatur: {repaired_error}; Rohtext ({} Zeichen) beginnt: {}; an der Fehlerstelle: {}",
                    payload.chars().count(),
                    truncate_chars(&payload.escape_debug().to_string(), 120),
                    excerpt_around_error(&repaired, &repaired_error)
                ))
            })?;
            crate::bench_events::eprint_line(&format!(
                "[inference] Tool-Call-Umschlag repariert (String-Escapes): {strict_error}"
            ));
            envelope
        }
    };
    if envelope.tool_calls.is_empty() {
        return Err(model_output_error(
            "Browser-Tool-Call-Umschlag enthaelt keine Tool Calls.",
        ));
    }

    let allowed: BTreeSet<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    let mut ids = BTreeSet::new();
    let mut calls = Vec::with_capacity(envelope.tool_calls.len());
    for call in envelope.tool_calls {
        if call.id.trim().is_empty() || !ids.insert(call.id.clone()) {
            return Err(model_output_error(
                "Tool-Call-IDs muessen nichtleer und eindeutig sein.",
            ));
        }
        if !allowed.contains(call.name.as_str()) {
            return Err(model_output_error(format!(
                "Browsermodell rief unbekanntes Tool '{}' auf.",
                call.name
            )));
        }
        if let BrowserToolChoice::Function(required) = choice {
            if &call.name != required {
                return Err(model_output_error(format!(
                    "Browsermodell rief '{}' statt des verlangten Tools '{}' auf.",
                    call.name, required
                )));
            }
        }
        calls.push(BrowserToolCall {
            id: call.id,
            name: call.name,
            arguments: call.arguments,
        });
    }

    Ok(BrowserInferenceResponse {
        text: None,
        tool_calls: calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_tool() -> BrowserTool {
        BrowserTool {
            name: "read_file".to_string(),
            description: Some("Datei lesen".to_string()),
            parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        }
    }

    #[test]
    fn required_prompt_bietet_keine_textantwort_an() {
        const TEXT_HINT: &str = "Wenn kein Werkzeug erforderlich ist, antworte normal";
        let tools = [read_tool()];

        let auto = prompt_with_tools("Auftrag", &tools, &BrowserToolChoice::Auto).unwrap();
        assert!(auto.contains(TEXT_HINT));

        let required = prompt_with_tools("Auftrag", &tools, &BrowserToolChoice::Required).unwrap();
        assert!(!required.contains(TEXT_HINT));
        assert!(required.contains("Rufe mindestens ein Tool auf."));

        let function = prompt_with_tools(
            "Auftrag",
            &tools,
            &BrowserToolChoice::Function("read_file".into()),
        )
        .unwrap();
        assert!(!function.contains(TEXT_HINT));
        assert!(function.contains("Rufe das Tool 'read_file' auf."));
    }

    #[test]
    fn t961_required_prompt_frames_data_and_mirrors_only_with_history() {
        // T-961 neue Strategie: Ergebnisse werden als Eingabedaten gerahmt
        // (kein Honesty-Konflikt), und der Verweis auf eigene Vorbilder
        // erscheint nur, wenn der Verlauf solche enthaelt.
        let tools = [read_tool()];
        let with_history = prompt_with_tools(
            "Verlauf\n[du – Werkzeugaufruf]\nWEBAGENT_INFERENCE/1\n{}",
            &tools,
            &BrowserToolChoice::Required,
        )
        .unwrap();
        assert!(
            with_history.contains("Eingabedaten dieser Aufgabe"),
            "Prompt muss Ergebnisse als Eingabedaten rahmen: {with_history}"
        );
        assert!(
            with_history.contains("Deine eigenen vorherigen Werkzeugaufrufe"),
            "Prompt muss auf die gespiegelten eigenen Aufrufe verweisen: {with_history}"
        );
        assert!(
            with_history.contains("ausschliesslich einen der aufgefuehrten Tool-Namen"),
            "Prompt muss erfundene Tool-Namen (files__search) ausschliessen: {with_history}"
        );

        let without_history =
            prompt_with_tools("Auftrag", &tools, &BrowserToolChoice::Required).unwrap();
        assert!(
            !without_history.contains("[du – Werkzeugaufruf] sind das Vorbild"),
            "ohne Verlauf darf keine leere Vorbild-Referenz stehen: {without_history}"
        );
    }

    #[test]
    fn empty_prompt_fails_before_browser_start() {
        let error = complete(BrowserInferenceRequest {
            brain: "nonexistent_brain_xyz",
            prompt: "  ",
            tools: &[],
            tool_choice: BrowserToolChoice::Auto,
            headless: true,
            timeout_secs: None,
            model: None,
        })
        .unwrap_err();

        assert!(error.contains("darf nicht leer sein"));
    }

    #[test]
    fn provider_chrome_and_reasoning_echo_are_stripped_for_all_brains() {
        assert_eq!(
            normalize_provider_text("zai", "Thought Process\n\nZAI_OK"),
            "ZAI_OK"
        );
        assert_eq!(
            normalize_provider_text("zai", "  thought process\r\nAntwort  \n"),
            "Antwort"
        );
        assert_eq!(normalize_provider_text("zai", "Thinking..."), "");
        assert_eq!(normalize_provider_text("mistral", "14:28"), "");
        assert_eq!(
            normalize_provider_text("chatgpt", "Thought Process\n\nInhalt"),
            "Inhalt"
        );
        // Chrome-only → empty (keep waiting / fail honest), not echoed as answer.
        assert_eq!(normalize_provider_text("zai", "Thought Process"), "");
        let kimi = "The user wants me to reply with exactly the token \"STREAM_OK\" and nothing else. This is a very simple request. I should not add any extra text.";
        assert_eq!(normalize_provider_text("kimi", kimi), "");
        assert_eq!(
            normalize_provider_text("kimi", &format!("{kimi}\n\nSTREAM_OK")),
            "STREAM_OK"
        );
    }

    #[test]
    fn strict_tool_envelope_is_normalized() {
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"README.md"}}]}"#
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();

        assert_eq!(response.finish_reason(), "tool_calls");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(response.tool_calls[0].arguments["path"], "README.md");
    }

    #[test]
    fn chatter_before_envelope_is_tolerated() {
        // Real 2026-11-20 (chatgpt via Pi, T-961): das Modell schrieb "Nur Text"
        // vor den Umschlag. Ein striktes strip_prefix verwarf den Tool-Call und
        // lieferte ihn als Text an Pi — der Tool-Loop blieb bei 0 Starts stehen.
        let raw = concat!(
            "Nur Text\n",
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"nonce-1.txt"}}]}"#
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Required).unwrap();
        assert_eq!(response.finish_reason(), "tool_calls");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(response.tool_calls[0].arguments["path"], "nonce-1.txt");
    }

    #[test]
    fn fenced_envelope_with_leading_chatter_is_tolerated() {
        let raw = concat!(
            "Hier ist der Aufruf:\n",
            "```text\n",
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"nonce-2.txt"}}]}"#,
            "\n```"
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Required).unwrap();
        assert_eq!(response.tool_calls[0].arguments["path"], "nonce-2.txt");
    }

    #[test]
    fn inline_marker_mention_is_not_a_tool_call() {
        // Der Marker muss am Zeilenanfang stehen, sonst ist er erklaerender
        // Fliesstext und darf keinen Tool-Call erfinden.
        let raw = "Der Marker WEBAGENT_INFERENCE/1 wird hier nur erwaehnt.";
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();
        assert!(response.tool_calls.is_empty());
        assert!(response
            .text
            .unwrap_or_default()
            .contains("WEBAGENT_INFERENCE/1"));
    }

    #[test]
    fn real_dsml_example_never_becomes_a_text_answer() {
        // Reales Beispiel aus docs/TASKBOARD.json (T-957, deepseek, 14.09.2026).
        // Die Zeichen zwischen den Tags sind die im Board gespeicherte, durch
        // eine Zeichencodierung verformte Form des DSML-Markers.
        let raw = "<´¢£´¢£DSML´¢£´¢£ calls><´¢£´¢£DSML´¢£´¢£ invoke name=\"write\"><´¢£´¢£DSML´¢£´¢£ parameter name=\"path\" string=\"true\">...</´¢£´¢£DSML´¢£´¢£ parameter></´¢£´¢£DSML´¢£´¢£ invoke></´¢£´¢£DSML´¢£´¢£ calls>";
        let error = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap_err();
        assert!(error.contains("DSML_TOOL_FORMAT_UNSUPPORTED"), "{error}");
        assert!(error.contains("WEBAGENT_INFERENCE/1"), "{error}");
    }

    #[test]
    fn dsml_with_fullwidth_marker_and_chatter_fails_closed() {
        let raw = concat!(
            "Ich rufe das Werkzeug auf:\n",
            "<｜｜DSML｜｜ calls><｜｜DSML｜｜ invoke name=\"write\"><｜｜DSML｜｜ parameter name=\"path\" string=\"true\">nonce-1.txt</｜｜DSML｜｜ parameter></｜｜DSML｜｜ invoke></｜｜DSML｜｜ calls>"
        );
        for choice in [BrowserToolChoice::Auto, BrowserToolChoice::Required] {
            let error = parse_response(raw, &[read_tool()], &choice).unwrap_err();
            assert!(error.contains("DSML_TOOL_FORMAT_UNSUPPORTED"), "{error}");
        }
    }

    #[test]
    fn unknown_tool_call_fails_closed() {
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"shell","arguments":{}}]}"#
        );
        let error = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap_err();
        assert!(error.contains("unbekanntes Tool"));
    }

    #[test]
    fn windows_path_with_single_backslashes_is_repaired() {
        // Real 2026-09-14 (chatgpt via Pi): "invalid escape" durch C:\Users.
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"C:\Users\storax\README.md"}}]}"#
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();
        assert_eq!(
            response.tool_calls[0].arguments["path"],
            r"C:\Users\storax\README.md"
        );
    }

    #[test]
    fn raw_newline_inside_string_is_repaired() {
        // Real 2026-09-14: "control character found while parsing a string".
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            "{\"tool_calls\":[{\"id\":\"call_1\",\"name\":\"read_file\",\"arguments\":{\"path\":\"a\nb\"}}]}"
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();
        assert_eq!(response.tool_calls[0].arguments["path"], "a\nb");
    }

    #[test]
    fn repair_keeps_valid_escapes_and_structure() {
        let valid = r#"{"a":"C:\\Users","b":"say \"hi\"","c":"\u00e4\n","d":[1,{"e":null}]}"#;
        assert_eq!(repair_json_string_escapes(valid), valid);
        assert_eq!(
            repair_json_string_escapes(r#"{"p":"C:\users\x"}"#),
            r#"{"p":"C:\\users\\x"}"#
        );
    }

    #[test]
    fn unescaped_quotes_in_shell_command_are_repaired() {
        // Real 2026-09-14 13:26 (chatgpt via Pi): "expected `,` or `}`" durch
        // test -z "$(find ...)" im bash-Befehl.
        let command = r#"test -z "$(find ../w -mindepth 1 -print -quit)" && echo 'workspace empty' || { echo 'workspace not empty'; exit 1; }"#;
        let raw = format!(
            "WEBAGENT_INFERENCE/1\n{{\"tool_calls\":[{{\"id\":\"call_1\",\"name\":\"read_file\",\"arguments\":{{\"command\":\"{command}\"}}}}]}}"
        );
        let response = parse_response(&raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();
        assert_eq!(response.tool_calls[0].arguments["command"], command);
    }

    #[test]
    fn unclosed_call_object_before_bracket_is_closed() {
        // Real 2026-09-14 15:21 und 15:22 (deepseek): "timeout":90}]} statt }}]}.
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"command":"find . -name '*.json' | head -20","timeout":90}]}"#
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();
        assert_eq!(
            response.tool_calls[0].arguments["command"],
            "find . -name '*.json' | head -20"
        );
        assert_eq!(response.tool_calls[0].arguments["timeout"], 90);
    }

    #[test]
    fn surplus_or_crossed_brackets_and_truncation_stay_errors() {
        for payload in [
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{}}}]}"#,
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":[1}}]}"#,
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"a"}"#,
        ] {
            let raw = format!("WEBAGENT_INFERENCE/1\n{payload}");
            assert!(
                parse_response(&raw, &[read_tool()], &BrowserToolChoice::Auto).is_err(),
                "{payload}"
            );
        }
    }

    #[test]
    fn quote_followed_by_json_delimiter_is_not_guessed() {
        // "x" gefolgt von "," sieht wie ein Stringende aus; der Rest bricht.
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"echo "x", done"}}]}"#
        );
        assert!(parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).is_err());
    }

    #[test]
    fn ambiguous_envelope_still_fails_closed_with_raw_excerpt() {
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"a", "b"}}]}"#
        );
        let error = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap_err();
        assert!(error.contains("Ungueltiger Browser-Tool-Call-Umschlag"));
        assert!(error.contains("Rohtext ("));
        assert!(error.contains("read_file"));
        assert!(error.contains(">>>HIER<<<"));
    }

    /// DoD T-953 (1): jeder Parserfehler der Modellausgabe traegt den stabilen
    /// Marker, anhand dessen die Bridge `server_error` statt
    /// `invalid_request_error` meldet. Gueltige Antworten bleiben unmarkiert.
    #[test]
    fn parse_fehler_tragen_den_modellausgabe_marker() {
        let broken = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"a", "b"}}]}"#
        );
        let error = parse_response(broken, &[read_tool()], &BrowserToolChoice::Auto).unwrap_err();
        assert!(
            error.starts_with(MODEL_OUTPUT_ERROR_MARKER),
            "Marker fehlt: {error}"
        );

        let required = "nur Text, kein Umschlag";
        let error =
            parse_response(required, &[read_tool()], &BrowserToolChoice::Required).unwrap_err();
        assert!(
            error.starts_with(MODEL_OUTPUT_ERROR_MARKER),
            "Marker fehlt: {error}"
        );

        let ok = parse_response("normale Antwort", &[read_tool()], &BrowserToolChoice::Auto)
            .expect("Textantwort ist bei Auto gueltig");
        assert!(ok.text.is_some());
    }

    #[test]
    fn error_excerpt_shows_the_failure_behind_a_long_prefix() {
        // Real 2026-09-14 13:55: Fehler "line 2 column 1" hinter Zeichen 400.
        let long = "x".repeat(600);
        let raw = format!(
            "WEBAGENT_INFERENCE/1\n{{\"tool_calls\":[{{\"id\":\"call_1\",\"name\":\"read_file\",\"arguments\":{{\"path\":\"{long}\"}}\nKAPUTT}}]}}"
        );
        let error = parse_response(&raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap_err();
        let at = error.split("an der Fehlerstelle: ").nth(1).unwrap();
        let (before, after) = at.split_once(">>>HIER<<<").unwrap();
        assert!(before.ends_with("\\n"), "{error}");
        assert!(after.starts_with("KAPUTT"), "{error}");
        assert!(error.contains("Zeichen) beginnt:"));
    }

    #[test]
    fn prompt_names_the_brain_first() {
        let prompt = prompt_with_identity("deepseek", "Wer bist du?");
        assert!(prompt
            .starts_with("[Identitaet] Du bist das Modell hinter dem WebAgent-Brain \"deepseek\""));
        assert!(prompt.contains("webagent/deepseek"));
        assert!(prompt.ends_with("\n\nWer bist du?"));
    }

    #[test]
    fn auto_brain_gets_no_identity_claim() {
        assert_eq!(prompt_with_identity("auto", "Hallo"), "Hallo");
        assert_eq!(prompt_with_identity("  ", "Hallo"), "Hallo");
    }

    #[test]
    fn required_tool_rejects_plain_text() {
        let error = parse_response(
            "Ich wuerde die Datei lesen.",
            &[read_tool()],
            &BrowserToolChoice::Required,
        )
        .unwrap_err();
        assert!(error.contains("verpflichtendem Tool-Call"));
    }

    #[test]
    fn auto_choice_requires_tools_for_explicit_or_unknown_data_requests() {
        let prompt = prompt_with_tools(
            "Lies die Datei nonce.txt.",
            &[read_tool()],
            &BrowserToolChoice::Auto,
        )
        .unwrap();

        assert!(prompt.contains("ein Tool ausdruecklich verlangt"));
        assert!(prompt.contains("dir nicht vorliegenden Daten"));
        assert!(prompt.contains("[Client-Werkzeuge]"));
        // T-956: Umschlag im Codeblock, sonst frisst das Rendering Zeichen.
        assert!(prompt.contains("in genau einem Markdown-Codeblock"));
        assert!(prompt.contains("```text\nWEBAGENT_INFERENCE/1\n"));
        assert!(!prompt.contains("gehoeren dem aufrufenden API-Client"));
        assert!(!prompt.contains("niemals, sie seien in dieser Umgebung nicht verfuegbar"));
        assert!(prompt.contains("WEBAGENT_INFERENCE/1"));
    }

    #[test]
    fn large_tool_descriptions_are_compacted_without_dropping_names_or_schema() {
        let long = "x".repeat(MAX_TOOL_SCHEMA_BYTES);
        let tools = vec![BrowserTool {
            name: "keep_this_tool".to_string(),
            description: Some(long.clone()),
            parameters: serde_json::json!({
                "type": "object",
                "description": long,
                "properties": {"path": {"type": "string", "description": "path"}}
            }),
        }];
        let serialized = compact_tool_json(&tools).unwrap();
        assert!(serialized.len() <= MAX_TOOL_SCHEMA_BYTES);
        assert!(serialized.contains("keep_this_tool"));
        assert!(serialized.contains("\"path\""));
        assert!(!serialized.contains(&"x".repeat(2048)));
    }
}
