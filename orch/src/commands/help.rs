//! Help and discovery output.

use std::path::Path;

use crate::commands::{command_registry, discover_scripts, find_command, CommandInfo};
use crate::fs_image::FilesystemKind;
use crate::qemu::list_known_disks;
use crate::tools::discover_tools;

pub fn print_help(root: &Path, topic: Option<&str>) {
    match topic {
        None | Some("help") => print_general_help(),
        Some("commands") => print_commands(),
        Some("scripts") => print_scripts(root),
        Some("tools") => print_tools(root),
        Some("mkfs") => print_mkfs_help(),
        Some("run") => print_run_help(),
        Some("clean") => print_clean_help(),
        Some(other) => {
            if let Some(cmd) = find_command(other) {
                print_command_detail(&cmd);
            } else {
                eprintln!("Unknown help topic: {other}");
                eprintln!();
                print_general_help();
            }
        }
    }
}

fn print_general_help() {
    eprintln!("orch — KolibriOS project operations orchestrator");
    eprintln!();
    eprintln!("The orchestrator is the canonical entry point for project operations.");
    eprintln!("Inspect available commands before creating ad-hoc scripts.");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  cargo run --manifest-path orch/Cargo.toml -- <command> [options]");
    eprintln!();
    eprintln!("Universal commands:");
    for cmd in command_registry() {
        eprintln!("  {:12} {}", cmd.name, cmd.summary);
        for alias in cmd.aliases {
            eprintln!("  {:12} (alias: {alias})", "");
        }
    }
    eprintln!();
    eprintln!("Discovery:");
    eprintln!("  help commands   List all commands");
    eprintln!("  help scripts    List Rhai workflows");
    eprintln!("  help tools      List ./tools/ utilities");
    eprintln!("  help mkfs       Filesystem image creation");
    eprintln!("  help run        QEMU run options");
    eprintln!();
    eprintln!("Artifact paths:");
    eprintln!("  build/          Production build artifacts");
    eprintln!("  dev_build/      Development/test artifacts");
    eprintln!("  images/         Persistent filesystem regression images");
}

fn print_commands() {
    eprintln!("Registered commands:");
    for cmd in command_registry() {
        let aliases = if cmd.aliases.is_empty() {
            String::new()
        } else {
            format!(" (aliases: {})", cmd.aliases.join(", "))
        };
        eprintln!("  {}{}", cmd.name, aliases);
        eprintln!("    {}", cmd.summary);
    }
}

fn print_scripts(root: &Path) {
    eprintln!("Rhai workflows (orch/scripts/):");
    match discover_scripts(root) {
        Ok(list) if list.is_empty() => eprintln!("  (none yet)"),
        Ok(list) => {
            for s in list {
                eprintln!("  {s}");
            }
        }
        Err(e) => eprintln!("  error: {e:#}"),
    }
    eprintln!();
    eprintln!("Run: script <name> [args…]");
}

fn print_tools(root: &Path) {
    eprintln!("Project tools (./tools/):");
    match discover_tools(root) {
        Ok(list) if list.is_empty() => eprintln!("  (none found)"),
        Ok(list) => {
            for t in list {
                eprintln!("  {t}");
            }
        }
        Err(e) => eprintln!("  error: {e:#}"),
    }
    eprintln!();
    eprintln!("Run: tool <path> [args…]");
}

fn print_mkfs_help() {
    eprintln!("mkfs — create or reuse persistent filesystem regression images");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  mkfs exfat 4M");
    eprintln!("  mkfs ntfs 4M");
    eprintln!("  mkfs exfat 128M --force");
    eprintln!();
    eprintln!("Output paths:");
    for fs in FilesystemKind::all() {
        eprintln!("  {} → ./images/{}-image.img", fs.name(), fs.name());
    }
    eprintln!();
    eprintln!("Outcomes: created | reused | force-recreated");
    eprintln!("Size suffixes: K, M, G, or plain bytes");
}

fn print_run_help() {
    eprintln!("run — build, package, and launch QEMU");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  run");
    eprintln!("  run --disk:ntfs");
    eprintln!("  run --disk:exfat");
    eprintln!("  run --disk:ntfs --disk:exfat");
    eprintln!("  run --disk:ntfs --memory:128M --serial --debug");
    eprintln!();
    eprintln!("Disk mapping:");
    for (name, path, exists) in list_known_disks(Path::new(".")) {
        let status = if exists { "present" } else { "missing" };
        eprintln!("  {name} → {} ({status})", path.display());
    }
    eprintln!();
    eprintln!("Global flags: --dry-run, --skip-tests, --headless");
}

fn print_clean_help() {
    eprintln!("clean — remove generated artifacts");
    eprintln!();
    eprintln!("Removes:");
    eprintln!("  ./build/");
    eprintln!("  ./dev_build/");
    eprintln!();
    eprintln!("Preserves:");
    eprintln!("  ./images/  (persistent regression disks)");
}

fn print_command_detail(cmd: &CommandInfo) {
    eprintln!("{} — {}", cmd.name, cmd.summary);
    if !cmd.aliases.is_empty() {
        eprintln!("Aliases: {}", cmd.aliases.join(", "));
    }
    eprintln!();
    eprintln!("{}", cmd.detail);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_topics_do_not_panic() {
        let root = Path::new(".");
        print_help(root, None);
        print_help(root, Some("mkfs"));
        print_help(root, Some("run"));
    }
}
