//! Repository-local orchestrator: Rust blobs → FASM kernel → disposable image → QEMU.

mod config;
mod pipeline;
mod util;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{repo_root_from_config, Config};
use pipeline::{doctor, Pipeline};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "kolibri_build",
    about = "Build the hybrid KolibriOS kernel, package a fresh test image, and run QEMU",
    long_about = "Coordinates Cuts A–AG Rust freestanding blob builds, FASM assemble, \
kolibri_img CoW packaging, and QEMU smoke. Configuration: tools/build/config.toml \
([[rust.blobs]] + [[rust.migrations]] USE_RUST_* gates)."
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
    /// Verify tools and configured paths.
    Doctor,
    /// Build Rust blobs + assemble `kernel.mnt`.
    Build,
    /// Build kernel and create a fresh temporary `.img`.
    Image,
    /// Build, package a fresh image, and launch QEMU.
    Qemu,
    /// Preferred one-shot: build → fresh image → QEMU.
    Run,
    /// Boot the immutable reference `.img` in QEMU (no rebuild; uses `-snapshot`).
    #[command(visible_alias = "original")]
    Ref,
}

fn default_config_path() -> PathBuf {
    // When run via `cargo run --manifest-path tools/build/Cargo.toml`, CARGO_MANIFEST_DIR
    // points at tools/build/.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("config.toml")
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
    let config_path = cli.config.unwrap_or_else(default_config_path);
    let cfg = Config::load(&config_path)
        .with_context(|| format!("loading {}", config_path.display()))?;
    let root = repo_root_from_config(&config_path)?;

    let mut pipe = Pipeline::new(&cfg, &root, cli.dry_run, cli.skip_tests, cli.headless);

    match cli.command {
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
        Commands::Qemu | Commands::Run => {
            let code = pipe.run_full()?;
            Ok(code as u8)
        }
        Commands::Ref => {
            let code = pipe.run_reference_qemu()?;
            Ok(code as u8)
        }
    }
}
