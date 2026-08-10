# Deprecated: use `dev_build/` instead

This directory is **no longer used** for new work.

| Old path | New path |
|----------|----------|
| `tmp_images/*.img` (disposable boot images) | `dev_build/test/` (orchestrator) |
| ad-hoc screendumps | `dev_build/` |
| persistent exFAT/NTFS regression disks | `images/exfat-image.img`, `images/ntfs-image.img` |

Use the orchestrator:

```powershell
cargo run --manifest-path orch/Cargo.toml -- help
cargo run --manifest-path orch/Cargo.toml -- mkfs exfat 4M
cargo run --manifest-path orch/Cargo.toml -- run -- --disk:exfat
```

Historical migration docs may still reference `tmp_images/` for completed cut
validation records.
