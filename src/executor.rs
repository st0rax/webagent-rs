//! executor — Plattformübergreifende Shell-Ausführung (PowerShell/sh).
//!
//! Portiert aus `../src/webagent/executor/powershell.py`. Hält eine persistente
//! Shell-Session pro Run (cd/Variablen/Env überleben zwischen Actions), mit
//! nonce-gebundenem Abschlussmarker und Base64-wrapped Commands auf PowerShell.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Trait für plattformübergreifende Shell-Ausführung.
pub trait ShellExecutor {
    /// Führt einen Shell-Befehl aus und gibt stdout/stderr/exit_code zurück.
    fn execute(&self, command: &str, timeout_seconds: f64) -> ExecutionResult;

    /// Startet die persistente Shell-Session (wie Python `PowerShellExecutor.start`).
    fn start(&self) {}
    fn stop(&self) {}
    fn send_interrupt(&self) {}
}

/// Ergebnis einer Shell-Ausführung.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub error: Option<String>,
}

/// Standard-Implementierung: eine persistente Shell-Session pro Executor-Instanz.
pub struct PlatformShellExecutor {
    session: Mutex<ShellSession>,
}

struct ShellSession {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    output_rx: Receiver<OutputLine>,
    generation: u64,
    start_dir: Option<PathBuf>,
}

#[derive(Debug)]
enum OutputLine {
    Stdout(String),
    Stderr(String),
}

impl PlatformShellExecutor {
    pub fn new() -> Self {
        let (_tx, rx) = mpsc::channel();
        Self {
            session: Mutex::new(ShellSession {
                child: None,
                stdin: None,
                output_rx: rx,
                generation: 0,
                start_dir: None,
            }),
        }
    }

    /// Startet die persistente Shell in einem expliziten Workspace. Damit
    /// arbeiten relative Shell-Pfade im selben Baum wie native Edit/Write-
    /// Actions und nachgelagerte Git-Messungen.
    pub fn new_in(start_dir: impl AsRef<Path>) -> Self {
        let (_tx, rx) = mpsc::channel();
        Self {
            session: Mutex::new(ShellSession {
                child: None,
                stdin: None,
                output_rx: rx,
                generation: 0,
                start_dir: Some(start_dir.as_ref().to_path_buf()),
            }),
        }
    }
}

impl Default for PlatformShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellExecutor for PlatformShellExecutor {
    fn start(&self) {
        let mut session = self.session.lock().expect("executor session lock");
        session.ensure_started();
    }

    fn stop(&self) {
        let mut session = self.session.lock().expect("executor session lock");
        session.stop();
    }

    fn send_interrupt(&self) {
        let mut session = self.session.lock().expect("executor session lock");
        session.send_interrupt();
    }

    fn execute(&self, command: &str, timeout_seconds: f64) -> ExecutionResult {
        let mut session = self.session.lock().expect("executor session lock");
        session.execute(command, timeout_seconds)
    }
}

impl ShellSession {
    fn ensure_started(&mut self) {
        if self.alive() {
            return;
        }
        if let Err(e) = self.launch() {
            crate::bench_events::eprint_line(&format!("executor: shell start failed: {e}"));
        }
    }

