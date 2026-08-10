//! Preflight validation and execution engine.

use crate::cli::{normalize_inline_units, ArgValue, ExecutionUnit, UnitKind};
use crate::cli::{GlobalOptions, Invocation};
use crate::execution::events::{EventSink, ExecutionEvent, ExecutionState};
use crate::execution::make_sink;
use crate::execution::rollback::{CleanupStack, RollbackStack, RollbackSummary};
use crate::execution::tree::{ClaimResult, ExecId, ExecutionTree, NodeKind};
use crate::exit_codes::ExitStatus;
use crate::registry::{discover_all, EntityType, Registry};
use crate::runtime::cancel::{install_ctrlc, CancelToken};
use crate::runtime::env::EnvOverlay;
use crate::runtime::path::ScriptCwd;
use crate::runtime::process::ProcessTracker;
use crate::script::{eval_script, ScriptContext, ScriptError, ScriptSource};
use rhai::{Dynamic, Engine, EvalAltResult, Map};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum PreflightError {
    Message(String),
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for PreflightError {}

#[derive(Debug)]
pub enum ExecutionError {
    Preflight(PreflightError),
    Failed(String),
    Cancelled,
    RollbackFailed(String),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preflight(e) => write!(f, "{e}"),
            Self::Failed(m) | Self::RollbackFailed(m) => write!(f, "{m}"),
            Self::Cancelled => write!(f, "cancellation requested"),
        }
    }
}

impl std::error::Error for ExecutionError {}

impl ExecutionError {
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Self::Preflight(_) => ExitStatus::ValidationFailure,
            Self::Failed(_) => ExitStatus::ExecutionFailure,
            Self::Cancelled => ExitStatus::Cancellation,
            Self::RollbackFailed(_) => ExitStatus::RollbackFailure,
        }
    }
}

struct EngineState {
    registry: Registry,
    tree: Arc<Mutex<ExecutionTree>>,
    sink: Arc<Mutex<dyn EventSink>>,
    cancel: CancelToken,
    cwd: ScriptCwd,
    env: EnvOverlay,
    rollback: Arc<Mutex<RollbackStack>>,
    poll_ms: u64,
    termination_timeout_ms: u64,
    processes: ProcessTracker,
    lib_dirs: Vec<std::path::PathBuf>,
    root: std::path::PathBuf,
    cancel_announced: bool,
}

impl EngineState {
    fn announce_cancel(&mut self) {
        if !self.cancel_announced {
            self.cancel_announced = true;
            self.sink
                .lock()
                .unwrap()
                .emit(ExecutionEvent::CancellationRequested {
                    message: "cancellation requested".into(),
                });
            self.processes.kill_all(self.termination_timeout_ms);
        }
    }

    fn lib_dirs_resolved(&self) -> Vec<std::path::PathBuf> {
        self.lib_dirs.clone()
    }
}

