# `kolibri_build`

Repository-local orchestrator for the hybrid FASM + Rust kernel workflow
(Cuts A–AE). Configuration: [`config.toml`](config.toml).

## Usage (from repository root)

```powershell
cargo run --manifest-path tools/build/Cargo.toml -- run
```

| Command | Meaning |
|---------|---------|
| `run` / `qemu` | Rust blobs → `kernel.mnt` → fresh `build/test/*.img` → QEMU |
| `ref` / `original` | Boot immutable reference `.img` only (`-snapshot`, no rebuild) |
| `image` | Build + package only |
| `build` | Rust blobs + FASM assemble only |
| `doctor` | Tooling / path / migration-gate checks |

Flags: `--dry-run`, `--skip-tests`, `--headless`, `--config <path>`.

See the root [`README.md`](../../README.md) for the full developer workflow.

## Migrations

`config.toml` lists every reloc-free blob under `[[rust.blobs]]` and every
independent production gate under `[[rust.migrations]]` (Cuts A–AE; currently
34 gated symbols plus the Phase C probe blob). Each migration maps `blob` →
`USE_RUST_*` → `kernel/rust/*.inc` → gate assignment file. Set
`enabled = true|false` per cut; the orchestrator writes `USE_RUST_* = 0|1` into
`gate_file` before assemble. Doctor verifies the live tree matches the registry.

Latest production checkpoint: Cut AE (`ntfs_datetime_to_bdfe`) —
[`docs/migration/cut-ae-implementation.md`](../../docs/migration/cut-ae-implementation.md)
(after validation). Prior: Cut AD (`is_protective_mbr`).
Migration index: [`docs/migration/migration-plan.md`](../../docs/migration/migration-plan.md).
