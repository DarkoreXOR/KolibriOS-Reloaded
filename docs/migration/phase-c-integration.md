# Phase C — Minimal FASM ↔ Rust Integration

**Date:** 2026-08-09  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Depends on:** Cut A ([`cut-a-implementation.md`](cut-a-implementation.md)), FASM baseline ([`fasm-baseline-restoration.md`](fasm-baseline-restoration.md)).

## Goal

Prove a single control-flow path:

```text
FASM kernel  →  Rust stdcall entry  →  Rust body  →  return to FASM
```

without migrating CRC/Unicode and without redesigning boot/memory.

## Architecture (chosen)

```text
cargo +nightly (i686-kolibri-none staticlib)
        ↓
libkolibri_utils.a   (ELF32 relocatable members inside ar)
        ↓
extract_phase_c_probe.py
  — require zero relocations on .text.rust_phase_c_probe
  — emit raw machine code blob
        ↓
rust_phase_c_probe.bin
        ↓
FASM `file` include at label rust_phase_c_probe
        ↓
flat kernel.mnt (org OS_BASE+$ high region)
        ↓
high_code calls phase_c_smoke_test → call rust_phase_c_probe
```

**LOCAL FACT:** FASM cannot consume a Rust `.a` / ELF object. Existing kernel already embeds opaque blobs via `file` (fonts/cursors pattern).

**LOCAL FACT:** For this probe, rustc emitted:

```text
b8 1c a1 de c0    mov eax, 0xC0DEA11C
c3                ret
```

(6 bytes, **no** relocations). Absolute link address does not matter; FASM places the bytes at `rust_phase_c_probe` after `org OS_BASE+$`.

### Why this mechanism

| Requirement | How it is met |
|-------------|----------------|
| i686 freestanding | Custom target `i686-kolibri-none.json` |
| No Rust runtime / libc | `no_std`, panic abort loop; probe touches nothing |
| Flat `kernel.mnt` | Raw bytes via FASM `file` |
| Valid at final VA | Zero relocs ⇒ position-independent for this function |
| Existing FASM ABI | `extern "stdcall"`; 0 args ⇒ `ret` (not `ret N`) |
| Removable | One include + one `call` + build script |

### Rejected alternatives

| Approach | Why rejected (for this proof / in general) |
|----------|--------------------------------------------|
| FASM links `.a` directly | **LOCAL FACT:** FASM has no ELF/COFF linker; only `file` blobs / asm |
| Blind concatenation of `.a` or whole `.o` | Contains unresolved relocs (`R_386_GOTOFF`, `R_386_PLT32`, undef `panic_bounds_check`) for CRC/Unicode helpers |
| `--emit asm` into FASM | GAS/LLVM asm ≠ FASM syntax; large fragile translate |
| Full `rust-lld --oformat binary` as *only* path for the probe | Valid future path (see below); unnecessary for a reloc-free 6-byte function |
| Replace CRC/Unicode now | Out of scope; FASM bodies remain authoritative |

### Future path (when functions have relocations)

Cut A objects for UTF-8/CP866 use **GOTOFF** / rodata and may reference `core::panicking`. Those **must not** be section-extracted without a link step.

Intended next step (**INFERENCE**, sketched, not required for this proof):

```text
rust-lld -flavor gnu -m elf_i386 --gc-sections --oformat binary \
  -T <script with VMA = FASM placement> \
  libkolibri_utils.a -o rust_blob.bin
```

Chicken-and-egg VMA: reserve/align a FASM label, link with that address, or use a two-pass size lock. Target may need `"relocation-model": "static"` to avoid GOT.

`rust-lld` is available from the nightly sysroot (`…/bin/rust-lld.exe`).

## ABI contract

| Item | Contract |
|------|----------|
| Symbol | `rust_phase_c_probe` (undecorated; ELF/`#[no_mangle]`) |
| Convention | **stdcall** (`extern "stdcall"`) |
| Args | none |
| Return | `EAX = 0xC0DEA11C` (`PHASE_C_PROBE_MAGIC`) |
| Stack | callee pops 0 bytes ⇒ plain `ret` (`C3`) |
| Alignment | caller provides normal kernel stack; 16-byte not required for this body |
| Preserved regs | probe clobbers only `EAX` (+flags); no callee-saved use |
| PIC | not required for this probe (no absolute data refs) |
| Relocations | **must be none** before `file` embed (enforced by extractor) |

FASM side:

