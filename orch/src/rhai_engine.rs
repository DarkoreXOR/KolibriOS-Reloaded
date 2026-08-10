//! Rhai workflow engine — composes orchestrator APIs into reusable workflows.

use crate::commands::clean_artifacts;
use crate::config::{resolve, Config};
use crate::fs_image::{mkfs, FilesystemKind};
use crate::pipeline::Pipeline;
use crate::qemu::{build_qemu_command, RunOptions};
use crate::tools::run_tool;
use crate::util::run_inherit;
use anyhow::{bail, Result};
use rhai::{Engine, EvalAltResult, Map, Scope};
use std::path::{Path, PathBuf};

const SCRIPTS_DIR: &str = "orch/scripts";

pub fn scripts_dir(root: &Path) -> PathBuf {
    resolve(root, SCRIPTS_DIR)
}

pub fn discover_scripts(root: &Path) -> Result<Vec<String>> {
    let dir = scripts_dir(root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rhai") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

fn script_path(root: &Path, name: &str) -> Result<PathBuf> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        bail!("invalid script name: {name}");
    }
    let path = scripts_dir(root).join(format!("{name}.rhai"));
    if !path.is_file() {
        bail!(
            "Rhai script not found: {name}\nExpected: {}\nRun `help scripts` to list workflows.",
            path.display()
        );
    }
    Ok(path)
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
    let path = script_path(root, name)?;
    eprintln!("== script {name} ==");
    eprintln!("  {}", path.display());

    let mut engine = Engine::new();
    register_api(
        &mut engine,
        cfg.clone(),
        root.to_path_buf(),
        dry_run,
        skip_tests,
        headless,
    );

    let mut scope = Scope::new();
    scope.push("args", args.to_vec());

    engine
        .run_file_with_scope(&mut scope, path)
        .map_err(|e: Box<EvalAltResult>| anyhow::anyhow!("Rhai error: {e}"))?;
    Ok(())
}

fn register_api(
    engine: &mut Engine,
    cfg: Config,
    root: PathBuf,
    dry_run: bool,
    skip_tests: bool,
    headless: bool,
) {
    let python = cfg.rust.extract.python.clone();
    let base_image = cfg.image.base_image.clone();
    let qemu_args = cfg.qemu.args.clone();
    let headless_args = cfg.qemu.headless_extra_args.clone();

    engine.register_fn("build_kernel", {
        let cfg = cfg.clone();
        let root = root.clone();
        move || -> Result<String, Box<EvalAltResult>> {
            let mut pipe = Pipeline::new(&cfg, &root, dry_run, skip_tests, headless);
            pipe.build_all().map_err(|e| e.to_string())?;
            Ok("ok".into())
        }
    });

    engine.register_fn("mkfs", {
        let root = root.clone();
        let python = python.clone();
        move |fs: &str, size: &str| -> Result<String, Box<EvalAltResult>> {
            let kind = FilesystemKind::parse(fs).map_err(|e| e.to_string())?;
            let bytes = crate::fs_image::parse_size(size).map_err(|e| e.to_string())?;
            let (_p, outcome) =
                mkfs(&root, &python, kind, bytes, false, dry_run).map_err(|e| e.to_string())?;
            Ok(outcome.as_str().into())
        }
    });

    engine.register_fn("run_qemu", {
        let cfg = cfg.clone();
        let root = root.clone();
        move |options: Map| -> Result<i64, Box<EvalAltResult>> {
            let mut run_opts = RunOptions::default();
            if let Some(disks) = options
                .get("disks")
                .and_then(|v| v.clone().try_cast::<rhai::Array>())
            {
                for d in disks {
                    if let Some(s) = d.clone().try_cast::<String>() {
                        run_opts.disks.push(s);
                    }
                }
            }
            if let Some(mem) = options
                .get("memory")
                .and_then(|v| v.clone().try_cast::<String>())
            {
                run_opts.memory = Some(mem);
            }
            if options
                .get("serial")
                .and_then(|v| v.as_bool().ok())
                .unwrap_or(false)
            {
                run_opts.serial = true;
            }
            let floppy = resolve(&root, &base_image);
            let mini_cfg = minimal_qemu_cfg(&cfg, &base_image, &qemu_args, &headless_args);
            let inv = build_qemu_command(&mini_cfg, &root, &floppy, &run_opts, headless, false)
                .map_err(|e| e.to_string())?;
            let code = run_inherit("QEMU", &inv.executable, &inv.args, &root, dry_run)
                .map_err(|e| e.to_string())?;
            Ok(code as i64)
        }
    });

    engine.register_fn("clean", {
        let root = root.clone();
        move || -> Result<(), Box<EvalAltResult>> {
            clean_artifacts(&root, dry_run).map_err(|e| e.to_string())?;
            Ok(())
        }
    });

    engine.register_fn("run_tool", {
        let root = root.clone();
        let python = python.clone();
        move |tool: &str, args: rhai::Array| -> Result<i64, Box<EvalAltResult>> {
            let str_args: Vec<String> = args
                .into_iter()
                .filter_map(|v| v.clone().try_cast::<String>())
                .collect();
            let run = run_tool(&root, &python, tool, &str_args, dry_run)
                .map_err(|e| e.to_string())?;
            Ok(run.result.status as i64)
        }
    });
}

fn minimal_qemu_cfg(
    base: &Config,
    base_image: &str,
    args: &[String],
    headless_args: &[String],
) -> Config {
    let mut cfg = base.clone();
    cfg.image.base_image = base_image.to_string();
    cfg.qemu.args = args.to_vec();
    cfg.qemu.headless_extra_args = headless_args.to_vec();
    cfg.testdisk.enabled = false;
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_scripts_finds_filesystem_regression() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let scripts = discover_scripts(&root).unwrap();
        assert!(scripts.iter().any(|s| s == "filesystem_regression"));
    }
}
