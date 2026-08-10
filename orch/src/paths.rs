//! Repository artifact path conventions.

use crate::config::{resolve, Config};
use std::path::{Path, PathBuf};

/// Production build artifacts (kernel packaging output, release images).
pub const BUILD_DIR: &str = "build";

/// Development / agent test artifacts (disposable boot images, screendumps, logs).
pub const DEV_BUILD_DIR: &str = "dev_build";

/// Persistent filesystem regression images (exFAT, NTFS, …).
pub const IMAGES_DIR: &str = "images";

/// Resolve the disposable boot-image output directory from config.
pub fn dev_image_output_dir(cfg: &Config, root: &Path) -> PathBuf {
    resolve(root, &cfg.image.output_dir)
}

/// Resolve `./images/`.
pub fn images_dir(root: &Path) -> PathBuf {
    root.join(IMAGES_DIR)
}

/// Stable persistent image path: `./images/{fs}-image.img`.
pub fn persistent_image_path(root: &Path, filesystem: &str) -> PathBuf {
    images_dir(root).join(format!("{filesystem}-image.img"))
}

/// `./build/` and `./dev_build/` cleanup roots (never `./images/`).
pub fn cleanup_roots(root: &Path) -> Vec<PathBuf> {
    vec![root.join(BUILD_DIR), root.join(DEV_BUILD_DIR)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_image_naming() {
        let root = Path::new("/repo");
        assert_eq!(
            persistent_image_path(root, "exfat"),
            PathBuf::from("/repo/images/exfat-image.img")
        );
        assert_eq!(
            persistent_image_path(root, "ntfs"),
            PathBuf::from("/repo/images/ntfs-image.img")
        );
    }

    #[test]
    fn cleanup_excludes_images() {
        let root = Path::new("/repo");
        let roots = cleanup_roots(root);
        assert!(roots.iter().any(|p| p.ends_with("build")));
        assert!(roots.iter().any(|p| p.ends_with("dev_build")));
        assert!(!roots.iter().any(|p| p.ends_with("images")));
    }
}