pub fn execute_invocation(mut inv: Invocation) -> Result<(), ExecutionError> {
    normalize_inline_units(&mut inv.units)
        .map_err(|e| ExecutionError::Preflight(PreflightError::Message(e.to_string())))?;

    let registry = discover_all(&inv.root, &inv.config).map_err(|e| {
        ExecutionError::Preflight(PreflightError::Message(format!(
            "error: registry discovery failed: {e}"
        )))
    })?;

    preflight(&registry, &inv.units)?;

    let sink = make_sink(&inv.globals);
    let cancel = CancelToken::new();
    let _ = install_ctrlc(cancel.clone());

    let lib_dirs: Vec<_> = inv
        .config
        .lib_dirs
        .iter()
        .map(|d| inv.config.resolve(&inv.root, d))
        .collect();

    let mut state = EngineState {
        registry,
        tree: Arc::new(Mutex::new(ExecutionTree::new())),
        sink,
        cancel,
        cwd: ScriptCwd::from_invocation(),
        env: EnvOverlay::new(),
        rollback: Arc::new(Mutex::new(RollbackStack::new())),
        poll_ms: inv.config.timer_poll_ms,
        termination_timeout_ms: inv.config.process_termination_timeout_ms,
        processes: ProcessTracker::new(),
        lib_dirs,
        root: inv.root.clone(),
        cancel_announced: false,
    };
    let _ = &state.root;

    let root_id = state.tree.lock().unwrap().ensure_root();
    let mut last_err: Option<ExecutionError> = None;

    for unit in &inv.units {
        if state.cancel.requested() {
            state.announce_cancel();
            last_err = Some(ExecutionError::Cancelled);
            break;
        }
        match run_unit(&mut state, root_id, unit) {
            Ok(()) => {}
            Err(e) => {
                if matches!(e, ExecutionError::Cancelled) {
                    state.announce_cancel();
                }
                last_err = Some(e);
                break;
            }
        }
    }

    let need_rollback = matches!(
        last_err,
        Some(ExecutionError::Failed(_)) | Some(ExecutionError::Cancelled)
    );
    let mut rollback_summary = RollbackSummary::default();
    if need_rollback {
        let has_entries = { state.rollback.lock().unwrap().has_entries() };
        if has_entries {
            rollback_summary = state.rollback.lock().unwrap().run_rollback(&state.sink);
        }
    }

    if let Some(err) = last_err {
        if rollback_summary.had_failures() {
            return Err(ExecutionError::RollbackFailed(format!(
                "{err}; rollback completed with errors"
            )));
        }
        return Err(err);
    }
    Ok(())
}

fn preflight(registry: &Registry, units: &[ExecutionUnit]) -> Result<(), ExecutionError> {
    let mut missing = Vec::new();
    for unit in units {
        match unit.kind {
            UnitKind::InlineAction => {
                if unit.name.trim().is_empty() {
                    missing.push("error: inline Action `$` has empty source".to_string());
                }
            }
            UnitKind::Action => {
                if registry.get(EntityType::Action, &unit.name).is_none() {
                    missing.push(format!(
                        "Action not found: @{} (CLI token: {})",
                        unit.name, unit.cli_token
                    ));
                }
            }
            UnitKind::Workflow => {
                if registry.get(EntityType::Workflow, &unit.name).is_none() {
                    missing.push(format!(
                        "Workflow not found: {} (CLI token: {})",
                        unit.name, unit.cli_token
                    ));
                }
            }
        }
    }
    if !missing.is_empty() {
        return Err(ExecutionError::Preflight(PreflightError::Message(
            missing.join("\n"),
        )));
    }

    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for entry in registry.actions().chain(registry.workflows()) {
        let key = format!("{}{}", entry.entity_type.prefix(), entry.name);
        let path = match &entry.source {
            crate::registry::EntitySource::File { path, .. } => path,
            _ => continue,
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let refs = extract_refs(&text);
        graph.insert(key, refs);
    }
    if let Some(cycle) = find_cycle(&graph) {
        return Err(ExecutionError::Preflight(PreflightError::Message(format!(
            "error: dependency cycle detected: {cycle}"
        ))));
    }
    Ok(())
}

fn extract_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        for (prefix, kind_prefix) in [
            ("execution::run_action(", "@"),
            ("execution__run_action(", "@"),
            ("execution::run_workflow(", ""),
            ("execution__run_workflow(", ""),
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                if let Some(name) = parse_string_literal(rest) {
                    refs.push(format!("{kind_prefix}{name}"));
                }
            }
        }
        // execution::run("@name") / execution::run("workflow")
        for prefix in ["execution::run(", "execution__run("] {
            if let Some(rest) = line.strip_prefix(prefix) {
                if let Some(token) = parse_string_literal(rest) {
                    if let Some(name) = token.strip_prefix('@') {
                        refs.push(format!("@{name}"));
                    } else {
                        refs.push(token);
                    }
                }
            }
        }
    }
    refs
}

