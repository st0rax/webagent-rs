import sys
p = 'C:/Users/storax/projects/GitHub/webagent-rs/src/browser_inference.rs'
src = open(p, encoding='utf-8').read()
if 'parse_envelope_lenient' in src:
 print('already patched')
 sys.exit(0)
NL = chr(10)
lines = [
 'fn repair_tool_envelope_json(payload: &str) -> String {',
 ' let bs = char::from(92);',
 ' let quote = char::from(34);',
 ' let mut out = String::with_capacity(payload.len() + 16);',
 ' let mut in_string = false;',
 ' let mut escaped = false;',
 ' let mut chars = payload.chars().peekable();',
 ' while let Some(ch) = chars.next() {',
 ' if !in_string {',
 ' if ch == quote { in_string = true; }',
 ' out.push(ch);',
 ' continue;',
 ' }',
 ' if escaped {',
 ' out.push(ch);',
 ' escaped = false;',
 ' continue;',
 ' }',
 ' if ch == bs {',
 ' let nxt = chars.peek().copied();',
 ' let valid = matches!(nxt, Some(c) if c == quote || c == bs || c == char::from(47) || c == char::from(98) || c == char::from(102) || c == char::from(110) || c == char::from(114) || c == char::from(116) || c == char::from(117));',
 ' if valid { out.push(bs); escaped = true; } else { out.push(bs); out.push(bs); }',
 ' continue;',
 ' }',
 ' if ch == quote { in_string = false; }',
 ' out.push(ch);',
 ' }',
 ' out',
 '}',
 '',
 'fn parse_envelope_lenient(payload: &str) -> Result<ToolEnvelope, serde_json::Error> {',
 ' match serde_json::from_str(payload) {',
 ' Ok(value) => Ok(value),',
 ' Err(first_error) => {',
 ' let repaired = repair_tool_envelope_json(payload);',
 ' match serde_json::from_str(&repaired) {',
 ' Ok(value) => Ok(value),',
 ' Err(_) => Err(first_error),',
 ' }',
 ' }',
 ' }',
 '}',
 '',
 '',
]
block = NL.join(lines)
anchor = NL + 'fn parse_response('
if anchor not in src:
 print('anchor not found')
 sys.exit(2)
src = src.replace(anchor, NL + block + 'fn parse_response(', 1)
old_call = 'serde_json::from_str(payload.trim())'
if old_call not in src:
 print('call not found')
 sys.exit(3)
src = src.replace(old_call, 'parse_envelope_lenient(payload.trim())', 1)
open(p, 'w', encoding='utf-8').write(src)
print('patched ok')
