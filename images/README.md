# Persistent filesystem regression images

Stable paths used by the orchestrator `mkfs` and `run --disk:TYPE` commands:

| Filesystem | Path |
|------------|------|
| exFAT | `images/exfat-image.img` |
| NTFS | `images/ntfs-image.img` |

Create or reuse images:

```powershell
cargo run --manifest-path orch/Cargo.toml -- mkfs exfat 4M
cargo run --manifest-path orch/Cargo.toml -- mkfs ntfs 8M
```

Images are gitignored (regenerated deterministically). Do not delete them
casually during active regression work — use `mkfs … --force` to recreate.

The orchestrator `clean` command never removes this directory.
