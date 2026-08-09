//! Configuration for `kolibri_build` (`tools/build/config.toml`).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rust: RustConfig,
    pub kernel: KernelConfig,
    pub image: ImageConfig,
    pub qemu: QemuConfig,
    #[serde(default)]
    pub cleanup: CleanupConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RustConfig {
    pub workspace: String,
    pub package: String,
    pub target_json: String,
    pub cargo_target_dir: String,
    pub out_dir: String,
    pub toolchain: String,
    pub run_host_tests: bool,
    pub force_recompile_staticlib: bool,
    pub clear_rustflags: bool,
    pub extract: ExtractConfig,
    pub blobs: Vec<BlobSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractConfig {
    pub python: String,
    pub generic_script: String,
    pub probe_script: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlobKind {
    Generic,
    Probe,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlobSpec {
    pub kind: BlobKind,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub expect_ret_imm: Option<u32>,
    pub out: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KernelConfig {
    pub fasm: String,
    pub asm: String,
    pub output: String,
    pub lang: String,
    pub memory_kib: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageConfig {
    pub tool_manifest: String,
    pub tool_bin: String,
    pub base_image: String,
    pub output_dir: String,
    pub filename_pattern: String,
    pub delete_before_replace: Vec<String>,
    pub kernel_fat_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QemuConfig {
    pub executables: Vec<String>,
    pub args: Vec<String>,
    #[serde(default)]
    pub headless_extra_args: Vec<String>,
    /// Extra args for `ref` (boot immutable base image). Default empty in serde;
    /// config.toml should include `-snapshot`.
    #[serde(default)]
    pub reference_extra_args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CleanupConfig {
    #[serde(default)]
    pub delete_image_on_success: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let cfg: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if self.rust.blobs.is_empty() {
            bail!("config [rust.blobs] is empty — hybrid kernel needs reloc-free blobs");
        }
        for (i, b) in self.rust.blobs.iter().enumerate() {
            match b.kind {
                BlobKind::Generic => {
                    if b.section.as_ref().map(|s| s.is_empty()).unwrap_or(true)
                        || b.symbol.as_ref().map(|s| s.is_empty()).unwrap_or(true)
                        || b.expect_ret_imm.is_none()
                    {
                        bail!(
                            "rust.blobs[{i}] kind=generic requires section, symbol, expect_ret_imm"
                        );
                    }
                }
                BlobKind::Probe => {}
            }
            if b.out.is_empty() {
                bail!("rust.blobs[{i}] missing out");
            }
        }
        if self.image.base_image.is_empty() {
            bail!("image.base_image is empty");
        }
        if self.qemu.executables.is_empty() {
            bail!("qemu.executables is empty");
        }
        Ok(())
    }
}

/// Find repository root by walking up from `start` until `kernel/kernel.asm` exists.
pub fn find_repo_root(start: &Path) -> Result<PathBuf> {
    let mut cur = if start.is_file() {
        start
            .parent()
            .unwrap_or(start)
            .to_path_buf()
    } else {
        start.to_path_buf()
    };

    // Best-effort canonicalize so relative `--config` paths cannot invent a fake root.
    if let Ok(canon) = fs::canonicalize(&cur) {
        cur = strip_verbatim_prefix(canon);
    }

    loop {
        let marker = cur.join("kernel").join("kernel.asm");
        if marker.is_file() {
            return Ok(cur);
        }
        if !cur.pop() {
            bail!(
                "could not find repository root (no kernel/kernel.asm) starting from {}",
                start.display()
            );
        }
    }
}

/// Resolve repository root from the config file location (walk upward).
pub fn repo_root_from_config(config_path: &Path) -> Result<PathBuf> {
    find_repo_root(config_path)
}

pub fn resolve(root: &Path, rel: &str) -> PathBuf {
    let p = Path::new(rel);
    if p.is_absolute() {
        strip_verbatim_prefix(p.to_path_buf())
    } else {
        strip_verbatim_prefix(root.join(p))
    }
}

/// Windows `fs::canonicalize` often yields `\\?\C:\...`, which older tools (FASM) reject.
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        // UNC: \\?\UNC\server\share → \\server\share
        if let Some(unc) = rest.strip_prefix("UNC\\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    path
}
