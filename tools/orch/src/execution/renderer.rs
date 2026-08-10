//! Default hierarchical stdout renderer (English).
//!
//! Nesting prefixes come from execution-tree `depth` on each event —
//! not from string parsing or hard-coded kind→depth maps.

use super::events::{EventSink, ExecutionEvent, ExecutionState};
use std::io::{self, Write};

pub struct StdoutRenderer {
    quiet: bool,
    verbose: bool,
    no_progress: bool,
}

impl StdoutRenderer {
    pub fn new(quiet: bool, verbose: bool, no_progress: bool) -> Self {
        Self {
            quiet,
            verbose,
            no_progress,
        }
    }

    fn prefix(depth: usize) -> String {
        if depth == 0 {
            String::new()
        } else {
            ">".repeat(depth)
        }
    }

    fn finish_prefix(depth: usize) -> String {
        if depth == 0 {
            String::new()
        } else {
            "<".repeat(depth)
        }
    }

    fn write_line(&self, line: &str) {
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "{line}");
    }

    fn write_err(&self, line: &str) {
        let mut out = io::stderr().lock();
        let _ = writeln!(out, "{line}");
    }
}

impl EventSink for StdoutRenderer {
    fn emit(&mut self, event: ExecutionEvent) {
        if self.quiet {
            match &event {
                ExecutionEvent::Error { .. }
                | ExecutionEvent::LogError { .. }
                | ExecutionEvent::CancellationRequested { .. }
                | ExecutionEvent::WorkflowFinished {
                    state: ExecutionState::Failed | ExecutionState::Cancelled,
                    ..
                }
                | ExecutionEvent::ActionFinished {
                    state: ExecutionState::Failed | ExecutionState::Cancelled,
                    ..
                }
                | ExecutionEvent::RhaiFinished {
                    state: ExecutionState::Failed | ExecutionState::Cancelled,
                    ..
                }
                | ExecutionEvent::RollbackFinished { .. } => {}
                _ => return,
            }
        }

        match event {
            ExecutionEvent::WorkflowStarted { name, depth, .. } => {
                let p = Self::prefix(depth);
                let line = if p.is_empty() {
                    format!("Starting workflow {name}")
                } else {
                    format!("{p} Starting workflow {name}")
                };
                self.write_line(&line);
            }
            ExecutionEvent::WorkflowFinished { name, state, depth, .. } => {
                let p = Self::finish_prefix(depth);
                let line = if p.is_empty() {
                    format!("Finished workflow {name} [{}]", state.tag())
                } else {
                    format!("{p} Finished workflow {name} [{}]", state.tag())
                };
                self.write_line(&line);
            }
            ExecutionEvent::ActionStarted { name, depth, .. } => {
                let p = Self::prefix(depth);
                self.write_line(&format!("{p} Starting action {name}"));
            }
            ExecutionEvent::ActionFinished { name, state, depth, .. } => {
                let p = Self::finish_prefix(depth);
                self.write_line(&format!("{p} Finished action {name} [{}]", state.tag()));
            }
            ExecutionEvent::RhaiStarted { name, depth, .. } => {
                let p = Self::prefix(depth);
                if name == "<inline>" {
                    self.write_line(&format!("{p} Starting inline action execution"));
                } else {
                    self.write_line(&format!("{p} Starting action execution"));
                }
            }
            ExecutionEvent::RhaiFinished { name, state, depth, .. } => {
                let p = Self::finish_prefix(depth);
                if name == "<inline>" {
                    self.write_line(&format!(
                        "{p} Finished inline action execution [{}]",
                        state.tag()
                    ));
                } else {
                    self.write_line(&format!(
                        "{p} Finished action execution [{}]",
                        state.tag()
                    ));
                }
            }
            ExecutionEvent::LogInfo { message, depth, .. } => {
                if !self.quiet {
                    let p = Self::prefix(depth);
                    self.write_line(&format!("{p} [info] {message}"));
                }
            }
            ExecutionEvent::LogWarn { message, depth, .. } => {
                let p = Self::prefix(depth);
                self.write_line(&format!("{p} [warn] {message}"));
            }
            ExecutionEvent::LogError { message, depth, .. } => {
                let p = Self::prefix(depth);
                // Errors go to stderr so --json stdout stays clean when mixed sinks are used.
                self.write_err(&format!("{p} [error] {message}"));
            }
            ExecutionEvent::ProcessStarted { program, depth, .. } => {
                if self.verbose && !self.no_progress {
                    let p = Self::prefix(depth);
                    self.write_line(&format!("{p} Starting process {program}"));
                }
            }
            ExecutionEvent::ProcessFinished {
                program,
                state,
                exit_code,
                depth,
                ..
            } => {
                if self.verbose {
                    let p = Self::finish_prefix(depth);
                    let code = exit_code
                        .map(|c| format!(" exit={c}"))
                        .unwrap_or_default();
                    self.write_line(&format!(
                        "{p} Finished process {program} [{}]{code}",
                        state.tag()
                    ));
                }
            }
            ExecutionEvent::CancellationRequested { message } => {
                self.write_err(&format!("[info] {message}"));
            }
            ExecutionEvent::CleanupStarted { .. } => {
                if self.verbose {
                    self.write_line("Starting cleanup");
                }
            }
            ExecutionEvent::CleanupFinished { state, .. } => {
                if self.verbose {
                    self.write_line(&format!("Cleanup finished [{}]", state.tag()));
                }
            }
            ExecutionEvent::RollbackStarted => {
                self.write_line("Starting rollback");
            }
            ExecutionEvent::RollbackActionStarted { name, .. } => {
                let p = Self::prefix(1);
                self.write_line(&format!("{p} Rolling back action {name}"));
            }
            ExecutionEvent::RollbackActionFinished { name, state, .. } => {
                let p = Self::finish_prefix(1);
                self.write_line(&format!(
                    "{p} Finished rollback action {name} [{}]",
                    state.tag()
                ));
            }
            ExecutionEvent::RollbackFinished { state } => {
                if state == ExecutionState::Succeeded {
                    self.write_line("Rollback finished");
                } else {
                    self.write_line(&format!("Rollback completed with errors [{}]", state.tag()));
                }
            }
            ExecutionEvent::Error { message, .. } => {
                self.write_err(&format!("error: {message}"));
            }
            ExecutionEvent::Skipped { kind, name, reason, .. } => {
                if self.verbose {
                    self.write_line(&format!("[info] Skipping {kind} {name}: {reason}"));
                }
            }
        }
    }
}
