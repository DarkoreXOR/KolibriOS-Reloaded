//! Persistent filesystem image creation, validation, and reuse logic.

use crate::config::resolve;
use crate::paths::{images_dir, persistent_image_path};
use crate::util::{find_python, run_checked};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Outcome of an `mkfs` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MkfsOutcome {
    Created,
    Reused,
    ForceRecreated,
}

impl MkfsOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Reused => "reused",
            Self::ForceRecreated => "force-recreated",
        }
    }
}

/// Supported persistent regression filesystem types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilesystemKind {
    Exfat,
    Ntfs,
}

impl FilesystemKind {
    pub fn all() -> &'static [Self] {
        &[Self::Exfat, Self::Ntfs]
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "exfat" => Ok(Self::Exfat),
            "ntfs" => Ok(Self::Ntfs),
            other => bail!(
                "unknown filesystem `{other}`\nSupported: exfat, ntfs\nRun `help mkfs` for details."
            ),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Exfat => "exfat",
            Self::Ntfs => "ntfs",
        }
    }

    pub fn generator_script(self) -> &'static str {
        match self {
            Self::Exfat => "tools/mkfs_utils/create_exfat_image.py",
            Self::Ntfs => "tools/mkfs_utils/create_ntfs_image.py",
        }
    }

    /// OEM / magic check at offset 3 for quick validity probing.
    pub fn minimum_size_bytes(self) -> u64 {
        match self {
            Self::Exfat => 1024 * 1024, // 1 MiB practical minimum for exFAT
            Self::Ntfs => 8 * 1024 * 1024,
        }
    }

    pub fn effective_size(self, requested: u64) -> u64 {
        requested.max(self.minimum_size_bytes())
    }

    /// OEM / magic check at offset 3 for quick validity probing.
    pub fn oem_marker(self) -> Option<&'static [u8]> {
        match self {
            Self::Exfat => Some(b"EXFAT   "),
            Self::Ntfs => Some(b"NTFS    "),
        }
    }
}

/// Parse human-readable sizes: `4M`, `128M`, `4096`, `1G`.
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("image size is empty");
    }
    let upper = s.to_ascii_uppercase();
    let (num_part, mult) = if upper.ends_with('K') {
        (&upper[..upper.len() - 1], 1024u64)
    } else if upper.ends_with('M') {
        (&upper[..upper.len() - 1], 1024 * 1024)
    } else if upper.ends_with('G') {
        (&upper[..upper.len() - 1], 1024 * 1024 * 1024)
    } else if upper.ends_with('B') && upper.len() > 1 {
        (&upper[..upper.len() - 1], 1u64)
    } else {
        (upper.as_str(), 1u64)
    };

    let value: u64 = if mult == 1 {
        num_part
            .parse()
            .with_context(|| format!("invalid size `{s}`"))?
    } else {
        let f: f64 = num_part
            .parse()
            .with_context(|| format!("invalid size `{s}`"))?;
        if f <= 0.0 {
            bail!("size must be positive, got `{s}`");
        }
        (f * mult as f64).round() as u64
    };

    if value == 0 {
        bail!("size must be > 0");
    }
    Ok(value)
}

pub fn format_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes % MIB == 0 {
        format!("{}M", bytes / MIB)
    } else if bytes % 1024 == 0 {
        format!("{}K", bytes / 1024)
    } else {
        format!("{bytes}B")
    }
}

pub fn resolve_disk_image(root: &Path, disk: &str) -> Result<PathBuf> {
    let fs = FilesystemKind::parse(disk)?;
    Ok(persistent_image_path(root, fs.name()))
}

pub fn image_is_valid(path: &Path, expect_size: u64, fs: FilesystemKind) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if meta.len() != expect_size {
        return false;
    }
    if let Some(marker) = fs.oem_marker() {
        return oem_name_matches(path, marker);
    }
    true
}

fn oem_name_matches(path: &Path, marker: &[u8]) -> bool {
    let mut buf = [0u8; 11];
    match fs::File::open(path).and_then(|mut f| {
        use std::io::Read;
        f.read_exact(&mut buf)
    }) {
        Ok(()) => &buf[3..3 + marker.len()] == marker,
        Err(_) => false,
    }
}

/// Create or reuse a persistent regression image under `./images/`.
pub fn mkfs(
    root: &Path,
    python: &str,
    fs: FilesystemKind,
    size_bytes: u64,
    force: bool,
    dry_run: bool,
) -> Result<(PathBuf, MkfsOutcome)> {
    let out_dir = images_dir(root);
    let out = persistent_image_path(root, fs.name());
    let generator = resolve(root, fs.generator_script());

    if !generator.is_file() && !dry_run {
        bail!(
            "ERROR: mkfs generator missing\nExpected: {}",
            generator.display()
        );
    }

    if !dry_run {
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("mkdir {}", out_dir.display()))?;
    }

    let valid = !force && image_is_valid(&out, size_bytes, fs);
    if valid {
        eprintln!(
            "mkfs {} {}: reused existing {}",
            fs.name(),
            format_size(size_bytes),
            out.display()
        );
        eprintln!("outcome: reused");
        return Ok((out, MkfsOutcome::Reused));
    }

    if out.is_file() && !force {
        let actual = out.metadata().map(|m| m.len()).unwrap_or(0);
        if actual != size_bytes {
            eprintln!(
                "WARNING: existing {} has size {} but requested {}; recreating",
                out.display(),
                actual,
                size_bytes
            );
        } else if !oem_name_matches(&out, fs.oem_marker().unwrap_or(b"")) {
            eprintln!(
                "WARNING: existing {} failed filesystem probe; recreating",
                out.display()
            );
        }
    }

    let outcome = if force && out.is_file() {
        MkfsOutcome::ForceRecreated
    } else {
        MkfsOutcome::Created
    };

    let py = find_python(python).context("python not found (needed for mkfs generators)")?;
    let size_arg = format_size(size_bytes);
    let mut args = vec![
        generator.to_string_lossy().into_owned(),
        "--size".into(),
        size_arg,
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];
    if force || out.is_file() {
        args.push("--force".into());
    }

    eprintln!(
        "mkfs {} {}: generating via {}",
        fs.name(),
        format_size(size_bytes),
        generator.display()
    );
    run_checked("mkfs", &py, &args, root, &[], dry_run)?;

    if !dry_run && !image_is_valid(&out, size_bytes, fs) {
        bail!(
            "ERROR: mkfs output invalid after generation\nExpected: {} ({} bytes)",
            out.display(),
            size_bytes
        );
    }

    eprintln!("outcome: {}", outcome.as_str());
    eprintln!("  path: {}", out.display());
    Ok((out, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_variants() {
        assert_eq!(parse_size("4M").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_size("128m").unwrap(), 128 * 1024 * 1024);
        assert_eq!(parse_size("4096").unwrap(), 4096);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("512K").unwrap(), 512 * 1024);
    }

    #[test]
    fn parse_size_rejects_empty() {
        assert!(parse_size("").is_err());
        assert!(parse_size("0M").is_err());
    }

    #[test]
    fn unknown_filesystem() {
        assert!(FilesystemKind::parse("btrfs").is_err());
    }

    #[test]
    fn disk_resolution() {
        let root = Path::new("/repo");
        let p = resolve_disk_image(root, "exfat").unwrap();
        assert!(p.ends_with("images/exfat-image.img"));
        assert!(resolve_disk_image(root, "unknown").is_err());
    }
}
