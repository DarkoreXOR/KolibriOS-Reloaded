//! CLI parsing: global options and execution units.

pub mod parser;

pub use parser::{
    normalize_inline_units, parse_argv, ArgValue, CliError, ExecutionUnit, GlobalOptions,
    ParsedCli, UnitKind,
};

pub mod help;

use crate::exit_codes::ExitStatus;
use crate::OrchConfig;
use std::path::PathBuf;

/// Fully resolved invocation context.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub globals: GlobalOptions,
    pub units: Vec<ExecutionUnit>,
    pub config_path: PathBuf,
    pub root: PathBuf,
    pub config: OrchConfig,
}

pub fn build_invocation(args: &[String]) -> Result<Invocation, CliError> {
    let mut parsed = parse_argv(args)?;
    if parsed.show_help {
        return Err(CliError::HelpRequested);
    }
    normalize_inline_units(&mut parsed.units)?;
    let config_path = parsed
        .globals
        .config
        .clone()
        .unwrap_or_else(OrchConfig::default_path);
    let config = OrchConfig::load(&config_path).map_err(|e| CliError::Config(e.to_string()))?;
    let root = crate::config::find_repo_root(&config_path)
        .map_err(|e| CliError::Config(e.to_string()))?;
    Ok(Invocation {
        globals: parsed.globals,
        units: parsed.units,
        config_path,
        root,
        config,
    })
}

pub fn print_help() {
    help::print_general_help();
}

pub fn print_version() {
    println!("orch {}", env!("CARGO_PKG_VERSION"));
}

pub fn map_cli_error(err: CliError) -> ExitStatus {
    match err {
        CliError::HelpRequested => {
            print_help();
            ExitStatus::Success
        }
        CliError::VersionRequested => {
            print_version();
            ExitStatus::Success
        }
        CliError::Usage(_) | CliError::Config(_) => {
            eprintln!("error: {err}");
            ExitStatus::ValidationFailure
        }
    }
}
