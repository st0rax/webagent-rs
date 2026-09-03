$ErrorActionPreference='Stop'
$base='http://127.0.0.1:8788'
$dir='C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-301'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$sess = Invoke-RestMethod -Method Post -Uri "$base/api/sessions" -ContentType 'application/json' -Body '{"brain":"claude","task":"T-301 live retest after dedupe"}'
$id = $sess.run_id
$sess | ConvertTo-Json -Depth 6 | Set-Content "$dir\session_create.json" -Encoding utf8
"session=$id" | Set-Content "$dir\run.log" -Encoding utf8
$sw=[Diagnostics.Stopwatch]::StartNew()
try {
  $chat = Invoke-RestMethod -Method Post -Uri "$base/api/sessions/$id/chat" -ContentType 'application/json' -Body '{"text":"Antworte mit genau einem Wort: PING"}' -TimeoutSec 240
  $chat | ConvertTo-Json -Depth 8 | Set-Content "$dir\chat_response.json" -Encoding utf8
  Add-Content "$dir\run.log" 'chat_ok'
} catch {
  $_ | Out-String | Set-Content "$dir\chat_error.txt" -Encoding utf8
  Add-Content "$dir\run.log" "chat_err: $_"
}
$sw.Stop()
Add-Content "$dir\run.log" ("chat_ms=" + $sw.ElapsedMilliseconds)
$after = Invoke-RestMethod -Uri "$base/api/sessions/$id/events"
$after | ConvertTo-Json -Depth 12 | Set-Content "$dir\events_after.json" -Encoding utf8
python "$dir\analyze.py"
Add-Content "$dir\run.log" '---done---'
Get-Content "$dir\run.log"
Get-Content "$dir\analysis.txt" -ErrorAction SilentlyContinue
