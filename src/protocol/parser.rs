use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

use super::types::{Action, ActionType, EditOperation, ParseResult, PROTOCOL_VERSION};

fn json_block_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"```(?:json)?\s*(\{.*\})\s*```").unwrap())
}

fn rendered_json_label_regex() -> &'static FancyRegex {
    static RE: OnceLock<FancyRegex> = OnceLock::new();
    RE.get_or_init(|| {
        FancyRegex::new(
            r"(?i)^json\s*\r?\n(?:(?:copy|kopieren)\s*\r?\n)?(?:(?:download|herunterladen)\s*\r?\n)?(?=\s*\{)",
        )
        .unwrap()
    })
}

pub fn ui_control_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // "plain" fehlte: kimi lieferte am 2026-07-21 abwechselnd
        // "JSON\nKopieren\n{...}" (ging durch) und "plain\nKopieren\n```json\n{...}"
        // (scheiterte). Das Sprachlabel des gerenderten Code-Blocks variiert je
        // nach Oberflaeche und Inhalt — deshalb die ueblichen mit aufnehmen.
        Regex::new(
            r"(?i)^(?:json|plain|plaintext|text|code|bash|sh|shell|powershell|ps1|rust|\d+|copy|kopieren|download|herunterladen)$",
        )
        .unwrap()
    })
}

pub fn script_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\AWEBAGENT/1 SHELL\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\ntimeout_seconds:\s*([0-9]+(?:\.[0-9]+)?)\r?\n---SCRIPT---\r?\n([\s\S]+?)\r?\n---END SCRIPT---\s*\z",
        )
        .unwrap()
    })
}

/// Kompatibles Kurzformat fuer Shell-Aktionen, das einige Provider trotz des
/// aktuellen SHELL-Vertrags aus aelteren Gespraechskontexten verwenden.
fn run_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?s)\AWEBAGENT/1 RUN\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\ncommand:\s*(\"[^\r\n]*\")\r?\n---END RUN---\s*\z"#,
        )
        .unwrap()
    })
}

/// Rohformat für `write` — Dateiinhalt roh zwischen Markern, ohne JSON-Escaping.
/// Löst den Fall, an dem Web-Brains mehrzeiligen Code mit Quotes nicht als
/// JSON-String kodieren konnten (Fund 2026-07-21, autonomer Selbstbau-Versuch).
/// caps: 1=id, 2=path, 3=content (darf leer sein).
pub fn write_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\AWEBAGENT/1 WRITE\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\npath:\s*([^\r\n]+?)\r?\n---CONTENT---\r?\n([\s\S]*?)\r?\n---END CONTENT---\s*\z",
        )
        .unwrap()
    })
}

/// Rohformat für `edit` — old/new roh zwischen Markern, ohne JSON-Escaping.
/// caps: 1=id, 2=path, 3=old_string, 4=new_string. old/new werden NICHT getrimmt
/// (Einrückung/Whitespace ist für den Anker signifikant).
pub fn edit_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\AWEBAGENT/1 EDIT\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\npath:\s*([^\r\n]+?)\r?\n---OLD---\r?\n([\s\S]*?)\r?\n---NEW---\r?\n([\s\S]*?)\r?\n---END EDIT---\s*\z",
        )
        .unwrap()
    })
}

pub(super) fn edit_batch_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\AWEBAGENT/1 EDIT_BATCH\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\n([\s\S]+)\r?\n---END BATCH---\s*\z",
        )
        .unwrap()
    })
}

