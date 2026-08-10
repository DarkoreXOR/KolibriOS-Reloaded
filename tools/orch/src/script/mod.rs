//! Rhai source evaluation (named Actions, Workflows, and inline `$`).
//!
//! This module is the generic Rhai eval layer — not a user-facing "Script" entity.
//! User-facing execution units are only: `$` / `@action` / workflow.

mod api;
mod engine;

pub use engine::{eval_script, ScriptContext, ScriptError, ScriptOutcome};
pub use api::register_runtime_modules;

#[derive(Debug, Clone)]
pub enum ScriptSource {
    File { path: std::path::PathBuf },
    Inline { source: String },
}

impl ScriptSource {
    pub fn label(&self) -> String {
        match self {
            Self::File { path } => path.display().to_string(),
            Self::Inline { .. } => "<inline>".into(),
        }
    }
}
