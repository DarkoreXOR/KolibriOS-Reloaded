//! `orch` CLI entry point.

use orch::cli::{build_invocation, map_cli_error, print_help, CliError};
use orch::execution::execute_invocation;
use orch::exit_codes::ExitStatus;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match build_invocation(&args) {
        Err(e) => map_cli_error(e).into(),
        Ok(inv) => {
            if inv.units.is_empty() {
                print_help();
                return ExitStatus::Success.into();
            }
            match execute_invocation(inv) {
                Ok(()) => ExitStatus::Success.into(),
                Err(e) => {
                    eprintln!("error: {e}");
                    e.exit_status().into()
                }
            }
        }
    }
}

// Re-export for binary-only clarity when help is requested via CliError.
#[allow(dead_code)]
fn _help_path(err: CliError) -> ExitStatus {
    map_cli_error(err)
}
