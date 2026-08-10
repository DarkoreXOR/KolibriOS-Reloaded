# Cut O Implementation — `test_app_header`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-o-plan.md`](cut-o-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `test_app_header` |
| Source | [`kernel/core/taskman.inc`](../../kernel/core/taskman.inc) |
| Callers | 1 (`fs_execute` at `taskman.inc` after `load_file`) |
| Rust symbol | `rust_test_app_header` |
| Pure helper | `kolibri_utils::test_app_header` |
| Subsystem | Process / MENUET app header parse |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `test_app_header` | **Selected** — first process/MENUET leaf; real `fs_execute` caller; Stage 6 foothold |
| `is_region_userspace` | Rejected — strong ZF-out syscall helper, but thinner body and ~31 callers |
| `window._.check_window_position` | Rejected — window novelty real, but Cut N already covered GUI; deferred |
| `strtoint_dec` | Rejected — `conf_lib` not linked (`if 0`); no live caller |
| `coff_get_align` | Rejected — PE leaf too thin after A–N |
| `memmove` | Rejected — memcpy helper-risk deferred again |
| `hotkey_*` / `DummyTest` / timers | Rejected — thin / non-local return / locks |
| FS / NTFS / FAT / video clip / mouse / TCP / font AA | Excluded by Cut O preference (covered G–N) |

---

## Why selected

Cut O’s research question: does Strategy A + C remain viable for a **process/MENUET binary-format leaf** that validates headers, injects `[pg_data.pages_free]` through the trampoline, partially mutates `APP_HDR.eip` on mid-check fail, and is exercised by the **real `fs_execute` app-launch path** under QEMU?

| Preference | Result |
|------------|--------|
| New subsystem | Yes — process / MENUET header parse (not FS/GUI/HID/TCP) |
| New semantic property | Yes — binary-format validation + multi-field structure fill + mid-fail partial mutate |
| Real caller path | Yes — `fs_execute` → `test_app_header` on every `/sys` app launch |
| Outside FS/NTFS/FAT/video/mouse/TCP/font | Yes |
| Strategy A feasible | No calls; trampoline injects `pages_free`; `OS_BASE` is a const |
| Clear ABI | `EAX` image / `EBX` APP_HDR → success leaves `EAX`, fail `EAX=0` |
| Testability | Crafted headers + grids + 200k PRNG |
| Limited blast radius | 1 caller; independent switch |

---

## Original implementation

FASM leaf in `taskman.inc` (retained under `USE_RUST_TEST_APP_HEADER=0`):