fn parse_string_literal(s: &str) -> Option<String> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn find_cycle(graph: &HashMap<String, Vec<String>>) -> Option<String> {
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut stack = Vec::new();

    fn dfs(
        node: &str,
        graph: &HashMap<String, Vec<String>>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<String> {
        if visited.contains(node) {
            return None;
        }
        if !visiting.insert(node.to_string()) {
            let idx = stack.iter().position(|n| n == node).unwrap_or(0);
            let mut cycle = stack[idx..].to_vec();
            cycle.push(node.to_string());
            return Some(cycle.join(" -> "));
        }
        stack.push(node.to_string());
        if let Some(edges) = graph.get(node) {
            for e in edges {
                if let Some(c) = dfs(e, graph, visiting, visited, stack) {
                    return Some(c);
                }
            }
        }
        stack.pop();
        visiting.remove(node);
        visited.insert(node.to_string());
        None
    }

    for key in graph.keys() {
        if let Some(c) = dfs(key, graph, &mut visiting, &mut visited, &mut stack) {
            return Some(c);
        }
    }
    None
}

fn run_unit(state: &mut EngineState, parent: ExecId, unit: &ExecutionUnit) -> Result<(), ExecutionError> {
    match unit.kind {
        UnitKind::InlineAction => run_inline(state, parent, unit),
        UnitKind::Action => run_action(
            state,
            parent,
            &unit.name,
            &unit.positionals,
            &unit.args,
            &unit.cli_token,
        ),
        UnitKind::Workflow => {
            run_workflow(state, parent, &unit.name, &unit.args, &unit.cli_token)
        }
    }
}

fn logical_key(kind: &str, name: &str, args: &[ArgValue], positionals: &[String]) -> String {
    let mut parts = vec![kind.to_string(), name.to_string()];
    for p in positionals {
        parts.push(format!("pos={p}"));
    }
    let mut args_sorted = args.to_vec();
    args_sorted.sort_by(|a, b| a.key.cmp(&b.key));
    for a in args_sorted {
        match &a.value {
            Some(v) => parts.push(format!("{}={v}", a.key)),
            None => parts.push(format!("{}=true", a.key)),
        }
    }
    parts.join("|")
}

fn claim_key(tree: &Arc<Mutex<ExecutionTree>>, logical_key: &str) -> bool {
    let mut tree = tree.lock().unwrap();
    matches!(tree.claim_or_existing(logical_key), ClaimResult::First)
}

fn args_to_map(args: &[ArgValue]) -> Map {
    let mut map = Map::new();
    for a in args {
        match &a.value {
            Some(v) => {
                map.insert(a.key.clone().into(), Dynamic::from(v.clone()));
            }
            None => {
                map.insert(a.key.clone().into(), Dynamic::from(true));
            }
        }
    }
    map
}

fn run_inline(
    state: &mut EngineState,
    parent: ExecId,
    unit: &ExecutionUnit,
) -> Result<(), ExecutionError> {
    let key = logical_key("inline", &unit.name, &unit.args, &unit.positionals);
    if !claim_key(&state.tree, &key) {
        emit_skip(state, "inline", "<inline>", "already executed in this graph");
        return Ok(());
    }
    let (id, depth) = {
        let mut tree = state.tree.lock().unwrap();
        let id = tree.add_child(
            parent,
            NodeKind::InlineAction,
            "<inline>",
            key.clone(),
            Some("<inline>".into()),
        );
        tree.mark_seen(key, id);
        tree.set_state(id, ExecutionState::Running);
        (id, tree.depth(id))
    };

    {
        let mut sink = state.sink.lock().unwrap();
        sink.emit(ExecutionEvent::ActionStarted {
            id: id.0.to_string(),
            parent: Some(parent.0.to_string()),
            name: "<inline>".into(),
            depth,
        });
    }

    let cleanup = Arc::new(Mutex::new(CleanupStack::new()));
    let nested = make_nested_register(state, id);

    // Inline Actions share the Action runtime surface; imports allowed for ad-hoc agent use.
    let ctx = ScriptContext {
        name: "<inline>".into(),
        source: ScriptSource::Inline {
            source: unit.name.clone(),
        },
        args: unit.positionals.clone(),
        unit_args: args_to_map(&unit.args),
        cwd: state.cwd.clone(),
        env: state.env.clone(),
        cancel: state.cancel.clone(),
        sink: Arc::clone(&state.sink),
        exec_id: id,
        depth,
        rollback: Arc::clone(&state.rollback),
        cleanup: Arc::clone(&cleanup),
        poll_ms: state.poll_ms,
        termination_timeout_ms: state.termination_timeout_ms,
        processes: state.processes.clone(),
        allow_imports: true,
        lib_dirs: state.lib_dirs_resolved(),
        extra_register: Some(nested),
    };

    let result = eval_script(ctx);
    cleanup.lock().unwrap().run();

    match result {
        Ok(_) => {
            state.tree.lock().unwrap().set_state(id, ExecutionState::Succeeded);
            state.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                id: id.0.to_string(),
                name: "<inline>".into(),
                state: ExecutionState::Succeeded,
                depth,
            });
            Ok(())
        }
        Err(ScriptError::Cancelled) => {
            state.announce_cancel();
            state.tree.lock().unwrap().set_state(id, ExecutionState::Cancelled);
            state.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                id: id.0.to_string(),
                name: "<inline>".into(),
                state: ExecutionState::Cancelled,
                depth,
            });
            Err(ExecutionError::Cancelled)
        }
        Err(e) => {
            state.tree.lock().unwrap().set_state(id, ExecutionState::Failed);
            state.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                id: id.0.to_string(),
                name: "<inline>".into(),
                state: ExecutionState::Failed,
                depth,
            });
            Err(ExecutionError::Failed(format!(
                "inline Action failed: {e}"
            )))
        }
    }
}

