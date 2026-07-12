//! executor — Plattformübergreifende Shell-Ausführung (PowerShell/sh).
//!
//! Portiert aus `../src/webagent/executor.py`. Windows nutzt PowerShell,
//! Unix-Systeme `sh` oder `bash`. Keine plattformspezifischen Annahmen
//! außer über `#[cfg(...)]`.

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Trait für plattformübergreifende Shell-Ausführung.
pub trait ShellExecutor {
    /// Führt einen Shell-Befehl aus und gibt stdout/stderr/exit_code zurück.
    fn execute(&self, command: &str, timeout_seconds: f64) -> ExecutionResult;
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

/// Standard-Implementierung für Windows (PowerShell) und Unix (sh/bash).
pub struct PlatformShellExecutor;

impl PlatformShellExecutor {
    pub fn new() -> Self {
        Self
    }

    #[cfg(windows)]
    fn get_shell_command() -> (&'static str, Vec<&'static str>) {
        // PowerShell ohne Profil, nicht-interaktiv. Hinweis: powershell.exe hat
        // KEINEN -OutputEncoding-Schalter (das ist eine Preference-Variable) —
        // ein ungueltiges Flag hier macht jeden Aufruf malformt.
        (
            "powershell.exe",
            vec!["-NoProfile", "-NonInteractive", "-Command"],
        )
    }

    #[cfg(unix)]
    fn get_shell_command() -> (&'static str, Vec<&'static str>) {
        // Bevorzuge bash, fallback auf sh — Verfuegbarkeit via std pruefen
        // (kein which-Crate, um rein-Rust ohne C-Toolchain zu bleiben).
        let has_bash = Command::new("bash")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if has_bash {
            ("bash", vec!["-c"])
        } else {
            ("sh", vec!["-c"])
        }
    }

    fn spawn_process(&self, command: &str) -> Result<Child, String> {
        let (shell, mut args) = Self::get_shell_command();
        args.push(command);

        Command::new(shell)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("Fehler beim Starten von {}: {}", shell, e))
    }

    fn read_stream_to_channel(
        mut reader: impl BufRead + Send + 'static,
        tx: Sender<String>,
    ) {
        thread::spawn(move || {
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                tx.send(line.clone()).ok();
                line.clear();
            }
        });
    }
}

impl ShellExecutor for PlatformShellExecutor {
    fn execute(&self, command: &str, timeout_seconds: f64) -> ExecutionResult {
        let mut child = match self.spawn_process(command) {
            Ok(c) => c,
            Err(e) => {
                return ExecutionResult {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: None,
                    timed_out: false,
                    error: Some(e),
                }
            }
        };

        let stdout_reader = BufReader::new(child.stdout.take().unwrap());
        let stderr_reader = BufReader::new(child.stderr.take().unwrap());

        let (stdout_tx, stdout_rx) = channel();
        let (stderr_tx, stderr_rx) = channel();

        Self::read_stream_to_channel(stdout_reader, stdout_tx);
        Self::read_stream_to_channel(stderr_reader, stderr_tx);

        let timeout = Duration::from_secs_f64(timeout_seconds);
        let start = Instant::now();

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        let mut timed_out = false;

        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                timed_out = true;
                child.kill().ok();
                break;
            }

            let remaining = timeout - elapsed;
            let poll_interval = Duration::from_millis(50).min(remaining);

            // Sammle alle verfügbaren Zeilen
            while let Ok(line) = stdout_rx.try_recv() {
                stdout_lines.push(line);
            }
            while let Ok(line) = stderr_rx.try_recv() {
                stderr_lines.push(line);
            }

            // Prüfe, ob Prozess beendet ist
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(poll_interval),
                Err(_) => break,
            }
        }

        // Finale Zeilen sammeln
        while let Ok(line) = stdout_rx.try_recv() {
            stdout_lines.push(line);
        }
        while let Ok(line) = stderr_rx.try_recv() {
            stderr_lines.push(line);
        }

        let exit_code = if timed_out {
            None
        } else {
            child.wait().ok().and_then(|s| s.code())
        };

        ExecutionResult {
            stdout: stdout_lines.join(""),
            stderr: stderr_lines.join(""),
            exit_code,
            timed_out,
            error: None,
        }
    }
}

impl Default for PlatformShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let executor = PlatformShellExecutor::new();
        
        #[cfg(windows)]
        let result = executor.execute("echo hello", 5.0);
        
        #[cfg(unix)]
        let result = executor.execute("echo hello", 5.0);

        assert!(result.error.is_none());
        assert!(!result.timed_out);
        assert!(result.stdout.contains("hello"));
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_timeout() {
        let executor = PlatformShellExecutor::new();
        
        #[cfg(windows)]
        let result = executor.execute("Start-Sleep -Seconds 10", 1.0);
        
        #[cfg(unix)]
        let result = executor.execute("sleep 10", 1.0);

        assert!(result.timed_out);
        assert!(result.exit_code.is_none());
    }

    #[test]
    fn test_nonzero_exit() {
        let executor = PlatformShellExecutor::new();
        
        #[cfg(windows)]
        let result = executor.execute("exit 42", 5.0);
        
        #[cfg(unix)]
        let result = executor.execute("exit 42", 5.0);

        assert!(!result.timed_out);
        assert_eq!(result.exit_code, Some(42));
    }

    #[test]
    fn test_stderr_capture() {
        let executor = PlatformShellExecutor::new();
        
        #[cfg(windows)]
        let result = executor.execute(
            "[Console]::Error.WriteLine('test error')",
            5.0
        );
        
        #[cfg(unix)]
        let result = executor.execute("echo 'test error' >&2", 5.0);

        assert!(result.stderr.contains("test error"));
    }

    #[test]
    fn test_invalid_command() {
        let executor = PlatformShellExecutor::new();
        let result = executor.execute("nonexistent_command_xyz", 5.0);

        // Sollte nicht mit error zurückkommen (Shell startet),
        // aber exit_code != 0 oder stderr gefüllt
        assert!(result.error.is_none());
        assert!(result.exit_code != Some(0) || !result.stderr.is_empty());
    }
}