    fn alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => c.try_wait().ok().flatten().is_none() && self.stdin.is_some(),
            None => false,
        }
    }

    fn launch(&mut self) -> Result<(), String> {
        self.stop();
        self.generation = self.generation.saturating_add(1);

        let (shell, args) = shell_launch_spec();
        let mut cmd = Command::new(shell);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(start_dir) = &self.start_dir {
            cmd.current_dir(start_dir);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Fehler beim Starten von {shell}: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Shell stdout nicht verfügbar".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Shell stderr nicht verfügbar".to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Shell stdin nicht verfügbar".to_string())?;

        let (tx, rx) = mpsc::channel();
        spawn_line_reader(BufReader::new(stdout), tx.clone(), OutputLine::Stdout);
        spawn_line_reader(BufReader::new(stderr), tx, OutputLine::Stderr);

        self.child = Some(child);
        self.stdin = Some(stdin);
        self.output_rx = rx;
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin = None;
        self.drain_queue();
    }

    fn drain_queue(&self) {
        while self.output_rx.try_recv().is_ok() {}
    }

    fn send_interrupt(&mut self) {
        if !self.alive() {
            return;
        }
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = stdin.write_all(b"\x03");
            let _ = stdin.flush();
        }
        thread::sleep(Duration::from_millis(300));
    }

    fn restart(&mut self) {
        self.send_interrupt();
        self.stop();
        let _ = self.launch();
    }

    fn execute(&mut self, command: &str, timeout_seconds: f64) -> ExecutionResult {
        if !self.alive() {
            self.restart();
        }

        let nonce = new_nonce();
        let marker_re = marker_regex(&nonce);
        let gen_at_start = self.generation;

        self.drain_queue();
        let wrapped = wrap_command(command, &nonce);

        let Some(stdin) = self.stdin.as_mut() else {
            return ExecutionResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                timed_out: false,
                error: Some("Shell nicht startbar.".to_string()),
            };
        };

        if stdin.write_all(wrapped.as_bytes()).is_err() || stdin.write_all(b"\n").is_err() {
            self.restart();
            return ExecutionResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                timed_out: false,
                error: Some("Shell stdin write failed.".to_string()),
            };
        }
        if stdin.flush().is_err() {
            self.restart();
            return ExecutionResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                timed_out: false,
                error: Some("Shell stdin flush failed.".to_string()),
            };
        }

        let timeout = Duration::from_secs_f64(timeout_seconds.max(0.1));
        let start = Instant::now();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code: Option<i32> = None;
        let mut timed_out = false;
        let mut shell_died = false;

        while start.elapsed() < timeout {
            if !self.alive() || self.generation != gen_at_start {
                shell_died = true;
                break;
            }

            match self.output_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(OutputLine::Stdout(line)) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if let Some(cap) = marker_re.captures(trimmed) {
                        exit_code = cap[1].parse().ok();
                        break;
                    }
                    stdout.push(line);
                }
                Ok(OutputLine::Stderr(line)) => {
                    stderr.push(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    shell_died = true;
                    break;
                }
            }
        }

        if exit_code.is_none() {
            timed_out = !shell_died && self.alive();
            self.restart();
        }

        ExecutionResult {
            stdout: stdout.join(""),
            stderr: stderr.join(""),
            exit_code,
            timed_out,
            error: None,
        }
    }
}