fn run_action(
    state: &mut EngineState,
    parent: ExecId,
    name: &str,
    positionals: &[String],
    args: &[ArgValue],
    cli_token: &str,
) -> Result<(), ExecutionError> {
    let key = logical_key("action", name, args, positionals);
    if !claim_key(&state.tree, &key) {
        emit_skip(state, "action", name, "already executed in this graph");
        return Ok(());
    }

    let entry = state
        .registry
        .require(EntityType::Action, name, Some(cli_token))
        .map_err(|e| ExecutionError::Failed(e.to_string()))?;
    let path = match &entry.source {
        crate::registry::EntitySource::File { path, .. } => path.clone(),
        _ => {
            return Err(ExecutionError::Failed(format!(
                "action '@{name}' has no file source"
            )))
        }
    };

    let (id, depth) = {
        let mut tree = state.tree.lock().unwrap();
        let id = tree.add_child(
            parent,
            NodeKind::Action,
            name,
            key.clone(),
            Some(path.display().to_string()),
        );
        tree.mark_seen(key, id);
        tree.set_state(id, ExecutionState::Running);
        (id, tree.depth(id))
    };

    {
        let mut sink = state.sink.lock().unwrap();
        sink.emit(ExecutionEvent::ActionStarted {
            id: id.0.to_string(),
            parent: Some(parent.0.to_string()),
            name: name.to_string(),
            depth,
        });
    }

    let cleanup = Arc::new(Mutex::new(CleanupStack::new()));
    let nested = make_nested_register(state, id);

    let ctx = ScriptContext {
        name: name.to_string(),
        source: ScriptSource::File { path },
        args: positionals.to_vec(),
        unit_args: args_to_map(args),
        cwd: state.cwd.clone(),
        env: state.env.clone(),
        cancel: state.cancel.clone(),
        sink: Arc::clone(&state.sink),
        exec_id: id,
        depth,
        rollback: Arc::clone(&state.rollback),
        cleanup: Arc::clone(&cleanup),
        poll_ms: state.poll_ms,
        termination_timeout_ms: state.termination_timeout_ms,
        processes: state.processes.clone(),
        allow_imports: false,
        lib_dirs: Vec::new(),
        extra_register: Some(nested),
    };

    let result = eval_script(ctx);
    cleanup.lock().unwrap().run();

    match result {
        Ok(_) => {
            state
                .rollback
                .lock()
                .unwrap()
                .commit_action(name.to_string(), id);
            state.tree.lock().unwrap().set_state(id, ExecutionState::Succeeded);
            state.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                id: id.0.to_string(),
                name: name.to_string(),
                state: ExecutionState::Succeeded,
                depth,
            });
            Ok(())
        }
        Err(ScriptError::Cancelled) => {
            state.announce_cancel();
            state.rollback.lock().unwrap().discard_pending();
            state.tree.lock().unwrap().set_state(id, ExecutionState::Cancelled);
            state.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                id: id.0.to_string(),
                name: name.to_string(),
                state: ExecutionState::Cancelled,
                depth,
            });
            Err(ExecutionError::Cancelled)
        }
        Err(e) => {
            state.rollback.lock().unwrap().discard_pending();
            state.tree.lock().unwrap().set_state(id, ExecutionState::Failed);
            state.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                id: id.0.to_string(),
                name: name.to_string(),
                state: ExecutionState::Failed,
                depth,
            });
            Err(ExecutionError::Failed(format!(
                "action '@{name}' failed: {e}"
            )))
        }
    }
}

