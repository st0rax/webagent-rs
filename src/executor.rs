//! executor — Plattformübergreifende Shell-Ausführung (PowerShell/sh).
//!
//! Portiert aus `../src/webagent/executor/powershell.py`. Hält eine persistente
//! Shell-Session pro Run (cd/Variablen/Env überleben zwischen Actions), mit
//! nonce-gebundenem Abschlussmarker und Base64-wrapped Commands auf PowerShell.

use std::io::{BufRead, BufReader, Write};
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
    output_rx: Receiver<String>,
    generation: u64,
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
            eprintln!("executor: shell start failed: {e}");
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
        spawn_line_reader(BufReader::new(stdout), tx.clone());
        spawn_line_reader(BufReader::new(stderr), tx);

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
        let mut collected = Vec::new();
        let mut exit_code: Option<i32> = None;
        let mut timed_out = false;
        let mut shell_died = false;

        while start.elapsed() < timeout {
            if !self.alive() || self.generation != gen_at_start {
                shell_died = true;
                break;
            }

            match self.output_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(line) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if let Some(cap) = marker_re.captures(trimmed) {
                        exit_code = cap[1].parse().ok();
                        break;
                    }
                    collected.push(line);
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
            stdout: collected.join(""),
            stderr: String::new(),
            exit_code,
            timed_out,
            error: None,
        }
    }
}

fn spawn_line_reader<R: BufRead + Send + 'static>(reader: R, tx: Sender<String>) {
    thread::spawn(move || {
        let reader = reader;
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(format!("{l}\n")).is_err() {
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
        "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); \
         $OutputEncoding = [Console]::OutputEncoding; \
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

    fn executor_with_session() -> PlatformShellExecutor {
        let ex = PlatformShellExecutor::new();
        ex.start();
        ex
    }

    #[test]
    fn test_simple_command() {
        let executor = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("Write-Output hello", 5.0);
        #[cfg(unix)]
        let result = executor.execute("echo hello", 5.0);

        assert!(result.error.is_none());
        assert!(!result.timed_out);
        assert!(result.stdout.contains("hello"));
        assert_eq!(result.exit_code, Some(0));
        executor.stop();
    }

    #[test]
    fn test_timeout() {
        let executor = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("Start-Sleep -Seconds 10", 1.0);
        #[cfg(unix)]
        let result = executor.execute("sleep 10", 1.0);

        assert!(result.timed_out);
        assert!(result.exit_code.is_none());
        executor.stop();
    }

    #[test]
    fn test_nonzero_exit() {
        let executor = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("cmd /c exit 42", 5.0);
        #[cfg(unix)]
        let result = executor.execute("exit 42", 5.0);

        assert!(!result.timed_out);
        assert_eq!(result.exit_code, Some(42));
        executor.stop();
    }

    #[test]
    fn test_stale_lastexitcode_not_inherited() {
        let executor = executor_with_session();

        #[cfg(windows)]
        {
            let r1 = executor.execute("cmd /c exit 7", 5.0);
            assert_eq!(r1.exit_code, Some(7));
            let r2 = executor.execute("$null = 1", 5.0);
            assert_eq!(r2.exit_code, Some(0));
            let r3 = executor.execute("Write-Output ok", 5.0);
            assert_eq!(r3.exit_code, Some(0));
            assert!(r3.stdout.contains("ok"));
        }

        #[cfg(unix)]
        {
            let r1 = executor.execute("false", 5.0);
            assert_eq!(r1.exit_code, Some(1));
            let r2 = executor.execute("true", 5.0);
            assert_eq!(r2.exit_code, Some(0));
        }

        executor.stop();
    }

    #[test]
    fn test_timeout_no_leak_to_next_action() {
        let executor = executor_with_session();

        #[cfg(windows)]
        {
            let result = executor.execute("Start-Sleep 3; Write-Output late", 0.8);
            assert!(result.timed_out);
            let result2 = executor.execute("Write-Output next", 5.0);
            assert!(result2.stdout.contains("next"));
            assert!(!result2.stdout.contains("late"));
        }

        #[cfg(unix)]
        {
            let result = executor.execute("sleep 3; echo late", 0.8);
            assert!(result.timed_out);
            let result2 = executor.execute("echo next", 5.0);
            assert!(result2.stdout.contains("next"));
            assert!(!result2.stdout.contains("late"));
        }

        executor.stop();
    }

    #[test]
    fn test_fake_marker_does_not_complete_early() {
        let executor = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("Write-Output \"__W2T_DONE_fake__0__\"", 5.0);
        #[cfg(unix)]
        let result = executor.execute("echo \"__W2T_DONE_fake__0__\"", 5.0);

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("__W2T_DONE_fake__0__"));
        executor.stop();
    }

    #[test]
    fn test_cwd_persists_across_commands() {
        let executor = executor_with_session();

        #[cfg(windows)]
        {
            let r1 = executor.execute("Set-Location $env:TEMP", 5.0);
            assert_eq!(r1.exit_code, Some(0));
            let r2 = executor.execute("(Get-Location).Path", 5.0);
            let out = r2.stdout.to_lowercase();
            assert!(out.contains("temp"), "expected TEMP in {:?}", r2.stdout);
        }

        #[cfg(unix)]
        {
            let r1 = executor.execute("cd /tmp", 5.0);
            assert_eq!(r1.exit_code, Some(0));
            let r2 = executor.execute("pwd", 5.0);
            assert!(r2.stdout.contains("/tmp"));
        }

        executor.stop();
    }

    #[test]
    fn test_stderr_capture() {
        let executor = executor_with_session();

        #[cfg(windows)]
        let result = executor.execute("[Console]::Error.WriteLine('test error')", 5.0);
        #[cfg(unix)]
        let result = executor.execute("echo 'test error' >&2", 5.0);

        // Persistent shell merges stderr into stdout (Python parity).
        assert!(result.stdout.contains("test error") || result.stderr.contains("test error"));
        executor.stop();
    }
}
