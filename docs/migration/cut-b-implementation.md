# Cut B Implementation — `cp866toUpper`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-b-plan.md`](cut-b-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Cut B target

| Field | Value |
|-------|-------|
| FASM symbol | `cp866toUpper` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Sole caller | [`kernel/fs/fat.inc:518`](../../kernel/fs/fat.inc) (`lodsb` → `call cp866toUpper` → `stosb`) |
| Rust symbol | `rust_cp866_to_upper` |
| Pure helper | `kolibri_utils::cp866_to_upper` |

---

## Original implementation

FASM leaf in `parse_fn.inc` (retained under `USE_RUST_CP866_UPPER=0`):

* ASCII `'a'..'z'` → subtract 32  
* CP866 `0xA0..0xAF` → subtract 32  
* CP866 `0xE0..0xEF` → subtract `0x50` (`0xE0-0x90`)  
* CP866 `0xF0..0xF7` → `and` with `~1` (ё→Ё pairs)  
* else unchanged  

No callees, no globals, no memory side effects.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/casefold.rs`](../../rust_kernel/kolibri_utils/src/casefold.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_cp866_to_upper` |
| Build | [`rust_kernel/kolibri_utils/build-cp866-upper.ps1`](../../rust_kernel/kolibri_utils/build-cp866-upper.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_cp866_to_upper.bin` |

`#![no_std]` freestanding; `#[inline(always)]` into FFI so the dedicated section stays self-contained.

---

## ABI

### FASM `cp866toUpper` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register / plain `call` + `ret` |
| Input | CP866 character in **AL** |
| Output | Uppercased CP866 character in **AL** |
| Stack cleanup | none (caller) |
| Flags | clobbered |
| Memory | none |

### Rust `rust_cp866_to_upper`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(ch: u32)` — low 8 bits used |
| Return | `EAX` zero-extended uppercased byte |
| Epilogue | `ret 4` |

### Trampoline

```asm
cp866toUpper:
        stdcall rust_cp866_to_upper, eax
        ret
```

---

## Link strategy

**Strategy A + D** (reloc-free raw blob + FASM trampoline).

| Rejected | Why |
|----------|-----|
| B `rust-lld` | No `.rodata` / relocs / helpers needed |
| C-only rewrite | Still needs Cut A embed path; A+D is sufficient |

---

## Section / blob / relocations

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_cp866_to_upper` |
| Blob size | **71** bytes |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 4` (`C2 04 00`) present |
| Blob SHA-256 | `8F171F0970ADB03A5DDF3FB0B012CB85CA254D84D1B1F7BFAADA063BDB7D27E5` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 4`.

---

## Build command

```powershell
powershell -File rust_kernel/kolibri_utils/build-cp866-upper.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-b-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-b-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-b-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-b-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4460,server,nowait
```

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **31/31** (was 27; +4 casefold) |
| Exhaustive differential `0..=255` vs FASM-oracle | **PASS** |
| Named edge vectors (ASCII / Cyrillic / ё) | **PASS** |
| In-kernel smoke (`cp866_upper_rust_smoke_test`) | **PASS** (hang-on-fail; desktop reached) |
| QEMU Rust ON (`USE_RUST_CP866_UPPER=1`) | **PASS** — desktop |
| QEMU Rust OFF (`USE_RUST_CP866_UPPER=0`) | **PASS** — desktop |
| Reloc validation | **PASS** |
| Clean rebuild ×2 blob hash | **PASS** (identical SHA-256) |

Screendumps: `dev_build/cut-b-on-desktop.png`, `dev_build/cut-b-off-desktop.png`.

---

## Kernel image sizes

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut B ON (default) | **223176** |
| Cut B OFF (FASM body; blob still embedded) | **223208** |
| Cut A baseline (pre–Cut B) | 223080 |

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Rollback switch

```asm
USE_RUST_CP866_UPPER = 0   ; original FASM body
USE_RUST_CP866_UPPER = 1   ; Rust trampoline (default)
```

Independent of Cut A `USE_RUST_CRC` / `UTF16` / `CP866` / `UTF8`.  
Original FASM body remains in the `else` branch of `parse_fn.inc`.

Wire-up:

* Trampoline / switch: `kernel/fs/parse_fn.inc`  
* Embed + smoke: `kernel/rust/cp866_upper.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed vectors; the sole FAT caller is not separately instrumented (desktop boot still proves smoke path ran).  
* Rollback OFF still embeds the Rust blob (same Cut A pattern) — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  

---

## Verification matrix

```text
Cut A regression tests                  PASS (31/31 includes prior suites)
Cut B Rust tests                        PASS
Cut B differential tests                PASS (exhaustive 256)
Cut B ABI/trampoline test               PASS (in-kernel smoke)
Cut B kernel smoke                      PASS
QEMU Rust ON                            PASS
QEMU Rust OFF                           PASS
Relocation validation                   PASS
Blob determinism                        PASS
Clean rebuild                           PASS
Rollback switch                         PASS
Documentation                           PASS
```

**STOP** — do not start Cut C in this session.
