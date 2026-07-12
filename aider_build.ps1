<#
  aider_build.ps1 — fährt Aider modulweise durch den Rust-Port des WebAgent.

  Voraussetzung: webagent-rs/.env enthält ANTHROPIC_API_KEY, Anthropic-Konto
  hat Guthaben. Modell/Optionen kommen aus .aider.conf.yml.

  Nutzung:
    ./aider_build.ps1                 # alle Module in Reihenfolge
    ./aider_build.ps1 -Only protocol  # nur ein Modul
    ./aider_build.ps1 -From run_store # ab diesem Modul weiter
#>
[CmdletBinding()]
param(
    [string]$Only = "",
    [string]$From = ""
)

$ErrorActionPreference = "Stop"
$env:Path = "C:\Users\storax\.local\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root
$log = Join-Path $root "aider_build.log"
"=== aider_build gestartet $(Get-Date -Format o) ===" | Out-File $log -Append -Encoding utf8

# Abhängigkeitsreihenfolge: Blätter zuerst, Controller/CLI zuletzt.
$modules = @(
    @{ name="protocol";  reads=@("../src/webagent/protocol.py","../tests/test_protocol.py");
       msg="Portiere ../src/webagent/protocol.py idiomatisch nach Rust in src/protocol.rs. Vollstaendige Semantik: Action/ParseResult, parse(), is_possibly_truncated(), format_observation(), format_protocol_error(), WEBAGENT/1 SHELL Rohskript-Format, Thought-Process/UI-Control-Prefix-Bereinigung, timeout_seconds endlich und 0<t<=3600. Uebernimm JEDEN Testfall aus ../tests/test_protocol.py als #[cfg(test)] mod tests. Nur src/protocol.rs." },

    @{ name="timeouts";  reads=@("../src/webagent/timeouts.py","../tests/test_timeouts.py");
       msg="Portiere ../src/webagent/timeouts.py nach src/timeouts.rs (resolve_timeout mit Operation-Basis, Brain-Multiplikator, Nachrichtengroesse, Env-Overrides WEBAGENT_TIMEOUT_MULT/_MIN/_MAX). Uebernimm alle Testfaelle aus ../tests/test_timeouts.py. Nur src/timeouts.rs." },

    @{ name="loop_guard"; reads=@("../src/webagent/loop_guard.py","../tests/test_loop_guard.py");
       msg="Portiere ../src/webagent/loop_guard.py nach src/loop_guard.rs (shell_read_fingerprint, loop_guard_message). Uebernimm alle Testfaelle aus ../tests/test_loop_guard.py. Nur src/loop_guard.rs." },

    @{ name="observer";  reads=@("../src/webagent/observer.py","../tests/test_observer.py");
       msg="Portiere die PLATTFORMREINEN Teile von ../src/webagent/observer.py nach src/observer.rs: is_transient_response_text, is_claude_limit_response_text und die zugehoerigen Regexes/Konstanten. Die DOM-abhaengige ResponseObserver.wait_for_response-Logik NICHT portieren (kommt spaeter als Trait am Browser-Rand) — nur als kurzen Doc-Kommentar vermerken. Uebernimm die auf diese Funktionen bezogenen Testfaelle aus ../tests/test_observer.py. Nur src/observer.rs." },

    @{ name="prompts";   reads=@("../src/webagent/prompts.py");
       msg="Portiere ../src/webagent/prompts.py nach src/prompts.rs (AUTONOMOUS_PREFIX-Aufbau, autonomous_task_prompt, resume_continue_prompt, resume_recovery_prompt). Nutze die Protokollversion aus crate::protocol. Fuege einen Test hinzu, der prueft, dass autonomous_task_prompt Task und Memory-Kontext enthaelt. Nur src/prompts.rs." },

    @{ name="config";    reads=@("../src/webagent/config.py");
       msg="Portiere ../src/webagent/config.py nach src/config.rs: Pfad-Konstanten/Funktionen (ROOT/DATA/PROFILES/RUNS/LOGS/SELECTORS/RUNTIME), BRAINS-Tabelle (url/selectors/profile), consensus_workspace(), bot2bot_root(), use_shared_browser(), persist_browser_tabs(), ensure_data_dirs(), load_selectors(), und die numerischen Budgets (MAX_OBSERVATION_CHARS usw.). Pfade plattformneutral via std::path::PathBuf. Fuege einen Test fuer use_shared_browser()-Env-Parsing hinzu. Nur src/config.rs." },

    @{ name="run_store"; reads=@("../src/webagent/persistence/run_store.py","../tests/test_run_store.py");
       msg="Portiere ../src/webagent/persistence/run_store.py nach src/run_store.rs: RunMeta (serde, Feldnamen exakt wie im Python-JSON), RunStore mit create/load/save/list_runs, Status-Uebergangsvalidierung (ALLOWED_STATUS_TRANSITIONS), Event-Log (events.jsonl), reconcile_stale_runs mit PID-Liveness via crate::pid_alive und Alter-Fallback. Zeitstempel via crate::now_rfc3339 / crate::now_run_stamp. Uebernimm alle Testfaelle aus ../tests/test_run_store.py. Nur src/run_store.rs." },

    @{ name="transcript"; reads=@("../src/webagent/persistence/transcript.py","../tests/test_transcript_compact.py");
       msg="Portiere ../src/webagent/persistence/transcript.py nach src/transcript.rs: append/read_all (JSON-Lines), _format_entry_line, recovery_tail, compact_summary. Nutzt crate::run_store::RunMeta fuer den Pfad, zeichen-sicheres Slicing via crate::char_prefix/char_suffix, Zeit via crate::now_rfc3339. Uebernimm die Testfaelle aus ../tests/test_transcript_compact.py. Nur src/transcript.rs." },

    @{ name="memory";    reads=@("../src/webagent/memory.py","../tests/test_memory.py");
       msg="Portiere ../src/webagent/memory.py nach src/memory.rs. WICHTIG: KEIN SQLite/rusqlite (keine C-Toolchain) — implementiere den Store als JSON-Lines-Datei mit derselben oeffentlichen API (search mit Token-Overlap-Ranking, record_run, scopes). Verhalten und Ranking wie im Original. Uebernimm die Testfaelle aus ../tests/test_memory.py sinngemaess (auf die JSON-Backed-Variante angepasst). Nur src/memory.rs." },

    @{ name="doctor";    reads=@("../src/webagent/doctor.py","../tests/test_doctor.py");
       msg="Portiere ../src/webagent/doctor.py nach src/doctor.rs: BrainCheck, DoctorReport, _find_lock_files, _find_last_done_run, _find_recent_run_meta, _infer_login_state (ready/stale/login_required/likely_ready/unknown), _build_recovery_hint, check_brain, run_doctor. Nutzt crate::config und crate::run_store. Uebernimm alle Testfaelle aus ../tests/test_doctor.py. Nur src/doctor.rs." },

    @{ name="executor";  reads=@("../src/webagent/executor/base.py","../src/webagent/executor/powershell.py","../tests/test_executor.py");
       msg="Portiere den ShellExecutor nach src/executor.rs: ein Trait ShellExecutor (start/stop/run(timeout)->ShellResult/send_interrupt) plus eine plattformgewaehlte Default-Impl — Windows: pwsh/powershell, Unix: sh/bash, via #[cfg(...)]. ShellResult wie in base.py (stdout/stderr/exit_code/interrupted). Uebernimm die sinnvoll portierbaren Testfaelle aus ../tests/test_executor.py (ohne echte Langlaeufer). Nur src/executor.rs." },

    @{ name="brain";     reads=@("../src/webagent/brains/base.py");
       msg="Portiere ../src/webagent/brains/base.py nach src/brain.rs: SessionState-Enum, BrainResponse-Struct und ein Trait BrainBackend mit denselben Methoden (start/stop/ensure_ready/session_state/new_chat/send/wait_response/is_logged_in/click_login/wait_for_login/get_conversation_ref/restore_conversation). KEINE konkrete Browser-Impl — nur die Trait-Grenze. Fuege einen Kompilier-Test mit einem Dummy-Backend hinzu. Nur src/brain.rs." },

    @{ name="controller"; reads=@("../src/webagent/controller_new.py","../tests/test_controller.py","../tests/test_resume.py");
       msg="Portiere ../src/webagent/controller_new.py nach src/controller.rs: AgentController mit run_once, _handle_response, _execute_actions_serial, Loop-Guard-Integration, Observation-Budget (_bounded_observation via crate::char_prefix/suffix), brain_incomplete-Retries, Resume-Logik, und die Haupt-run()-Zustandsmaschine. Generisch ueber die Traits crate::brain::BrainBackend und crate::executor::ShellExecutor. Nutzt crate::run_store, crate::transcript, crate::protocol, crate::prompts, crate::loop_guard, crate::config. Uebernimm die mit Mock-Brain/Executor testbaren Faelle aus ../tests/test_controller.py und ../tests/test_resume.py. Nur src/controller.rs." },

    @{ name="main";      reads=@("../src/webagent/cli.py");
       msg="Erstelle src/main.rs: eine clap-basierte CLI mit den Kern-Befehlen aus ../src/webagent/cli.py — run (--brain --task --resume --headless --max-cycles), doctor (--json), maintenance-check (--json). Verdrahte doctor mit crate::doctor::run_doctor und gib bei --json serde_json aus. run/repl duerfen vorerst einen klaren 'noch nicht verdrahtet'-Hinweis ausgeben, wenn die Browser-Backends fehlen. Nur src/main.rs." }
)

