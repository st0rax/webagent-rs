//! Bettet den Git-Commit (kurz) + Dirty-Flag als WEBAGENT_GIT_HASH ins Binary ein,
//! damit `webagent --version` zeigt, aus welchem Stand ein deploytes Binary gebaut
//! wurde (Abgleich deployte Kopie vs. HEAD, siehe delivery/post_deploy_check.ps1).
//! Bewusst nur std — kein zusätzliches Build-Dependency.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let hash = git(&["rev-parse", "--short=9", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let stamp = if dirty { format!("{hash}+dirty") } else { hash };
    println!("cargo:rustc-env=WEBAGENT_GIT_HASH={stamp}");
    // Neu bauen, wenn sich der Commit-Stand ändert (nicht bei jedem Build).
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    // `cfg(windows)` beschreibt hier den Host des Build-Skripts. Für einen
    // Windows-GNU-Crossbuild muss die Ressource anhand des Zieltripletts
    // aktiviert werden, sonst bleibt die EXE ohne Versionsinformationen.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        let mut resource = winres::WindowsResource::new();
        resource.set("FileDescription", "WebAgent");
        resource.set("ProductName", "WebAgent");
        resource.set("CompanyName", "st0rax");
        resource.set("LegalCopyright", "MIT License");
        resource.set("OriginalFilename", "webagent.exe");
        resource.set("InternalName", "webagent");
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
        resource
            .compile()
            .expect("Windows version resource compilation failed");
        // Bei mingw-gcc kann die von winres erzeugte statische Resource-Bibliothek
        // mangels referenzierter Symbole aus dem Link entfernt werden. Das direkte
        // Linken des erzeugten COFF-Objekts stellt sicher, dass VERSIONINFO auch in
        // einer Release-EXE vorhanden ist.
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR missing");
        println!(
            "cargo:rustc-link-arg={}",
            std::path::Path::new(&out_dir).join("resource.o").display()
        );
    }
}
