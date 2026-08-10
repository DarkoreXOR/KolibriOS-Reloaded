//! Generic process runtime with poll/wait, IO modes, and cancellation.

use super::cancel::CancelToken;
use super::env::EnvOverlay;
use super::path::ScriptCwd;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoMode {
    Inherit,
    Capture,
    Pipe,
    Null,
    File,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub stdin: IoMode,
    pub stdout: IoMode,
    pub stderr: IoMode,
    pub stdout_file: Option<PathBuf>,
    pub stderr_file: Option<PathBuf>,
    pub stdin_file: Option<PathBuf>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: None,
            stdin: IoMode::Inherit,
            stdout: IoMode::Inherit,
            stderr: IoMode::Inherit,
            stdout_file: None,
            stderr_file: None,
            stdin_file: None,
        }
    }
}

#[derive(Debug)]
pub enum PollResult {
    Running,
    Finished(ExitStatus),
}

impl PollResult {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished(_))
    }
}

/// Tracks live child processes so cancellation can terminate them.
#[derive(Clone, Default)]
pub struct ProcessTracker {
    live: Arc<Mutex<Vec<ProcessHandle>>>,
}

impl ProcessTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn track(&self, handle: ProcessHandle) {
        self.live.lock().unwrap().push(handle);
    }

    pub fn untrack(&self, handle: &ProcessHandle) {
        let mut g = self.live.lock().unwrap();
        g.retain(|h| !Arc::ptr_eq(&h.inner, &handle.inner));
    }

    /// Kill all tracked children and wait briefly for them to exit.
    pub fn kill_all(&self, timeout_ms: u64) {
        let handles: Vec<ProcessHandle> = {
            let mut g = self.live.lock().unwrap();
            g.drain(..).collect()
        };
        for h in &handles {
            let _ = h.kill();
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        for h in &handles {
            while Instant::now() < deadline {
                match h.poll() {
                    Ok(PollResult::Finished(_)) | Err(_) => break,
                    Ok(PollResult::Running) => {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        }
    }
}

struct ProcessInner {
    child: Option<Child>,
    finished: Option<ExitStatus>,
    stdout_buf: Arc<Mutex<Vec<u8>>>,
    stderr_buf: Arc<Mutex<Vec<u8>>>,
    program: String,
}

#[derive(Clone)]
pub struct ProcessHandle {
    inner: Arc<Mutex<ProcessInner>>,
    cancel: CancelToken,
    poll_ms: u64,
    tracker: ProcessTracker,
    termination_timeout_ms: u64,
}

impl ProcessHandle {
    pub fn spawn(
        spec: ProcessSpec,
        script_cwd: &ScriptCwd,
        env: &EnvOverlay,
        cancel: CancelToken,
        poll_ms: u64,
        tracker: ProcessTracker,
        termination_timeout_ms: u64,
    ) -> std::io::Result<Self> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        let cwd = spec.cwd.clone().unwrap_or_else(|| script_cwd.get());
        cmd.current_dir(&cwd);
        env.apply_to(&mut cmd);

        cmd.stdin(stdio_from(spec.stdin, spec.stdin_file.as_deref(), true)?);
        cmd.stdout(stdio_from(spec.stdout, spec.stdout_file.as_deref(), false)?);
        cmd.stderr(stdio_from(spec.stderr, spec.stderr_file.as_deref(), false)?);

        let mut child = cmd.spawn()?;
        let stdout_buf = Arc::new(Mutex::new(Vec::new()));
        let stderr_buf = Arc::new(Mutex::new(Vec::new()));

        if matches!(spec.stdout, IoMode::Capture | IoMode::Pipe) {
            if let Some(mut out) = child.stdout.take() {
                let buf = Arc::clone(&stdout_buf);
                thread::spawn(move || {
                    let mut tmp = Vec::new();
                    let _ = out.read_to_end(&mut tmp);
                    if let Ok(mut g) = buf.lock() {
                        g.extend_from_slice(&tmp);
                    }
                });
            }
        }
        if matches!(spec.stderr, IoMode::Capture | IoMode::Pipe) {
            if let Some(mut err) = child.stderr.take() {
                let buf = Arc::clone(&stderr_buf);
                thread::spawn(move || {
                    let mut tmp = Vec::new();
                    let _ = err.read_to_end(&mut tmp);
                    if let Ok(mut g) = buf.lock() {
                        g.extend_from_slice(&tmp);
                    }
                });
            }
        }

        let handle = Self {
            inner: Arc::new(Mutex::new(ProcessInner {
                child: Some(child),
                finished: None,
                stdout_buf,
                stderr_buf,
                program: spec.program,
            })),
            cancel,
            poll_ms,
            tracker: tracker.clone(),
            termination_timeout_ms,
        };
        tracker.track(handle.clone());
        Ok(handle)
    }

    pub fn poll(&self) -> std::io::Result<PollResult> {
        let mut g = self.inner.lock().unwrap();
        if let Some(status) = g.finished {
            return Ok(PollResult::Finished(status));
        }
        let child = g.child.as_mut().expect("child missing");
        match child.try_wait()? {
            Some(status) => {
                g.finished = Some(status);
                g.child = None;
                drop(g);
                self.tracker.untrack(self);
                Ok(PollResult::Finished(status))
            }
            None => Ok(PollResult::Running),
        }
    }

    pub fn wait(&self) -> Result<ExitStatus, WaitError> {
        loop {
            if self.cancel.requested() {
                // Terminate this child and any siblings before surfacing cancel.
                let _ = self.kill();
                self.tracker.kill_all(self.termination_timeout_ms);
                return Err(WaitError::Cancelled);
            }
            match self.poll().map_err(WaitError::Io)? {
                PollResult::Running => {
                    thread::sleep(Duration::from_millis(self.poll_ms.max(1)));
                }
                PollResult::Finished(status) => return Ok(status),
            }
        }
    }

    pub fn kill(&self) -> std::io::Result<()> {
        let mut g = self.inner.lock().unwrap();
        if let Some(child) = g.child.as_mut() {
            child.kill()?;
            let _ = child.wait();
        }
        Ok(())
    }

    pub fn stdout_captured(&self) -> String {
        let g = self.inner.lock().unwrap();
        let bytes = g.stdout_buf.lock().unwrap().clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn stderr_captured(&self) -> String {
        let g = self.inner.lock().unwrap();
        let bytes = g.stderr_buf.lock().unwrap().clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn program(&self) -> String {
        self.inner.lock().unwrap().program.clone()
    }

    pub fn write_stdin(&self, data: &[u8]) -> std::io::Result<()> {
        let mut g = self.inner.lock().unwrap();
        if let Some(child) = g.child.as_mut() {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(data)?;
                return Ok(());
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "process stdin is not available",
        ))
    }

    pub fn close_stdin(&self) -> std::io::Result<()> {
        let mut g = self.inner.lock().unwrap();
        if let Some(child) = g.child.as_mut() {
            let _ = child.stdin.take();
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum WaitError {
    Cancelled,
    Io(std::io::Error),
}

impl std::fmt::Display for WaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancellation requested while waiting for process"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WaitError {}

fn stdio_from(mode: IoMode, file: Option<&Path>, for_stdin: bool) -> std::io::Result<Stdio> {
    Ok(match mode {
        IoMode::Inherit => Stdio::inherit(),
        IoMode::Null => Stdio::null(),
        IoMode::Capture | IoMode::Pipe => {
            if for_stdin {
                Stdio::piped()
            } else {
                Stdio::piped()
            }
        }
        IoMode::File => {
            let path = file.ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "file IO mode requires path")
            })?;
            if for_stdin {
                Stdio::from(std::fs::File::open(path)?)
            } else {
                Stdio::from(std::fs::File::create(path)?)
            }
        }
    })
}
