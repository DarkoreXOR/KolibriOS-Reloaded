# Build System

Evidence labels: see [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

## Summary

**LOCAL FACT:** The kernel is a flat FASM binary (`format binary as "mnt"`), assembled from a single entry file [`kernel/kernel.asm`](../../kernel/kernel.asm). There is no ELF/PE linker step for the kernel image itself. Optional `kerpack` compression appears in alternate build scripts, not the primary Makefile.

Repo layout (FASM `kernel/` vs Rust `rust_kernel/`, image/QEMU rules): [`../_meta/project-structure.md`](../_meta/project-structure.md).

**Assembler:** use the vendored [`../../fasm/FASM.EXE`](../../fasm/FASM.EXE) (this repo does not require a system-wide `fasm` on `PATH`).

**Verified Windows one-liner** (ephemeral `lang.inc`):

```text
Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc
```

Produces **~240 KiB** uncompressed hybrid `kernel.mnt` with Cuts A–AB production
gates on (historical Cut A-only size was **223080** bytes / ~218 KiB). Distribution
floppy kernels are typically **kerpack**’d (~107 KiB on the reference image); host
`kerpack` is not in this tree — see FASM baseline doc.

Preferred orchestrated build (blobs + gate sync + FASM):

```text
cargo run --manifest-path orch/Cargo.toml -- build
```

Blob/gate registry: [`../../orch/config.toml`](../../orch/config.toml).
Migration status: [`../migration/migration-plan.md`](../migration/migration-plan.md).

## Primary build (Makefile)

**LOCAL FACT** — [`kernel/Makefile`](../../kernel/Makefile):

```text
make lang=en_US
```

Steps:

1. Validate `lang` ∈ `{en_US,ru_RU,de_DE,et_EE,es_ES}`.
2. Write ephemeral `lang.inc`: `lang fix <locale>`.
3. `fasm -m 262144 kernel.asm bin/kernel.mnt`
4. `fasm -m 262144 kernel.asm -dextended_primary_loader=1 bin/kernel.bin`
5. Remove `lang.inc`.
6. Separately: `fasm … bootloader/boot_fat12.asm bin/boot_fat12.bin`

## Alternate builds

| Mechanism | Output | Notes |
|-----------|--------|-------|
| `Tupfile.lua` | `kernel.mnt`, `kernel.mnt.ext_loader`, `kernel.mnt.pretest` | Optional `KERPACK_CMD`, build metadata defines |
| `build.sh` | `kernel.mnt`, optional pack/image copy | **LOCAL FACT:** also invokes `bootbios.asm` which is **absent** in this tree — treat script as partially stale |
| `build.bat` (root) | Only builds `kordldr.win.asm` | Not main kernel |

## Entry points

| Stage | Symbol | File | Address model |
|-------|--------|------|---------------|
| 16-bit | `start_of_code` | `bootbios.inc` | `org 0` in binary |
| Header | `jmp` + `'KolibriOS '` + version + `b32_offset` | `bootbios.inc` / `kernel_header` | File offset 0 |
| 32-bit low | `B32` | `kernel.asm` | Linked at `KERNEL_BASE` (`0x10000`) via `org $+KERNEL_BASE` |
| 32-bit high | `high_code` | `kernel.asm` | After `org OS_BASE+$` — VA `OS_BASE + file_offset` |
| Idle OS | `osloop` | `kernel.asm` | Running kernel |

**LOCAL FACT:** Header field `b32_offset = B32 - KERNEL_BASE` lets UEFI/extended loaders jump directly to 32-bit entry, skipping real mode.

## Include hierarchy

See [`../_meta/source-inventory.md`](../_meta/source-inventory.md). Hub files:

- `bootbios.inc` — boot
- `init.inc` — early init (**restored** 2026-08-09; see [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md))
- `kernel32.inc` — OS body
- `data32.inc` — globals / GDT / BSS

## Macros / construction tools

**LOCAL FACT:**

- `macros.inc` — general macros, app-facing helpers historically shared
- `struct.inc` — `struct`/`ends`, `sizeof.*`
- `proc32.inc` — `proc`/`endp`, stdcall helpers
- `kglobals.inc` — `iglobal`/`uglobal` data sections
- `export.inc` + `exports.inc` — PE-style export directory embedded for drivers

## Final binary format

| Property | Value | Evidence |
|----------|-------|----------|
| Format | Raw flat binary | `format binary as "mnt"` in `kernel.asm` |
| Default name | `kernel.mnt` / `KERNEL.MNT` | Makefile, loaders, docs |
| Extended-loader build | `kernel.bin` (Makefile) / `kernel.mnt.ext_loader` (Tup) | `-dextended_primary_loader=1` |
| Sections | None (no ELF sections) | FASM binary |
| Load address (phys) | `KERNEL_BASE = 0x10000` | `const.inc` |
| Runtime map | Kernel identity-mapped under `OS_BASE` (`0x80000000`) after paging | `const.inc`, `memmap.inc` |
| Relocations | None for kernel image as a whole; PE **drivers** relocate via `peload.inc`. Phase C Rust probe is reloc-free raw bytes via `file` — see [`../migration/phase-c-integration.md`](../migration/phase-c-integration.md) | LOCAL FACT |
| Alignment | Page/code `align 4` / `align 16` widely used; page size 4096 | LOCAL FACT |

## Memory layout of the binary (conceptual)

```text
file offset 0
  16-bit boot header + bootbios code/data
  …
file offset corresponding to KERNEL_BASE linking
  32-bit low code (B32 … paging enable) assembled with org KERNEL_BASE
  …
  after org OS_BASE+$
  high_code and rest of kernel (linked as if at OS_BASE+offset)
  data32 / uglobals / endofcode
```

**INFERENCE:** Loaders place the file at physical `0x10000` such that the 32-bit linked addresses match physical addresses before paging; after paging, the same bytes are accessed at `VA = OS_BASE + phys` for the kernel half.

## Bootloader assumptions (summary)

See also [`../compatibility/external-contract.md`](../compatibility/external-contract.md).

**LOCAL FACT** ([`kernel/docs/loader_doc.txt`](../../kernel/docs/loader_doc.txt)):

- Optional `AX='KL'` + `DS:SI` save-settings structure.
- Optional `CX='HA'`, `DX='RD'`, `BX` = `/sys` device encoding.
- Kernel expects `boot_data` filled at `BOOT_VARS` (`0x9000`) by boot path (VESA, e820, disks, etc.).

## Runtime symbols of interest

Not a dynamic symbol table for apps. Driver-visible exports are the PE export directory from `core/exports.inc` (module name `'KERNEL'`). Last export slot `LFBAddress` is special (address cell, not a function).

## Generated files

- `lang.inc` (ephemeral)
- Optional kerpack output (**UNKNOWN** exact format without kerpack sources in tree)
