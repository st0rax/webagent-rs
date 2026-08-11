@echo off
set WEBAGENT_VERIFY_TRACE=1
cd /d C:\Users\storax\Desktop\desktop.archiv\webagent\webagent-rs
target\debug\webagent.exe verify --brain chatgpt --brain claude --brain deepseek --brain gemini --brain kimi --brain mistral --brain qwen --brain zai >> "%TEMP%\opencode\verify-final.log" 2>&1
echo RUN_FINAL_DONE >> "%TEMP%\opencode\verify-final.log"
