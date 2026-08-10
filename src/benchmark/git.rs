//! Git- und Eval-Helfer der Messstrecke: Arbeitsbaum-Zustand, Patch-Sicherung
//! und -Analyse, Resets sowie der Timeout der Eval-Kommandos.

use std::path::Path;

/// Timeout je Eval-Kommando (`cargo build`/`cargo test`) in Sekunden.
const EVAL_TIMEOUT_SECS: u64 = 300;

/// `true`, wenn der Working Tree Änderungen enthält (inkl. neu angelegter,
/// untracked Dateien — `git diff --quiet` allein übersähe die von write-Actions
/// erzeugten neuen Dateien; deshalb `git status --porcelain`).
pub(crate) fn tree_changed(workdir: &Path) -> bool {
    // Ein fehlgeschlagenes `git status` darf NICHT als „nichts geaendert"
    // durchgehen.
    //
    // Vorher stand hier `Err(_) => false`. Damit loescht ein einziger
    // transienter Git-Fehler — etwa eine `index.lock`-Kollision mit den
    // parallel laufenden Eval-Kommandos — die gesamte Arbeit eines Brains aus
    // der Messung. Und weil `is_pass` zwingend `did_change` verlangt, kann in
    // diesem Fall nie eine Ernte entstehen.
    //
    // Real beobachtet am 30.07.2026, Lauf 20260730_024130_167ab580 (deepseek):
    // der Executor meldete „edit ok: src/protocol.rs — Ersetzung ab Zeile 971.
    // Datei jetzt 1767 Zeilen", HEAD hatte 1567 — die Aenderung war also echt.
    // Gewertet wurde „did_change=nein build=x test=x -> SKIP (keine
    // Aenderung)".
    //
    // Also: einmal wiederholen (Lock-Kollisionen sind kurz), und wenn es dann
    // immer noch nicht geht, laut sagen statt still falsch zu messen. Im
    // Zweifel `true`: eine faelschlich gemeldete Aenderung faellt sofort im
    // Build- oder Test-Tor auf, eine faelschlich verschwiegene ist unsichtbar.
    for versuch in 1..=2 {
        match std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(workdir)
            .output()
        {
            Ok(out) if out.status.success() => {
                return !String::from_utf8_lossy(&out.stdout).trim().is_empty();
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if versuch == 2 {
                    crate::bench_events::eprint_line(&format!(
                        "[benchmark] WARNUNG: `git status` in {} scheiterte ({err}) — \
                         Aenderung wird vorsichtshalber als vorhanden gewertet",
                        workdir.display()
                    ));
                    return true;
                }
            }
            Err(e) => {
                if versuch == 2 {
                    crate::bench_events::eprint_line(&format!(
                        "[benchmark] WARNUNG: `git` nicht ausfuehrbar in {} ({e}) — \
                         Aenderung wird vorsichtshalber als vorhanden gewertet",
                        workdir.display()
                    ));
                    return true;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    true
}

/// Führt ein Eval-Kommando aus und wertet nur den Exit-Code (0 = ok) — nutzt den
/// Kommando-Runner mit Timeout aus autoresearch.
/// Führt ein Eval-Kommando aus: `(exit==0, Ausgabe)`. Die Ausgabe geht als
/// Kontext in den Repair-Versuch (Compiler-/Testfehler zurück ans Brain).
pub(crate) fn run_eval_detail(cmd: &str, workdir: &Path) -> (bool, String) {
    match crate::autoresearch::run_eval_with_timeout(cmd, workdir, EVAL_TIMEOUT_SECS) {
        Ok((code, out)) => (code == Some(0), out),
        Err(e) => (false, e),
    }
}

/// Baut den Folgeauftrag nach einem fehlgeschlagenen Build/Test: die
/// Original-Aufgabe plus die echte Fehlerausgabe als Kontext.
///
/// Ohne diesen Schritt zählte ein Brain als Fehlschlag, obwohl es oft nur einen
/// Tippfehler von grün entfernt war — echte Coding-Agenten lesen den
/// Compilerfehler und korrigieren (Storax-Vorschlag 2026-07-21).
/// Anstoss, wenn ein Brain „fertig" meldet, ohne eine Datei geändert zu haben.
///
/// Das trifft die Explore-and-give-up-Fälle: claude las die Zieldatei und
/// verweigerte, kimi meldete per message-Action „Implementierung erfolgreich" —
/// beide ohne einen einzigen Edit. Der Anstoss macht unmissverständlich, dass
/// ein Edit die Aufgabe IST, nicht eine Meldung darüber.
pub fn build_no_change_prompt(task: &str) -> String {
    format!(
        "{task}\n\n--- KEIN EDIT ERKANNT ---\n\
         Du hast KEINE Datei geändert. Eine Nachricht oder Zusammenfassung zählt \
         NICHT — die Aufgabe ist erst gelöst, wenn die Datei tatsächlich \
         geändert ist. Gib JETZT genau eine Änderung im Rohformat aus \
         (WEBAGENT/1 EDIT oder WEBAGENT/1 WRITE) und sonst nichts. Behaupte \
         keinen Erfolg, ohne editiert zu haben.",
        task = task.trim()
    )
}

pub fn build_repair_prompt(task: &str, stage: &str, output: &str) -> String {
    format!(
        "{task}\n\n--- KORREKTUR NÖTIG ---\n\
         Deine bisherige Änderung ist im Arbeitsverzeichnis vorhanden, aber \
         `{stage}` schlägt fehl. Lies die Fehlerausgabe, finde die Ursache und \
         korrigiere sie mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). Fange NICHT \
         von vorne an — repariere das Vorhandene.\n\n\
         Fehlerausgabe (gekürzt):\n{out}",
        task = task.trim(),
        stage = stage,
        out = crate::char_prefix(output.trim(), 2500)
    )
}

/// Setzt den Working Tree hart auf `baseline` zurück und entfernt untracked
/// Dateien — jeder Brain-Run startet identisch, der Benchmark hinterlässt nichts.
/// Fuehrt ein Git-Kommando aus und macht einen Fehlschlag sichtbar.
///
/// Frueher wurden diese Aufrufe mit `let _ = ...` verworfen. Das ist genau
/// dort gefaehrlich, wo der Aufraeumschritt scheitert: ein misslungenes
/// `git clean` laesst Dateien des vorigen Brains stehen, das naechste Brain
/// startet auf einem verschmutzten Tree und seine Messung ist wertlos —
/// ohne dass irgendwo ein Hinweis auftaucht.
fn git_checked(workdir: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()
        .map_err(|e| format!("git {} nicht startbar: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} fehlgeschlagen (Code {}): {}",
            args.join(" "),
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub(crate) fn reset_repo(workdir: &Path, baseline: &str) -> Result<(), String> {
    // `git add -A` davor, damit auch neu angelegte (untracked) Dateien vom
    // Full-Reset erfasst werden; `git clean -fd` fegt den Rest weg.
    git_checked(workdir, &["add", "-A"])?;
    crate::autoresearch::git_reset_hard(workdir, baseline)?;
    git_checked(workdir, &["clean", "-fd"])?;
    Ok(())
}

/// Sichert die Arbeit des laufenden Brains als Patch, BEVOR `reset_repo` sie
/// verwirft. `git add -A` davor, damit neue Dateien im Diff auftauchen.
pub(crate) fn capture_patch(workdir: &Path) -> Result<String, String> {
    git_checked(workdir, &["add", "-A"])?;
    // `--no-renames`: eine Umbenennung soll als Loeschung PLUS Neuanlage im
    // Patch stehen. Sonst verdichtet git sie zu `rename from/to` ohne
    // `+++ b/`-Zeile — beide betroffenen Pfade waeren fuer die Validierung
    // unsichtbar. Die Erkennung in `validate_harvest_patch` faengt die
    // Rename-Form zusaetzlich ab, falls die Option einmal wegfaellt.
    git_checked(workdir, &["diff", "--cached", "--binary", "--no-renames"])
}

/// Schutzgitter für autonom geernteten Code. Ein Benchmark darf kleine,
/// nachvollziehbare Rust-Änderungen verbessern, aber weder Dependencies noch
/// Build-/CI-Konfiguration umformen. Solche Eingriffe brauchen eine bewusste
/// menschliche Entscheidung und werden deshalb nie automatisch geerntet.
/// Loest die Pfadangabe einer Diff-Kopfzeile auf.
///
/// git zitiert Pfade mit Sonderzeichen (`--- "a/mit leerzeichen.rs"`); ohne
/// Beruecksichtigung rutschte so ein Pfad ungeprueft durch.
fn diff_path(rest: &str, prefix: char) -> Option<String> {
    let rest = rest.trim();
    let unquoted = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(rest);
    if unquoted == "/dev/null" {
        return None;
    }
    let mut want = String::from(prefix);
    want.push('/');
    unquoted.strip_prefix(&want).map(|p| p.to_string())
}

/// Sammelt JEDEN vom Patch beruehrten Pfad — Aenderung, Neuanlage, Loeschung
/// und Umbenennung.
///
/// Frueher wurden nur `+++ b/`-Zeilen gelesen. Eine Loeschung traegt dort aber
/// `/dev/null`, ihr echter Pfad steht ausschliesslich in `--- a/`; eine
/// Umbenennung erscheint als `rename from/to` ganz ohne `+++`-Zeile. Beide
/// waren damit fuer das Schutzgitter unsichtbar: ein Patch durfte eine
/// erlaubte Datei aendern und nebenbei `Cargo.toml` loeschen.
pub(crate) fn patch_touched_paths(patch: &str) -> (Vec<String>, Vec<String>) {
    let mut paths = Vec::new();
    let mut deleted = Vec::new();
    let mut pending_old: Option<String> = None;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            pending_old = diff_path(rest, 'a');
            if let Some(p) = &pending_old {
                paths.push(p.clone());
            }
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            match diff_path(rest, 'b') {
                Some(p) => paths.push(p),
                // `+++ /dev/null` = die Datei aus der `--- a/`-Zeile davor
                // wird geloescht.
                None => {
                    if let Some(p) = pending_old.take() {
                        deleted.push(p);
                    }
                }
            }
            pending_old = None;
        } else if let Some(rest) = line.strip_prefix("rename from ") {
            let p = rest.trim().to_string();
            paths.push(p.clone());
            deleted.push(p);
        } else if let Some(rest) = line.strip_prefix("rename to ") {
            paths.push(rest.trim().to_string());
        }
    }
    paths.sort();
    paths.dedup();
    deleted.sort();
    deleted.dedup();
    (paths, deleted)
}