```asm
call    rust_phase_c_probe
cmp     eax, PHASE_C_PROBE_MAGIC
```

Smoke wrapper: [`kernel/rust/phase_c.inc`](../../kernel/rust/phase_c.inc)  
Call site: [`kernel/kernel.asm`](../../kernel/kernel.asm) `high_code` immediately after first TLB flush (paging + segments already live).

## Object / artifact facts

| Artifact | Fact |
|----------|------|
| `libkolibri_utils.a` | ar of **ELF32 EM_386** relocatable objects |
| Probe section | `.text.rust_phase_c_probe` |
| Extractor | [`rust_kernel/kolibri_utils/scripts/extract_phase_c_probe.py`](../../rust_kernel/kolibri_utils/scripts/extract_phase_c_probe.py) |
| Blob | [`rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin`](../../rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin) (gitignored; generated) |

**LOCAL FACT:** Cut A `rust_crc_32` / `rust_unicode_utf16_encode` / `rust_unicode_cp866_encode` (after stack-table rewrite) text sections also had **no** relocs in the inspected build; UTF-8 text **did** (GOTOFF). Do not generalize “extract `.text.*`” without the reloc check.

## Reproduce from clean checkout

### Tools

- Nightly Rust + `rust-src` (for `-Z build-std`)
- Python 3
- Vendored [`fasm/FASM.EXE`](../../fasm/FASM.EXE)
- `tools/kolibri_img`
- QEMU i386 (e.g. `C:\Program Files\qemu\qemu-system-i386.exe`)

### Commands

```powershell
# 1) Rust tests + freestanding build + extract blob
powershell -File rust_kernel/kolibri_utils/build-phase-c.ps1

# 2) Assemble hybrid kernel
Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

# 3) CoW image (never touch the reference .img)
cd tools\kolibri_img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\phase-c-boot.img
cargo run --release -- delete ..\..\tmp_images\phase-c-boot.img DOCPACK
cargo run --release -- replace ..\..\tmp_images\phase-c-boot.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

# 4) QEMU smoke (~10s to desktop)
& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\phase-c-boot.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown
```

**LOCAL FACT (2026-08-09):** Assembled `kernel.mnt` = **221960** bytes (baseline FASM-only was 221912). Probe opcode sequence present once in the image. QEMU `query-status` → `running`; screendump showed full KolibriOS desktop (icons, taskbar, wallpaper). Failure path is `jmp $` with `EAX=0xDEADBEEF` before desktop — reaching desktop implies `cmp` succeeded.

Reference image SHA-256 remained:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

### FASM `file` path caveat

**LOCAL FACT:** `file` paths are resolved relative to the **include file’s directory**, not `kernel.asm`. From `kernel/rust/phase_c.inc` the blob path is:

```text
../../rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
```

## Kernel changes (minimal)

| File | Change |
|------|--------|
| `kernel/kernel.asm` | `call phase_c_smoke_test` once in `high_code` |
| `kernel/kernel32.inc` | `include "rust/phase_c.inc"` |
| `kernel/rust/phase_c.inc` | **new** — trampoline + `file` blob + result dword |
| `kernel/crc.inc` / `unicode.inc` | **unchanged** |
| `kernel/init.inc` | **unchanged** |

Observable store: `rust_phase_c_result` (iglobal) set to magic on success.

## Rollback

1. Remove `call phase_c_smoke_test` from `kernel.asm`.
2. Remove `include "rust/phase_c.inc"` from `kernel32.inc`.
3. Delete `kernel/rust/phase_c.inc` (optional).
4. Rebuild `kernel.mnt`.

## Known limitations

1. Only reloc-free functions may use section extraction; others need `rust-lld` + fixed VMA.
2. Probe is a temporary smoke symbol — remove after broader hybrid link exists.
3. ~~CRC/Unicode still FASM-authoritative.~~ **Update:** Cut A complete (CRC + UTF-16 + CP866 + UTF-8) — see [`cut-a-final-architecture.md`](cut-a-final-architecture.md).
4. Host `kerpack` still absent (floppy space via deleting `DOCPACK` on CoW copies).

## Status

| Gate | Result |
|------|--------|
| Rust compiles (freestanding) | Pass |
| Blob extracted with reloc check | Pass (6 bytes) |
| Symbol available as FASM label | Pass |
| `kernel.mnt` produced | Pass (221960) |
| QEMU boot + desktop | Pass |
| FASM called Rust (hang-on-fail + magic) | Pass |
