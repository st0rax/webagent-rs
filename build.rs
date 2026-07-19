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
    let dirty = git(&["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    let stamp = if dirty { format!("{hash}+dirty") } else { hash };
    println!("cargo:rustc-env=WEBAGENT_GIT_HASH={stamp}");
    // Neu bauen, wenn sich der Commit-Stand ändert (nicht bei jedem Build).
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}
