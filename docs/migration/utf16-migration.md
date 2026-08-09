# UTF-16 Encode Migration (FASM → Rust)

**Date:** 2026-08-09  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Depends on:** Cut A ([`cut-a-implementation.md`](cut-a-implementation.md)), Phase C ([`phase-c-integration.md`](phase-c-integration.md)), CRC32 ([`crc32-migration.md`](crc32-migration.md)).

## Goal

Replace the in-kernel `unicode.utf16.encode` **implementation** with freestanding Rust while preserving the existing FASM register ABI for all call sites.

```text
existing kernel callers (ext.inc, xfs.asm, …)
        ↓
unicode.utf16.encode  (FASM ABI: EAX in/out)
        ↓
FASM trampoline (kernel/unicode.inc, USE_RUST_UTF16=1)
        ↓
rust_unicode_utf16_encode (embedded reloc-free blob)
        ↓
EAX = packed UTF-16 result
```

UTF-8 decode and CP866 encode are **not** migrated in this stage. CRC32 migration is unchanged.

## Relocation analysis (LOCAL FACT)

Inspected member of `libkolibri_utils.a` after freestanding release build:

| Item | Result |
|------|--------|
| Section | `.text.rust_unicode_utf16_encode` |
| Size | **85** bytes |
| Relocations targeting that section | **none** (extractor refused otherwise) |
| `.rodata` / GOT / GOTOFF refs from UTF-16 | **none** |
| External undef used by UTF-16 | **none** |
| Epilogue | `ret 4` (`C2 04 00`) — stdcall, 1 dword arg |

`utf16_encode` is `#[inline(always)]` into the FFI entry so the extractable section stays self-contained.

**Decision:** same Phase C / CRC mechanism — section extract + FASM `file` — is valid for UTF-16 encode. `rust-lld` is **not** required for this function.

## ABI contracts

### FASM `unicode.utf16.encode` (unchanged for callers)

| Item | Contract |
|------|----------|
| Symbol | `unicode.utf16.encode` |
| Convention | register / plain `call` + `ret` (not stdcall) |
| `EAX` in | Unicode code point |
| `EAX` out | BMP: code point in low 16 bits (high 16 = 0); supplementary: low word = high surrogate, high word = low surrogate (`or` with `0xDC00D800` after `ror`); invalid / surrogate-range BMP → `0xFFFD` |
| Clobbered | `EAX`, flags |
| Input rules | `>= 0x110000` → error; `[0xD800, 0xE000)` → error; else BMP pass-through or surrogate pack |

Authoritative source: [`kernel/unicode.inc`](../../kernel/unicode.inc) (original body kept under `USE_RUST_UTF16=0`).

### Rust `rust_unicode_utf16_encode`

| Item | Contract |
|------|----------|
| Symbol | `rust_unicode_utf16_encode` (`#[no_mangle]`, section `.text.rust_unicode_utf16_encode`) |
| Convention | `extern "stdcall"` |
| Args | `(cp: u32)` on stack |
| Return | `EAX` packed result |
| Stack cleanup | callee `ret 4` |

### Trampoline

```asm
unicode.utf16.encode:
        stdcall rust_unicode_utf16_encode, eax
        ret
```

Callers continue to use `call unicode.utf16.encode` with the code point in `EAX`.

## Build / extract pipeline

```text
cargo +nightly … i686-kolibri-none staticlib
        ↓
libkolibri_utils.a
        ↓
extract_reloc_free_text.py
  --section .text.rust_unicode_utf16_encode
  --symbol rust_unicode_utf16_encode
  --expect-ret-imm 4
        ↓
out/rust_unicode_utf16_encode.bin
        ↓
kernel/rust/utf16.inc  (FASM `file` at label rust_unicode_utf16_encode)
```

Script: [`rust_kernel/kolibri_utils/build-utf16.ps1`](../../rust_kernel/kolibri_utils/build-utf16.ps1)  
(also re-extracts CRC + Phase C probe blobs needed by the current hybrid kernel)

