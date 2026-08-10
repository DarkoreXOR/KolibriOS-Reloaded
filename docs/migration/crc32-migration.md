# CRC32 Migration (FASM → Rust)

> Historical label in early notes: “Phase D” meant this CRC step after Phase C.
> It does **not** mean “remove FASM bodies” — originals remain under `USE_RUST_CRC=0`.
> Cut A baseline: [`cut-a-final-architecture.md`](cut-a-final-architecture.md).

**Date:** 2026-08-09  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Depends on:** Cut A ([`cut-a-implementation.md`](cut-a-implementation.md)), Phase C ([`phase-c-integration.md`](phase-c-integration.md)).

## Goal

Replace the in-kernel `crc_32` **implementation** with freestanding Rust while preserving the existing FASM calling convention for all call sites.

```text
existing kernel callers (disk.inc, ext.inc, …)
        ↓
crc_32  (FASM ABI: stdcall + EAX partial)
        ↓
FASM trampoline (kernel/crc.inc, USE_RUST_CRC=1)
        ↓
rust_crc_32 (embedded reloc-free blob)
        ↓
EAX = updated CRC
```

Unicode is **not** migrated in this stage.

## Relocation analysis (LOCAL FACT)

Inspected member of `libkolibri_utils.a` after freestanding release build:

| Item | Result |
|------|--------|
| Section | `.text.rust_crc_32` |
| Size | **226** bytes |
| Relocations targeting that section | **none** (no `.rel.text.rust_crc_32`) |
| `.rodata` / GOT / GOTOFF refs from CRC | **none** |
| External undef used by CRC | **none** (`panic_bounds_check` / GOT only on Unicode paths) |
| Epilogue | `ret 16` (`C2 10 00`) — stdcall, 4 dword args |
| Callee-saved used | `EBP`,`EBX`,`EDI`,`ESI` pushed/popped |

`crc32_update` is `#[inline(always)]` into the FFI entry so the extractable section stays self-contained.

**Decision:** same Phase C mechanism — section extract + FASM `file` — is valid for CRC. `rust-lld` is **not** required for this function.

## ABI contracts

### FASM `crc_32` (unchanged for callers)

| Item | Contract |
|------|----------|
| Symbol | `crc_32` |
| Convention | stdcall (`retn 12`) |
| Stack | `[esp+4]=poly`, `[esp+8]=buffer`, `[esp+12]=length` (+ `proc` frame) |
| `EAX` | partial CRC **in** and updated CRC **out** |
| Preserved | `EBX`,`ECX`,`EDX`,`ESI` (trampoline matches original) |
| Clobbered | flags; `EDI` unused |

### Rust `rust_crc_32`

| Item | Contract |
|------|----------|
| Symbol | `rust_crc_32` (`#[no_mangle]`, section `.text.rust_crc_32`) |
| Convention | `extern "stdcall"` |
| Args | `(partial, poly, buffer, length)` — all stack; partial **not** live-in `EAX` |
| Return | `EAX` |
| Stack cleanup | callee `ret 16` |

### Trampoline

```asm
proc crc_32 _poly, _buffer, _length
        push    ebx ecx edx esi
        stdcall rust_crc_32, eax, [_poly], [_buffer], [_length]
        pop     esi edx ecx ebx
        ret
endp
```

## Build / extract pipeline

```text
cargo +nightly … i686-kolibri-none staticlib
        ↓
libkolibri_utils.a
        ↓
extract_reloc_free_text.py
  --section .text.rust_crc_32 --symbol rust_crc_32 --expect-ret-imm 16
        ↓
out/rust_crc_32.bin
        ↓
kernel/rust/crc.inc  (FASM `file` at label rust_crc_32)
```

Script: [`rust_kernel/kolibri_utils/build-crc.ps1`](../../rust_kernel/kolibri_utils/build-crc.ps1)  
Extractor: [`rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py)

Extractor **refuses** non-zero reloc sections and checks `ret imm16 == 16`.

## Kernel wiring

| File | Role |
|------|------|
| [`kernel/crc.inc`](../../kernel/crc.inc) | `USE_RUST_CRC=1` trampoline; original FASM body under `else` |
| [`kernel/rust/crc.inc`](../../kernel/rust/crc.inc) | blob + `crc_rust_smoke_test` |
| [`kernel/kernel32.inc`](../../kernel/kernel32.inc) | includes `rust/crc.inc` |
| [`kernel/kernel.asm`](../../kernel/kernel.asm) | `call crc_rust_smoke_test` after Phase C smoke |
| Phase C probe | **unchanged** |

## Runtime verification

Boot-time smoke (after paging / Phase C probe):

1. `EAX = -1`
2. `stdcall crc_32, 0xEDB88320, "123456789", 9` (goes through trampoline → Rust)
3. `xor eax, -1`
4. Expect `0xCBF43926`; store to `rust_crc_smoke_result`
5. On mismatch: hang with `EAX=0xDEADC2C0` (never reaches desktop)

Reaching the desktop implies the Rust CRC path executed and matched the known vector.

## Differential tests

`cargo test -p kolibri_utils` — **19/19 pass**, including:

- zero length preserves partial
- GPT poly `0xEDB88320` vs FASM oracle (empty, 1-byte, short, binary, chunked)
- ext poly `0x82F63B78`
- IEEE finalize `"123456789"` → `0xCBF43926`

## Reproduce

```powershell
powershell -File rust_kernel/kolibri_utils/build-crc.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\phase-d-crc-boot.img
cargo run --release -- delete ..\..\dev_build\phase-d-crc-boot.img DOCPACK
cargo run --release -- replace ..\..\dev_build\phase-d-crc-boot.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\phase-d-crc-boot.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown
```

### Verified run (2026-08-09)

| Gate | Result |
|------|--------|
| Reloc-free CRC extract | Pass (226 bytes, `ret 16`) |
| `cargo test -p kolibri_utils` | Pass (19) |
| `kernel.mnt` | **222248** bytes (Phase C was 221960) |
| CRC + probe blobs present in image | Pass |
| Reference SHA-256 | Unchanged `1901F3A8…C8BA` |
| QEMU `query-status` | `running` |
| Screendump | Full KolibriOS desktop |

## Rollback

1. Set `USE_RUST_CRC = 0` in `kernel/crc.inc` (restores original FASM body).
2. Optionally remove `call crc_rust_smoke_test` and `include "rust/crc.inc"`.
3. Rebuild `kernel.mnt`.

Original algorithm text remains in the `else` branch of `crc.inc`.

## Known limitations

1. Only reloc-free functions may use this extract path. UTF-16, CP866, and UTF-8 decode were later proven reloc-free and use the same path — see [`utf16-migration.md`](utf16-migration.md), [`cp866-migration.md`](cp866-migration.md), [`utf8-migration.md`](utf8-migration.md). (`rust-lld` is not required for Cut A; future functions may still need it.)
2. Smoke tests (Phase C + CRC) are temporary hang-on-fail diagnostics.
3. Host binary differential vs assembled FASM `crc.inc` still not built; confidence rests on oracle + known vector + in-kernel smoke.
4. `rust_crc_32` treats null buffer + nonzero length as “return partial”; FASM would fault — callers always pass valid buffers.

## Status

| Gate | Result |
|------|--------|
| Rust CRC freestanding | Pass |
| Reloc analysis | Pass (zero) |
| ABI trampoline | Pass |
| Differential unit tests | Pass |
| Runtime smoke proves Rust path | Pass (desktop) |
| Unicode still FASM at this step (historical) | Yes — later migrated |
| Stop after CRC | Yes |
