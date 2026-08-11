@echo off
set WEBAGENT_VERIFY_TRACE=1
cd /d C:\Users\storax\Desktop\desktop.archiv\webagent\webagent-rs
target\debug\webagent.exe verify >> "%TEMP%\opencode\verify-sauber.log" 2>&1
echo RUN_CLEAN_DONE >> "%TEMP%\opencode\verify-sauber.log"