fn parse_raw_edit_batch(text: &str) -> Result<Option<Action>, String> {
    let Some(caps) = edit_batch_envelope_regex().captures(text) else {
        return Ok(None);
    };
    let body = caps[2].replace("\r\n", "\n");
    let edit_re = Regex::new(
        r"(?s)\Apath:\s*([^\n]+?)\n---OLD---\n([\s\S]*?)\n---NEW---\n([\s\S]*?)\n---END EDIT---\s*\z",
    )
    .unwrap();
    let mut edits = Vec::new();
    for part in body.split("---EDIT---\n").skip(1) {
        let item = edit_re.captures(part).ok_or_else(|| {
            "EDIT_BATCH enthält einen ungültigen EDIT-Block oder Text außerhalb der Blöcke"
                .to_string()
        })?;
        let path = item[1].trim().to_string();
        let old_string = item[2].to_string();
        let new_string = item[3].to_string();
        if path.is_empty() || old_string.is_empty() || old_string == new_string {
            return Err(
                "EDIT_BATCH braucht je Block path sowie verschiedene old/new-Inhalte".to_string(),
            );
        }
        edits.push(EditOperation {
            path,
            old_string,
            new_string,
        });
    }
    if edits.is_empty() || !body.starts_with("---EDIT---\n") {
        return Err("EDIT_BATCH braucht mindestens einen ---EDIT--- Block".to_string());
    }
    let mut action = Action::base(caps[1].to_string(), ActionType::EditBatch);
    action.edits = edits;
    Ok(Some(action))
}

/// Rohformat fuer eine abschliessende Nutzerantwort. Anders als Edit/Write
/// braucht Message keinen Endmarker: alles nach `text:` gehoert zur Antwort.
pub fn message_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\AWEBAGENT/1 MESSAGE\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\ntext:\s*([\s\S]+?)\s*\z",
        )
        .unwrap()
    })
}

/// `true`, wenn die Zeile ein Sprachlabel eines gerenderten Code-Blocks ist
/// (und kein Bedienelement wie „Kopieren").
fn is_language_label(label: &str) -> bool {
    const LANGS: &[&str] = &[
        "json",
        "plain",
        "plaintext",
        "text",
        "code",
        "bash",
        "sh",
        "shell",
        "powershell",
        "ps1",
        "rust",
    ];
    LANGS.iter().any(|l| label.eq_ignore_ascii_case(l))
}

pub fn strip_rendered_ui_controls(text: &str) -> String {
    let normalized = text
        .replace(['\u{00a0}', '\u{202f}'], " ")
        .replace('\u{200b}', "")
        .trim()
        .to_string();

    let lines: Vec<&str> = normalized.lines().collect();
    let mut index = 0;
    // Frueher wurde NUR ein "json"-Label als Startsignal akzeptiert; jede andere
    // Sprachbezeichnung liess den Vorspann stehen und das JSON scheiterte.
    let mut saw_language_label = false;
    let re = ui_control_line_regex();

    while index < lines.len() {
        let label = lines[index].trim();
        if !re.is_match(label) {
            break;
        }
        if is_language_label(label) {
            saw_language_label = true;
        } else if !saw_language_label {
            break;
        }
        index += 1;
    }

    if saw_language_label {
        lines[index..].join("\n").trim().to_string()
    } else {
        normalized
    }
}

