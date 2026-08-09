# CP866 Encode Migration (FASM → Rust)

**Date:** 2026-08-09  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Depends on:** Cut A ([`cut-a-implementation.md`](cut-a-implementation.md)), Phase C ([`phase-c-integration.md`](phase-c-integration.md)), CRC32 ([`crc32-migration.md`](crc32-migration.md)), UTF-16 ([`utf16-migration.md`](utf16-migration.md)).

## Goal

Replace the in-kernel `unicode.cp866.encode` **implementation** with freestanding Rust while preserving the existing FASM register ABI for all call sites.

```text
existing kernel callers (xfs.asm, ext.inc, …)
        ↓
unicode.cp866.encode  (FASM ABI: AX in, AL out)
        ↓
FASM trampoline (kernel/unicode.inc, USE_RUST_CP866=1)
        ↓
rust_unicode_cp866_encode (embedded reloc-free blob)
        ↓
AL = CP866 byte
```

UTF-8 decode is **not** migrated in this stage. CRC32 and UTF-16 migrations are unchanged.

## Relocation analysis (LOCAL FACT)

Initial freestanding builds of `rust_unicode_cp866_encode` were **not** reloc-free:

| Cause | Detail |
|-------|--------|
| `match` / dense if-else | LLVM `SwitchToLookupTable` → `.rodata..Lswitch.table.*` + `R_386_GOTOFF` |
| `const [u8; N]` table | Same: `.rodata` + GOTOFF |

**Fix (implemented):** materialize the FASM 8-byte special table on the **stack** with `write_volatile` / `read_volatile`, then scan like FASM `repnz scasb`. No `.rodata` references remain in the extractable section.

Inspected member of `libkolibri_utils.a` after freestanding release build:

| Item | Result |
|------|--------|
| Section | `.text.rust_unicode_cp866_encode` |
| Size | **294** bytes |
| Relocations targeting that section | **none** (extractor refused otherwise) |
| `.rodata` / GOT / GOTOFF refs from CP866 | **none** (after stack-table rewrite) |
| External undef used by CP866 | **none** |
| Epilogue | `ret 4` (`C2 04 00`) present (stdcall, 1 dword arg); LLVM may end the section with `jmp` to a shared internal epilogue |

**Decision:** same Phase C / CRC / UTF-16 mechanism — section extract + FASM `file` — is valid for CP866 encode after the stack-table rewrite. `rust-lld` is **not** required for this function.

## ABI contracts

### FASM `unicode.cp866.encode` (unchanged for callers)

| Item | Contract |
|------|----------|
| Symbol | `unicode.cp866.encode` |
| Convention | register / plain `call` + `ret` (not stdcall) |
| Original body | `call uni2ansi_char` / `ret` ([`kernel/unicode.inc`](../../kernel/unicode.inc)) |
| Real logic | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) `uni2ansi_char` |
| `AX` in | Unicode code point (low 16 bits; FASM compares `ax` only) |
| `AL` out | CP866 byte; unknown → `'_'`; `U+00B6` → `20` |
| Clobbered (original) | `AL`/`AX` as written; special path push/pop `ECX`/`EDI` |
| Call sites | e.g. `stosb` after encode in `xfs.asm` / `ext.inc` (only `AL` consumed) |

Mapping summary (FASM-authoritative):

| Input (`AX`) | Output (`AL`) |
|--------------|---------------|
| `0x00..0x7F` | unchanged (ASCII) |
| `0x00B6` | `20` |
| `0x0410..0x043F` | `0x80..0xAF` (`add al, 0x70`) |
| `0x0440..0x044F` | `0xE0..0xEF` (`add al, 0xA0`) |
| Specials via `.table` | `Ё/ё` and related → `0xF0..0xF7` |
| else | `'_'` |

`.table db 1, 51h, 4, 54h, 7, 57h, 0Eh, 5Eh` with `repnz scasb` → `AL = 0xF7 - remaining_ECX`.

Authoritative rollback source: `USE_RUST_CP866=0` branch in [`kernel/unicode.inc`](../../kernel/unicode.inc). `uni2ansi_char` remains in FASM for its other call sites.

### Rust `rust_unicode_cp866_encode`

| Item | Contract |
|------|----------|
| Symbol | `rust_unicode_cp866_encode` (`#[no_mangle]`, section `.text.rust_unicode_cp866_encode`) |
| Convention | `extern "stdcall"` |
| Args | `(cp: u32)` on stack; truncated to `u16` like FASM `AX` |
| Return | `EAX` with CP866 byte in `AL` (high bits cleared by construction) |
| Stack cleanup | callee `ret 4` |

### Trampoline

```asm
unicode.cp866.encode:
        stdcall rust_unicode_cp866_encode, eax
        ret
```

Callers continue to use `call unicode.cp866.encode` with the code point in `EAX`.

## Build / extract pipeline

```text
cargo +nightly … i686-kolibri-none staticlib
        ↓
libkolibri_utils.a
        ↓
extract_reloc_free_text.py
  --section .text.rust_unicode_cp866_encode
  --symbol rust_unicode_cp866_encode
  --expect-ret-imm 4
        ↓
out/rust_unicode_cp866_encode.bin
        ↓
kernel/rust/cp866.inc  (FASM `file` at label rust_unicode_cp866_encode)
```

