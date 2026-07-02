use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_TERMINAL_LINES: usize = 4_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalLine {
    pub seq: u64,
    pub stream: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalSnapshot {
    pub running: bool,
    pub session_id: Option<String>,
    pub shell: Option<String>,
    pub cwd: Option<String>,
    pub status: String,
    pub next_seq: u64,
    pub lines: Vec<TerminalLine>,
}

struct TerminalSession {
    id: String,
    shell: String,
    cwd: PathBuf,
    child: Child,
    stdin: ChildStdin,
    output: Arc<Mutex<VecDeque<TerminalLine>>>,
    next_seq: Arc<AtomicU64>,
    exit_status: Option<String>,
}

#[derive(Default)]
struct TerminalManager {
    session: Option<TerminalSession>,
}

static TERMINAL_MANAGER: OnceLock<Mutex<TerminalManager>> = OnceLock::new();

pub fn start_terminal_session(
    workspace: &Workspace,
    cwd: Option<&str>,
    shell: Option<&str>,
) -> anyhow::Result<TerminalSnapshot> {
    let cwd = workspace.resolve_existing(cwd.unwrap_or("."))?;
    if !cwd.is_dir() {
        anyhow::bail!("terminal_start cwd must be a directory");
    }
    let shell = shell.unwrap_or(default_shell()).to_ascii_lowercase();
    let mut manager = terminal_manager()
        .lock()
        .expect("terminal manager poisoned");
    manager.start(cwd, shell)?;
    Ok(manager.snapshot(Some(300), None))
}

pub fn write_terminal_input(input: &str, enter: bool) -> anyhow::Result<TerminalSnapshot> {
    let mut manager = terminal_manager()
        .lock()
        .expect("terminal manager poisoned");
    manager.write_input(input, enter)?;
    Ok(manager.snapshot(Some(300), None))
}

pub fn read_terminal_snapshot(lines: Option<usize>, since_seq: Option<u64>) -> TerminalSnapshot {
    let mut manager = terminal_manager()
        .lock()
        .expect("terminal manager poisoned");
    manager.snapshot(lines, since_seq)
}

pub fn stop_terminal_session() -> anyhow::Result<TerminalSnapshot> {
    let mut manager = terminal_manager()
        .lock()
        .expect("terminal manager poisoned");
    manager.stop()?;
    Ok(manager.snapshot(Some(300), None))
}

pub fn clear_terminal_output() -> TerminalSnapshot {
    let mut manager = terminal_manager()
        .lock()
        .expect("terminal manager poisoned");
    manager.clear();
    manager.snapshot(Some(300), None)
}

fn terminal_manager() -> &'static Mutex<TerminalManager> {
    TERMINAL_MANAGER.get_or_init(|| Mutex::new(TerminalManager::default()))
}

impl TerminalManager {
    fn start(&mut self, cwd: PathBuf, shell: String) -> anyhow::Result<()> {
        if let Some(session) = &mut self.session {
            session.refresh_exit();
            if session.is_running() {
                anyhow::bail!("terminal session is already running");
            }
        }

        let mut child = spawn_shell(&shell, &cwd)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("terminal stdin was not captured"))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let output = Arc::new(Mutex::new(VecDeque::new()));
        let next_seq = Arc::new(AtomicU64::new(1));
        let id = format!("terminal-{}", uuid::Uuid::new_v4());

        if let Some(stdout) = stdout {
            spawn_output_reader(stdout, "stdout", output.clone(), next_seq.clone());
        }
        if let Some(stderr) = stderr {
            spawn_output_reader(stderr, "stderr", output.clone(), next_seq.clone());
        }

