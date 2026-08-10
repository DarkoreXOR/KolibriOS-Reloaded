//! Evaluate ScriptSource with the shared runtime.

use super::api::{register_runtime_modules, rewrite_module_calls, RuntimeHandles};
use super::ScriptSource;
use crate::execution::events::{EventSink, ExecutionEvent, ExecutionState};
use crate::execution::rollback::{CleanupStack, RollbackStack};
use crate::execution::tree::ExecId;
use crate::runtime::cancel::CancelToken;
use crate::runtime::env::EnvOverlay;
use crate::runtime::path::ScriptCwd;
use crate::runtime::process::ProcessTracker;
use rhai::module_resolvers::DummyModuleResolver;
use rhai::{Dynamic, Engine, Map, Scope};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum ScriptError {
    Cancelled,
    Runtime(String),
    Io(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancellation requested"),
            Self::Runtime(m) | Self::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ScriptError {}

#[derive(Debug)]
pub struct ScriptOutcome {
    pub value: Dynamic,
}

pub struct ScriptContext {
    pub name: String,
    pub source: ScriptSource,
    pub args: Vec<String>,
    pub unit_args: Map,
    pub cwd: ScriptCwd,
    pub env: EnvOverlay,
    pub cancel: CancelToken,
    pub sink: Arc<Mutex<dyn EventSink>>,
    pub exec_id: ExecId,
    pub depth: usize,
    pub rollback: Arc<Mutex<RollbackStack>>,
    pub cleanup: Arc<Mutex<CleanupStack>>,
    pub poll_ms: u64,
    pub termination_timeout_ms: u64,
    pub processes: ProcessTracker,
    /// Actions/Workflows must not import; inline Actions may import from `lib_dirs`.
    pub allow_imports: bool,
    /// Optional module search paths when imports are allowed.
    pub lib_dirs: Vec<PathBuf>,
    /// Extra native functions (e.g. execution::run_action) registered by the caller.
    pub extra_register: Option<Box<dyn FnOnce(&mut Engine) + Send>>,
}

pub fn eval_script(ctx: ScriptContext) -> Result<ScriptOutcome, ScriptError> {
    let id = ctx.exec_id.0.to_string();
    let display_name = ctx.name.clone();
    let source_label = ctx.source.label();

    {
        let mut sink = ctx.sink.lock().unwrap();
        sink.emit(ExecutionEvent::RhaiStarted {
            id: id.clone(),
            parent: None,
            name: display_name.clone(),
            source: Some(source_label.clone()),
            depth: ctx.depth,
        });
    }

    let raw = match &ctx.source {
        ScriptSource::File { path } => {
            std::fs::read_to_string(path).map_err(|e| ScriptError::Io(e.to_string()))?
        }
        ScriptSource::Inline { source } => source.clone(),
    };

    if ctx.cancel.requested() {
        finish(
            &ctx.sink,
            &id,
            &display_name,
            ExecutionState::Cancelled,
            ctx.depth,
        );
        return Err(ScriptError::Cancelled);
    }

    if !ctx.allow_imports {
        if let Some(line) = first_import_line(&raw) {
            let msg = format!(
                "imports are not allowed in Actions/Workflows (found `import` at line {line}); \
                 compose via execution::run_action / execution::run_workflow / execution::run instead"
            );
            {
                let mut sink = ctx.sink.lock().unwrap();
                sink.emit(ExecutionEvent::LogError {
                    id: id.clone(),
                    message: msg.clone(),
                    depth: ctx.depth,
                });
            }
            finish(
                &ctx.sink,
                &id,
                &display_name,
                ExecutionState::Failed,
                ctx.depth,
            );
            return Err(ScriptError::Runtime(msg));
        }
    }

    let rewritten = rewrite_module_calls(&raw);
    let mut engine = Engine::new();
    // Allow larger scripts
    engine.set_max_expr_depths(64, 64);

    if ctx.allow_imports {
        if let Some(dir) = ctx.lib_dirs.first() {
            let resolver =
                rhai::module_resolvers::FileModuleResolver::new_with_path(dir.clone());
            engine.set_module_resolver(resolver);
        }
    } else {
        engine.set_module_resolver(DummyModuleResolver::new());
    }

    let handles = RuntimeHandles {
        cwd: ctx.cwd.clone(),
        env: ctx.env.clone(),
        cancel: ctx.cancel.clone(),
        sink: Arc::clone(&ctx.sink),
        exec_id: ctx.exec_id,
        depth: ctx.depth,
        rollback: Arc::clone(&ctx.rollback),
        cleanup: Arc::clone(&ctx.cleanup),
        poll_ms: ctx.poll_ms,
        termination_timeout_ms: ctx.termination_timeout_ms,
        processes: ctx.processes.clone(),
        args: ctx.args.clone(),
        unit_args: ctx.unit_args.clone(),
    };
    register_runtime_modules(&mut engine, handles);

    if let Some(extra) = ctx.extra_register {
        extra(&mut engine);
    }

    let mut scope = Scope::new();
    let args_array: rhai::Array = ctx
        .args
        .iter()
        .map(|s| Dynamic::from(s.clone()))
        .collect();
    scope.push("args", args_array);
    scope.push("unit_args", ctx.unit_args);

    let result = engine.eval_with_scope::<Dynamic>(&mut scope, &rewritten);

    // Always run cleanup for this script scope
    ctx.cleanup.lock().unwrap().run();

    match result {
        Ok(value) => {
            if ctx.cancel.requested() {
                finish(
                    &ctx.sink,
                    &id,
                    &display_name,
                    ExecutionState::Cancelled,
                    ctx.depth,
                );
                Err(ScriptError::Cancelled)
            } else {
                finish(
                    &ctx.sink,
                    &id,
                    &display_name,
                    ExecutionState::Succeeded,
                    ctx.depth,
                );
                Ok(ScriptOutcome { value })
            }
        }
        Err(e) => {
            let msg = e.to_string();
            let cancelled = ctx.cancel.requested() || msg.contains("cancellation");
            let state = if cancelled {
                ExecutionState::Cancelled
            } else {
                ExecutionState::Failed
            };
            {
                let mut sink = ctx.sink.lock().unwrap();
                sink.emit(ExecutionEvent::LogError {
                    id: id.clone(),
                    message: msg.clone(),
                    depth: ctx.depth,
                });
            }
            finish(&ctx.sink, &id, &display_name, state, ctx.depth);
            if cancelled {
                Err(ScriptError::Cancelled)
            } else {
                Err(ScriptError::Runtime(msg))
            }
        }
    }
}

fn first_import_line(source: &str) -> Option<usize> {
    for (i, line) in source.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("import ") || t.starts_with("import\t") {
            return Some(i + 1);
        }
    }
    None
}

fn finish(
    sink: &Arc<Mutex<dyn EventSink>>,
    id: &str,
    name: &str,
    state: ExecutionState,
    depth: usize,
) {
    sink.lock().unwrap().emit(ExecutionEvent::RhaiFinished {
        id: id.to_string(),
        name: name.to_string(),
        state,
        depth,
    });
}