fn run_workflow(
    state: &mut EngineState,
    parent: ExecId,
    name: &str,
    args: &[ArgValue],
    cli_token: &str,
) -> Result<(), ExecutionError> {
    let key = logical_key("workflow", name, args, &[]);
    if !claim_key(&state.tree, &key) {
        emit_skip(state, "workflow", name, "already executed in this graph");
        return Ok(());
    }

    let entry = state
        .registry
        .require(EntityType::Workflow, name, Some(cli_token))
        .map_err(|e| ExecutionError::Failed(e.to_string()))?;
    let path = match &entry.source {
        crate::registry::EntitySource::File { path, .. } => path.clone(),
        _ => {
            return Err(ExecutionError::Failed(format!(
                "workflow '{name}' has no file source"
            )))
        }
    };

    let (id, depth) = {
        let mut tree = state.tree.lock().unwrap();
        let id = tree.add_child(
            parent,
            NodeKind::Workflow,
            name,
            key.clone(),
            Some(path.display().to_string()),
        );
        tree.mark_seen(key, id);
        tree.set_state(id, ExecutionState::Running);
        (id, tree.depth(id))
    };

    {
        let mut sink = state.sink.lock().unwrap();
        sink.emit(ExecutionEvent::WorkflowStarted {
            id: id.0.to_string(),
            parent: Some(parent.0.to_string()),
            name: name.to_string(),
            depth,
        });
    }

    let cleanup = Arc::new(Mutex::new(CleanupStack::new()));
    let nested = make_nested_register(state, id);

    let ctx = ScriptContext {
        name: name.to_string(),
        source: ScriptSource::File { path },
        args: Vec::new(),
        unit_args: args_to_map(args),
        cwd: state.cwd.clone(),
        env: state.env.clone(),
        cancel: state.cancel.clone(),
        sink: Arc::clone(&state.sink),
        exec_id: id,
        depth,
        rollback: Arc::clone(&state.rollback),
        cleanup,
        poll_ms: state.poll_ms,
        termination_timeout_ms: state.termination_timeout_ms,
        processes: state.processes.clone(),
        allow_imports: false,
        lib_dirs: Vec::new(),
        extra_register: Some(nested),
    };

    match eval_script(ctx) {
        Ok(_) => {
            state.tree.lock().unwrap().set_state(id, ExecutionState::Succeeded);
            state.sink.lock().unwrap().emit(ExecutionEvent::WorkflowFinished {
                id: id.0.to_string(),
                name: name.to_string(),
                state: ExecutionState::Succeeded,
                depth,
            });
            Ok(())
        }
        Err(ScriptError::Cancelled) => {
            state.announce_cancel();
            state.tree.lock().unwrap().set_state(id, ExecutionState::Cancelled);
            state.sink.lock().unwrap().emit(ExecutionEvent::WorkflowFinished {
                id: id.0.to_string(),
                name: name.to_string(),
                state: ExecutionState::Cancelled,
                depth,
            });
            Err(ExecutionError::Cancelled)
        }
        Err(e) => {
            state.tree.lock().unwrap().set_state(id, ExecutionState::Failed);
            state.sink.lock().unwrap().emit(ExecutionEvent::WorkflowFinished {
                id: id.0.to_string(),
                name: name.to_string(),
                state: ExecutionState::Failed,
                depth,
            });
            Err(ExecutionError::Failed(format!(
                "workflow '{name}' failed: {e}"
            )))
        }
    }
}

