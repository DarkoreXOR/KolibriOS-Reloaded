# Repository layout (actual)

Evidence labels: see [`evidence-policy.md`](evidence-policy.md).

## Top-level directories

| Path | Role |
|------|------|
| [`kernel/`](../../kernel/) | Upstream KolibriOS **FASM** kernel sources and its own build scripts. Produces `kernel.mnt`. |
| [`rust_kernel/`](../../rust_kernel/) | **Rust** Cargo workspace for the freestanding kernel migration. Clear boundary vs `kernel/`. |
| [`tools/`](../../tools/) | Host utilities (image inspect/CoW, etc.). **Not** linked into the kernel. |
| [`docs/`](../../docs/) | Architecture, compatibility, migration, and tooling notes for this repo. |
| [`tools/fasm/`](../../tools/fasm/) | Vendored FASM toolchain (`FASM.EXE`, includes, examples). |
| [`dev_build/`](../../dev_build/) | Disposable CoW / test images and temp artifacts (gitignored). Script default: `dev_build/test/`. Delete unused temps after use (see `.cursor/rules/dev-build.mdc`). |
| [`images/`](../../images/) | Persistent filesystem regression disks (`exfat-image.img`, `ntfs-image.img`). |
| [`scripts/`](../../scripts/) | Plain Python project automation (build, image, QEMU, clean, doctor). |
| [`project/`](../../project/) | CONFIG_DATA (`build.toml`). |
| `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img` | **Original reference floppy image — read-only.** Do not modify in place. |

Name choice: `rust_kernel/` (not `kernel-rs/` or a root `crates/`) so the FASM vs Rust split is obvious next to `kernel/`.

## Rust kernel organization

Cargo workspace root: [`rust_kernel/Cargo.toml`](../../rust_kernel/Cargo.toml).

```text
rust_kernel/
  Cargo.toml                 # workspace members
  kolibri_utils/             # Cuts A–AB freestanding utils (staticlib + rlib)
    Cargo.toml
    i686-kolibri-none.json   # custom freestanding i686 target
    build-utf8to16.ps1       # Cut AB: host test + freestanding build + extract all blobs
    build-pid-to-slot.ps1    # Cut AA helper (prior; still builds full blob set)
    build-*.ps1              # per-cut helpers (A–AA); prefer python scripts/build.py
    scripts/extract_phase_c_probe.py
    scripts/extract_reloc_free_text.py
    out/                     # generated *.bin blobs (gitignored)
    src/                     # crc, unicode, utf8to16, pid_to_slot, …, ffi.rs
  target/                    # local build output (gitignored; may be overridden by CARGO_TARGET_DIR)
```

Preferred automation: [`../../scripts/`](../../scripts/) — Python scripts extract registered blobs and sync `USE_RUST_*` gates from [`project/build.toml`](../../project/build.toml).

Phase C FASM glue: [`kernel/rust/phase_c.inc`](../../kernel/rust/phase_c.inc). Docs: [`../migration/phase-c-integration.md`](../migration/phase-c-integration.md).  
Cut A Unicode/CRC embeds: `kernel/rust/{crc,utf16,cp866,utf8}.inc` + gates in `kernel/{crc,unicode}.inc`.  
**Current status (Cuts A–AB):** [`../migration/migration-plan.md`](../migration/migration-plan.md).  
**Latest cut:** [`../migration/cut-ab-implementation.md`](../migration/cut-ab-implementation.md).  
**Cut A baseline architecture:** [`../migration/cut-a-final-architecture.md`](../migration/cut-a-final-architecture.md).

Build from the workspace directory:

```text
cd rust_kernel
cargo test -p kolibri_utils
cargo +nightly build -Z build-std=core,compiler_builtins -Z json-target-spec `
  -p kolibri_utils --release --target kolibri_utils/i686-kolibri-none.json
```

Preferred one-shot for **all** current blobs:  
`powershell -File rust_kernel/kolibri_utils/build-utf8to16.ps1`  
Or: `python scripts/build.py`

See [`../architecture/build-system.md`](../architecture/build-system.md) and [`../architecture/boot-sequence.md`](../architecture/boot-sequence.md).

Summary (**LOCAL FACT**):

1. `kernel/Makefile` → `fasm -m 262144 kernel.asm bin/kernel.mnt` (after writing ephemeral `lang.inc`).
2. Loaders place the flat binary at physical `0x10000`.
3. 16-bit boot (`bootbios.inc`) → `B32` → paging → `high_code` → `osloop`.

**Status:** [`kernel/init.inc`](../../kernel/init.inc) restored (2026-08-09) from upstream commit matching the reference image (`944d74f01`). Tree assembles and boots. Details: [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md). Historical notes: [`upstream-init-diff.md`](upstream-init-diff.md).

## FASM toolchain usage

Repo-local assembler:

```text
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm <out>
```

Use this tree’s `./tools/fasm/` rather than assuming a system-wide `fasm` on `PATH`. Include paths are relative to the assembled file’s directory (FASM default), matching how `kernel/` was written.

## QEMU testing

System QEMU (this machine): `C:\Program Files\qemu\qemu-system-i386.exe` (v11.x). It is **not** necessarily on `PATH`; use the full path or add it to PATH yourself.

Boot the **reference** image read-only via a disposable copy (preferred):

```text
tools\kolibri_img cow kolibrios-0.7.7.0-9160-g944d74f01-en_US.img dev_build\boot-smoke.img

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda dev_build\boot-smoke.img -boot a `
  -m 256 -display none -serial stdio `
  -no-reboot -no-shutdown
