# Deprecated — use `dev_build/`

This directory is **not used**. Disposable boot images, screendumps, and other
temporary artifacts belong under **`dev_build/`** (orchestrator default:
`dev_build/test/`). Persistent FS regression disks belong under **`images/`**.

| Role | Path |
|------|------|
| Disposable CoW / boot images | `dev_build/test/` via `orch @prepare_image` |
| Screendumps / ad-hoc temps | `dev_build/` — delete after use |
| Persistent exFAT/NTFS disks | `images/exfat-image.img`, `images/ntfs-image.img` |
| Full wipe of disposables | `orch @clean` (removes `build/` + `dev_build/`) |

```powershell
cargo run --manifest-path tools/orch/Cargo.toml -- --% @mkfs exfat 4M
cargo run --manifest-path tools/orch/Cargo.toml -- --% run:dev
cargo run --manifest-path tools/orch/Cargo.toml -- --% @clean
```

See `.cursor/rules/dev-build.mdc`.