fn emit_skip(state: &EngineState, kind: &str, name: &str, reason: &str) {
    state.sink.lock().unwrap().emit(ExecutionEvent::Skipped {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.into(),
        name: name.into(),
        reason: reason.into(),
    });
}

/// Register nested execution::run_action / run_workflow / run.
fn make_nested_register(
    state: &EngineState,
    parent: ExecId,
) -> Box<dyn FnOnce(&mut Engine) + Send> {
    let shared = NestedShared {
        registry: snapshot_registry_paths(&state.registry),
        sink: Arc::clone(&state.sink),
        cancel: state.cancel.clone(),
        cwd: state.cwd.clone(),
        env: state.env.clone(),
        rollback: Arc::clone(&state.rollback),
        poll_ms: state.poll_ms,
        termination_timeout_ms: state.termination_timeout_ms,
        processes: state.processes.clone(),
        parent,
        tree: Arc::clone(&state.tree),
    };

    Box::new(move |engine: &mut Engine| {
        register_nested_fns(engine, shared);
    })
}

fn register_nested_fns(engine: &mut Engine, shared: NestedShared) {
    // Must return Result<_, Box<EvalAltResult>> so nested failures abort the caller.
    let s1 = shared.clone();
    engine.register_fn(
        "execution__run_action",
        move |name: &str| -> Result<(), Box<EvalAltResult>> {
            s1.run_action(name, &[], &[]).map_err(|e| e.into())
        },
    );
    let s2 = shared.clone();
    engine.register_fn(
        "execution__run_action",
        move |name: &str, args: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let positionals = dyn_to_strings(args).unwrap_or_default();
            s2.run_action(name, &positionals, &[])
                .map_err(|e| e.into())
        },
    );
    let s3 = shared.clone();
    engine.register_fn(
        "execution__run_action_args",
        move |name: &str, args: Map| -> Result<(), Box<EvalAltResult>> {
            let converted = map_to_args(&args);
            s3.run_action(name, &[], &converted)
                .map_err(|e| e.into())
        },
    );
    let s4 = shared.clone();
    engine.register_fn(
        "execution__run_workflow",
        move |name: &str| -> Result<(), Box<EvalAltResult>> {
            s4.run_workflow(name, &[]).map_err(|e| e.into())
        },
    );
    let s5 = shared.clone();
    engine.register_fn(
        "execution__run",
        move |token: &str| -> Result<(), Box<EvalAltResult>> {
            s5.run_token(token).map_err(|e| e.into())
        },
    );
}

#[derive(Clone)]
struct NestedShared {
    registry: HashMap<(EntityType, String), std::path::PathBuf>,
    sink: Arc<Mutex<dyn EventSink>>,
    cancel: CancelToken,
    cwd: ScriptCwd,
    env: EnvOverlay,
    rollback: Arc<Mutex<RollbackStack>>,
    poll_ms: u64,
    termination_timeout_ms: u64,
    processes: ProcessTracker,
    parent: ExecId,
    tree: Arc<Mutex<ExecutionTree>>,
}

impl NestedShared {
    fn run_token(&self, token: &str) -> Result<(), String> {
        if let Some(name) = token.strip_prefix('@') {
            self.run_action(name, &[], &[])
        } else {
            self.run_workflow(token, &[])
        }
    }

