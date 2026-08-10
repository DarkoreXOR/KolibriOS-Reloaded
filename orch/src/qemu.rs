//! QEMU command-line construction (testable without launching QEMU).

use crate::config::{resolve, Config};
use crate::fs_image::resolve_disk_image;
use crate::paths::persistent_image_path;
use crate::util::resolve_tool;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Parsed `run` extras: `--disk:ntfs`, `--memory:128M`, `--serial`, `--debug`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    pub disks: Vec<String>,
    pub memory: Option<String>,
    pub serial: bool,
    pub debug: bool,
}

impl RunOptions {
    pub fn parse_extra_args(args: &[String]) -> Result<Self> {
        let mut opts = Self::default();
        for arg in args {
            if let Some(disk) = arg.strip_prefix("--disk:") {
                if disk.is_empty() {
                    bail!("--disk: requires a filesystem name (exfat, ntfs)");
                }
                // Validate early.
                let _ = crate::fs_image::FilesystemKind::parse(disk)?;
                opts.disks.push(disk.to_ascii_lowercase());
                continue;
            }
            if let Some(mem) = arg.strip_prefix("--memory:") {
                if mem.is_empty() {
                    bail!("--memory: requires a size (e.g. 128M)");
                }
                opts.memory = Some(mem.to_string());
                continue;
            }
            match arg.as_str() {
                "--serial" => opts.serial = true,
                "--debug" => opts.debug = true,
                other => bail!("unknown run option `{other}`\nSupported: --disk:TYPE, --memory:SIZE, --serial, --debug"),
            }
        }
        Ok(opts)
    }
}

/// Fully built QEMU invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuInvocation {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

/// Build a QEMU command for booting a floppy with optional persistent IDE disks.
pub fn build_qemu_command(
    cfg: &Config,
    root: &Path,
    floppy: &Path,
    run_opts: &RunOptions,
    headless: bool,
    reference_mode: bool,
) -> Result<QemuInvocation> {
    let qemu = resolve_qemu(cfg, root)?;
    let mut args: Vec<String> = Vec::new();
    args.push("-fda".into());
    args.push(floppy.to_string_lossy().into_owned());
    args.extend(cfg.qemu.args.iter().cloned());

    if let Some(mem) = &run_opts.memory {
        // Replace default `-m` from config when explicit memory is requested.
        args = replace_memory_arg(args, mem);
    }

    if run_opts.serial {
        args.push("-serial".into());
        args.push("stdio".into());
    }
    if run_opts.debug {
        args.push("-d".into());
        args.push("int,cpu_reset".into());
        args.push("-D".into());
        args.push(
            root.join("dev_build")
                .join("qemu-debug.log")
                .to_string_lossy()
                .into_owned(),
        );
    }

    if reference_mode {
        args.extend(cfg.qemu.reference_extra_args.iter().cloned());
    }

    // Attach persistent regression disks in order (index 0, 1, …).
    for (i, disk) in run_opts.disks.iter().enumerate() {
        let image = resolve_disk_image(root, disk)?;
        if !image.is_file() {
            bail!(
                "disk image missing for `{disk}`\nExpected: {}\nCreate it with: mkfs {disk} 4M",
                image.display()
            );
        }
        args.push("-drive".into());
        args.push(format!(
            "file={},format=raw,if=ide,index={i},media=disk",
            image.to_string_lossy()
        ));
    }

    // Legacy [testdisk] auto-attach when no explicit disks and testdisk enabled.
    if run_opts.disks.is_empty() && cfg.testdisk.enabled {
        let td = resolve(root, &cfg.testdisk.image);
        if td.is_file() {
            for tmpl in &cfg.testdisk.drive_args {
                args.push(tmpl.replace("{image}", &td.to_string_lossy()));
            }
        }
    }

    if headless {
        args.extend(cfg.qemu.headless_extra_args.iter().cloned());
    }

    Ok(QemuInvocation { executable: qemu, args })
}

fn replace_memory_arg(args: Vec<String>, mem: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() + 1);
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-m" {
            out.push("-m".into());
            out.push(mem.to_string());
            i += 2; // skip old value
            continue;
        }
        out.push(args[i].clone());
        i += 1;
    }
    if !out.iter().any(|a| a == "-m") {
        out.push("-m".into());
        out.push(mem.to_string());
    }
    out
}

pub fn resolve_qemu(cfg: &Config, root: &Path) -> Result<PathBuf> {
    for cand in &cfg.qemu.executables {
        if let Some(p) = resolve_tool(cand, root) {
            return Ok(p);
        }
    }
    bail!(
        "ERROR: QEMU executable not found\nTried: {:?}\nInstall qemu-system-i386 or set qemu.executables in orch/config.toml",
        cfg.qemu.executables
    )
}

pub fn list_known_disks(root: &Path) -> Vec<(String, PathBuf, bool)> {
    crate::fs_image::FilesystemKind::all()
        .iter()
        .map(|fs| {
            let name = fs.name().to_string();
            let path = persistent_image_path(root, fs.name());
            let exists = path.is_file();
            (name, path, exists)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::path::PathBuf;

    fn minimal_cfg() -> Config {
        let text = include_str!("../config.toml");
        toml::from_str(text).expect("config.toml parses")
    }

    #[test]
    fn parse_run_options() {
        let opts = RunOptions::parse_extra_args(&[
            "--disk:ntfs".into(),
            "--disk:exfat".into(),
            "--memory:128M".into(),
            "--serial".into(),
        ])
        .unwrap();
        assert_eq!(opts.disks, vec!["ntfs", "exfat"]);
        assert_eq!(opts.memory.as_deref(), Some("128M"));
        assert!(opts.serial);
    }

    #[test]
    fn unknown_disk_rejected() {
        assert!(RunOptions::parse_extra_args(&["--disk:btrfs".into()]).is_err());
    }

    #[test]
    fn qemu_command_includes_disks() {
        let cfg = minimal_cfg();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let opts = RunOptions {
            disks: vec!["ntfs".into(), "exfat".into()],
            ..Default::default()
        };
        // Only test command construction when images exist; otherwise skip disk attachment check.
        let floppy = root.join("kolibrios-0.7.7.0-9160-g944d74f01-en_US.img");
        if !floppy.is_file() {
            return;
        }
        let ntfs = root.join("images/ntfs-image.img");
        let exfat = root.join("images/exfat-image.img");
        if !ntfs.is_file() || !exfat.is_file() {
            return;
        }
        let inv = build_qemu_command(&cfg, &root, &floppy, &opts, false, false).unwrap();
        let joined = inv.args.join(" ");
        assert!(joined.contains("ntfs-image.img"));
        assert!(joined.contains("exfat-image.img"));
        assert!(joined.contains("index=0"));
        assert!(joined.contains("index=1"));
    }

    #[test]
    fn memory_override() {
        let args = vec![
            "-boot".into(),
            "a".into(),
            "-m".into(),
            "256".into(),
            "-vga".into(),
            "std".into(),
        ];
        let out = replace_memory_arg(args, "128M");
        assert!(out.contains(&"-m".into()));
        assert!(out.contains(&"128M".into()));
        assert!(!out.contains(&"256".into()));
    }
}
