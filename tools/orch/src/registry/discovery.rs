//! Filesystem discovery for Actions and Workflows.
//!
//! Naming (Windows-safe directory hierarchy):
//!   relative path under the entity directory, components joined with `:`,
//!   `.rhai` stripped. Final component `default` is omitted.
//!
//! Examples:
//!   actions/clean.rhai              → @clean
//!   actions/build/default.rhai      → @build
//!   actions/build/dev.rhai          → @build:dev
//!   actions/build/kernel/release.rhai → @build:kernel:release
//!   workflows/run/default.rhai      → run
//!   workflows/run/dev.rhai          → run:dev

use super::{EntityType, Registry, RegistryEntry};
use crate::config::OrchConfig;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum EntitySource {
    File { path: PathBuf, line: u32 },
    Inline { label: String },
    Builtin { label: String },
}

impl EntitySource {
    pub fn display(&self) -> String {
        match self {
            Self::File { path, line } => format!("{}:{line}", path.display()),
            Self::Inline { label } => format!("inline:{label}"),
            Self::Builtin { label } => format!("builtin:{label}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredEntity {
    pub name: String,
    pub entity_type: EntityType,
    pub path: PathBuf,
}

pub fn discover_all(root: &Path, cfg: &OrchConfig) -> Result<Registry> {
    let mut reg = Registry::new();
    for dir in &cfg.actions_dirs {
        discover_into(&mut reg, root, &cfg.resolve(root, dir), EntityType::Action)?;
    }
    for dir in &cfg.workflows_dirs {
        discover_into(
            &mut reg,
            root,
            &cfg.resolve(root, dir),
            EntityType::Workflow,
        )?;
    }
    Ok(reg)
}

fn discover_into(
    reg: &mut Registry,
    _root: &Path,
    dir: &Path,
    ty: EntityType,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_rhai_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, path) in files {
        reg.register(RegistryEntry {
            name,
            entity_type: ty,
            source: EntitySource::File {
                path: path.clone(),
                line: 1,
            },
            metadata: {
                let mut m = BTreeMap::new();
                m.insert("path".into(), path.display().to_string());
                m
            },
        })
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}

fn collect_rhai_files(
    base: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rhai_files(base, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rhai") {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .with_extension("");
            let name = logical_name_from_rel(&rel, &path)?;
            out.push((name, path));
        }
    }
    Ok(())
}

/// Convert a relative path (no `.rhai`) into a logical entity name.
pub fn logical_name_from_rel(rel: &Path, source: &Path) -> Result<String> {
    let mut components: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if components.is_empty() || components.iter().any(|c| c == ".." || c.is_empty()) {
        anyhow::bail!("invalid entity path: {}", source.display());
    }
    if components.last().map(|s| s.as_str()) == Some("default") {
        components.pop();
    }
    if components.is_empty() {
        anyhow::bail!(
            "invalid entity path: {} (default.rhai at entity root has empty name)",
            source.display()
        );
    }
    for c in &components {
        if c.contains(':') {
            anyhow::bail!(
                "invalid entity path: {} (path component must not contain ':')",
                source.display()
            );
        }
    }
    Ok(components.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn nested_path_becomes_colon_name() {
        let tmp = tempdir().unwrap();
        let actions = tmp.path().join("actions");
        fs::create_dir_all(actions.join("build")).unwrap();
        fs::write(actions.join("build").join("dev.rhai"), "// test\n").unwrap();
        fs::write(actions.join("clean.rhai"), "// test\n").unwrap();

        let reg = discover_all(
            tmp.path(),
            &OrchConfig {
                actions_dirs: vec!["actions".into()],
                workflows_dirs: vec![],
                ..OrchConfig::default()
            },
        )
        .unwrap();

        assert!(reg.get(EntityType::Action, "build:dev").is_some());
        assert!(reg.get(EntityType::Action, "clean").is_some());
    }

    #[test]
    fn default_rhai_omits_final_component() {
        let tmp = tempdir().unwrap();
        let actions = tmp.path().join("actions");
        fs::create_dir_all(actions.join("build").join("kernel")).unwrap();
        fs::write(actions.join("build").join("default.rhai"), "// @build\n").unwrap();
        fs::write(
            actions.join("build").join("kernel").join("release.rhai"),
            "// @build:kernel:release\n",
        )
        .unwrap();
        fs::write(
            actions.join("build").join("kernel").join("default.rhai"),
            "// @build:kernel\n",
        )
        .unwrap();

        let reg = discover_all(
            tmp.path(),
            &OrchConfig {
                actions_dirs: vec!["actions".into()],
                workflows_dirs: vec![],
                ..OrchConfig::default()
            },
        )
        .unwrap();

        assert!(reg.get(EntityType::Action, "build").is_some());
        assert!(reg.get(EntityType::Action, "build:kernel").is_some());
        assert!(reg.get(EntityType::Action, "build:kernel:release").is_some());
        assert!(reg.get(EntityType::Action, "build:default").is_none());
    }

    #[test]
    fn duplicate_logical_name_is_fatal() {
        let tmp = tempdir().unwrap();
        let actions = tmp.path().join("actions");
        fs::create_dir_all(actions.join("build")).unwrap();
        // Both map to @build
        fs::write(actions.join("build.rhai"), "// a\n").unwrap();
        fs::write(actions.join("build").join("default.rhai"), "// b\n").unwrap();

        let err = discover_all(
            tmp.path(),
            &OrchConfig {
                actions_dirs: vec!["actions".into()],
                workflows_dirs: vec![],
                ..OrchConfig::default()
            },
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("duplicate") || msg.contains("@build"));
    }

    #[test]
    fn workflow_default_discovery() {
        let tmp = tempdir().unwrap();
        let workflows = tmp.path().join("workflows");
        fs::create_dir_all(workflows.join("run")).unwrap();
        fs::write(workflows.join("run").join("default.rhai"), "// run\n").unwrap();
        fs::write(workflows.join("run").join("dev.rhai"), "// run:dev\n").unwrap();

        let reg = discover_all(
            tmp.path(),
            &OrchConfig {
                actions_dirs: vec![],
                workflows_dirs: vec!["workflows".into()],
                ..OrchConfig::default()
            },
        )
        .unwrap();

        assert!(reg.get(EntityType::Workflow, "run").is_some());
        assert!(reg.get(EntityType::Workflow, "run:dev").is_some());
    }
}