    fn run_workflow(&self, name: &str, args: &[ArgValue]) -> Result<(), String> {
        let key = logical_key("workflow", name, args, &[]);
        if !claim_key(&self.tree, &key) {
            self.sink.lock().unwrap().emit(ExecutionEvent::Skipped {
                id: uuid::Uuid::new_v4().to_string(),
                kind: "workflow".into(),
                name: name.into(),
                reason: "already executed in this graph".into(),
            });
            return Ok(());
        }
        let path = self
            .registry
            .get(&(EntityType::Workflow, name.to_string()))
            .ok_or_else(|| format!("Workflow not found: {name}"))?
            .clone();
        let (id, depth) = {
            let mut tree = self.tree.lock().unwrap();
            let id = tree.add_child(
                self.parent,
                NodeKind::Workflow,
                name,
                key.clone(),
                Some(path.display().to_string()),
            );
            tree.mark_seen(key, id);
            tree.set_state(id, ExecutionState::Running);
            (id, tree.depth(id))
        };
        self.sink.lock().unwrap().emit(ExecutionEvent::WorkflowStarted {
            id: id.0.to_string(),
            parent: Some(self.parent.0.to_string()),
            name: name.to_string(),
            depth,
        });
        let cleanup = Arc::new(Mutex::new(CleanupStack::new()));
        let mut shared = self.clone();
        shared.parent = id;
        let ctx = ScriptContext {
            name: name.to_string(),
            source: ScriptSource::File { path },
            args: Vec::new(),
            unit_args: args_to_map(args),
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            cancel: self.cancel.clone(),
            sink: Arc::clone(&self.sink),
            exec_id: id,
            depth,
            rollback: Arc::clone(&self.rollback),
            cleanup,
            poll_ms: self.poll_ms,
            termination_timeout_ms: self.termination_timeout_ms,
            processes: self.processes.clone(),
            allow_imports: false,
            lib_dirs: Vec::new(),
            extra_register: Some(Box::new(move |engine| {
                register_nested_fns(engine, shared);
            })),
        };
        match eval_script(ctx) {
            Ok(_) => {
                self.tree.lock().unwrap().set_state(id, ExecutionState::Succeeded);
                self.sink.lock().unwrap().emit(ExecutionEvent::WorkflowFinished {
                    id: id.0.to_string(),
                    name: name.to_string(),
                    state: ExecutionState::Succeeded,
                    depth,
                });
                Ok(())
            }
            Err(ScriptError::Cancelled) => {
                self.tree.lock().unwrap().set_state(id, ExecutionState::Cancelled);
                self.sink.lock().unwrap().emit(ExecutionEvent::WorkflowFinished {
                    id: id.0.to_string(),
                    name: name.to_string(),
                    state: ExecutionState::Cancelled,
                    depth,
                });
                Err("cancellation requested".into())
            }
            Err(e) => {
                self.tree.lock().unwrap().set_state(id, ExecutionState::Failed);
                self.sink.lock().unwrap().emit(ExecutionEvent::WorkflowFinished {
                    id: id.0.to_string(),
                    name: name.to_string(),
                    state: ExecutionState::Failed,
                    depth,
                });
                Err(format!("workflow '{name}' failed: {e}"))
            }
        }
    }

