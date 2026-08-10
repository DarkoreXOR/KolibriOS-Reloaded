# UTF-8 Decode Migration (FASM → Rust)

**Date:** 2026-08-09  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Depends on:** Cut A ([`cut-a-implementation.md`](cut-a-implementation.md)), Phase C ([`phase-c-integration.md`](phase-c-integration.md)), CRC32 ([`crc32-migration.md`](crc32-migration.md)), UTF-16 ([`utf16-migration.md`](utf16-migration.md)), CP866 ([`cp866-migration.md`](cp866-migration.md)).

## Goal

Replace the in-kernel `unicode.utf8.decode` **implementation** with freestanding Rust while preserving the existing FASM register ABI for all call sites.

```text
existing kernel callers (xfs.asm, ext.inc, …)
        ↓
unicode.utf8.decode  (FASM ABI: ESI/ECX in-out, EAX out)
        ↓
FASM trampoline (kernel/unicode.inc, USE_RUST_UTF8=1)
        ↓
rust_unicode_utf8_decode (embedded reloc-free blob)
        ↓
EAX = code point (or 0xFFFD); ESI/ECX advanced
```

This completes **Unicode Cut A**. CRC32, UTF-16, and CP866 migrations are unchanged.

## Relocation analysis (LOCAL FACT)

Inspected member of `libkolibri_utils.a` after freestanding release build:

| Item | Result |
|------|--------|
| Section | `.text.rust_unicode_utf8_decode` |
| Size | **318** bytes |
| Relocations targeting that section | **none** (extractor refused otherwise) |
| `.rodata` / GOT / GOTOFF refs from UTF-8 | **none** |
| External undef used by UTF-8 | **none** |
| Epilogue | `ret 8` (`C2 08 00`) present (stdcall, 2 dword args); LLVM may end the section with `jmp` to a shared internal epilogue |

Helpers (`utf8_decode`, `read2`/`read3`/`read4`, `cont_ok`) are `#[inline(always)]` into the FFI entry so no cross-section calls remain.

Unlike CP866, UTF-8 needed **no** stack-volatile table rewrite — the bit-shift algorithm has no lookup tables.

**Decision:** same Phase C / CRC / UTF-16 / CP866 mechanism — section extract + FASM `file` — is valid for UTF-8 decode. `rust-lld` is **not** required for this function.

**Cut A integration architecture (final):** relocation-free Rust blobs + FASM trampolines for all four functions (CRC32, UTF-16, CP866, UTF-8).

## ABI contracts

### FASM `unicode.utf8.decode` (unchanged for callers)

| Item | Contract |
|------|----------|
| Symbol | `unicode.utf8.decode` |
| Convention | register / plain `call` + `ret` (not stdcall) |
| `ESI` in/out | pointer to next UTF-8 byte; advanced by consumed bytes |
| `ECX` in/out | remaining length; decreased by consumed bytes |
| `EAX` out | Unicode scalar, or `0xFFFD` on error |
| Clobbered | `EAX`, `ESI`, `ECX`, flags (`EBX`/`EDX`/`EDI` unused by original) |
| Call sites | `kernel/fs/xfs.asm`, `kernel/fs/ext.inc` (paired with encode) |

Behavior (FASM-authoritative):

| Case | Result |
|------|--------|
| `ECX == 0` | jump to `.done` without writing `EAX`/`ESI` |
| ASCII (`AL` bit7 clear) | consume 1; `EAX` = byte |
| 2/3/4-byte valid lead + cont | consume N; bit-shift assemble into `EAX` |
| Truncated / bad lead / bad cont | `EAX = 0xFFFD`; consume **1** |
| Overlong / surrogate / `> U+10FFFF` | **not** rejected beyond bit-parse result |

Authoritative rollback source: `USE_RUST_UTF8=0` branch in [`kernel/unicode.inc`](../../kernel/unicode.inc).

### Rust `rust_unicode_utf8_decode`

| Item | Contract |
|------|----------|
| Symbol | `rust_unicode_utf8_decode` (`#[no_mangle]`, section `.text.rust_unicode_utf8_decode`) |
| Convention | `extern "stdcall"` |
| Args | `(ptr_inout: *mut *const u8, len_inout: *mut u32)` |
| Return | `EAX` = code point; updates `*ptr_inout` / `*len_inout` like FASM `ESI`/`ECX` |
| Empty length | returns `0`, leaves pointers unchanged (FASM leaves stale `EAX`; callers must not rely on `EAX` when `ECX` was 0) |
| Stack cleanup | callee `ret 8` |

### Trampoline

```asm
unicode.utf8.decode:
        push    ecx
        mov     eax, esp      ; &length
        push    esi
        mov     edx, esp      ; &ptr
        stdcall rust_unicode_utf8_decode, edx, eax
        pop     esi
        pop     ecx
        ret
```

Callers continue to use `call unicode.utf8.decode` with buffer in `ESI` and length in `ECX`.

## Build / extract pipeline

```text
cargo +nightly … i686-kolibri-none staticlib
        ↓
libkolibri_utils.a
        ↓
extract_reloc_free_text.py
  --section .text.rust_unicode_utf8_decode
  --symbol rust_unicode_utf8_decode
  --expect-ret-imm 8
        ↓
out/rust_unicode_utf8_decode.bin
        ↓
kernel/rust/utf8.inc  (FASM `file` at label rust_unicode_utf8_decode)
```

