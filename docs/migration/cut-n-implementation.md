# Cut N Implementation — `antiAliasing`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-n-plan.md`](cut-n-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `antiAliasing` |
| Source | [`kernel/gui/font.inc`](../../kernel/gui/font.inc) |
| Callers | 2 (`drawChar` left/right AA when `fontSmoothing == 1`) |
| Rust symbol | `rust_anti_aliasing` |
| Pure helper | `kolibri_utils::anti_aliasing` |
| Subsystem | GUI / font smoothing |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `antiAliasing` | **Selected** — first GUI leaf; novel BP/EBP dual-use ABI |
| `test_app_header` | Rejected — process/MENUET leaf deferred (Stage 6 foothold; globals) |
| `strtoint_dec` | Rejected — conf-parse fallback; thinner novelty after A–D |
| `memmove` | Rejected — memcpy helper-risk deferred again |
| `coff_get_align` | Rejected — PE leaf too thin after A–M |
| `DummyTest` | Rejected — non-local `pop` fail return |
| `hotkey_test*` / `hotkey_do_test` | Rejected — thin predicates / IRQ+table risk after Cut L |
| timers / clipboard / sound / sync / unpack | Rejected — not Strategy A+C leaves |
| FS / NTFS / FAT / video clip / mouse / TCP | Excluded by Cut N preference (covered A–M) |

---

## Why selected

Cut N’s research question: does Strategy A + C remain viable for a **GUI font color-blend leaf** whose FASM body uses a **16-bit `BP` counter sharing `EBP`**, later restored as the 32-bit background color — reloc-free, byte-exact, smokeable?

| Preference | Result |
|------------|--------|
| New subsystem | Yes — GUI font smoothing (not Cut H blitter geometry) |
| New ABI property | Yes — BP/EBP dual-use; trampoline owns `mov ebp, ebx` |
| Outside FS/NTFS/FAT/video-clip/mouse/TCP | Yes |
| Strategy A feasible | Pure arithmetic; no globals/tables/`.rodata` |
| Clear ABI | `EAX`=fg, `EBX`=bg → `EAX` blend; `EBP=EBX` |
| Testability | Exhaustive single-channel + grids + 200k PRNG |
| Limited blast radius | 2 callers; independent switch |

---

## Original implementation

FASM leaf in `font.inc` (retained under `USE_RUST_ANTI_ALIASING=0`):

* `mov bp, 3` loop over low three bytes  
* Per byte: `out = (3*fg + bg) >> 2` via `lea`/`add`/`shr`  
* `ror eax/ebx, 8` between channels; fourth `ror` restores lanes  
* High byte of `EAX` is **not** blended  
* Ends with `mov ebp, ebx`