Extractor: [`rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py)

## Kernel wiring

| File | Role |
|------|------|
| [`kernel/unicode.inc`](../../kernel/unicode.inc) | `USE_RUST_UTF16=1` trampoline; original FASM body under `else` |
| [`kernel/rust/utf16.inc`](../../kernel/rust/utf16.inc) | blob + `utf16_rust_smoke_test` |
| [`kernel/kernel32.inc`](../../kernel/kernel32.inc) | includes `rust/utf16.inc` |
| [`kernel/kernel.asm`](../../kernel/kernel.asm) | `call utf16_rust_smoke_test` after CRC smoke |
| CRC / Phase C | **unchanged** |

## Runtime verification

Boot-time smoke (after Phase C + CRC smokes):

1. `EAX = 0x1F600` (U+1F600)
2. `call unicode.utf16.encode` (goes through trampoline → Rust)
3. Expect `0xDE00D83D` (low `D83D`, high `DE00`); store to `rust_utf16_smoke_result`
4. On mismatch: hang with `EAX=0xDEADF160` (never reaches desktop)

Reaching the desktop implies the Rust UTF-16 path executed and matched the known vector.

## Differential tests

`cargo test -p kolibri_utils` — **20/20 pass**, including:

- boundary set (ASCII, BMP edges, surrogates, `0xFFFF`, `0x10000`, `0x10FFFF`, invalids)
- **exhaustive** oracle match for `0..=0x120000` plus high sentinels
- packed surrogate check for `0x1F600` and `0x10000`

Oracle is an independent second transcription of the FASM control flow in `unicode.inc`.

## Reproduce

```powershell
powershell -File rust_kernel/kolibri_utils/build-utf16.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\phase-e-utf16-boot.img
cargo run --release -- delete ..\..\tmp_images\phase-e-utf16-boot.img DOCPACK
cargo run --release -- replace ..\..\tmp_images\phase-e-utf16-boot.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\phase-e-utf16-boot.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown
```

### Verified run (2026-08-09)

| Gate | Result |
|------|--------|
| Reloc-free UTF-16 extract | Pass (85 bytes, `ret 4`) |
| `cargo test -p kolibri_utils` | Pass (20) |
| `kernel.mnt` | **222360** bytes (CRC stage was 222248) |
| UTF-16 + CRC + probe blobs present once each | Pass |
| Reference SHA-256 | Unchanged `1901F3A8…C8BA` |
| QEMU `query-status` | `running` |
| Screendump | Full KolibriOS desktop |

## Rollback

1. Set `USE_RUST_UTF16 = 0` in `kernel/unicode.inc` (restores original FASM body).
2. Optionally remove `call utf16_rust_smoke_test` and `include "rust/utf16.inc"`.
3. Rebuild `kernel.mnt`.

Original algorithm text remains in the `else` branch of `unicode.inc`.

## Known limitations

1. Only reloc-free functions may use this extract path. CP866 encode (stack-table rewrite) and UTF-8 decode were later migrated the same way — see [`cp866-migration.md`](cp866-migration.md), [`utf8-migration.md`](utf8-migration.md). (`rust-lld` is not required for Cut A; future functions may still need it.)
2. Smoke tests (Phase C + CRC + UTF-16) are temporary hang-on-fail diagnostics.
3. Host binary differential vs assembled FASM `unicode.inc` still not built; confidence rests on exhaustive oracle + in-kernel smoke.
4. Historical note: at the time of this migration, UTF-8 was still FASM-authoritative; it is now migrated (see UTF-8 doc).

## Status

| Gate | Result |
|------|--------|
| Rust UTF-16 freestanding | Pass |
| Reloc analysis | Pass (zero) |
| ABI trampoline | Pass |
| Differential / exhaustive unit tests | Pass |
| Runtime smoke proves Rust path | Pass (desktop) |
| CRC migration still intact | Yes |
| Stop after UTF-16 encode (at the time) | Yes |
| Later: UTF-8 / CP866 also migrated | See [`utf8-migration.md`](utf8-migration.md), [`cp866-migration.md`](cp866-migration.md) |