/// Repariert die zwei Defekte, mit denen Brains reihenweise gültiges Protokoll
/// zerschießen, sobald ein Shell-Befehl selbst Anführungszeichen enthält.
///
/// Gemessen am 29.07.2026: Läufe wie `20260729_205525_94d2066c` bestanden aus
/// drei Brain-Turns, alle drei `protocol_invalid` — und nach dem Repair-Prompt
/// schickte das Brain **denselben** Befehl erneut (`step-2` → `repair-1` →
/// `step-3`). Der Roundtrip half also nie, der Lauf lief in den Stall. Beispiel:
///
/// ```text
/// {"…","command":"Select-String -Pattern "a|b" -Context 2,4","…":30}]}
/// ```
///
/// Zwei Ursachen, beide im String-Wert:
/// 1. **Unescaptes `"`** mitten im Wert. Erkennbar daran, dass nach dem
///    Anführungszeichen (ohne Leerraum) kein struktureller Folger `, } ] :`
///    kommt — dann ist es Inhalt und kein String-Ende.
/// 2. **Regex-Escapes** wie `\[cfg\(test\)\]`. JSON kennt nur
///    `" \ / b f n r t u` — `\[` ist also kein Escape, sondern ein literaler
///    Backslash.
///
/// Absichtlich **nicht** repariert werden Backslashes vor Buchstaben (`\U` in
/// `C:\Users`, `\d`, `\w`). Dort ist die Absicht mehrdeutig, und
/// `test_never_repair_unescaped_windows_path_for_shell` hält das bewusst offen:
/// ein Shell-Befehl darf nicht stillschweigend umgedeutet werden. Repariert wird
/// nur Regex-Interpunktion, die weder in JSON noch in PowerShell ein Escape ist
/// — da gibt es keine zweite Lesart.
///
/// Rein syntaktisch, ein einziger Durchlauf mit mitgeführtem String-Zustand.
/// `None`, wenn nichts zu reparieren war.
pub fn repair_unescaped_quotes_in_strings(json_text: &str) -> Option<String> {
    let chars: Vec<char> = json_text.chars().collect();
    let mut out = String::with_capacity(json_text.len() + 16);
    let mut in_string = false;
    let mut changed = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if !in_string {
            if c == '"' {
                in_string = true;
            }
            out.push(c);
            i += 1;
            continue;
        }

        // Ab hier: innerhalb eines String-Werts.
        if c == '\\' {
            match chars.get(i + 1).copied() {
                // Gültige JSON-Escapes unverändert übernehmen.
                Some(n @ ('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u')) => {
                    out.push(c);
                    out.push(n);
                    i += 2;
                }
                // Regex-Interpunktion: eindeutig ein literaler Backslash.
                Some(
                    '[' | ']' | '(' | ')' | '{' | '}' | '.' | '+' | '*' | '?' | '|' | '^' | '$'
                    | '-' | '#',
                ) => {
                    out.push_str("\\\\");
                    changed = true;
                    i += 1;
                }
                // Alles andere (insbesondere `\U`, `\d`) bleibt mehrdeutig und
                // wird nicht angetastet — siehe Doc-Kommentar.
                _ => {
                    out.push(c);
                    i += 1;
                }
            }
            continue;
        }

        if c == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            // Struktureller Folger → echtes String-Ende.
            if matches!(chars.get(j), Some(',' | '}' | ']' | ':') | None) {
                in_string = false;
                out.push(c);
            } else {
                out.push_str("\\\"");
                changed = true;
            }
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn repair_message_windows_paths(json_text: &str) -> Option<String> {
    // Nur reparieren wenn alle Actions vom Typ "message" sind
    if !json_text.contains(r#""type""#) || !json_text.contains(r#""message""#) {
        return None;
    }
    if json_text.contains(r#""shell""#) || json_text.contains(r#""finish""#) {
        return None;
    }

    let re = Regex::new(r#"[A-Za-z]:\\[^"\r\n]*"#).unwrap();
    let repaired = re.replace_all(json_text, |caps: &regex::Captures| {
        caps[0].replace('\\', "\\\\")
    });

    if repaired != json_text {
        Some(repaired.to_string())
    } else {
        None
    }
}

// ============================================================================
// SCHEMA-REFERENZ webagent/1 (Single Source of Truth — spiegelt sich in
// docs/PROTOCOL_SCHEMA.md, das aus diesem Block abgeleitet ist).
//
// Envelope (Wurzel-Objekt):
//   protocol : String  == "webagent/1"           (Pflicht)
//   actions  : Array   nicht-leer                 (Pflicht)
//
// Jede Action ist ein Objekt. Erlaubte Felder je type — KEINE anderen Felder
// zugelassen (unbekannte Felder → invalid, damit Tippfehler wie "comand" statt
// "command" nicht als leerer Befehl durchrutschen). Gemeinsam für alle: {id, type}.
//
//   type "shell"   +{command, timeout_seconds}
//                    command: nicht-leer (getrimmt)
//                    timeout_seconds: Zahl, 0 < x <= 3600 (Default 30)
//   type "message" +{text}
//                    text: nicht-leer (getrimmt)
//   type "finish"   (nur id, type)
//   type "edit"    +{path, old_string, new_string}
//                    path: nicht-leer (getrimmt); old_string: nicht-leer,
//                    old_string != new_string
//   type "write"   +{path, content}
//                    path: nicht-leer (getrimmt); content: Pflicht (auch "" ok)
//
// Zusatzregeln (in parse(), nicht pro Action): finish und message müssen jeweils
// die EINZIGE Action der Antwort sein; Action-ids müssen eindeutig sein.
// ============================================================================

/// Erlaubte Feldnamen je Action-`type`, inklusive der gemeinsamen `id`/`type`.
/// Grundlage der Strikt-Validierung gegen unbekannte Felder.
fn allowed_fields(action_type: &ActionType) -> &'static [&'static str] {
    match action_type {
        ActionType::Shell => &["id", "type", "command", "timeout_seconds"],
        ActionType::Message => &["id", "type", "text"],
        ActionType::Finish => &["id", "type"],
        ActionType::Edit => &["id", "type", "path", "old_string", "new_string"],
        ActionType::EditBatch => &["id", "type", "edits"],
        ActionType::Write => &["id", "type", "path", "content"],
    }
}

fn action_from_value(val: &Value) -> Result<Action, String> {
    let obj = val.as_object().ok_or("jede Action muss ein Objekt sein")?;

    let action_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("jede Action braucht eine id")?
        .to_string();

    let action_type_str = obj
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Action {} braucht type", action_id))?;

    let action_type = match action_type_str {
        "shell" => ActionType::Shell,
        "message" => ActionType::Message,
        "finish" => ActionType::Finish,
        "edit" => ActionType::Edit,
        "edit_batch" => ActionType::EditBatch,
        "write" => ActionType::Write,
        _ => return Err(format!("unbekannter type: {:?}", action_type_str)),
    };

    // Strikte Schema-Prüfung: keine unbekannten Felder je Action-type.
    let allowed = allowed_fields(&action_type);
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "Action {}: unbekanntes Feld {:?} für type {:?}; erlaubt sind {:?}",
                action_id, key, action_type_str, allowed
            ));
        }
    }

    let str_field = |key: &str| -> String {
        obj.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    match action_type {
        ActionType::Shell => {
            let command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if command.is_empty() {
                return Err(format!("shell action {} braucht command", action_id));
            }

            let default_timeout = Value::Number(30.into());
            let raw_timeout = obj.get("timeout_seconds").unwrap_or(&default_timeout);

            // Prüfe ob es ein bool ist (nicht erlaubt)
            if raw_timeout.is_boolean() {
                return Err(format!(
                    "shell action {}: timeout_seconds muss eine Zahl sein",
                    action_id
                ));
            }

            let timeout = raw_timeout.as_f64().ok_or_else(|| {
                format!(
                    "shell action {}: timeout_seconds muss eine Zahl sein",
                    action_id
                )
            })?;

            if !timeout.is_finite() || timeout <= 0.0 || timeout > 3600.0 {
                return Err(format!(
                    "shell action {}: timeout_seconds muss endlich und groesser als 0 und hoechstens 3600 sein",
                    action_id
                ));
            }

            // Prüfe auf verschachteltes Rohskript
            let re = script_envelope_regex();
            if let Some(caps) = re.captures(&command) {
                let nested_id = &caps[1];
                if nested_id != action_id {
                    return Err(
                        "verschachtelte Rohskript-ID stimmt nicht mit Action-ID überein"
                            .to_string(),
                    );
                }
                let nested_command = caps[3].trim().to_string();
                let nested_timeout: f64 = caps[2].parse().unwrap();
                let mut a = Action::base(action_id, ActionType::Shell);
                a.command = nested_command;
                a.timeout_seconds = nested_timeout;
                return Ok(a);
            }

            let mut a = Action::base(action_id, ActionType::Shell);
            a.command = command;
            a.timeout_seconds = timeout;
            Ok(a)
        }
        ActionType::Message => {
            let text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if text.is_empty() {
                return Err(format!("message action {} braucht text", action_id));
            }

            let mut a = Action::base(action_id, ActionType::Message);
            a.text = text;
            Ok(a)
        }
        ActionType::Finish => Ok(Action::base(action_id, ActionType::Finish)),
        ActionType::Edit => {
            let path = str_field("path").trim().to_string();
            let old_string = str_field("old_string");
            let new_string = str_field("new_string");
            if path.is_empty() {
                return Err(format!("edit action {} braucht path", action_id));
            }
            if old_string.is_empty() {
                return Err(format!(
                    "edit action {} braucht old_string (exakter, eindeutiger Anker aus der Datei)",
                    action_id
                ));
            }
            if old_string == new_string {
                return Err(format!(
                    "edit action {}: old_string und new_string sind identisch",
                    action_id
                ));
            }
            let mut a = Action::base(action_id, ActionType::Edit);
            a.path = path;
            a.old_string = old_string;
            a.new_string = new_string;
            Ok(a)
        }
        ActionType::EditBatch => {
            let raw_edits = obj
                .get("edits")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("edit_batch action {} braucht edits", action_id))?;
            if raw_edits.is_empty() {
                return Err(format!(
                    "edit_batch action {} braucht mindestens ein edit",
                    action_id
                ));
            }
            let mut edits = Vec::with_capacity(raw_edits.len());
            for (index, raw) in raw_edits.iter().enumerate() {
                let edit: EditOperation = serde_json::from_value(raw.clone()).map_err(|e| {
                    format!(
                        "edit_batch action {}: edit {} ungültig: {e}",
                        action_id,
                        index + 1
                    )
                })?;
                if edit.path.trim().is_empty() || edit.old_string.is_empty() {
                    return Err(format!(
                        "edit_batch action {}: edit {} braucht path und old_string",
                        action_id,
                        index + 1
                    ));
                }
                if edit.old_string == edit.new_string {
                    return Err(format!(
                        "edit_batch action {}: edit {} hat identische old/new-Inhalte",
                        action_id,
                        index + 1
                    ));
                }
                edits.push(edit);
            }
            let mut action = Action::base(action_id, ActionType::EditBatch);
            action.edits = edits;
            Ok(action)
        }
        ActionType::Write => {
            let path = str_field("path").trim().to_string();
            if path.is_empty() {
                return Err(format!("write action {} braucht path", action_id));
            }
            if !obj.contains_key("content") {
                return Err(format!("write action {} braucht content", action_id));
            }
            let mut a = Action::base(action_id, ActionType::Write);
            a.path = path;
            a.content = str_field("content");
            Ok(a)
        }
    }
}