Call sites (`drawChar`) set `ebx = ebp` (font color as bg) and `xor ecx, ecx` before `call antiAliasing`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/font.rs`](../../rust_kernel/kolibri_utils/src/font.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_anti_aliasing` |
| Build | [`rust_kernel/kolibri_utils/build-anti-aliasing.ps1`](../../rust_kernel/kolibri_utils/build-anti-aliasing.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_anti_aliasing.bin` |

`#![no_std]` freestanding; unrolled 3-channel blend in the final machine code; no slice fills (avoids memset/GOT).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | loop/`bp` counter in source; compiler unrolled to straight-line |
| Memory | none (pure register dword transform) |
| vs Cut H | Color blend vs rectangle clip geometry |
| vs Cut L/M | GUI leaf vs HID/TCP |

---

## ABI

### FASM `antiAliasing` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` = foreground RGB; `EBX` = background RGB |
| Output | `EAX` = blended (low 3 bytes); high byte preserved |
| Side effect | `EBP := EBX` |
| Clobbered | `ECX`, `EDX` (original); trampoline may preserve more |

### Rust `rust_anti_aliasing`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(fg: u32, bg: u32) -> u32` |
| Epilogue | `ret 8` |

### Trampoline

```asm
antiAliasing:
        stdcall rust_anti_aliasing, eax, ebx
        mov     ebp, ebx
        ret
```

EBP restoration stays in FASM so the Rust blob has no frame quirks and remains reloc-free.

---

## Compiler artifact audit

| Check | Result |
|-------|--------|
| Relocations targeting section | **0** (extractor gate) |
| External symbols | **none** |
| `CALL` (`E8`) in blob | **none** |
| GOT / PLT | **none** |
| memset / memcpy / memmove / memcmp | **none** |
| Integer / FP helpers | **none** |
| `.rodata` refs | **none** |
| Epilogue | trailing `ret 8` (`C2 08 00`) |

Disassembly observation: three unrolled channel blends (`movzx`/`lea`×3+bg/`shr 2`/`rol`-style rotate via `ror` encoded as rotate), `push`/`pop esi`, `ret 8`. False-positive `E9` bytes are `SHR` ModR/M, not jumps.

---

## Extraction

| Field | Value |
|-------|-------|
| Section | `.text.rust_anti_aliasing` |
| Symbol offset | **0** |
| Epilogue | `ret 8` trailing |
| Blob size | **88** bytes |
| Blob SHA-256 | `BDC8875C09B269A2C62B9CC2934CBD345B19CA755BA71054AA1C0D1CAFF5FAE1` |
| Relocations | **0** |
| Compiler dependencies | **none** |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 8`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-anti-aliasing.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-n-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-n-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-n-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-n-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4540,server,nowait
```

Rollback OFF: set `USE_RUST_ANTI_ALIASING = 0`, assemble to `kernel-off.mnt`, CoW/replace, QEMU (port 4541 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **142/142** (was 134; +8 font) |
| Equal-color identity | **PASS** |
| Toward-black / toward-white named vectors | **PASS** |
| High-byte preservation | **PASS** |
| Exhaustive single-channel pairs (256×256) | **PASS** |
| Grid low-bytes × sample high lanes | **PASS** |
| Deterministic PRNG (200 000, seed `0xAA11A51A`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle in `font.rs` tests | **PASS** |
| Compare full `EAX` dword after blend | **PASS** |
| High byte unchanged through rotate cycle | **PASS** |

Oracle assumes zeroed high parts of `ECX`/`EDX` during the leaf (matches `drawChar` `xor ecx, ecx` and byte-sized glyph-row EDX before `mov dl, bl`).

---

## ABI / trampoline / registers

In-kernel smoke `anti_aliasing_rust_smoke_test` calls real `antiAliasing`:

| Check | Result |
|-------|--------|
| Equal `0x00AABBCC` identity | **PASS** |
| `EBP == EBX` after return | **PASS** |
| Toward black `0xFF → 0xBF` | **PASS** |
| Multi-channel `0x00AABBCC → 0x007F8C99` | **PASS** |
| High byte `0xA1020304 → 0xA1010203` | **PASS** |
| Toward white `0 → 0x3F` with `EBP=0xFF` | **PASS** |
| ESI/EDI preserved across call | **PASS** |
| Public symbol smoke | **PASS** |

---

## Kernel smoke

Called from `high_code` after Cut M smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

Real caller path: public `antiAliasing` symbol used by smoke (same trampoline as `drawChar`). Live `drawChar` AA under every QEMU glyph is **NOT PROVEN** (depends on `fontSmoothing` and which characters are drawn).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_ANTI_ALIASING=1`) | **PASS** — `running`; screendump `dev_build/cut-n-on.ppm` (2359312 bytes) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `dev_build/cut-n-off.ppm` (2359312 bytes) |

---

## Regression lock (Cuts A–M)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (142/142) |
| Cut M blob `rust_tcp_xmit_timer.bin` | **PASS** — `D469B83C…6CFC01` unchanged |
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
| #1 | 88 | `BDC8875C09B269A2C62B9CC2934CBD345B19CA755BA71054AA1C0D1CAFF5FAE1` |
| #2 (full clean `i686-kolibri-none` rebuild + extract) | 88 | `BDC8875C09B269A2C62B9CC2934CBD345B19CA755BA71054AA1C0D1CAFF5FAE1` |

---

## Rollback

```
USE_RUST_ANTI_ALIASING = 0   ; original FASM body
USE_RUST_ANTI_ALIASING = 1   ; Rust trampoline (default)
```

Independent of Cuts A–M switches.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel.mnt` Rust ON | 229784 | `8A6CDF2F5A2B1698FB78D61BCFA1533A149090772898446D0474CFF5383673EF` |
| `kernel-off.mnt` Rust OFF | 229800 | `24EB1AF920EA023CADB4645F37C9B303E9FC889CA6F9888B8CF0BB5A482647B2` |

(OFF is larger because the original FASM body exceeds the trampoline; the Rust blob remains embedded under both configs via `rust/anti_aliasing.inc`.)

---

## Evidence summary

### PROVEN

* Candidate audit selecting GUI `antiAliasing` over process/conf/memmove/PE/keyboard/timer alternates  
* Freestanding Rust + reloc-free 88-byte blob, 0 relocations, no helpers/GOT  
* RGB blend oracle (exhaustive low-byte + 200k PRNG seed `0xAA11A51A`)  
* Trampoline ABI: `ret 8`, `EBP = EBX` restoration in FASM  
* In-kernel public-symbol smoke (hang-on-fail)  
* QEMU ON/OFF → `running` + screendumps  
* Cuts A–M blob hashes unchanged  
* Determinism ×2  

### NOT PROVEN

* Every desktop glyph path through `drawChar` AA under QEMU (depends on `fontSmoothing` and drawn text)  
* Host FASM binary vs Rust blob instruction-level equivalence (oracle is semantic; Rust is unrolled)  
* Subpixel smoothing paths (`.subpixelLeft` / `.subpixelRight`)  

### OUT OF SCOPE

* Migrating `drawChar` or subpixel blend formulas  
* Changing `fontSmoothing` defaults  
* `test_app_header` / `strtoint_dec` / `memmove` / Cut O  

---

## Remaining issues

none

---

## Completion

**Cut N COMPLETE — STOP.** Do not start Cut O.