* Accept banner `MENU` + `ET` + `01`/`02`  
* Write `eip` from `start` **before** memory checks (partial mutate on later fail)  
* Require `mem_size >= i_end`, `mem_size < OS_BASE`, `mem_size < pages_free<<12`  
* On success fill `_emem`, `esp`, `cmdline`, `path`, `_edata`  
* On fail `xor eax, eax`  
* `version` dword at `+8` is **not** validated  

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/app_header.rs`](../../rust_kernel/kolibri_utils/src/app_header.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_test_app_header` |
| Build | [`rust_kernel/kolibri_utils/build-test-app-header.ps1`](../../rust_kernel/kolibri_utils/build-test-app-header.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_test_app_header.bin` |

`#![no_std]` freestanding; magic compares use immediates (not `.rodata` string literals); dword stores via `write_volatile` (avoids memset/GOT).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | Magic fail; three mem_size fails; success fill |
| Memory | Reads 36-byte MENUET header; writes up to 6 `APP_HDR` dwords |
| Partial mutate | `eip` may be written before a mem_size fail |
| vs Cut M | Process header parse vs TCP socket field update |
| vs Cut N | Process leaf vs GUI color blend |

---

## ABI

### FASM `test_app_header` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` → app image; `EBX` → `APP_HDR` |
| Success | `EAX` unchanged (image pointer); six fields filled |
| Fail | `EAX = 0`; possibly `eip` already written |
| Clobbered | `ECX`, `EDX` |
| Preserved | `EBX` (stdcall callee-saved; smoke verified) |
| Global | `[pg_data.pages_free]` read by trampoline |

### Rust `rust_test_app_header`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(header, app_hdr, pages_free) -> u32` |
| Return | header pointer on success; `0` on fail |
| Epilogue | `ret 12` |
| Globals | none |

### Trampoline

```asm
test_app_header:
        mov     ecx, [pg_data.pages_free]
        stdcall rust_test_app_header, eax, ebx, ecx
        ret
```

---

## Compiler artifact audit

| Check | Result |
|-------|--------|
| Section | `.text.rust_test_app_header` |
| Relocations | **0** (extractor rejects otherwise) |
| External symbols | **none** |
| `CALL` / GOT / PLT | **none** (no `E8` in blob) |
| memset/memcpy/memmove helpers | **none** |
| `.rodata` / string literals | **none** (magic immediates) |
| Epilogue | `ret 12` (`C2 0C 00`) |
| Blob size | **113** bytes |
| Blob SHA-256 | `83DFD0E3AEB632EF0C7B2E148A22E14EDCDE5E0D097F8DD64C35574A6396B305` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 12`.

Observation: LLVM reordered the three mem_size checks and implemented `mem_size >= OS_BASE` as `test`/`js` (sign bit ≡ `>= 0x80000000` unsigned). Partial-mutate semantics preserved (`eip` store remains before all checks).

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-test-app-header.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-o-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-o-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-o-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-o-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4550,server,nowait
```

Rollback OFF: set `USE_RUST_TEST_APP_HEADER = 0`, assemble to `kernel-off.mnt`, CoW/replace, QEMU (port 4551 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **152/152** (was 142; +10 app_header) |
| MENUET01 success fills all six fields | **PASS** |
| MENUET02 accepted | **PASS** |
| Bad magic fails without mutate | **PASS** |
| `mem_size < i_end` partial `eip` write | **PASS** |
| `OS_BASE` / `pages_free<<12` boundaries | **PASS** |
| Structured banner×mem×i_end×pages grid | **PASS** |
| Deterministic PRNG (200 000, seed `0x7E57A0AD`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle in `app_header.rs` | **PASS** |
| Compare success/fail + full 0x18 `APP_HDR` bytes | **PASS** |
| Partial-mutate on mid-fail | **PASS** |

---

## ABI / trampoline / registers

In-kernel smoke `test_app_header_rust_smoke_test` calls real `test_app_header`:

| Check | Result |
|-------|--------|
| MENUET01 success field mapping | **PASS** |
| Bad magic no mutate | **PASS** |
| Mid-fail `eip` written, `_emem` not | **PASS** |
| MENUET02 success | **PASS** |
| ESI/EDI preserved | **PASS** |
| Public symbol smoke | **PASS** |

---

## Real caller validation

| Path | Result |
|------|--------|
| Identified caller | `fs_execute` → `call test_app_header` |
| QEMU exercise | Desktop bring-up launches `/sys` apps through `fs_execute` |
| Evidence | QEMU ON/OFF → `query-status=running` + screendump (desktop with apps) |
| Synthetic smoke | Public-symbol hang-on-fail also **PASS** |

**PROVEN:** live `fs_execute` path under QEMU desktop launch (same trampoline as production).  
Hang-on-fail smoke would prevent desktop if Rust ABI mismatched.

---

## Kernel smoke

Called from `high_code` after Cut N smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_TEST_APP_HEADER=1`) | **PASS** — `running`; screendump `dev_build/cut-o-on.ppm` (2359312 bytes) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `dev_build/cut-o-off.ppm` (2359312 bytes) |

---

## Regression lock (Cuts A–N)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (152/152) |
| Cut N blob `rust_anti_aliasing.bin` | **PASS** — `BDC8875C…FAE1` unchanged |
| Cut M blob `rust_tcp_xmit_timer.bin` | **PASS** — `D469B83C…FC01` unchanged |
| Cut L blob `rust_mouse_acceleration.bin` | **PASS** — `D1E51E85…169A` unchanged |
| Cut K blob `rust_fat_next_short_name.bin` | **PASS** — `E9CFFE65…A52F` unchanged |
| Cut J blob `rust_ntfs_restore_usa.bin` | **PASS** — `851FC92B…C417` unchanged |
| Cut I blob `rust_ntfs_decode_mcb_entry.bin` | **PASS** — `DA888977…39C9` unchanged |
| Cut H blob `rust_block_clip.bin` | **PASS** — `C79E5D83…E5D6` unchanged |
| Cut G blob `rust_fs_calculate_time.bin` | **PASS** — `B7B1AB42…1777` unchanged |
| Cut F blob `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Cut E blob `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut D blob `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut C blob `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut B blob `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut A UTF-8 / CP866 / UTF-16 / CRC blobs | **PASS** — unchanged vs prior locks |

---

## Determinism

| Build | Size | SHA-256 |
|-------|------|---------|
| #1 | 113 | `83DFD0E3AEB632EF0C7B2E148A22E14EDCDE5E0D097F8DD64C35574A6396B305` |
| #2 (full clean `i686-kolibri-none` rebuild + extract) | 113 | `83DFD0E3AEB632EF0C7B2E148A22E14EDCDE5E0D097F8DD64C35574A6396B305` |

---

## Rollback

```
USE_RUST_TEST_APP_HEADER = 0   ; original FASM body
USE_RUST_TEST_APP_HEADER = 1   ; Rust trampoline (default)
```

Independent of Cuts A–N switches.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel.mnt` Rust ON | 230456 | `6D2CF18BDF82810C29C2036C635AC9D74D5A66AE3649685F0872A9AB8881565F` |
| `kernel-off.mnt` Rust OFF | 230536 | `C5C757350282FE818629B9D6B6BDF94539C56FBAEF080F6DD210153AF67A7936` |

(OFF is larger because the original FASM body exceeds the trampoline; the Rust blob remains embedded under both configs via `rust/test_app_header.inc`.)

---

## Evidence summary

### PROVEN

* Candidate audit selecting process `test_app_header` over syscall/window/conf/memmove/PE/keyboard alternates  
* Freestanding Rust + reloc-free 113-byte blob, 0 relocations, no helpers/GOT  
* FASM-faithful oracle (grid + 200k PRNG seed `0x7E57A0AD`) including mid-fail partial mutate  
* Trampoline ABI: `ret 12`, `pages_free` injected from FASM  
* In-kernel public-symbol smoke (hang-on-fail)  
* **Live `fs_execute` caller** under QEMU desktop app launch  
* QEMU ON/OFF → `running` + screendumps  
* Cuts A–N blob hashes unchanged  
* Determinism ×2 (including full freestanding clean)  

### NOT PROVEN

* Exhaustive launch of every `/sys` binary under QEMU (desktop subset exercised)  
* Host FASM binary vs Rust blob instruction-level equivalence (oracle is semantic; LLVM reordered checks)  
* MENUET `version` dword semantics beyond current FASM (still ignored)  

### OUT OF SCOPE

* Migrating `fs_execute` / `create_process` / scheduler  
* Re-enabling `conf_lib` / `strtoint_dec`  
* `is_region_userspace` / window clamp / `memmove` / Cut P  

---

## Remaining issues

none
