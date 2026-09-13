@echo off
set WEBAGENT_VERIFY_TRACE=1
cd /d C:\Users\storax\projects\GitHub\webagent-rs
if not defined CARGO_HOME set "CARGO_HOME=%USERPROFILE%\.cargo"
set "WV2="
for /d %%R in ("%CARGO_HOME%\registry\src\*.crates.io-*") do for /d %%V in ("%%R\webview2-com-sys-*") do if exist "%%V\x64\WebView2Loader.dll" set "WV2=%%V\x64\WebView2Loader.dll"
if defined WV2 if not exist "target\debug\WebView2Loader.dll" copy /y "%WV2%" "target\debug\WebView2Loader.dll" >nul 2>&1
if not defined WV2 echo [loaderguard] WebView2Loader.dll im Cargo-Cache nicht gefunden (webview2-com-sys fehlt?) >&2
target\debug\webagent.exe verify --brain chatgpt --brain claude >> "%TEMP%\opencode\verify-occl.log" 2>&1
echo RUN_OCCL_DONE >> "%TEMP%\opencode\verify-occl.log"
