# `orch`

Central project operations orchestrator for the hybrid FASM + Rust kernel
(Cuts A–AH). Configuration: [`config.toml`](config.toml).

**The orchestrator is the canonical entry point.** Inspect `help` before creating
ad-hoc scripts. See [`.cursor/rules/orchestrator.mdc`](../.cursor/rules/orchestrator.mdc).

## Usage (from repository root)

```powershell
cargo run --manifest-path orch/Cargo.toml -- run
cargo run --manifest-path orch/Cargo.toml -- help
```

| Command | Meaning |
|---------|---------|
| `run` / `qemu` | Rust → `kernel.mnt` → fresh `dev_build/test/*.img` → QEMU |
| `run -- --disk:ntfs --disk:exfat` | Attach persistent regression disks from `./images/` |
| `mkfs exfat 4M` | Create/reuse `./images/exfat-image.img` |
| `mkfs ntfs 8M` | Create/reuse `./images/ntfs-image.img` (`4M` request → 8M minimum) |
| `clean` | Remove `./build/` + `./dev_build/` (never `./images/`) |
| `help [topic]` | Discovery: `help mkfs`, `help run`, `help scripts`, `help tools` |
| `script <name>` | Run Rhai workflow from `orch/scripts/` |
| `tool <path>` | Invoke utility under `./tools/` |
| `ref` / `original` | Boot reference `.img` with `-snapshot` |
| `build` / `image` / `doctor` | As before |
| `testdisk` | Legacy (prefer `mkfs exfat`) |

Flags: `--dry-run`, `--skip-tests`, `--headless`, `--config <path>`.

## Artifact paths

| Path | Purpose |
|------|---------|
| `build/` | Production build artifacts |
| `dev_build/` | Disposable boot images, agent workspace |
| `images/` | Persistent filesystem regression images |

## Extension model

1. Reuse orchestrator commands/APIs
2. Add Rhai workflows under `orch/scripts/`
3. Extend Rust modules (`fs_image`, `qemu`, `tools`, …)
4. Add utilities under `tools/mkfs_utils/`, etc.

## Migrations

See [`config.toml`](config.toml) for blob/migration registry (Cuts A–AH).
