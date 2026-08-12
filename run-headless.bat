@echo off
set WEBAGENT_VERIFY_TRACE=1
cd /d C:\Users\storax\Desktop\desktop.archiv\webagent\webagent-rs
target\debug\webagent.exe verify --brain chatgpt --brain claude --headless >> "%TEMP%\opencode\verify-headless.log" 2>&1
echo RUN_HEADLESS_DONE >> "%TEMP%\opencode\verify-headless.log"
