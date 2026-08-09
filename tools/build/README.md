# `kolibri_build`

Repository-local orchestrator for the hybrid FASM + Rust kernel workflow
(Cuts A–O). Configuration: [`config.toml`](config.toml).

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
| `doctor` | Tooling / path checks |

Flags: `--dry-run`, `--skip-tests`, `--headless`, `--config <path>`.

See the root [`README.md`](../../README.md) for the full developer workflow.
