# Cut L Implementation — `mouse_acceleration`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-l-plan.md`](cut-l-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `mouse_acceleration` |
| Source | [`kernel/hid/mousedrv.inc`](../../kernel/hid/mousedrv.inc) |
| Callers | 2 (`set_mouse_data` relative X and Y paths) |
| Rust symbol | `rust_mouse_acceleration` |
| Pure helper | `kolibri_utils::mouse_acceleration` |
| Subsystem | HID / mouse input |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `mouse_acceleration` | **Selected** — first HID leaf; accel curve + EAX high-word sign quirk |
| `UTF16to8` | Rejected — SF ABI interesting but FS-parse callers / post-G/I/J/K FS weight |
| `memmove` | Rejected — memcpy helper-risk deferred; ~24 callers; thin vs HID novelty |
| `coff_get_align` | Rejected — PE leaf too small after A–K |
| `unpack` | Rejected — LZMA body too large for leaf-cut culture |
| `strrchr` | Rejected — DF already proven by Cut K; string family |
| `hotkey_do_test` | Rejected — table/global/IRQ-adjacent |
| `antiAliasing` | Rejected — quirky EBP; GUI-adjacent |

---

## Why selected

Cut L’s research question: does Strategy A + C remain viable for a **HID mouse acceleration leaf** (input subsystem; AX-only abs; mul-based curve; EAX high-word sign restore; trampoline-injected tunables) as a reloc-free blob with a byte-exact differential oracle?

| Preference | Result |
|------------|--------|
| Outside FS / NTFS / FAT / video geometry | Yes — HID / mouse |
| New ABI / memory property | Yes — first input leaf; trampoline loads globals into stack args; EAX high-word sign quirk |
| Strategy A feasible | Pure arithmetic; no tables / `.rodata` |
| Clear ABI | `EAX` in/out; `CX` clobber; plain `ret` |
| Testability | Exhaust AX; grid delay/factor; 200k PRNG |
| Limited blast radius | 2 callers; independent switch |

---

## Original implementation

FASM leaf in `mousedrv.inc` (retained under `USE_RUST_MOUSE_ACCELERATION=0`):

* `neg ax` / `jl` loop → absolute value on `AX` only (high half of `EAX` intact)  
* `add al, [mouse_delay]` → `mul al` → `dec ax` / `shr ax, cl` / `inc ax` with `CX = [mouse_speed_factor]`  
* `test eax, eax` / `jns` / `neg ax` restores sign using surviving high word  
* Defaults: `mouse_delay = 3`, `mouse_speed_factor = 4`

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/mouse.rs`](../../rust_kernel/kolibri_utils/src/mouse.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_mouse_acceleration` |
| Build | [`rust_kernel/kolibri_utils/build-mouse-acceleration.ps1`](../../rust_kernel/kolibri_utils/build-mouse-acceleration.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_mouse_acceleration.bin` |

`#![no_std]` freestanding; SHR count modeled as x86 5-bit mask with 16..31 → zero (avoids Rust debug shift panic while matching CPU).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | Abs loop; shift≥16 zero path; signed restore |
| Loops | Bounded abs (≤2 iterations for normal deltas) |
| Memory | None in Rust body — tunables passed as args |
| vs Cut K | New subsystem (HID); no DF/CF/buffer mutate |

---

## ABI

### FASM `mouse_acceleration` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` motion delta |
| Output | accelerated `EAX`/`AX` |
| Clobbers | `CX` (original loads speed factor) |
| Globals | reads `[mouse_delay]`, `[mouse_speed_factor]` |

### Rust `rust_mouse_acceleration`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(delta: u32, delay: u32, speed_factor: u32)` |
| Return | `u32` in `EAX` |
| Epilogue | `ret 12` |

### Trampoline

```asm
mouse_acceleration:
        movzx   ecx, byte [mouse_delay]
        movzx   edx, word [mouse_speed_factor]
        stdcall rust_mouse_acceleration, eax, ecx, edx
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | `set_mouse_data` relative X; relative Y (after `neg eax`) |
| Upstream | PS/2 / USB / COM mouse → `set_mouse_data` |
| Related (not migrated) | `sysfn_mouse_acceleration` get/set tunables |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state in Rust | none (trampoline loads tunables) |
| Static data / `.rodata` | none |
| External calls | none |
| Compiler helpers | none observed in final blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Compiler artifact audit

| Check | Result (final) |
|-------|----------------|
| Section | `.text.rust_mouse_acceleration` |
| Relocations targeting section | **0** |
| Symbol at section offset 0 | **yes** |
| Trailing `ret 12` (`C2 0C 00`) | **yes** |
| `CALL rel32` (`E8` + disp32) | **none** (bytes `E8` present only as `jg` rel8 / `shr eax,cl` opcode forms) |
| `memset`/`memcpy`/`memmove`/`memcmp` / GOT | **none** |
| External symbols | **none** |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_mouse_acceleration` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 12` trailing |
| Blob size | **88** bytes |
| Blob SHA-256 | `D1E51E85E1FB27AAE8F2E5C79D04A4607DDF8E1E941E206D3E3C1DB6D43B169A` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 12`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-mouse-acceleration.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-l-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-l-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-l-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-l-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4520,server,nowait
```

Rollback OFF: set `USE_RUST_MOUSE_ACCELERATION = 0`, reassemble, CoW/replace, QEMU (port 4521 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **126/126** (was 117; +9 mouse) |
| Named hand values (defaults delay=3,factor=4) | **PASS** |
| Signed i32 caller-shape ±200 × tunables | **PASS** |
| Exhaust AX 0..0xFFFF (+ sign-extended form) @ defaults | **PASS** |
| Grid delay 0..20 × factor 0..16 × delta ±40 | **PASS** |
| Deterministic PRNG (200 000, seed `0xA11CE70D`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle in `mouse.rs` tests | **PASS** |
| Compare full `EAX` (not only `AX`) | **PASS** |
| AL wrap / large products / shift 16..31 | **PASS** |

---

## ABI / trampoline / registers

In-kernel smoke `mouse_acceleration_rust_smoke_test` calls real `mouse_acceleration` with defaults:

| Check | Result |
|-------|--------|
| `EAX=1` → `AX=1` | **PASS** |
| `EAX=5` → `AX=4` | **PASS** |
| `EAX=-5` → `AX=-4` (high-word sign restore) | **PASS** |
| `EAX=0` → `AX=1` | **PASS** |
| `EAX=100` → `AX=664` | **PASS** |
| EBX/ESI/EDI/EBP preserved | **PASS** |
| Public symbol smoke | **PASS** |

---

## Kernel smoke

Called from `high_code` after Cut K smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_MOUSE_ACCELERATION=1`) | **PASS** — `running`; screendump `dev_build/cut-l-on.ppm` (2359312 bytes) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `dev_build/cut-l-off.ppm` (2359312 bytes) |

