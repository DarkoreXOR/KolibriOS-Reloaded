# `kolibri_img`

Host-side helper for KolibriOS `.img` files. **Not** part of `rust_kernel/`.

## Build

```text
cd tools/kolibri_img
cargo build --release
```

Binary: `tools/kolibri_img/target/release/kolibri_img.exe`  
(Avoid a leftover `CARGO_TARGET_DIR` pointing at `rust_kernel/target` when building this crate.)

## Commands

| Command | Purpose |
|---------|---------|
| `inspect <img>` | BPB / FAT kind / `KERNEL.MNT` location |
| `ls <img> [--path DIR]` | List 8.3 root or subdirectory |
| `cow <src.img> <dst.img>` | Byte-copy to a disposable image (refuses same path) |
| `extract <img> <NAME> <out>` | Extract a root 8.3 file (e.g. `KERNEL.MNT`) |
| `delete [--ignore-missing] …` | Delete a file on a **writable** copy. `--ignore-missing` succeeds if the path is already absent. |
| `replace <img> <NAME> <host-file>` | Replace a root 8.3 file on a **writable** copy |
| `put <img> <NAME> <host-file>` | Create or replace a root 8.3 file on a **writable** copy |

`delete` / `replace` / `put` refuse filenames that look like the immutable `kolibrios-*.img` reference.

Always keep `kolibrios-*-en_US.img` at the repo root read-only; operate on `dev_build/` copies.

See [`../../docs/_meta/project-structure.md`](../../docs/_meta/project-structure.md),
[`../../docs/migration/migration-plan.md`](../../docs/migration/migration-plan.md),
and [`../../docs/migration/cut-a-final-architecture.md`](../../docs/migration/cut-a-final-architecture.md).
