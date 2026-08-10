//! Execution tree, events, engine, rollback, and rendering.

pub mod engine;
pub mod events;
pub mod renderer;
pub mod rollback;
pub mod tree;

pub use engine::{execute_invocation, ExecutionError, PreflightError};
pub use events::{EventSink, ExecutionEvent, ExecutionState, JsonSink, MultiSink};
pub use renderer::StdoutRenderer;
pub use rollback::{CleanupStack, RollbackStack, RollbackSummary};
pub use tree::{ClaimResult, ExecId, ExecutionNode, ExecutionTree, NodeKind};

use crate::cli::GlobalOptions;
use std::sync::{Arc, Mutex};

pub fn make_sink(globals: &GlobalOptions) -> Arc<Mutex<dyn EventSink>> {
    if globals.json {
        Arc::new(Mutex::new(JsonSink::new(globals.quiet)))
    } else {
        Arc::new(Mutex::new(StdoutRenderer::new(
            globals.quiet,
            globals.verbose,
            globals.no_progress,
        )))
    }
}