Script: [`rust_kernel/kolibri_utils/build-cp866.ps1`](../../rust_kernel/kolibri_utils/build-cp866.ps1)  
(also re-extracts UTF-16 + CRC + Phase C probe blobs needed by the current hybrid kernel)

Extractor: [`rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py)  
(`--expect-ret-imm` requires the stdcall `ret imm16` bytes somewhere in the blob; trailing shared-epilogue `jmp` is allowed.)

## Kernel wiring

| File | Role |
|------|------|
| [`kernel/unicode.inc`](../../kernel/unicode.inc) | `USE_RUST_CP866=1` trampoline; original FASM body under `else` |
| [`kernel/rust/cp866.inc`](../../kernel/rust/cp866.inc) | blob + `cp866_rust_smoke_test` |
| [`kernel/kernel32.inc`](../../kernel/kernel32.inc) | includes `rust/cp866.inc` |
| [`kernel/kernel.asm`](../../kernel/kernel.asm) | `call cp866_rust_smoke_test` after UTF-16 smoke |
| CRC / UTF-16 / Phase C | **unchanged** |

## Runtime verification

Boot-time smoke (after Phase C + CRC + UTF-16 smokes):

1. `EAX = 0x0401` (U+0401 CYRILLIC CAPITAL LETTER IO)
2. `call unicode.cp866.encode` (goes through trampoline → Rust)
3. Expect `AL = 0xF0`; store to `rust_cp866_smoke_result`
4. On mismatch: hang with `EAX=0xDEAD8660` (never reaches desktop)

Reaching the desktop implies the Rust CP866 path executed and matched the known vector (special-table path).

## Differential tests

`cargo test -p kolibri_utils` — **22/22 pass**, including:

- ASCII / Cyrillic / specials / `U+00B6` / unknowns
- boundary set around `0x80`, `0xB6`, `0x400..0x460`, high sentinels
- **exhaustive** oracle match for every `AX` value `0..=0xFFFF`, plus high-bit sentinels truncated like FASM

Oracle is an independent second transcription of the FASM `uni2ansi_char` control flow (table + `repnz scasb` indexing). Production `special_40x` uses the stack-volatile table scan for reloc-free codegen.

## Reproduce

```powershell
powershell -File rust_kernel/kolibri_utils/build-cp866.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
cargo run --release -- cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\phase-f-cp866-boot.img
cargo run --release -- delete ..\..\tmp_images\phase-f-cp866-boot.img DOCPACK
cargo run --release -- replace ..\..\tmp_images\phase-f-cp866-boot.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\phase-f-cp866-boot.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4446,server,nowait
```

### Verified run (2026-08-09)

| Gate | Result |
|------|--------|
| Reloc-free CP866 extract | Pass (294 bytes; `ret 4` in body) |
| `cargo test -p kolibri_utils` | Pass (22) |
| `kernel.mnt` | **222712** bytes (UTF-16 stage was 222360) |
| CP866 + UTF-16 + CRC + probe blobs present once each | Pass |
| Reference SHA-256 | Unchanged `1901F3A8…C8BA` |
| QEMU `query-status` | `running` |
| Screendump | Full KolibriOS desktop (`tmp_images/phase-f-cp866-desktop.png`) |

## Rollback

1. Set `USE_RUST_CP866 = 0` in `kernel/unicode.inc` (restores original `call uni2ansi_char`).
2. Optionally remove `call cp866_rust_smoke_test` and `include "rust/cp866.inc"`.
3. Rebuild `kernel.mnt`.

Original wrapper text remains in the `else` branch of `unicode.inc`. `uni2ansi_char` is never deleted.

## Known limitations

1. Only reloc-free functions may use this extract path; all Cut A Unicode functions (UTF-16, CP866, UTF-8) plus CRC32 proved reloc-free after appropriate inlining / stack-table rewrite.
2. CP866 specials use a stack-volatile table to defeat LLVM lookup-table emission; slightly larger than a `.rodata` table would be (294 bytes) but keeps the proven blob mechanism.
3. Smoke tests (Phase C + CRC + UTF-16 + CP866 + UTF-8) are temporary hang-on-fail diagnostics.
4. Host binary differential vs assembled FASM `uni2ansi_char` still not built; confidence rests on exhaustive `AX`-domain oracle + in-kernel smoke.
5. UTF-8 decode migrated — see [`utf8-migration.md`](utf8-migration.md).

## Status

| Gate | Result |
|------|--------|
| Rust CP866 freestanding | Pass |
| Reloc analysis | Pass (zero after stack-table rewrite) |
| ABI trampoline | Pass |
| Differential / exhaustive unit tests | Pass (`0..=0xFFFF`) |
| Runtime smoke proves Rust path | Pass (desktop) |
| CRC + UTF-16 migrations still intact | Yes |
| UTF-8 untouched | Migrated — see [`utf8-migration.md`](utf8-migration.md) |
| Stop after CP866 encode | Yes (at time of CP866 stage) |