---

## Regression lock (Cuts A–K)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (126/126) |
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
| #1 | 88 | `D1E51E85E1FB27AAE8F2E5C79D04A4607DDF8E1E941E206D3E3C1DB6D43B169A` |
| #2 (forced clean recompile + extract) | 88 | `D1E51E85E1FB27AAE8F2E5C79D04A4607DDF8E1E941E206D3E3C1DB6D43B169A` |

---

## Rollback

```
USE_RUST_MOUSE_ACCELERATION = 0   ; original FASM body
USE_RUST_MOUSE_ACCELERATION = 1   ; Rust trampoline (default)
```

Independent of Cuts A–K switches.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel.mnt` Rust ON | 229080 | `585C4B5E9331E69E0FA6F137A52DD24AD133BDED784B8C65148FEEC2AD03C929` |
| `kernel-off.mnt` Rust OFF | 229080 | `471E06805D4FFE1C92E4CEB3A7AB280127CFC764628604D7BDF8D61F446B93E4` |

(Size equal because the Rust blob remains embedded under OFF; only the trampoline vs FASM body differs.)

---

## Evidence summary

### PROVEN

* Candidate audit selecting HID `mouse_acceleration` over FS/encoding/memmove/loader/compression alternates  
* Freestanding Rust + reloc-free 88-byte blob, 0 relocations, no helpers/GOT  
* Differential oracle (exhaustive AX @ defaults, grids, 200k PRNG seed `0xA11CE70D`)  
* Trampoline ABI: tunable injection, `ret 12`, register preservation in smoke  
* In-kernel public-symbol smoke (hang-on-fail)  
* QEMU ON/OFF → `running` + screendumps  
* Cuts A–K blob hashes unchanged  
* Determinism ×2  

### NOT PROVEN

* Live PS/2/USB/COM mouse packet → `set_mouse_data` → acceleration under QEMU (no injected HID traffic in this audit)  
* Host FASM binary vs Rust blob instruction-level equivalence (oracle is semantic)  
* Behavior under non-default tunables set via syscall 18.19 at runtime in QEMU  

### OUT OF SCOPE

* Migrating `set_mouse_data`, `sysfn_mouse_acceleration`, or cursor draw path  
* Absolute-motion mouse paths (bypass acceleration)  
* `memmove` / `UTF16to8` / Cut M  

---

## Remaining issues

none

---

## Stop

**Cut L COMPLETE — STOP.** Do not start Cut M.
