//! Command registry and discovery.

pub mod help;

pub use help::print_help;

use crate::config::{repo_root_from_config, Config};
use crate::fs_image::{mkfs, FilesystemKind, MkfsOutcome};
use crate::paths::cleanup_roots;
use crate::pipeline::Pipeline;
use crate::qemu::{build_qemu_command, RunOptions};
use crate::rhai_engine;
use crate::tools;
use crate::util::run_inherit;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata for a registered orchestrator command (used by help/discovery).
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub detail: &'static str,
}

pub fn command_registry() -> Vec<CommandInfo> {
    vec![
        CommandInfo {
            name: "run",
            aliases: &["qemu"],
            summary: "Build kernel, package fresh boot image, launch QEMU",
            detail: "Full pipeline. Accepts --disk:TYPE, --memory:SIZE, --serial, --debug.",
        },
        CommandInfo {
            name: "build",
            aliases: &[],
            summary: "Build Rust blobs and assemble kernel.mnt",
            detail: "Rust freestanding build + FASM assemble.",
        },
        CommandInfo {
            name: "image",
            aliases: &[],
            summary: "Build kernel and create disposable boot image",
            detail: "Outputs to dev_build/ (configured in config.toml).",
        },
        CommandInfo {
            name: "mkfs",
            aliases: &[],
            summary: "Create or reuse persistent filesystem regression images",
            detail: "Usage: mkfs <exfat|ntfs> <size> [--force]\nOutput: ./images/{fs}-image.img",
        },
        CommandInfo {
            name: "clean",
            aliases: &[],
            summary: "Remove generated artifacts under build/ and dev_build/",
            detail: "Never removes ./images/ persistent regression disks.",
        },
        CommandInfo {
            name: "help",
            aliases: &["?"],
            summary: "Show commands, scripts, and tools",
            detail: "Usage: help [topic] — topics: run, mkfs, scripts, tools, …",
        },
        CommandInfo {
            name: "script",
            aliases: &[],
            summary: "Run a Rhai workflow from orch/scripts/",
            detail: "Usage: script <name> [args…]",
        },
        CommandInfo {
            name: "tool",
            aliases: &[],
            summary: "Invoke a reusable utility under ./tools/",
            detail: "Usage: tool <path-under-tools> [args…]",
        },
        CommandInfo {
            name: "doctor",
            aliases: &[],
            summary: "Verify tools, paths, and migration registry",
            detail: "Checks FASM, cargo, python, qemu, blobs, gates.",
        },
        CommandInfo {
            name: "ref",
            aliases: &["original"],
            summary: "Boot immutable reference image in QEMU (-snapshot)",
            detail: "No rebuild. Optional --disk:TYPE extras like run.",
        },
        CommandInfo {
            name: "testdisk",
            aliases: &[],
            summary: "Legacy: ensure exFAT testdisk (prefer mkfs exfat)",
            detail: "Deprecated in favor of `mkfs exfat`. Still supported.",
        },
    ]
}

pub fn find_command(name: &str) -> Option<CommandInfo> {
    command_registry()
        .into_iter()
        .find(|c| c.name == name || c.aliases.contains(&name))
}

/// Safe cleanup of build/ and dev_build/ (never images/).
pub fn clean_artifacts(root: &Path, dry_run: bool) -> Result<()> {
    eprintln!("== clean ==");
    eprintln!("Removing generated artifacts under build/ and dev_build/");
    eprintln!("Preserving ./images/ persistent regression disks.");

    for dir in cleanup_roots(root) {
        if !dir.is_dir() {
            eprintln!("  skip (missing): {}", dir.display());
            continue;
        }
        if dry_run {
            eprintln!("  would remove: {}", dir.display());
            continue;
        }
        eprintln!("  removing: {}", dir.display());
        fs::remove_dir_all(&dir)
            .with_context(|| format!("remove {}", dir.display()))?;
    }
    eprintln!("clean: done");
    Ok(())
}

pub fn run_mkfs(
    cfg: &Config,
    root: &Path,
    fs_name: &str,
    size: &str,
    force: bool,
    dry_run: bool,
) -> Result<MkfsOutcome> {
    let fs = FilesystemKind::parse(fs_name)?;
    let size_bytes = fs.effective_size(crate::fs_image::parse_size(size)?);
    let (_path, outcome) = mkfs(
        root,
        &cfg.rust.extract.python,
        fs,
        size_bytes,
        force,
        dry_run,
    )?;
    Ok(outcome)
}

pub fn run_script(
    cfg: &Config,
    root: &Path,
    name: &str,
    args: &[String],
    dry_run: bool,
    skip_tests: bool,
    headless: bool,
) -> Result<()> {
    rhai_engine::run_script(cfg, root, name, args, dry_run, skip_tests, headless)
}

pub fn discover_scripts(root: &Path) -> Result<Vec<String>> {
    rhai_engine::discover_scripts(root)
}

pub fn run_ref_qemu(
    cfg: &Config,
    root: &Path,
    pipe: &Pipeline,
    run_opts: &RunOptions,
    dry_run: bool,
) -> Result<i32> {
    eprintln!("== QEMU (reference image) ==");
    let image = crate::config::resolve(root, &cfg.image.base_image);
    if !dry_run && !image.is_file() {
        bail!("reference image missing: {}", image.display());
    }

    // Ensure legacy testdisk if no explicit disks.
    if run_opts.disks.is_empty() && cfg.testdisk.enabled {
        pipe.ensure_testdisk(false)?;
    }

    let inv = build_qemu_command(cfg, root, &image, run_opts, pipe.headless, true)?;
    eprintln!("QEMU: {}", inv.executable.display());
    eprintln!("  {}", inv.args.join(" "));
    run_inherit("QEMU (reference)", &inv.executable, &inv.args, root, dry_run)
}

pub fn config_path_default() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.toml")
}

pub fn invoke_tool(
    cfg: &Config,
    root: &Path,
    tool: &str,
    args: &[String],
    dry_run: bool,
) -> Result<()> {
    tools::run_tool(root, &cfg.rust.extract.python, tool, args, dry_run)?;
    Ok(())
}

pub fn load_context(config_path: &Path) -> Result<(Config, PathBuf)> {
    let cfg = Config::load(config_path)
        .with_context(|| format!("loading {}", config_path.display()))?;
    let root = repo_root_from_config(config_path)?;
    Ok((cfg, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_mkfs_and_clean() {
        let reg = command_registry();
        assert!(reg.iter().any(|c| c.name == "mkfs"));
        assert!(reg.iter().any(|c| c.name == "clean"));
        assert!(reg.iter().any(|c| c.name == "script"));
    }

    #[test]
    fn find_command_aliases() {
        assert!(find_command("qemu").is_some());
        assert!(find_command("original").is_some());
        assert!(find_command("nonexistent").is_none());
    }
}
