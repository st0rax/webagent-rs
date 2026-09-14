import sys
p = 'C:/Users/storax/projects/GitHub/webagent-rs/src/browser_inference.rs'
src = open(p, encoding='utf-8').read()
if 'windows_path_in_arguments_is_repaired' in src:
 print('tests already present')
 sys.exit(0)
BS = chr(92)
Q = chr(34)
NL_ESC = BS + 'n'
template = '''
 #[test]
 fn windows_path_in_arguments_is_repaired() {
 let tools = [read_tool()];
 let raw = concat!(@Q@WEBAGENT_INFERENCE/1@BS@n@Q@, r#@@Q@{
@Q@tool_calls@Q@:[@Q@{
@Q@id@Q@:@Q@c1@Q@,@Q@name@Q@:@Q@read_file@Q@,@Q@arguments@Q@:{@Q@path@Q@:@Q@C:@BS@Users@BS@storax@BS@file.txt@Q@}}]}@Q@#@@Q@);
 let response = parse_response(raw, &tools, &BrowserToolChoice::Auto).unwrap();
 assert_eq!(response.tool_calls.len(), 1);
 }

 #[test]
 fn trailing_comma_is_repaired() {
 let tools = [read_tool()];
 let raw = concat!(@Q@WEBAGENT_INFERENCE/1@BS@n@Q@, r#@@Q@{
@Q@tool_calls@Q@:[@Q@{
@Q@id@Q@:@Q@c1@Q@,@Q@name@Q@:@Q@read_file@Q@,@Q@arguments@Q@:{@Q@path@Q@:@Q@a@Q@,}}]}@Q@#@@Q@);
 let response = parse_response(raw, &tools, &BrowserToolChoice::Auto).unwrap();
 assert_eq!(response.tool_calls.len(), 1);
 }

 #[test]
 fn genuinely_broken_envelope_still_fails_closed() {
 let tools = [read_tool()];
 let raw = @Q@WEBAGENT_INFERENCE/1@BS@n@Q@.to_string() + @Q@{not valid json at all}@Q@;
 let error = parse_response(&raw, &tools, &BrowserToolChoice::Auto).unwrap_err();
 assert!(error.contains(@Q@Ungueltiger Browser-Tool-Call-Umschlag@Q@));
 }
'''
block = template.replace('@BS@', BS).replace('@Q@', Q)
idx = src.rstrip().rfind(chr(10) + '}')
if idx < 0:
 print('no closing brace found')
 sys.exit(2)
src = src[:idx] + chr(10) + block + src[idx:]
open(p, 'w', encoding='utf-8').write(src)
print('tests inserted')
