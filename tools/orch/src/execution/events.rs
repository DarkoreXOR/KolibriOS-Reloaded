//! Structured execution events.

use serde::Serialize;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use super::tree::{ExecId, NodeKind};
use crate::registry::EntityType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
}

impl ExecutionState {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Succeeded => "OK",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Skipped => "SKIPPED",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionEvent {
    WorkflowStarted {
        id: String,
        parent: Option<String>,
        name: String,
        depth: usize,
    },
    WorkflowFinished {
        id: String,
        name: String,
        state: ExecutionState,
        depth: usize,
    },
    ActionStarted {
        id: String,
        parent: Option<String>,
        name: String,
        depth: usize,
    },
    ActionFinished {
        id: String,
        name: String,
        state: ExecutionState,
        depth: usize,
    },
    /// Rhai body started (nested under Action / Workflow / inline Action).
    RhaiStarted {
        id: String,
        parent: Option<String>,
        name: String,
        source: Option<String>,
        depth: usize,
    },
    RhaiFinished {
        id: String,
        name: String,
        state: ExecutionState,
        depth: usize,
    },
    LogInfo {
        id: String,
        message: String,
        depth: usize,
    },
    LogWarn {
        id: String,
        message: String,
        depth: usize,
    },
    LogError {
        id: String,
        message: String,
        depth: usize,
    },
    ProcessStarted {
        id: String,
        parent: Option<String>,
        program: String,
        depth: usize,
    },
    ProcessFinished {
        id: String,
        program: String,
        state: ExecutionState,
        exit_code: Option<i32>,
        depth: usize,
    },
    CancellationRequested {
        message: String,
    },
    CleanupStarted {
        id: String,
    },
    CleanupFinished {
        id: String,
        state: ExecutionState,
    },
    RollbackStarted,
    RollbackActionStarted {
        id: String,
        name: String,
    },
    RollbackActionFinished {
        id: String,
        name: String,
        state: ExecutionState,
    },
    RollbackFinished {
        state: ExecutionState,
    },
    Error {
        id: Option<String>,
        message: String,
    },
    Skipped {
        id: String,
        kind: String,
        name: String,
        reason: String,
    },
}

impl ExecutionEvent {
    pub fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }
}

pub trait EventSink: Send {
    fn emit(&mut self, event: ExecutionEvent);
}

pub struct MultiSink {
    pub sinks: Vec<Box<dyn EventSink>>,
}

impl EventSink for MultiSink {
    fn emit(&mut self, event: ExecutionEvent) {
        for s in &mut self.sinks {
            s.emit(event.clone());
        }
    }
}

pub struct JsonSink {
    quiet: bool,
}

impl JsonSink {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl EventSink for JsonSink {
    fn emit(&mut self, event: ExecutionEvent) {
        if self.quiet {
            match &event {
                ExecutionEvent::LogInfo { .. } | ExecutionEvent::ProcessStarted { .. } => return,
                _ => {}
            }
        }
        let mut out = io::stdout().lock();
        if let Ok(line) = serde_json::to_string(&event) {
            let _ = writeln!(out, "{line}");
        }
    }
}

pub fn entity_type_of(kind: NodeKind) -> Option<EntityType> {
    match kind {
        NodeKind::Workflow => Some(EntityType::Workflow),
        NodeKind::Action | NodeKind::InlineAction => Some(EntityType::Action),
        _ => None,
    }
}

pub fn id_str(id: ExecId) -> String {
    id.0.to_string()
}
