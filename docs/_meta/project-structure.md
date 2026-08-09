# Repository layout (actual)

Evidence labels: see [`evidence-policy.md`](evidence-policy.md).

## Top-level directories

| Path | Role |
|------|------|
| [`kernel/`](../../kernel/) | Upstream KolibriOS **FASM** kernel sources and its own build scripts. Produces `kernel.mnt`. |
| [`rust_kernel/`](../../rust_kernel/) | **Rust** Cargo workspace for the freestanding kernel migration. Clear boundary vs `kernel/`. |
| [`tools/`](../../tools/) | Host utilities (image inspect/CoW, etc.). **Not** linked into the kernel. |
| [`docs/`](../../docs/) | Architecture, compatibility, migration, and tooling notes for this repo. |
| [`fasm/`](../../fasm/) | Vendored FASM toolchain (`FASM.EXE`, includes, examples). |
| [`tmp_images/`](../../tmp_images/) | Disposable CoW / test images (gitignored). Never put the reference image here as the only copy. |
| `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img` | **Original reference floppy image — read-only.** Do not modify in place. |

Name choice: `rust_kernel/` (not `kernel-rs/` or a root `crates/`) so the FASM vs Rust split is obvious next to `kernel/`.

## Rust kernel organization

Cargo workspace root: [`rust_kernel/Cargo.toml`](../../rust_kernel/Cargo.toml).

```text
rust_kernel/
  Cargo.toml                 # workspace members
  kolibri_utils/             # Cut A: CRC32 + Unicode helpers (staticlib + rlib)
    Cargo.toml
    i686-kolibri-none.json   # custom freestanding i686 target
    build-cut-a.ps1          # host test + freestanding build helper
    build-phase-c.ps1        # Phase C: build + extract probe blob
    build-crc.ps1            # Phase D: build + extract CRC (+ probe) blobs
    build-utf16.ps1          # UTF-16 encode: build + extract utf16 (+ CRC + probe)
    build-cp866.ps1          # CP866 encode: build + extract cp866 (+ prior blobs)
    build-utf8.ps1           # UTF-8 decode: build + extract all Cut A blobs + probe
    scripts/extract_phase_c_probe.py
    scripts/extract_reloc_free_text.py
    out/                     # generated *.bin blobs (gitignored)
    fasm/trampolines.inc.example
    src/{lib,crc,unicode,ffi}.rs
  target/                    # local build output (gitignored; may be overridden by CARGO_TARGET_DIR)
```

Phase C FASM glue: [`kernel/rust/phase_c.inc`](../../kernel/rust/phase_c.inc). Docs: [`../migration/phase-c-integration.md`](../migration/phase-c-integration.md).  
Phase D CRC: [`kernel/rust/crc.inc`](../../kernel/rust/crc.inc) + `USE_RUST_CRC` in [`kernel/crc.inc`](../../kernel/crc.inc). Docs: [`../migration/crc32-migration.md`](../migration/crc32-migration.md).  
UTF-16 encode: [`kernel/rust/utf16.inc`](../../kernel/rust/utf16.inc) + `USE_RUST_UTF16` in [`kernel/unicode.inc`](../../kernel/unicode.inc). Docs: [`../migration/utf16-migration.md`](../migration/utf16-migration.md).  
CP866 encode: [`kernel/rust/cp866.inc`](../../kernel/rust/cp866.inc) + `USE_RUST_CP866` in [`kernel/unicode.inc`](../../kernel/unicode.inc). Docs: [`../migration/cp866-migration.md`](../migration/cp866-migration.md).  
UTF-8 decode: [`kernel/rust/utf8.inc`](../../kernel/rust/utf8.inc) + `USE_RUST_UTF8` in [`kernel/unicode.inc`](../../kernel/unicode.inc). Docs: [`../migration/utf8-migration.md`](../migration/utf8-migration.md).  
**Cut A baseline:** [`../migration/cut-a-final-architecture.md`](../migration/cut-a-final-architecture.md).

Build from the workspace directory:

```text
cd rust_kernel
cargo test -p kolibri_utils
cargo +nightly build -Z build-std=core,compiler_builtins -Z json-target-spec `
  -p kolibri_utils --release --target kolibri_utils/i686-kolibri-none.json