pub fn parse(response_text: &str) -> ParseResult {
    let text = strip_rendered_ui_controls(response_text);

    if text.is_empty() {
        return ParseResult::invalid("Leere Antwort.", text);
    }

    match parse_raw_edit_batch(&text) {
        Ok(Some(action)) => return ParseResult::valid(vec![action], text),
        Err(error) => return ParseResult::invalid(error, text),
        Ok(None) => {}
    }

    // WEBAGENT/1 SHELL Rohskript-Format
    let script_re = script_envelope_regex();
    if let Some(caps) = script_re.captures(&text) {
        let timeout: f64 = caps[2].parse().unwrap();
        if !timeout.is_finite() || timeout <= 0.0 || timeout > 3600.0 {
            return ParseResult::invalid(
                "timeout_seconds muss groesser als 0 und hoechstens 3600 sein",
                text,
            );
        }
        let mut a = Action::base(caps[1].to_string(), ActionType::Shell);
        a.command = caps[3].trim().to_string();
        a.timeout_seconds = timeout;
        return ParseResult::valid(vec![a], text);
    }

    // Legacy-Kurzform, strikt als Top-Level-Huelle und mit JSON-kodiertem
    // Command. Damit wird keine erklaerende Prosa versehentlich ausgefuehrt.
    if let Some(caps) = run_envelope_regex().captures(&text) {
        let command = match serde_json::from_str::<String>(&caps[2]) {
            Ok(command) if !command.trim().is_empty() => command,
            Ok(_) => return ParseResult::invalid("run: command darf nicht leer sein", text),
            Err(error) => {
                return ParseResult::invalid(
                    format!("run: command ist kein gueltiger JSON-String: {error}"),
                    text,
                )
            }
        };
        let mut action = Action::base(caps[1].to_string(), ActionType::Shell);
        action.command = command;
        action.timeout_seconds = 120.0;
        return ParseResult::valid(vec![action], text);
    }

    // WEBAGENT/1 WRITE Rohformat (Dateiinhalt ohne JSON-Escaping).
    if let Some(caps) = write_envelope_regex().captures(&text) {
        let path = caps[2].trim().to_string();
        if path.is_empty() {
            return ParseResult::invalid("write: path darf nicht leer sein", text);
        }
        let mut a = Action::base(caps[1].to_string(), ActionType::Write);
        a.path = path;
        a.content = caps[3].to_string();
        return ParseResult::valid(vec![a], text);
    }

    // WEBAGENT/1 EDIT Rohformat (old/new ohne JSON-Escaping).
    if let Some(caps) = edit_envelope_regex().captures(&text) {
        let path = caps[2].trim().to_string();
        let old_string = caps[3].to_string();
        let new_string = caps[4].to_string();
        if path.is_empty() {
            return ParseResult::invalid("edit: path darf nicht leer sein", text);
        }
        if old_string.is_empty() {
            return ParseResult::invalid("edit: old_string braucht einen Anker", text);
        }
        if old_string == new_string {
            return ParseResult::invalid("edit: old_string und new_string sind identisch", text);
        }
        let mut a = Action::base(caps[1].to_string(), ActionType::Edit);
        a.path = path;
        a.old_string = old_string;
        a.new_string = new_string;
        return ParseResult::valid(vec![a], text);
    }

    // WEBAGENT/1 MESSAGE Rohformat fuer robuste Abschlussantworten.
    if let Some(caps) = message_envelope_regex().captures(&text) {
        let message = caps[2].trim().to_string();
        if message.is_empty() {
            return ParseResult::invalid("message: text darf nicht leer sein", text);
        }
        let mut a = Action::base(caps[1].to_string(), ActionType::Message);
        a.text = message;
        return ParseResult::valid(vec![a], text);
    }

    // Entferne gerenderte JSON-Labels
    let label_re = rendered_json_label_regex();
    let text = label_re.replace(&text, "").trim().to_string();

    // Suche nach JSON-Codeblock
    let block_re = json_block_regex();
    let json_str = if let Some(caps) = block_re.captures(&text) {
        let before = &text[..caps.get(0).unwrap().start()];
        let after = &text[caps.get(0).unwrap().end()..];
        let outside = format!("{}{}", before, after).trim().to_string();

        if !outside.is_empty() {
            return ParseResult::invalid(
                "Text außerhalb des JSON-Codeblocks ist nicht erlaubt.",
                text,
            );
        }
        caps[1].trim().to_string()
    } else {
        text.clone()
    };

    // Parse JSON
    let doc = match serde_json::from_str::<Value>(&json_str) {
        Ok(v) => v,
        Err(exc) => {
            // Provider-Hinweise erst dann klassifizieren, wenn die Antwort kein
            // gueltiges WebAgent-Protokoll ist. Andernfalls wuerden legitime
            // Actions wie `rg "rate limit" src` allein wegen ihres Inhalts
            // faelschlich als Provider-Sperre verworfen.
            if looks_like_capacity_notice(&text) {
                return ParseResult::invalid("Model capacity / rate limit.", text);
            }
            // Anführungszeichen/Escapes im String-Wert reparieren (häufigster
            // Fall: ein Shell-Befehl mit eigenen Quotes oder Regex-Escapes).
            if let Some(repaired) = repair_unescaped_quotes_in_strings(&json_str) {
                if let Ok(v) = serde_json::from_str::<Value>(&repaired) {
                    v
                } else {
                    return ParseResult::invalid(format!("Ungültiges JSON: {}", exc), text);
                }
            }
            // Versuche Windows-Path-Reparatur
            else if let Some(repaired) = repair_message_windows_paths(&json_str) {
                match serde_json::from_str::<Value>(&repaired) {
                    Ok(v) => v,
                    Err(_) => {
                        return ParseResult::invalid(format!("Ungültiges JSON: {}", exc), text);
                    }
                }
            } else {
                return ParseResult::invalid(format!("Ungültiges JSON: {}", exc), text);
            }
        }
    };

    let obj = match doc.as_object() {
        Some(o) => o,
        None => {
            return ParseResult::invalid("Wurzel muss ein JSON-Objekt sein.", text);
        }
    };

    if obj.get("protocol").and_then(|v| v.as_str()) != Some(PROTOCOL_VERSION) {
        return ParseResult::invalid(
            format!("protocol muss \"{}\" sein.", PROTOCOL_VERSION),
            text,
        );
    }

    let raw_actions = match obj.get("actions").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => {
            return ParseResult::invalid("actions muss eine nicht-leere Liste sein.", text);
        }
    };

    let mut actions = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for item in raw_actions {
        match action_from_value(item) {
            Ok(action) => {
                if !seen_ids.insert(action.id.clone()) {
                    return ParseResult::invalid(
                        format!("doppelte Action-id: {}", action.id),
                        text,
                    );
                }
                actions.push(action);
            }
            Err(e) => return ParseResult::invalid(e, text),
        }
    }

    // Validierung: finish muss alleine sein
    let finish_count = actions
        .iter()
        .filter(|a| a.action_type == ActionType::Finish)
        .count();
    if finish_count > 0 && actions.len() != 1 {
        return ParseResult::invalid("finish muss die einzige Action der Antwort sein", text);
    }

    // Validierung: message muss alleine sein
    let message_count = actions
        .iter()
        .filter(|a| a.action_type == ActionType::Message)
        .count();
    if message_count > 0 && actions.len() != 1 {
        return ParseResult::invalid(
            "message muss nach allen Werkzeugbeobachtungen als einzige Action in einer eigenen Antwort stehen",
            text,
        );
    }

    ParseResult::valid(actions, text)
}

