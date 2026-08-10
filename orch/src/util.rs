//! Command execution helpers with loud failure reporting.

use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub struct CmdResult {
    #[allow(dead_code)]
    pub status: i32,
    #[allow(dead_code)]
    pub stdout: String,
    #[allow(dead_code)]
    pub stderr: String,
}

pub fn format_cmdline(program: &OsStr, args: &[impl AsRef<OsStr>]) -> String {
    let mut parts = Vec::new();
    parts.push(quote_os(program));
    for a in args {
        parts.push(quote_os(a.as_ref()));
    }
    parts.join(" ")
}

fn quote_os(s: &OsStr) -> String {
    let t = s.to_string_lossy();
    if t.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("\"{}\"", t.replace('"', "\\\""))
    } else {
        t.into_owned()
    }
}

pub fn run_checked(
    stage: &str,
    program: impl AsRef<OsStr>,
    args: &[impl AsRef<OsStr>],
    cwd: &Path,
    env_clear: &[(&str, Option<&str>)],
    dry_run: bool,
) -> Result<CmdResult> {
    let program = program.as_ref();
    let cmdline = format_cmdline(program, args);
    eprintln!("  $ {cmdline}");
    eprintln!("    cwd: {}", cwd.display());

    if dry_run {
        return Ok(CmdResult {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    let mut cmd = Command::new(program);
    cmd.args(args.iter().map(|a| a.as_ref()))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (k, v) in env_clear {
        match v {
            Some(val) => {
                cmd.env(k, val);
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }

    let output = cmd
        .output()
        .with_context(|| {
            format!(
                "ERROR: {stage} failed to start\nCommand: {cmdline}\nWorking directory: {}",
                cwd.display()
            )
        })?;

    forward_output(&output);

    let status = output.status.code().unwrap_or(1);
    let result = CmdResult {
        status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };

    if !output.status.success() {
        bail!(
            "ERROR: {stage} failed (exit {status})\nCommand: {cmdline}\nWorking directory: {}",
            cwd.display()
        );
    }
    Ok(result)
}

/// Like `run_checked` but inherit stdio (needed for interactive QEMU).
/// Returns the process exit code without treating non-zero as a Rust error
/// (caller decides). Still errors if the process cannot be started.
pub fn run_inherit(
    stage: &str,
    program: impl AsRef<OsStr>,
    args: &[impl AsRef<OsStr>],
    cwd: &Path,
    dry_run: bool,
) -> Result<i32> {
    let program = program.as_ref();
    let cmdline = format_cmdline(program, args);
    eprintln!("  $ {cmdline}");
    eprintln!("    cwd: {}", cwd.display());

    if dry_run {
        return Ok(0);
    }

    let status = Command::new(program)
        .args(args.iter().map(|a| a.as_ref()))
        .current_dir(cwd)
        .status()
        .with_context(|| {
            format!(
                "ERROR: {stage} failed to start\nCommand: {cmdline}\nWorking directory: {}",
                cwd.display()
            )
        })?;

    Ok(status.code().unwrap_or(1))
}

fn forward_output(output: &Output) {
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

pub fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
            let with_cmd = dir.join(format!("{name}.cmd"));
            if with_cmd.is_file() {
                return Some(with_cmd);
            }
            let with_bat = dir.join(format!("{name}.bat"));
            if with_bat.is_file() {
                return Some(with_bat);
            }
        }
    }
    None
}

pub fn resolve_tool(configured: &str, root: &Path) -> Option<PathBuf> {
    let p = Path::new(configured);
    if p.is_absolute() {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    let from_root = root.join(p);
    if from_root.is_file() {
        return Some(from_root);
    }
    which_on_path(configured)
}

pub fn find_python(preferred: &str) -> Option<PathBuf> {
    if let Some(p) = which_on_path(preferred) {
        return Some(p);
    }
    for alt in ["python3", "py"] {
        if alt == preferred {
            continue;
        }
        if let Some(p) = which_on_path(alt) {
            return Some(p);
        }
    }
    None
}