Script: [`rust_kernel/kolibri_utils/build-utf8.ps1`](../../rust_kernel/kolibri_utils/build-utf8.ps1)  
(also re-extracts CP866 + UTF-16 + CRC + Phase C probe blobs needed by the current hybrid kernel)

Extractor: [`rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py)

## Kernel wiring

| File | Role |
|------|------|
| [`kernel/unicode.inc`](../../kernel/unicode.inc) | `USE_RUST_UTF8=1` trampoline; original FASM body under `else` |
| [`kernel/rust/utf8.inc`](../../kernel/rust/utf8.inc) | blob + `utf8_rust_smoke_test` |
| [`kernel/kernel32.inc`](../../kernel/kernel32.inc) | includes `rust/utf8.inc` |
| [`kernel/kernel.asm`](../../kernel/kernel.asm) | `call utf8_rust_smoke_test` after CP866 smoke |
| CRC / UTF-16 / CP866 / Phase C | **unchanged** |

## Runtime verification

Boot-time smoke (after Phase C + CRC + UTF-16 + CP866 smokes):

1. ASCII `"A"` → `EAX=0x41`, `ECX=0`
2. Cyrillic `D0 90` → `EAX=0x0410`, `ECX=0`
3. Euro `E2 82 AC` → `EAX=0x20AC`, `ECX=0`
4. Emoji `F0 9F 98 80` → `EAX=0x1F600`, `ECX=0`; store to `rust_utf8_smoke_result`
5. Truncated `D0` → `EAX=0xFFFD`, `ECX=0`
6. Bad cont `D0 20` → `EAX=0xFFFD`, `ECX=1`
7. On mismatch: hang with `EAX=0xDEADF800` (never reaches desktop)

Reaching the desktop implies the Rust UTF-8 path executed and matched all vectors.

## Differential tests

`cargo test -p kolibri_utils` — **27/27 pass**, including:

- Named vectors (ASCII, 2/3/4-byte, truncated, bad cont, overlong, surrogate encoding, `> U+10FFFF`, invalid leads)
- Boundary scalars `U+0080`, `U+07FF`, `U+0800`, `U+FFFF`, `U+10000`, `U+10FFFF`, `U+1F600`
- **Exhaustive** oracle match for every 1-byte and every 2-byte buffer
- **Exhaustive** 3-byte for all `.read3` leads `0xE0..=0xEF`
- Boundary/sampled 4-byte corpus for `.read4` leads

Oracle is an independent second transcription of the FASM `unicode.utf8.decode` control flow (not a call into production helpers).

**Limitation:** host binary differential vs assembled FASM `unicode.utf8.decode` still not built; confidence rests on line-by-line FASM algorithm oracle + exhaustive short-length sweeps + in-kernel smoke.

## Reproduce

```powershell
powershell -File rust_kernel/kolibri_utils/build-utf8.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\phase-g-utf8-boot.img
cargo run --release -- delete ..\..\dev_build\phase-g-utf8-boot.img DOCPACK
cargo run --release -- replace ..\..\dev_build\phase-g-utf8-boot.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\phase-g-utf8-boot.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4450,server,nowait
```

### Verified run (2026-08-09)

| Gate | Result |
|------|--------|
| Reloc-free UTF-8 extract | Pass (318 bytes; `ret 8` in body) |
| `cargo test -p kolibri_utils` | Pass (27) |
| `kernel.mnt` | **223080** bytes (CP866 stage was 222712) |
| UTF-8 + CP866 + UTF-16 + CRC + probe blobs present once each | Pass |
| Reference SHA-256 | Unchanged `1901F3A8…C8BA` |
| QEMU `query-status` | `running` |
| Screendump | Full KolibriOS desktop (`dev_build/phase-g-utf8-desktop.png`) |

## Rollback

1. Set `USE_RUST_UTF8 = 0` in `kernel/unicode.inc` (restores original FASM body).
2. Optionally remove `call utf8_rust_smoke_test` and `include "rust/utf8.inc"`.
3. Rebuild `kernel.mnt`.

Original FASM text remains in the `else` branch of `unicode.inc`.

## Known limitations

1. Empty-input `EAX`: Rust trampoline path returns `0`; original FASM left stale `EAX`. In-tree callers loop on `ECX` and do not call with `ECX=0` expecting a meaningful `EAX`.
2. FASM semantics intentionally accept overlongs / surrogate encodings / out-of-range bit-parse results — Rust matches FASM, not Unicode Standard strict UTF-8.
3. Smoke tests (Phase C + CRC + UTF-16 + CP866 + UTF-8) are temporary hang-on-fail diagnostics.
4. Host binary differential vs assembled FASM still not built.

## Status

| Gate | Result |
|------|--------|
| Rust UTF-8 freestanding | Pass |
| Reloc analysis | Pass (zero; no `rust-lld` needed) |
| ABI trampoline | Pass |
| Differential / exhaustive unit tests | Pass |
| Runtime smoke proves Rust path | Pass (desktop) |
| CRC + UTF-16 + CP866 still intact | Yes |
| Unicode Cut A complete | Yes |
| Stop after UTF-8 | Yes |
