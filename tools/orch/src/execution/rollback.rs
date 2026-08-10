//! Rollback / compensation stack (LIFO, optional, best-effort).

use crate::execution::events::{EventSink, ExecutionEvent, ExecutionState};
use crate::execution::tree::ExecId;
use std::sync::{Arc, Mutex};

pub type RollbackFn = Box<dyn FnMut() -> Result<(), String> + Send>;

pub struct RollbackEntry {
    pub action_name: String,
    pub exec_id: ExecId,
    pub handler: Option<RollbackFn>,
}

#[derive(Default)]
pub struct RollbackStack {
    /// Completed reversible scopes (LIFO).
    entries: Vec<RollbackEntry>,
    /// Handlers registered during the currently running action/script.
    pending: Vec<RollbackFn>,
    in_rollback: bool,
}

impl RollbackStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_entries(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn in_rollback(&self) -> bool {
        self.in_rollback
    }

    /// Register dynamic compensation for the current scope.
    pub fn on_rollback(&mut self, f: RollbackFn) -> Result<(), String> {
        if self.in_rollback {
            return Err("cannot register rollback handlers during the rollback phase".into());
        }
        self.pending.push(f);
        Ok(())
    }

    /// Called when an action completes successfully: seal pending handlers.
    pub fn commit_action(&mut self, action_name: String, exec_id: ExecId) {
        let handlers = std::mem::take(&mut self.pending);
        if handlers.is_empty() {
            self.entries.push(RollbackEntry {
                action_name,
                exec_id,
                handler: None,
            });
        } else {
            // Compose multiple handlers into one LIFO chain (reverse of registration
            // within the action, then actions themselves are LIFO).
            let mut composed: Vec<RollbackFn> = handlers;
            self.entries.push(RollbackEntry {
                action_name,
                exec_id,
                handler: Some(Box::new(move || {
                    let mut first_err = None;
                    while let Some(mut h) = composed.pop() {
                        if let Err(e) = h() {
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                    match first_err {
                        Some(e) => Err(e),
                        None => Ok(()),
                    }
                })),
            });
        }
    }

    /// Discard pending handlers for a failed action (cleanup handles resources).
    pub fn discard_pending(&mut self) {
        self.pending.clear();
    }

    pub fn run_rollback(&mut self, sink: &Arc<Mutex<dyn EventSink>>) -> RollbackSummary {
        self.in_rollback = true;
        {
            let mut s = sink.lock().unwrap();
            s.emit(ExecutionEvent::RollbackStarted);
        }

        let mut summary = RollbackSummary::default();
        while let Some(mut entry) = self.entries.pop() {
            let id = entry.exec_id.0.to_string();
            {
                let mut s = sink.lock().unwrap();
                s.emit(ExecutionEvent::RollbackActionStarted {
                    id: id.clone(),
                    name: entry.action_name.clone(),
                });
            }

            let state = match &mut entry.handler {
                None => {
                    let mut s = sink.lock().unwrap();
                    s.emit(ExecutionEvent::LogWarn {
                        id: id.clone(),
                        message: format!(
                            "Action '@{}' has no rollback handler",
                            entry.action_name
                        ),
                        depth: 1,
                    });
                    summary.unavailable.push(entry.action_name.clone());
                    ExecutionState::Succeeded
                }
                Some(handler) => match handler() {
                    Ok(()) => {
                        summary.successful.push(entry.action_name.clone());
                        ExecutionState::Succeeded
                    }
                    Err(e) => {
                        let mut s = sink.lock().unwrap();
                        s.emit(ExecutionEvent::LogError {
                            id: id.clone(),
                            message: format!(
                                "Rollback of action '@{}' failed: {e}",
                                entry.action_name
                            ),
                        depth: 1,
                        });
                        summary.failed.push(entry.action_name.clone());
                        ExecutionState::Failed
                    }
                },
            };

            {
                let mut s = sink.lock().unwrap();
                s.emit(ExecutionEvent::RollbackActionFinished {
                    id,
                    name: entry.action_name,
                    state,
                });
            }
        }

        let final_state = if summary.failed.is_empty() {
            ExecutionState::Succeeded
        } else {
            ExecutionState::Failed
        };
        {
            let mut s = sink.lock().unwrap();
            s.emit(ExecutionEvent::RollbackFinished {
                state: final_state,
            });
            if !summary.failed.is_empty() || !summary.unavailable.is_empty() {
                if !summary.successful.is_empty() {
                    s.emit(ExecutionEvent::LogInfo {
                        id: "rollback".into(),
                        message: format!("Successful: {}", summary.successful.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(", ")),
                        depth: 0,
                    });
                }
                if !summary.unavailable.is_empty() {
                    s.emit(ExecutionEvent::LogWarn {
                        id: "rollback".into(),
                        message: format!("Unavailable: {}", summary.unavailable.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(", ")),
                        depth: 0,
                    });
                }
                if !summary.failed.is_empty() {
                    s.emit(ExecutionEvent::LogError {
                        id: "rollback".into(),
                        message: format!("Failed: {}", summary.failed.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(", ")),
                        depth: 0,
                    });
                }
            }
        }
        self.in_rollback = false;
        summary
    }
}

#[derive(Debug, Default)]
pub struct RollbackSummary {
    pub successful: Vec<String>,
    pub unavailable: Vec<String>,
    pub failed: Vec<String>,
}

impl RollbackSummary {
    pub fn had_failures(&self) -> bool {
        !self.failed.is_empty()
    }
}

/// Cleanup handlers (always run; distinct from rollback).
#[derive(Default)]
pub struct CleanupStack {
    handlers: Vec<Box<dyn FnMut() + Send>>,
}

impl CleanupStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_cleanup(&mut self, f: Box<dyn FnMut() + Send>) {
        self.handlers.push(f);
    }

    pub fn run(&mut self) {
        while let Some(mut h) = self.handlers.pop() {
            h();
        }
    }
}
