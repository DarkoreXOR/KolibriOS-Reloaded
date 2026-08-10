//! Universal Rust/Rhai automation orchestrator.
//!
//! Rust provides generic runtime primitives. Rhai provides automation intelligence.
//! Project-specific logic belongs in Actions, Workflows, or `./tools/`.
//!
//! CLI model: `$` = anonymous Action · `@name` = named Action · `name` = Workflow.

pub mod cli;
pub mod config;
pub mod execution;
pub mod exit_codes;
pub mod registry;
pub mod runtime;
pub mod script;

pub use config::OrchConfig;
pub use exit_codes::ExitStatus;
pub use registry::Registry;