fn looks_like_capacity_notice(text: &str) -> bool {
    static CAPACITY_RE: OnceLock<Regex> = OnceLock::new();
    CAPACITY_RE
        .get_or_init(|| {
            Regex::new(
                r"(?i)(Höchstgrenze|Kapazität|capacity|rate limit|zu viele|erneut versuchen)",
            )
            .expect("capacity regex")
        })
        .is_match(text)
}

#[cfg(test)]
mod edit_batch_tests {
    use super::*;

    #[test]
    fn raw_edit_batch_parst_mehrere_dateien_und_hunks() {
        let response = "WEBAGENT/1 EDIT_BATCH\nid: refactor-1\n---EDIT---\npath: src/a.rs\n---OLD---\nalt a\n---NEW---\nneu a\n---END EDIT---\n---EDIT---\npath: src/b.rs\n---OLD---\nalt b\n---NEW---\nneu b\n---END EDIT---\n---END BATCH---";
        let parsed = parse(response);
        assert!(parsed.valid, "{}", parsed.error);
        assert_eq!(parsed.actions[0].action_type, ActionType::EditBatch);
        assert_eq!(parsed.actions[0].edits.len(), 2);
        assert_eq!(parsed.actions[0].edits[1].path, "src/b.rs");
    }

    #[test]
    fn json_edit_batch_parst_strikt() {
        let response = r#"{"protocol":"webagent/1","actions":[{"id":"batch-1","type":"edit_batch","edits":[{"path":"a.rs","old_string":"a","new_string":"b"}]}]}"#;
        let parsed = parse(response);
        assert!(parsed.valid, "{}", parsed.error);
        assert_eq!(parsed.actions[0].edits.len(), 1);
        let unknown = r#"{"protocol":"webagent/1","actions":[{"id":"batch-2","type":"edit_batch","edits":[{"path":"a.rs","old_string":"a","new_string":"b","surprise":true}]}]}"#;
        assert!(!parse(unknown).valid);
    }

    #[test]
    fn raw_edit_batch_verwirft_text_zwischen_bloecken() {
        let response = "WEBAGENT/1 EDIT_BATCH\nid: bad-1\nHier ist mein Edit\n---EDIT---\npath: a.rs\n---OLD---\na\n---NEW---\nb\n---END EDIT---\n---END BATCH---";
        assert!(!parse(response).valid);
    }

    #[test]
    fn eingebettete_actions_in_prosa_werden_nicht_ausgefuehrt() {
        let raw = "Hier ein Beispiel:\nWEBAGENT/1 EDIT\nid: example-1\npath: a.rs\n---OLD---\na\n---NEW---\nb\n---END EDIT---";
        assert!(!parse(raw).valid);
        let json = r#"Ich würde so antworten: {"protocol":"webagent/1","actions":[{"id":"x","type":"finish"}]}"#;
        assert!(!parse(json).valid);
    }
}