```

For interactive VGA, omit `-display none` (uses the QEMU window).  
`-snapshot` is an alternative that avoids writing the backing file, but an explicit CoW file under `dev_build/` is clearer for patch experiments.

Delete disposable images when finished:

```text
Remove-Item dev_build\boot-smoke.img -Force
```

## Reference image rules

| Rule | Detail |
|------|--------|
| Immutable reference | `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img` at repo root |
| Size / type | 1 474 560 bytes = 2880×512 — **FAT12** 1.44 MiB floppy (`OEM=KOLIBRI`, `FS=FAT12`) |
| Payload | Root contains `KERNEL.MNT` (~106 618 bytes) plus apps/dirs |
| SHA-256 (this tree) | `1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA` |
| Never | Open the reference for write, `fasm` into it, or mount it with write tools |
| Always | `kolibri_img cow …` (or `Copy-Item`) to `dev_build/`, experiment on the copy, compare back to the original if needed |

## Image utility: `tools/kolibri_img`

Host crate (own `Cargo.toml`, **not** a `rust_kernel` workspace member):

```text
cd tools/kolibri_img
cargo run --release -- inspect ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img
cargo run --release -- ls ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\work.img
cargo run --release -- extract ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img KERNEL.MNT ..\..\dev_build\KERNEL.MNT
cargo run --release -- delete ..\..\dev_build\work.img DOCPACK
cargo run --release -- replace ..\..\dev_build\work.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

Commands: `inspect`, `ls [--path DIR]`, `cow`, `extract`, `delete`, `replace`.  
Supports FAT12/FAT16 BPB detection; directory walk for simple 8.3 paths; refuses `cow` onto the same path; `delete`/`replace` refuse filenames that look like the immutable `kolibrios-*.img` reference.

**LOCAL FACT:** Assembled hybrid `kernel.mnt` with Cuts A–AB production gates on is on the order of **~240 KiB** uncompressed (e.g. ~240664 bytes after Cut AB). Larger than the kerpack’d `KERNEL.MNT` on the reference floppy (~107 KiB). Replacing without `kerpack` requires freeing clusters first (authorized deletes: `DOCPACK`, `DEVELOP/FASM`, `3D/VIEW3DS`, `GAMES/DINO`).

## Assumptions, limitations, blockers

| Item | Status |
|------|--------|
| Rust workspace lives only under `rust_kernel/` | **Done** |
| FASM `kernel/` build | **Done** — assembles with vendored `tools/fasm/FASM.EXE` |
| Boot smoke (QEMU + CoW image) | **Done** — desktop reached with rebuilt uncompressed `KERNEL.MNT` |
| Hybrid Rust↔FASM link | **Cuts A–AB complete** — reloc-free blobs + trampolines; see [`../migration/migration-plan.md`](../migration/migration-plan.md) |
| Host `kerpack` | **Absent** — optional; without it, free floppy space before replacing `KERNEL.MNT` |
| QEMU on PATH | May be absent; full path under `C:\Program Files\qemu\` works here |
| `kolibri_img` replace/delete | **Implemented** — mutate CoW copies only |
| Original image | Must remain byte-identical; verify with SHA-256 above after experiments |

## Related docs

- FASM build details: [`../architecture/build-system.md`](../architecture/build-system.md)
- Boot sequence: [`../architecture/boot-sequence.md`](../architecture/boot-sequence.md)
- Migration status: [`../migration/migration-plan.md`](../migration/migration-plan.md)
- Latest cut (AB): [`../migration/cut-ab-implementation.md`](../migration/cut-ab-implementation.md)
- Cut A Rust utils: [`../migration/cut-a-implementation.md`](../migration/cut-a-implementation.md)
- Cut A final architecture: [`../migration/cut-a-final-architecture.md`](../migration/cut-a-final-architecture.md)
- Scripts: [`../../scripts/README.md`](../../scripts/README.md)
- FASM baseline restoration: [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md)
- Source inventory: [`source-inventory.md`](source-inventory.md)
