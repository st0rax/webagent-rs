@echo off
set WEBAGENT_VERIFY_TRACE=1
cd /d C:\Users\storax\projects\GitHub\webagent-rs
if not exist "target\debug\WebView2Loader.dll" copy /y "C:\Users\storax\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\webview2-com-sys-0.33.0\x64\WebView2Loader.dll" "target\debug\WebView2Loader.dll" >nul 2>&1
target\debug\webagent.exe verify --brain chatgpt --brain claude --brain deepseek --brain gemini --brain kimi --brain mistral --brain qwen --brain zai >> "%TEMP%\opencode\verify-final.log" 2>&1
echo RUN_FINAL_DONE >> "%TEMP%\opencode\verify-final.log"