```

Or: `powershell -File rust_kernel/kolibri_utils/build-cut-a.ps1 -All`  
Preferred one-shot for all Cut A blobs: `powershell -File rust_kernel/kolibri_utils/build-utf8.ps1`  
Phase C blob only: `powershell -File rust_kernel/kolibri_utils/build-phase-c.ps1`  
CRC + probe: `powershell -File rust_kernel/kolibri_utils/build-crc.ps1`  
UTF-16 (+ CRC + probe): `powershell -File rust_kernel/kolibri_utils/build-utf16.ps1`  
CP866 (+ prior): `powershell -File rust_kernel/kolibri_utils/build-cp866.ps1`

See [`../architecture/build-system.md`](../architecture/build-system.md) and [`../architecture/boot-sequence.md`](../architecture/boot-sequence.md).

Summary (**LOCAL FACT**):

1. `kernel/Makefile` → `fasm -m 262144 kernel.asm bin/kernel.mnt` (after writing ephemeral `lang.inc`).
2. Loaders place the flat binary at physical `0x10000`.
3. 16-bit boot (`bootbios.inc`) → `B32` → paging → `high_code` → `osloop`.

**Status:** [`kernel/init.inc`](../../kernel/init.inc) restored (2026-08-09) from upstream commit matching the reference image (`944d74f01`). Tree assembles and boots. Details: [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md). Historical notes: [`upstream-init-diff.md`](upstream-init-diff.md).

## FASM toolchain usage

Repo-local assembler:

```text
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm <out>
```

Use this tree’s `./fasm/` rather than assuming a system-wide `fasm` on `PATH`. Include paths are relative to the assembled file’s directory (FASM default), matching how `kernel/` was written.

## QEMU testing

System QEMU (this machine): `C:\Program Files\qemu\qemu-system-i386.exe` (v11.x). It is **not** necessarily on `PATH`; use the full path or add it to PATH yourself.

Boot the **reference** image read-only via a disposable copy (preferred):

```text
tools\kolibri_img cow kolibrios-0.7.7.0-9160-g944d74f01-en_US.img tmp_images\boot-smoke.img

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda tmp_images\boot-smoke.img -boot a `
  -m 256 -display none -serial stdio `
  -no-reboot -no-shutdown
```

For interactive VGA, omit `-display none` (uses the QEMU window).  
`-snapshot` is an alternative that avoids writing the backing file, but an explicit CoW file under `tmp_images/` is clearer for patch experiments.

Delete disposable images when finished:

```text
Remove-Item tmp_images\boot-smoke.img -Force
```

## Reference image rules

| Rule | Detail |
|------|--------|
| Immutable reference | `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img` at repo root |
| Size / type | 1 474 560 bytes = 2880×512 — **FAT12** 1.44 MiB floppy (`OEM=KOLIBRI`, `FS=FAT12`) |
| Payload | Root contains `KERNEL.MNT` (~106 618 bytes) plus apps/dirs |
| SHA-256 (this tree) | `1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA` |
| Never | Open the reference for write, `fasm` into it, or mount it with write tools |
| Always | `kolibri_img cow …` (or `Copy-Item`) to `tmp_images/`, experiment on the copy, compare back to the original if needed |

## Image utility: `tools/kolibri_img`

Host crate (own `Cargo.toml`, **not** a `rust_kernel` workspace member):

```text
cd tools/kolibri_img
cargo run --release -- inspect ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img
cargo run --release -- ls ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\work.img
cargo run --release -- extract ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img KERNEL.MNT ..\..\tmp_images\KERNEL.MNT
cargo run --release -- delete ..\..\tmp_images\work.img DOCPACK
cargo run --release -- replace ..\..\tmp_images\work.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

Commands: `inspect`, `ls [--path DIR]`, `cow`, `extract`, `delete`, `replace`.  
Supports FAT12/FAT16 BPB detection; directory walk for simple 8.3 paths; refuses `cow` onto the same path; `delete`/`replace` refuse filenames that look like the immutable `kolibrios-*.img` reference.

**LOCAL FACT:** Assembled hybrid `kernel.mnt` is **223080** bytes with all Cut A Rust switches on (~218 KiB; docs may say “~222 KiB”). Larger than the kerpack’d `KERNEL.MNT` on the reference floppy (~107 KiB). Replacing without `kerpack` requires freeing clusters first (e.g. delete `DOCPACK`).

## Assumptions, limitations, blockers

| Item | Status |
|------|--------|
| Rust workspace lives only under `rust_kernel/` | **Done** |
| FASM `kernel/` build | **Done** — assembles with vendored `fasm/FASM.EXE` |
| Boot smoke (QEMU + CoW image) | **Done** — desktop reached with rebuilt uncompressed `KERNEL.MNT` |
| Hybrid Rust↔FASM link | **Cut A complete** — reloc-free blobs + trampolines; see [`../migration/cut-a-final-architecture.md`](../migration/cut-a-final-architecture.md) |
| Host `kerpack` | **Absent** — optional; without it, free floppy space before replacing `KERNEL.MNT` |
| QEMU on PATH | May be absent; full path under `C:\Program Files\qemu\` works here |
| `kolibri_img` replace/delete | **Implemented** — mutate CoW copies only |
| Original image | Must remain byte-identical; verify with SHA-256 above after experiments |

## Related docs

- FASM build details: [`../architecture/build-system.md`](../architecture/build-system.md)
- Boot sequence: [`../architecture/boot-sequence.md`](../architecture/boot-sequence.md)
- Cut A Rust utils: [`../migration/cut-a-implementation.md`](../migration/cut-a-implementation.md)
- Cut A final architecture: [`../migration/cut-a-final-architecture.md`](../migration/cut-a-final-architecture.md)
- FASM baseline restoration: [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md)
- Source inventory: [`source-inventory.md`](source-inventory.md)
