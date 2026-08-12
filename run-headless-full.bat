@echo off
set WEBAGENT_VERIFY_TRACE=1
cd /d C:\Users\storax\Desktop\desktop.archiv\webagent\webagent-rs
target\debug\webagent.exe verify --headless >> "%TEMP%\opencode\verify-headless-full.log" 2>&1
echo RUN_FULL_DONE >> "%TEMP%\opencode\verify-headless-full.log"