    fn run_action(
        &self,
        name: &str,
        positionals: &[String],
        args: &[ArgValue],
    ) -> Result<(), String> {
        let key = logical_key("action", name, args, positionals);
        if !claim_key(&self.tree, &key) {
            self.sink.lock().unwrap().emit(ExecutionEvent::Skipped {
                id: uuid::Uuid::new_v4().to_string(),
                kind: "action".into(),
                name: name.into(),
                reason: "already executed in this graph".into(),
            });
            return Ok(());
        }
        let path = self
            .registry
            .get(&(EntityType::Action, name.to_string()))
            .ok_or_else(|| format!("Action not found: @{name}"))?
            .clone();
        let (id, depth) = {
            let mut tree = self.tree.lock().unwrap();
            let id = tree.add_child(
                self.parent,
                NodeKind::Action,
                name,
                key.clone(),
                Some(path.display().to_string()),
            );
            tree.mark_seen(key, id);
            tree.set_state(id, ExecutionState::Running);
            (id, tree.depth(id))
        };
        self.sink.lock().unwrap().emit(ExecutionEvent::ActionStarted {
            id: id.0.to_string(),
            parent: Some(self.parent.0.to_string()),
            name: name.to_string(),
            depth,
        });
        let cleanup = Arc::new(Mutex::new(CleanupStack::new()));
        let mut shared = self.clone();
        shared.parent = id;
        let ctx = ScriptContext {
            name: name.to_string(),
            source: ScriptSource::File { path },
            args: positionals.to_vec(),
            unit_args: args_to_map(args),
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            cancel: self.cancel.clone(),
            sink: Arc::clone(&self.sink),
            exec_id: id,
            depth,
            rollback: Arc::clone(&self.rollback),
            cleanup: Arc::clone(&cleanup),
            poll_ms: self.poll_ms,
            termination_timeout_ms: self.termination_timeout_ms,
            processes: self.processes.clone(),
            allow_imports: false,
            lib_dirs: Vec::new(),
            extra_register: Some(Box::new(move |engine| {
                register_nested_fns(engine, shared);
            })),
        };
        let result = eval_script(ctx);
        cleanup.lock().unwrap().run();
        match result {
            Ok(_) => {
                self.rollback
                    .lock()
                    .unwrap()
                    .commit_action(name.to_string(), id);
                self.tree.lock().unwrap().set_state(id, ExecutionState::Succeeded);
                self.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                    id: id.0.to_string(),
                    name: name.to_string(),
                    state: ExecutionState::Succeeded,
                    depth,
                });
                Ok(())
            }
            Err(ScriptError::Cancelled) => {
                self.rollback.lock().unwrap().discard_pending();
                self.tree.lock().unwrap().set_state(id, ExecutionState::Cancelled);
                self.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                    id: id.0.to_string(),
                    name: name.to_string(),
                    state: ExecutionState::Cancelled,
                    depth,
                });
                Err("cancellation requested".into())
            }
            Err(e) => {
                self.rollback.lock().unwrap().discard_pending();
                self.tree.lock().unwrap().set_state(id, ExecutionState::Failed);
                self.sink.lock().unwrap().emit(ExecutionEvent::ActionFinished {
                    id: id.0.to_string(),
                    name: name.to_string(),
                    state: ExecutionState::Failed,
                    depth,
                });
                Err(format!("action '@{name}' failed: {e}"))
            }
        }
    }
}

fn snapshot_registry_paths(
    reg: &Registry,
) -> HashMap<(EntityType, String), std::path::PathBuf> {
    let mut map = HashMap::new();
    for e in reg.actions() {
        if let crate::registry::EntitySource::File { path, .. } = &e.source {
            map.insert((EntityType::Action, e.name.clone()), path.clone());
        }
    }
    for e in reg.workflows() {
        if let crate::registry::EntitySource::File { path, .. } = &e.source {
            map.insert((EntityType::Workflow, e.name.clone()), path.clone());
        }
    }
    map
}

fn dyn_to_strings(args: Dynamic) -> Result<Vec<String>, String> {
    if args.is_array() {
        let arr = args.into_array().map_err(|e| e.to_string())?;
        Ok(arr.into_iter().map(|v| v.to_string()).collect())
    } else if args.is_unit() {
        Ok(vec![])
    } else {
        Ok(vec![args.to_string()])
    }
}

fn map_to_args(map: &Map) -> Vec<ArgValue> {
    let mut out = Vec::new();
    for (k, v) in map.iter() {
        let key = k.to_string();
        if v.is_bool() {
            if v.as_bool().unwrap_or(false) {
                out.push(ArgValue { key, value: None });
            }
        } else if v.is_unit() {
            out.push(ArgValue { key, value: None });
        } else {
            out.push(ArgValue {
                key,
                value: Some(v.to_string()),
            });
        }
    }
    out
}

/// Public helper for tests / library users.
pub fn run_with_globals(
    root: std::path::PathBuf,
    config: crate::OrchConfig,
    globals: GlobalOptions,
    units: Vec<ExecutionUnit>,
) -> Result<(), ExecutionError> {
    execute_invocation(Invocation {
        globals,
        units,
        config_path: crate::OrchConfig::default_path(),
        root,
        config,
    })
}