function Invoke-AiderModule($m) {
    $readArgs = @()
    foreach ($r in $m.reads) { if (Test-Path $r) { $readArgs += @("--read", $r) } }
    $file = "src/$($m.name).rs"
    if ($m.name -eq "main") { $file = "src/main.rs" }

    "--- [$($m.name)] aider start $(Get-Date -Format o) ---" | Tee-Object -FilePath $log -Append
    & aider --message $m.msg @readArgs --file $file --no-check-update --no-show-model-warnings 2>&1 |
        Tee-Object -FilePath $log -Append

    # Build-Gate + ein Reparaturdurchgang.
    $build = & cargo build 2>&1
    $build | Out-File $log -Append -Encoding utf8
    if ($LASTEXITCODE -ne 0) {
        "--- [$($m.name)] cargo build FEHLER — Reparaturdurchgang ---" | Tee-Object -FilePath $log -Append
        $errText = ($build | Out-String)
        if ($errText.Length -gt 6000) { $errText = $errText.Substring(0, 6000) }
        $fix = "cargo build meldet folgende Fehler. Behebe sie ausschliesslich in $file (keine neuen Dependencies, nutze die Helfer aus lib.rs):`n$errText"
        & aider --message $fix --file $file --no-check-update --no-show-model-warnings 2>&1 |
            Tee-Object -FilePath $log -Append
        & cargo build 2>&1 | Out-File $log -Append -Encoding utf8
        if ($LASTEXITCODE -ne 0) {
            "!!! [$($m.name)] baut nach Reparatur weiter NICHT — manuell pruefen." | Tee-Object -FilePath $log -Append
        }
    }
    "--- [$($m.name)] fertig (build exit=$LASTEXITCODE) ---" | Tee-Object -FilePath $log -Append
}

# Selektion
$selected = $modules
if ($Only) { $selected = $modules | Where-Object { $_.name -eq $Only } }
elseif ($From) {
    $idx = ($modules.name).IndexOf($From)
    if ($idx -ge 0) { $selected = $modules[$idx..($modules.Count-1)] }
}

foreach ($m in $selected) { Invoke-AiderModule $m }

"=== Abschluss: cargo test ===" | Tee-Object -FilePath $log -Append
& cargo test 2>&1 | Tee-Object -FilePath $log -Append
"=== aider_build beendet $(Get-Date -Format o) (test exit=$LASTEXITCODE) ===" | Tee-Object -FilePath $log -Append