        let session = TerminalSession {
            id,
            shell,
            cwd,
            child,
            stdin,
            output,
            next_seq,
            exit_status: None,
        };
        session.push_line(
            "system",
            format!(
                "terminal started: {} in {}",
                session.shell,
                session.cwd.display()
            ),
        );
        self.session = Some(session);
        Ok(())
    }

    fn write_input(&mut self, input: &str, enter: bool) -> anyhow::Result<()> {
        let Some(session) = &mut self.session else {
            anyhow::bail!("terminal session is not running");
        };
        session.refresh_exit();
        if !session.is_running() {
            anyhow::bail!("terminal session has exited");
        }
        session.stdin.write_all(input.as_bytes())?;
        if enter {
            session.stdin.write_all(line_ending().as_bytes())?;
        }
        session.stdin.flush()?;
        session.push_line(
            "stdin",
            if enter {
                input.to_string()
            } else {
                format!("{input}<no-enter>")
            },
        );
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        let Some(session) = &mut self.session else {
            anyhow::bail!("terminal session is not running");
        };
        session.refresh_exit();
        if session.is_running() {
            session.child.kill()?;
            session.exit_status = Some("killed".to_string());
            session.push_line("system", "terminal killed");
        }
        Ok(())
    }

    fn clear(&mut self) {
        if let Some(session) = &mut self.session {
            if let Ok(mut output) = session.output.lock() {
                output.clear();
            }
            session.push_line("system", "terminal output cleared");
        }
    }

    fn snapshot(&mut self, lines: Option<usize>, since_seq: Option<u64>) -> TerminalSnapshot {
        let Some(session) = &mut self.session else {
            return TerminalSnapshot {
                running: false,
                session_id: None,
                shell: None,
                cwd: None,
                status: "not started".to_string(),
                next_seq: 1,
                lines: Vec::new(),
            };
        };
        session.refresh_exit();
        let running = session.is_running();
        let mut output = session
            .output
            .lock()
            .map(|output| output.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(since_seq) = since_seq {
            output.retain(|line| line.seq > since_seq);
        }
        let limit = lines.unwrap_or(300).clamp(1, 1_000);
        if output.len() > limit {
            output = output[output.len() - limit..].to_vec();
        }

        TerminalSnapshot {
            running,
            session_id: Some(session.id.clone()),
            shell: Some(session.shell.clone()),
            cwd: Some(session.cwd.display().to_string()),
            status: session
                .exit_status
                .clone()
                .unwrap_or_else(|| "running".to_string()),
            next_seq: session.next_seq.load(Ordering::SeqCst),
            lines: output,
        }
    }
}

impl TerminalSession {
    fn is_running(&self) -> bool {
        self.exit_status.is_none()
    }

    fn refresh_exit(&mut self) {
        if self.exit_status.is_some() {
            return;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let status = format!("exited: {status}");
                self.exit_status = Some(status.clone());
                self.push_line("system", status);
            }
            Ok(None) => {}
            Err(err) => {
                let status = format!("status check failed: {err}");
                self.exit_status = Some(status.clone());
                self.push_line("system", status);
            }
        }
    }

    fn push_line(&self, stream: impl Into<String>, text: impl Into<String>) {
        push_line(&self.output, &self.next_seq, stream, text);
    }
}

fn spawn_shell(shell: &str, cwd: &Path) -> anyhow::Result<Child> {
    let mut command = match shell {
        "cmd" | "cmd.exe" => {
            let mut command = Command::new("cmd.exe");
            command.arg("/Q").arg("/K");
            command
        }
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
            let executable = if shell.starts_with("pwsh") {
                "pwsh.exe"
            } else {
                "powershell.exe"
            };
            let mut command = Command::new(executable);
            command
                .arg("-NoLogo")
                .arg("-NoProfile")
                .arg("-ExecutionPolicy")
                .arg("Bypass");
            command
        }
        unsupported => anyhow::bail!("unsupported terminal shell: {unsupported}"),
    };

    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(anyhow::Error::from)
}

fn spawn_output_reader<R>(
    stream: R,
    label: &'static str,
    output: Arc<Mutex<VecDeque<TerminalLine>>>,
    next_seq: Arc<AtomicU64>,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let text = line.trim_end_matches(['\r', '\n']).to_string();
                    if !text.is_empty() {
                        push_line(&output, &next_seq, label, text);
                    }
                }
                Err(err) => {
                    push_line(&output, &next_seq, label, format!("reader error: {err}"));
                    break;
                }
            }
        }
    });
}

fn push_line(
    output: &Arc<Mutex<VecDeque<TerminalLine>>>,
    next_seq: &AtomicU64,
    stream: impl Into<String>,
    text: impl Into<String>,
) {
    let seq = next_seq.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut output) = output.lock() {
        output.push_back(TerminalLine {
            seq,
            stream: stream.into(),
            text: text.into(),
        });
        while output.len() > MAX_TERMINAL_LINES {
            output.pop_front();
        }
    }
}

fn default_shell() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "powershell"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "sh"
    }
}

fn line_ending() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "\r\n"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "\n"
    }
}

#[allow(dead_code)]
fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_without_session_is_stopped() {
        let mut manager = TerminalManager::default();
        let snapshot = manager.snapshot(Some(10), None);

        assert!(!snapshot.running);
        assert_eq!(snapshot.status, "not started");
        assert!(snapshot.lines.is_empty());
    }
}
