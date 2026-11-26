//! Bettet den Git-Commit (kurz) + Dirty-Flag als WEBAGENT_GIT_HASH ins Binary ein,
//! damit `webagent --version` zeigt, aus welchem Stand ein deploytes Binary gebaut
//! wurde (Abgleich deployte Kopie vs. HEAD, siehe delivery/post_deploy_check.ps1).
//! Bewusst nur std — kein zusätzliches Build-Dependency.

use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Kopiert WebView2Loader.dll neben webagent.exe, damit der Default-Build
/// lokal startbar ist. `webview2-com-sys` legt die DLL nur unter
/// `target/<profile>/build/webview2-com-sys[-<hash>]/out/x64` ab; ohne Kopie
/// endet der Start vor `main` mit 0xC0000135 (DLL not found), nicht mit einer
/// Access Violation. Das Skript `scripts/copy-webview2-loader.ps1` bleibt als
/// manueller Fallback und fuer den CI-Release-Schritt erhalten.
fn copy_webview2_loader(target: &str) {
    if !target.contains("windows") || std::env::var_os("CARGO_FEATURE_WEBVIEW").is_none() {
        return;
    }
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    // Cargo 1.100 legt OUT_DIR unter target/<profile>/build/webagent/<hash>/out
    // ab,aelter Layouts unter target/<profile>/build/webagent-<hash>/out. Statt
    // eine feste Anzahl Eltern zu zaehlen, suchen wir das Profilverzeichnis,
    // das `build/webview2-com-sys[-<hash>]` enthaelt.
    let has_sys_dir = |dir: &Path| {
        let build = dir.join("build");
        build.join("webview2-com-sys").is_dir()
            || std::fs::read_dir(&build).is_ok_and(|entries| {
                entries.flatten().any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("webview2-com-sys-")
                })
            })
    };
    let Some(profile_dir) = Path::new(&out_dir).ancestors().find(|dir| has_sys_dir(dir)) else {
        return;
    };
    let build_dir = profile_dir.join("build");
    let Ok(entries) = std::fs::read_dir(&build_dir) else {
        println!(
            "cargo:warning=WebView2Loader.dll: build directory {} missing; run scripts/copy-webview2-loader.ps1 for local smoke",
            build_dir.display()
        );
        return;
    };

    // Cargo schreibt OUT_DIR inzwischen unter build/webview2-com-sys/<hash>/out,
    // frueher unter build/webview2-com-sys-<hash>/out. Beide Layouts zaehlen.
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("webview2-com-sys-") {
            candidates.push(entry.path());
        } else if name == "webview2-com-sys" {
            if let Ok(inner) = std::fs::read_dir(entry.path()) {
                candidates.extend(
                    inner
                        .flatten()
                        .filter(|child| child.file_type().is_ok_and(|kind| kind.is_dir()))
                        .map(|child| child.path()),
                );
            }
        }
    }

    let source = candidates
        .into_iter()
        .map(|dir| dir.join("out").join("x64").join("WebView2Loader.dll"))
        .find(|path| path.is_file());
    let Some(source) = source else {
        println!(
            "cargo:warning=WebView2Loader.dll not found under {}; run scripts/copy-webview2-loader.ps1 for local smoke",
            build_dir.display()
        );
        return;
    };
    let destination = profile_dir.join("WebView2Loader.dll");
    if let Err(error) = std::fs::copy(&source, &destination) {
        println!(
            "cargo:warning=could not copy WebView2Loader.dll to {}: {error}",
            destination.display()
        );
    }
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
        // winres verknüpft das Ergebnis je Toolchain unterschiedlich: unter MSVC
        // erzeugt es resource.lib und meldet es selbst per cargo:rustc-link-lib;
        // unter GNU/mingw erzeugt es resource.o + libresource.a, wobei die
        // statische Bibliothek mangels referenzierter Symbole aus dem Link
        // entfernt werden kann. Nur im GNU-Fall wird das COFF-Objekt deshalb
        // zusätzlich direkt gelinkt, damit VERSIONINFO in der Release-EXE bleibt.
        let target_lower = target.to_ascii_lowercase();
        if target_lower.contains("gnu") && !target_lower.contains("android") {
            let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR missing");
            println!(
                "cargo:rustc-link-arg={}",
                std::path::Path::new(&out_dir).join("resource.o").display()
            );
        }
    }
    copy_webview2_loader(&target);
}