fn spawn_line_reader<R, F>(reader: R, tx: Sender<OutputLine>, wrap: F)
where
    R: BufRead + Send + 'static,
    F: Fn(String) -> OutputLine + Send + 'static,
{
    thread::spawn(move || {
        let reader = reader;
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(wrap(format!("{l}\n"))).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn shell_launch_spec() -> (&'static str, Vec<&'static str>) {
    #[cfg(windows)]
    {
        if shell_available("pwsh") {
            ("pwsh", vec!["-NoLogo", "-NoProfile", "-Command", "-"])
        } else {
            (
                "powershell.exe",
                vec!["-NoLogo", "-NoProfile", "-Command", "-"],
            )
        }
    }
    #[cfg(unix)]
    {
        if shell_available("bash") {
            ("bash", vec!["--noprofile", "--norc", "-s"])
        } else {
            ("sh", vec!["-s"])
        }
    }
}

fn shell_available(bin: &str) -> bool {
    Command::new(bin)
        .arg(if bin == "pwsh" {
            "-Version"
        } else {
            "--version"
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn new_nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t:032x}{pid:08x}{c:08x}")
}

fn marker_regex(nonce: &str) -> regex::Regex {
    regex::Regex::new(&format!(r"^__W2T_DONE_{nonce}__(\d+)__$")).expect("marker regex")
}

fn wrap_command(command: &str, nonce: &str) -> String {
    #[cfg(windows)]
    {
        wrap_powershell_command(command, nonce)
    }
    #[cfg(unix)]
    {
        wrap_bash_command(command, nonce)
    }
}

#[cfg(windows)]
fn wrap_powershell_command(command: &str, nonce: &str) -> String {
    let encoded = base64_encode(command.trim().as_bytes());
    format!(
        "[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false); \
         [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); \
         $OutputEncoding = [Console]::OutputEncoding; \
         $PSDefaultParameterValues['Get-Content:Encoding'] = 'utf8'; \
         $__w2t_ec = 0; \
         $LASTEXITCODE = $null; \
         try {{ \
           $__w2t_script = [System.Text.Encoding]::UTF8.GetString(\
[System.Convert]::FromBase64String('{encoded}')); \
           . ([System.Management.Automation.ScriptBlock]::Create($__w2t_script)); \
           $__w2t_ok = $?; \
           $__w2t_native = $LASTEXITCODE; \
           if (-not $__w2t_ok) {{ \
             $__w2t_ec = if ($null -ne $__w2t_native) {{ $__w2t_native }} else {{ 1 }} \
           }} elseif ($null -ne $__w2t_native) {{ \
             $__w2t_ec = $__w2t_native \
           }} else {{ \
             $__w2t_ec = 0 \
           }} \
         }} catch {{ \
           if ($_.Exception.Message) {{ Write-Output $_.Exception.Message }} \
           else {{ Write-Output ($_ | Out-String) }}; \
           $__w2t_ec = 1 \
         }}; \
         Write-Output (\"__W2T_DONE_{nonce}__\" + $__w2t_ec + \"__\")"
    )
}

#[cfg(unix)]
fn wrap_bash_command(command: &str, nonce: &str) -> String {
    let encoded = base64_encode(command.trim().as_bytes());
    format!(
        "__w2t_ec=0; \
         if command -v base64 >/dev/null 2>&1; then \
           __w2t_script=$(printf '%s' '{encoded}' | base64 -d 2>/dev/null || printf '%s' '{encoded}' | base64 -D 2>/dev/null); \
         else \
           __w2t_script=''; \
         fi; \
         eval \"$__w2t_script\"; \
         __w2t_ec=$?; \
         printf '__W2T_DONE_{nonce}__%s__\\n' \"$__w2t_ec\""
    )
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    // Jeder Test hier spawnt einen echten PowerShell/sh-Prozess und verlaesst
    // sich auf enge Timing-Fenster (z.B. 0.8s Timeout gegen einen 3s Sleep).
    // Liefen alle acht parallel (Standard-`cargo test`-Verhalten), kam es unter
    // Systemlast reproduzierbar zu 1-7 Fehlschlaegen quer durch alle Tests --
    // nicht nur die timing-kritischen, was fuer echte Prozess-Spawn-Kontention
    // spricht (PowerShell-Start ist unter Last/AV-Scan langsam), nicht fuer
    // einen Logik-Bug. Serialisiert per Lock statt die Zeitfenster aufzuweiten:
    // die Fenster testen ein echtes Verhalten (kein Leak in die naechste Action),
    // nicht Performance-Grenzwerte.
    lazy_static::lazy_static! {
        static ref SHELL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    }

    fn executor_with_session() -> (PlatformShellExecutor, MutexGuard<'static, ()>) {
        let guard = SHELL_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let ex = PlatformShellExecutor::new();
        ex.start();
        (ex, guard)
    }

    #[cfg(windows)]
    #[test]
    fn powershell_wrapper_establishes_utf8_for_input_output_and_file_reads() {
        let wrapped = wrap_powershell_command("Get-Content x", "nonce");
        assert!(wrapped.contains("[Console]::InputEncoding"));
        assert!(wrapped.contains("[Console]::OutputEncoding"));
        assert!(wrapped.contains("$PSDefaultParameterValues['Get-Content:Encoding'] = 'utf8'"));
        assert!(wrapped.contains("[System.Text.Encoding]::UTF8.GetString"));
    }

    #[cfg(windows)]
    #[test]
    fn direct_get_content_preserves_utf8_without_bom() {
        let _guard = SHELL_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let path = std::env::temp_dir().join(format!(
            "webagent_executor_utf8_{}_{}.txt",
            std::process::id(),
            crate::now_run_stamp()
        ));
        std::fs::write(&path, "Grüße …").unwrap();
        let executor = PlatformShellExecutor::new();
        executor.start();
        let escaped = path.display().to_string().replace(char::from(39), "''");
        let result = executor.execute(
            &format!("Get-Content -LiteralPath '{escaped}'"),
            TEST_CMD_TIMEOUT,
        );
        executor.stop();
        let _ = std::fs::remove_file(&path);

        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert!(result.stdout.contains("Grüße …"), "{result:?}");
    }

    #[test]
    fn test_simple_command() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("Write-Output hello", TEST_CMD_TIMEOUT);
        #[cfg(unix)]
        let result = executor.execute("echo hello", TEST_CMD_TIMEOUT);

        assert!(result.error.is_none());
        assert!(!result.timed_out);
        assert!(result.stdout.contains("hello"));
        assert_eq!(result.exit_code, Some(0));
        executor.stop();
    }

    #[test]
    fn test_timeout() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("Start-Sleep -Seconds 10", 1.0);
        #[cfg(unix)]
        let result = executor.execute("sleep 10", 1.0);

        assert!(result.timed_out);
        assert!(result.exit_code.is_none());
        executor.stop();
    }

    /// Timeout fuer Kommandos, die sofort fertig sein SOLLTEN.
    ///
    /// 5s reichten nicht: laeuft parallel ein Benchmark (PowerShell-Sessions,
    /// cargo-Builds), braucht allein das Hochfahren der Shell-Session laenger,
    /// und `test_nonzero_exit` schlug mit `timed_out` fehl — 3/3 gruen, sobald
    /// die Maschine frei war. Der Wert prueft nichts, er wartet nur; die
    /// eigentlichen Timeout-Tests arbeiten bewusst mit knappen Werten (0.8/1.0)
    /// und bleiben davon unberuehrt.
    const TEST_CMD_TIMEOUT: f64 = 30.0;

    #[test]
    fn test_nonzero_exit() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("cmd /c exit 42", TEST_CMD_TIMEOUT);
        #[cfg(unix)]
        // Child shell: plain `exit` würde die persistente bash-Session beenden.
        let result = executor.execute("bash -c 'exit 42'", TEST_CMD_TIMEOUT);

        assert!(!result.timed_out);
        assert_eq!(result.exit_code, Some(42));
        executor.stop();
    }

    #[test]
    fn test_stale_lastexitcode_not_inherited() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        {
            let r1 = executor.execute("cmd /c exit 7", TEST_CMD_TIMEOUT);
            assert_eq!(r1.exit_code, Some(7));
            let r2 = executor.execute("$null = 1", TEST_CMD_TIMEOUT);
            assert_eq!(r2.exit_code, Some(0));
            let r3 = executor.execute("Write-Output ok", TEST_CMD_TIMEOUT);
            assert_eq!(r3.exit_code, Some(0));
            assert!(r3.stdout.contains("ok"));
        }

        #[cfg(unix)]
        {
            let r1 = executor.execute("false", TEST_CMD_TIMEOUT);
            assert_eq!(r1.exit_code, Some(1));
            let r2 = executor.execute("true", TEST_CMD_TIMEOUT);
            assert_eq!(r2.exit_code, Some(0));
        }

        executor.stop();
    }

    #[test]
    fn test_timeout_no_leak_to_next_action() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        {
            let result = executor.execute("Start-Sleep 3; Write-Output late", 0.8);
            assert!(result.timed_out);
            let result2 = executor.execute("Write-Output next", TEST_CMD_TIMEOUT);
            assert!(result2.stdout.contains("next"));
            assert!(!result2.stdout.contains("late"));
        }

        #[cfg(unix)]
        {
            let result = executor.execute("sleep 3; echo late", 0.8);
            assert!(result.timed_out);
            let result2 = executor.execute("echo next", TEST_CMD_TIMEOUT);
            assert!(result2.stdout.contains("next"));
            assert!(!result2.stdout.contains("late"));
        }

        executor.stop();
    }

    #[test]
    fn test_fake_marker_does_not_complete_early() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("Write-Output \"__W2T_DONE_fake__0__\"", TEST_CMD_TIMEOUT);
        #[cfg(unix)]
        let result = executor.execute("echo \"__W2T_DONE_fake__0__\"", TEST_CMD_TIMEOUT);

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("__W2T_DONE_fake__0__"));
        executor.stop();
    }

    #[test]
    fn test_cwd_persists_across_commands() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        {
            let r1 = executor.execute("Set-Location $env:TEMP", TEST_CMD_TIMEOUT);
            assert_eq!(r1.exit_code, Some(0));
            let r2 = executor.execute("(Get-Location).Path", TEST_CMD_TIMEOUT);
            let out = r2.stdout.to_lowercase();
            assert!(out.contains("temp"), "expected TEMP in {:?}", r2.stdout);
        }

        #[cfg(unix)]
        {
            let r1 = executor.execute("cd /tmp", TEST_CMD_TIMEOUT);
            assert_eq!(r1.exit_code, Some(0));
            let r2 = executor.execute("pwd", TEST_CMD_TIMEOUT);
            assert!(r2.stdout.contains("/tmp"));
        }

        executor.stop();
    }

    #[test]
    fn test_explicit_start_dir_is_shell_cwd() {
        let _guard = SHELL_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "webagent_executor_cwd_{}_{}",
            std::process::id(),
            crate::now_run_stamp()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let expected = dir.canonicalize().unwrap();
        let executor = PlatformShellExecutor::new_in(&expected);
        executor.start();

        #[cfg(windows)]
        let result = executor.execute("(Get-Location).Path", TEST_CMD_TIMEOUT);
        #[cfg(unix)]
        let result = executor.execute("pwd", TEST_CMD_TIMEOUT);

        assert_eq!(result.exit_code, Some(0));
        let actual = std::path::PathBuf::from(result.stdout.trim())
            .canonicalize()
            .unwrap();
        assert_eq!(actual, expected);
        executor.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_stderr_capture() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("[Console]::Error.WriteLine('test error')", TEST_CMD_TIMEOUT);
        #[cfg(unix)]
        let result = executor.execute("echo 'test error' >&2", TEST_CMD_TIMEOUT);

        assert!(
            result.stderr.contains("test error"),
            "stderr was not captured separately: {result:?}"
        );
        assert!(
            !result.stdout.contains("test error"),
            "stderr leaked into stdout: {result:?}"
        );
        executor.stop();
    }

    #[test]
    fn test_marker_on_stderr_does_not_complete_command() {
        let (executor, _guard) = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute(
            "[Console]::Error.WriteLine('__W2T_DONE_fake__0__'); Write-Output after",
            TEST_CMD_TIMEOUT,
        );
        #[cfg(unix)]
        let result = executor.execute(
            "echo '__W2T_DONE_fake__0__' >&2; echo after",
            TEST_CMD_TIMEOUT,
        );

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stderr.contains("__W2T_DONE_fake__0__"));
        assert!(result.stdout.contains("after"));
        executor.stop();
    }
}
