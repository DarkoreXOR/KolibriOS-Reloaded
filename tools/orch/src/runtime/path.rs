//! Working directory and path helpers.
//!
//! # CWD semantics
//!
//! - **Orchestrator invocation CWD**: process CWD when `orch` started.
//! - **Script CWD**: starts as invocation CWD; `path::chdir` changes it for the
//!   current script scope and nested executions unless overridden.
//! - **Child process CWD**: inherits Script CWD by default; may be overridden
//!   per-process. Changing Script CWD does not affect already-running processes.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ScriptCwd {
    inner: Arc<Mutex<PathBuf>>,
}

impl ScriptCwd {
    pub fn new(initial: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn from_invocation() -> Self {
        Self::new(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    pub fn get(&self) -> PathBuf {
        self.inner.lock().unwrap().clone()
    }

    pub fn chdir(&self, path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        let path = path.as_ref();
        let mut guard = self.inner.lock().unwrap();
        let next = if path.is_absolute() {
            path.to_path_buf()
        } else {
            guard.join(path)
        };
        let canon = std::fs::canonicalize(&next).unwrap_or(next);
        *guard = canon.clone();
        Ok(canon)
    }

    pub fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.get().join(path)
        }
    }
}
