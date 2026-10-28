use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let hash = git(&["rev-parse", "--short=9", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty()).unwrap_or(false);
    let stamp = if dirty { format!("{hash}+dirty") } else { hash };
    println!("cargo:rustc-env=WEBAGENT_GIT_HASH={stamp}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    // G-003: statische All-in-One-Verteilung (eingebettete DLLs, keine externen .res-Dateien)
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        let mut resource = winres::WindowsResource::new();
        resource.set("FileDescription", "WebAgent (static all-in-one)");
        resource.set("ProductName", "WebAgent");
        resource.set("CompanyName", "st0rax");
        resource.set("LegalCopyright", "MIT License");
        resource.set("OriginalFilename", "webagent.exe");
        resource.set("InternalName", "webagent");
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
        resource.compile().expect("Windows version resource compilation failed");
        // G-003: statisch verlinken (kein externes .res, kein Kopieren von DLLs nötig)
        let target_lower = target.to_ascii_lowercase();
        if target_lower.contains("gnu") && !target_lower.contains("android") {
            let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR missing");
            println!("cargo:rustc-link-arg={}", std::path::Path::new(&out_dir).join("resource.o").display());
        }
    }
}
