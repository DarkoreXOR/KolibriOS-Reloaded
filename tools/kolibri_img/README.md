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
| `delete <img> <NAME>` | Delete a root 8.3 file on a **writable** copy |
| `replace <img> <NAME> <host-file>` | Replace a root 8.3 file on a **writable** copy |

`delete` / `replace` refuse filenames that look like the immutable `kolibrios-*.img` reference.

Always keep `kolibrios-*-en_US.img` at the repo root read-only; operate on `tmp_images/` copies.

See [`../../docs/_meta/project-structure.md`](../../docs/_meta/project-structure.md) and [`../../docs/migration/cut-a-final-architecture.md`](../../docs/migration/cut-a-final-architecture.md).
