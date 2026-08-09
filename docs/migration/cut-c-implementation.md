# Cut C Implementation — `utf16toUpper`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-c-plan.md`](cut-c-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `utf16toUpper` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | **11** live FS sites (`fat.inc`, `exfat.inc`, `ntfs.inc`, `iso9660.inc`) |
| Rust symbol | `rust_utf16_to_upper` |
| Pure helper | `kolibri_utils::utf16_to_upper` |

---

## Original implementation

FASM leaf in `parse_fn.inc` (retained under `USE_RUST_UTF16_UPPER=0`):

* ASCII `'a'..'z'` → subtract 32  
* Cyrillic `U+0430..U+044F` → subtract 32  
* Cyrillic extensions `U+0450..U+045F` → subtract 80 (`0x50`)  
* else unchanged  

No callees, no globals, no memory side effects.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/casefold.rs`](../../rust_kernel/kolibri_utils/src/casefold.rs) `utf16_to_upper` |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_utf16_to_upper` |
| Build | [`rust_kernel/kolibri_utils/build-utf16-upper.ps1`](../../rust_kernel/kolibri_utils/build-utf16-upper.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_utf16_to_upper.bin` |

`#![no_std]` freestanding; `#[inline(always)]` into FFI so the dedicated section stays self-contained.

---

## ABI

### FASM `utf16toUpper` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register / plain `call` + `ret` |
| Input | UTF-16 code unit in **AX** |
| Output | Uppercased unit in **AX** |
| Stack cleanup | none (caller) |
| Flags | clobbered |
| Memory | none |

### Rust `rust_utf16_to_upper`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(ch: u32)` — low 16 bits used |
| Return | `EAX` zero-extended uppercased unit |
| Epilogue | `ret 4` |

### Trampoline

```asm
utf16toUpper:
        stdcall rust_utf16_to_upper, eax
        ret
```

---

## Link strategy

**Strategy A + D** (reloc-free raw blob + FASM trampoline).

| Rejected | Why |
|----------|-----|
| B `rust-lld` | No `.rodata` / relocs / helpers needed |
| C-only rewrite | Still needs Cut A embed path; A+D is sufficient |
| Reject candidate | Not needed — leaf is suitable |

---

## Section

`.text.rust_utf16_to_upper`

---

## Blob size

**71** bytes

---

## Relocations

**0** (extractor hard-fail if any)

---

## Extractor result

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_utf16_to_upper` |
| `SHT_PROGBITS` | yes |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 4` (`C2 04 00`) present |
| Blob SHA-256 | `B2D5C5E9ED75F71991F374D14A4226A6A2FABE2CCF0C41555E77D1F530F5CCE1` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 4`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-utf16-upper.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-c-on.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-c-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-c-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-c-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4470,server,nowait
```

Rollback OFF: set `USE_RUST_UTF16_UPPER = 0`, reassemble, CoW/replace, QEMU (port 4471 used in audit).

---

## Rust tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **34/34** (was 31; +3 utf16 casefold) |
| Named Latin / Cyrillic / boundary vectors | **PASS** |

---

## Differential tests

| Suite | Result |
|-------|--------|
| Exhaustive `0..=0xFFFF` vs FASM-oracle | **PASS** |

Host differential is algorithm-oracle level (mirrors `parse_fn.inc` control flow), not a host-assembled FASM binary.

---

## ABI test

In-kernel smoke `utf16_upper_rust_smoke_test` calls real `utf16toUpper` trampoline with:

* `'a' → 'A'`  
* `U+0450 → U+0400`  

Hang-on-fail (`EAX=0xDEAD1651`) if mismatch. Reaching desktop implies trampoline path matched.

---

## Kernel smoke

Called from `high_code` after Cut B smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_UTF16_UPPER=1`) | **PASS** — `query-status=running`; desktop screendump `tmp_images/cut-c-on-desktop.png` |
| Rust OFF (`USE_RUST_UTF16_UPPER=0`) | **PASS** — `query-status=running`; desktop screendump `tmp_images/cut-c-off-desktop.png` |

---

## Determinism

Two deep-clean freestanding rebuilds (delete `i686-kolibri-none/release`, recompile `core`+crate):

| Build | Size | SHA-256 |
|------:|-----:|---------|
| #1 | 71 | `B2D5C5E9ED75F71991F374D14A4226A6A2FABE2CCF0C41555E77D1F530F5CCE1` |
| #2 | 71 | `B2D5C5E9ED75F71991F374D14A4226A6A2FABE2CCF0C41555E77D1F530F5CCE1` |

**PASS** — identical size and bytes.

---

## Rollback

```asm
USE_RUST_UTF16_UPPER = 0   ; original FASM body
USE_RUST_UTF16_UPPER = 1   ; Rust trampoline (default)
```

Independent of Cut A (`USE_RUST_CRC` / `UTF16` / `CP866` / `UTF8`) and Cut B (`USE_RUST_CP866_UPPER`).  
Original FASM body remains in the `else` branch of `parse_fn.inc`.

Wire-up:

* Trampoline / switch: `kernel/fs/parse_fn.inc`  
* Embed + smoke: `kernel/rust/utf16_upper.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Image size

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut C ON (default) | **223288** |
| Cut C OFF (FASM body; blob still embedded) | **223320** |
| Cut B baseline (pre–Cut C) | 223176 |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob (same Cut A/B pattern). Size delta itself is not a failure.

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Known limitations

* Smoke exercises the trampoline with fixed vectors; the 11 FS callers are not separately instrumented (desktop + hang-on-fail proves smoke path ran).  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Full Unicode casefold is **not** claimed — only the Kolibri FASM ranges.

---

## Evidence

### PROVEN

* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 4`  
* Rust unit + exhaustive differential `0..=0xFFFF`  
* In-kernel trampoline smoke (hang-on-fail)  
* QEMU ON and OFF reach desktop  
* Blob determinism across two deep-clean rebuilds  
* Independent rollback switch builds both ways  
* Cut B blob hash unchanged (`rust_cp866_to_upper.bin` = `8F171F09…27E5`)  
* Full `kolibri_utils` suite green (34/34)

### NOT PROVEN

* Byte-identical behavior of every FS caller path under real filesystem workloads  
* Host-assembled FASM binary vs Rust binary differential  
* Full Unicode case folding beyond FASM’s Latin/Cyrillic ranges

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging  
* Other `parse_fn.inc` leaves (`uni2ansi_char`, `utf8to16`, …)  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Cut D

---

## Verification matrix

```text
Candidate audit                         PASS
Cut C plan                              PASS
Rust implementation                     PASS
Rust tests                              PASS (34/34)
Differential/oracle                     PASS (exhaustive 65536)
ABI/trampoline                          PASS (in-kernel smoke)
Relocation validation                   PASS
Kernel smoke                            PASS
QEMU Rust ON                            PASS
QEMU Rust OFF                           PASS
Deterministic rebuild ×2                PASS
Rollback switch                         PASS
Cut A regression                        PASS (suite + blob extract)
Cut B regression                        PASS (suite + cp866_upper hash)
Documentation                           PASS
```

**STOP** — do not start Cut D in this session.
