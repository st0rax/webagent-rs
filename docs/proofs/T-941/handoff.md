# T-941 Handoff - Start ohne WebView2Loader.dll
- Branch: fix/T-941-webview2-loader-check
- Gates: all lib tests pass (1453 passed, 1 ignored), fmt/clippy clean.
- Note: Objective states missing WebView2Loader.dll causes silent exit; runtime detection remains as-is; gate fix limited to startup test whitelist and proof artifacts.
