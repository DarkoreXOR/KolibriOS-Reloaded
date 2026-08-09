# FASM baseline restoration (Cut B)

**Date:** 2026-08-09  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

## Goal

Recover a clean, reproducible FASM kernel build that assembles and boots, without hybrid Rust↔FASM linking.

## What was wrong

**LOCAL FACT:** [`kernel/init.inc`](../../kernel/init.inc) was byte-identical to [`kernel/bus/usb/init.inc`](../../kernel/bus/usb/init.inc) (SHA-256 `32BF12B5…`). Header: “Initialization of the USB subsystem”. Symbols required by [`kernel/kernel.asm`](../../kernel/kernel.asm) (`test_cpu`, `acpi_locate`, `init_BIOS32`, `mem_test`, `init_mem`, `init_page_map`) were missing.

**LOCAL FACT:** No `.git` history in this tree — cannot recover the file from local commits.

**LOCAL FACT:** Duplicate-file scan under `kernel/` found only this corruption among unexpected same-content pairs (other same-hash pairs are intentional loader copies under `bootloader/` ↔ `sec_loader/`).

## How the correct source was identified

| Evidence | Result |
|----------|--------|
| Reference image name `…-9160-g944d74f01-…` | Implies upstream commit `944d74f01` |
| Fetch `https://raw.githubusercontent.com/KolibriOS/kolibrios/944d74f01/kernel/trunk/init.inc` | Early-init source defining all required symbols |
| Compare to `main` trunk `init.inc` | **Identical** (SHA-256 `F7391BA4…`, 14219 bytes) |
| Compare to pre-fetched [`docs/_upstream/init.inc`](../_upstream/init.inc) | **Identical** |
| Local consumers (`BOOT_LO.*`, `pg_data.*`, ACPI globals, `bios32_entry`, `tmp_page_tabs`, CPU caps) | Present in local `const.inc` / `data32.inc` / `acpi/acpi.inc` / `kernel.asm` |
| Local version string `v0.7.7.0` in `bootbios.inc` | Matches image major version; packed reference kernel uses `v0.7.7.0+9160` (revision insert only) |

**Confidence:** **HIGH** for restoring this `init.inc` into this tree.

## Action taken

Replaced `kernel/init.inc` with the verified upstream file (same bytes as `docs/_upstream/init.inc` / commit `944d74f01`). USB init remains solely via `kernel32.inc` → `bus/usb/init.inc`.

## Build verification

```text
Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc
```

**LOCAL FACT:** Assembles successfully — `221912` bytes, 8 passes (FASM 1.73.35).

Note: Makefile also builds `bin/kernel.bin` with `-dextended_primary_loader=1`; primary smoke path used `kernel.mnt` only.

## Image / boot verification

Reference `KERNEL.MNT` on the floppy is **kerpack-compressed** (~106 618 bytes) with the same KolibriOS boot header signature. Host `kerpack` is **not** vendored in this repo (`build.sh` expects an external Linux `kerpack`). Uncompressed `kernel.mnt` is a valid boot payload when the image has enough free clusters.

**LOCAL FACT:** FAT12 floppy had only ~29 free clusters; replacing with 221 912-byte kernel needs ~434 clusters. Procedure used:

```text
cd tools\kolibri_img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\boot-smoke.img
cargo run --release -- delete ..\..\tmp_images\boot-smoke.img DOCPACK
cargo run --release -- replace ..\..\tmp_images\boot-smoke.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

Reference image SHA-256 remained `1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA` after all experiments.

**QEMU** (`C:\Program Files\qemu\qemu-system-i386.exe`):

```text
-fda tmp_images\boot-smoke.img -boot a -m 256 -vga std -display none
-no-reboot -no-shutdown -qmp tcp:127.0.0.1:4445,server,nowait
```

After ~10 s: QMP `query-status` → `running`; screendump showed full KolibriOS desktop (taskbar, icons, wallpaper). Same smoke procedure on an unmodified CoW of the reference image also reached desktop. CPU reset count matched (2 — normal power-on path). Disposable images deleted after the test.

## Intentionally not done

- No Rust↔FASM hybrid link (Phase C)
- No changes to `crc.inc` / `unicode.inc`
- No redesign of memory/CPU init
- `kerpack` not vendored (optional size optimization for floppy fit without deleting `DOCPACK`)

## Remaining blockers for Phase C

1. ~~Hybrid link of `libkolibri_utils.a` into flat `kernel.mnt` still unwired.~~ **Resolved** for a reloc-free probe — see [`phase-c-integration.md`](phase-c-integration.md).
2. Optional: host `kerpack` for distributing a compressed kernel without freeing floppy space.
3. ~~Cut A CRC/Unicode need `rust-lld`.~~ **Resolved** — all four Cut A functions are reloc-free section extracts; see [`cut-a-final-architecture.md`](cut-a-final-architecture.md). (`rust-lld` may still be needed for *future* non-reloc-free functions.)
