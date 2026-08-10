//! Generic orchestrator configuration.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OrchConfig {
    pub actions_dirs: Vec<String>,
    pub workflows_dirs: Vec<String>,
    pub tools_dirs: Vec<String>,
    pub lib_dirs: Vec<String>,
    pub temp_dir: String,
    pub process_termination_timeout_ms: u64,
    pub timer_poll_ms: u64,
    /// HTTP response body size limit (bytes).
    pub http_max_body_bytes: u64,
}

impl Default for OrchConfig {
    fn default() -> Self {
        Self {
            actions_dirs: vec![".orch/actions".into()],
            workflows_dirs: vec![".orch/workflows".into()],
            tools_dirs: vec!["tools".into()],
            lib_dirs: vec![".orch/lib".into()],
            temp_dir: ".orch-tmp".into(),
            process_termination_timeout_ms: 5_000,
            timer_poll_ms: 50,
            http_max_body_bytes: 16 * 1024 * 1024,
        }
    }
}

impl OrchConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: OrchConfig = toml::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))?;
        Ok(cfg)
    }

    /// Prefer repo-root `.orch/config.toml`; fall back to package-local `config.toml`.
    pub fn default_path() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Ok(root) = find_repo_root(&manifest) {
            let preferred = root.join(".orch").join("config.toml");
            if preferred.is_file() {
                return preferred;
            }
        }
        manifest.join("config.toml")
    }

    pub fn resolve(&self, root: &Path, rel: &str) -> PathBuf {
        let p = Path::new(rel);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        }
    }
}

/// Find project root by walking upward until a generic marker exists.
///
/// Markers (project-agnostic):
/// - `tools/orch/Cargo.toml` (monorepo layout)
/// - `.orch/actions` or `.orch/config.toml` (project Rhai + config)
/// - `orch/Cargo.toml` (legacy monorepo layout)
/// - `Cargo.toml` naming package `orch` (when started from inside the package)
/// - `config.toml` beside an `actions/` directory (standalone mini-project)
pub fn find_repo_root(start: &Path) -> Result<PathBuf> {
    let mut cur = if start.is_file() {
        start.parent().unwrap_or(start).to_path_buf()
    } else {
        start.to_path_buf()
    };
    if let Ok(canon) = fs::canonicalize(&cur) {
        cur = strip_verbatim(canon);
    }
    loop {
        if cur.join("tools").join("orch").join("Cargo.toml").is_file() {
            return Ok(cur);
        }
        if cur.join(".orch").join("actions").is_dir()
            || cur.join(".orch").join("config.toml").is_file()
        {
            return Ok(cur);
        }
        if cur.join("orch").join("Cargo.toml").is_file() {
            return Ok(cur);
        }
        if is_orch_package_dir(&cur) {
            if cur
                .file_name()
                .map(|n| n == "orch")
                .unwrap_or(false)
            {
                if let Some(parent) = cur.parent() {
                    if parent
                        .file_name()
                        .map(|n| n == "tools")
                        .unwrap_or(false)
                    {
                        if let Some(grand) = parent.parent() {
                            return Ok(grand.to_path_buf());
                        }
                    }
                    return Ok(parent.to_path_buf());
                }
            }
            return Ok(cur);
        }
        if cur.join("config.toml").is_file() && cur.join("actions").is_dir() {
            // `.orch/` holds project Rhai; the project root is its parent.
            if cur
                .file_name()
                .map(|n| n == ".orch")
                .unwrap_or(false)
            {
                if let Some(parent) = cur.parent() {
                    return Ok(parent.to_path_buf());
                }
            }
            return Ok(cur);
        }
        if !cur.pop() {
            anyhow::bail!(
                "could not find project root starting from {} \
                 (expected tools/orch/Cargo.toml, .orch/, an orch package, or config.toml + actions/)",
                start.display()
            );
        }
    }
}

fn is_orch_package_dir(dir: &Path) -> bool {
    let cargo = dir.join("Cargo.toml");
    if !cargo.is_file() {
        return false;
    }
    fs::read_to_string(&cargo)
        .map(|t| {
            t.lines().any(|l| {
                let s = l.trim();
                s == "name = \"orch\"" || s == "name = 'orch'"
            })
        })
        .unwrap_or(false)
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix("UNC\\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses_empty() {
        let cfg: OrchConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.actions_dirs, vec![".orch/actions"]);
        assert_eq!(cfg.workflows_dirs, vec![".orch/workflows"]);
    }
}
