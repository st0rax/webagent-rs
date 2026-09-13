from pathlib import Path

src = Path("src/api_bridge.rs").read_text(encoding="utf-8").splitlines(True)
body = "".join(src[2109:2377])
out = []
for line in body.splitlines(True):
    if line.startswith("fn "):
        out.append("pub(super) " + line)
    else:
        out.append(line)
body = "".join(out)
header = '''//! Modellkatalog und Auto-Router der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Keine neuen Modelle, keine Live-Verfuegbarkeit behaupten. HTTP-Routing
//! bleibt in `routing.rs`. Modalitaeten nur laut bestaetigter Smokes.
//! T-921 verdrahtet `mod catalog`.

use super::BridgeConfig;
use serde_json::{json, Value};

'''
Path("src/api_bridge/catalog.rs").write_text(header + body, encoding="utf-8")
print("bytes", Path("src/api_bridge/catalog.rs").stat().st_size)
print("available_brains", "pub fn available_brains" in Path("src/api_bridge/catalog.rs").read_text(encoding="utf-8"))
