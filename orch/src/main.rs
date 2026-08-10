//! Repository-local orchestrator: central entry point for KolibriOS project operations.

mod commands;
mod config;
mod fs_image;
mod paths;
mod pipeline;
mod qemu;
mod rhai_engine;
mod tools;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{
    clean_artifacts, config_path_default, load_context, print_help, run_mkfs, run_ref_qemu,
    run_script,
};
use pipeline::{doctor, Pipeline};
use qemu::RunOptions;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "orch",
    about = "KolibriOS project operations orchestrator",
    long_about = "Central entry point for build, test, image creation, QEMU, and workflows.\n\
Extension order: reuse orchestrator API → Rhai workflow → Rust capability → ./tools/ utility.\n\
Configuration: orch/config.toml",
    disable_help_subcommand = true
)]
struct Cli {
    /// Path to config.toml (default: next to this tool's sources).
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Print commands without executing them.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Skip `cargo test -p kolibri_utils` during the Rust stage.
    #[arg(long, global = true)]
    skip_tests: bool,

    /// Add headless QEMU extras from config (`-display none`, etc.).
    #[arg(long, global = true)]
    headless: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Show commands, scripts, tools, and usage (`help mkfs`, `help run`, …).
    Help {
        /// Topic: run, mkfs, scripts, tools, clean, …
        topic: Option<String>,
    },
    /// Verify tools and configured paths.
    Doctor,
    /// Build Rust blobs + assemble `kernel.mnt`.
    Build,
    /// Build kernel and create a fresh disposable boot `.img`.
    Image,
    /// Build, package a fresh image, and launch QEMU.
    Qemu {
        /// Extra run options: `--disk:ntfs`, `--memory:128M`, `--serial`, `--debug`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Preferred one-shot: build → fresh image → QEMU.
    Run {
        /// Extra run options: `--disk:ntfs`, `--memory:128M`, `--serial`, `--debug`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Create or reuse a persistent filesystem regression image.
    Mkfs {
        /// Filesystem type: exfat, ntfs
        filesystem: String,
        /// Image size: 4M, 128M, 4096, …
        size: String,
        /// Force recreation even if a valid image exists.
        #[arg(long)]
        force: bool,
    },
    /// Remove generated artifacts under build/ and dev_build/ (preserves images/).
    Clean,
    /// Run a Rhai workflow from orch/scripts/.
    Script {
        /// Script name (without .rhai extension).
        name: String,
        /// Arguments passed to the script as `args`.
        #[arg(trailing_var_arg = true)]
        script_args: Vec<String>,
    },
    /// Invoke a reusable utility under ./tools/.
    Tool {
        /// Tool path relative to tools/ (e.g. mkfs_utils/create_exfat_image.py).
        tool: String,
        #[arg(trailing_var_arg = true)]
        tool_args: Vec<String>,
    },
    /// Boot the immutable reference `.img` in QEMU (no rebuild; uses `-snapshot`).
    #[command(visible_alias = "original")]
    Ref {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Legacy: ensure / recreate exFAT testdisk (prefer `mkfs exfat`).
    Testdisk {
        #[arg(long)]
        force: bool,
    },
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("{err:#}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> Result<u8> {
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(config_path_default);
    let (cfg, root) = load_context(&config_path)?;

    let mut pipe = Pipeline::new(&cfg, &root, cli.dry_run, cli.skip_tests, cli.headless);

    match cli.command {
        Commands::Help { topic } => {
            print_help(&root, topic.as_deref());
            Ok(0)
        }
        Commands::Doctor => {
            doctor(&cfg, &root)?;
            Ok(0)
        }
        Commands::Build => {
            pipe.build_all()?;
            pipe.print_summary_paths();
            Ok(0)
        }
        Commands::Image => {
            pipe.build_all()?;
            pipe.create_image()?;
            pipe.print_summary_paths();
            Ok(0)
        }
        Commands::Qemu { extra } | Commands::Run { extra } => {
            let run_opts = RunOptions::parse_extra_args(&extra)?;
            let code = pipe.run_full_with_opts(&run_opts)?;
            Ok(code as u8)
        }
        Commands::Mkfs {
            filesystem,
            size,
            force,
        } => {
            run_mkfs(&cfg, &root, &filesystem, &size, force, cli.dry_run)?;
            Ok(0)
        }
        Commands::Clean => {
            clean_artifacts(&root, cli.dry_run)?;
            Ok(0)
        }
        Commands::Script { name, script_args } => {
            run_script(
                &cfg,
                &root,
                &name,
                &script_args,
                cli.dry_run,
                cli.skip_tests,
                cli.headless,
            )?;
            Ok(0)
        }
        Commands::Tool { tool, tool_args } => {
            commands::invoke_tool(&cfg, &root, &tool, &tool_args, cli.dry_run)?;
            Ok(0)
        }
        Commands::Ref { extra } => {
            let run_opts = RunOptions::parse_extra_args(&extra)?;
            let code = run_ref_qemu(&cfg, &root, &pipe, &run_opts, cli.dry_run)?;
            Ok(code as u8)
        }
        Commands::Testdisk { force } => {
            pipe.ensure_testdisk(force)?;
            Ok(0)
        }
    }
}
