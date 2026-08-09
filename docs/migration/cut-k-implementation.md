# Cut K Implementation — `fat_next_short_name`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-k-plan.md`](cut-k-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fat_next_short_name` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 2 (`fat_gen_short_name` lossy path; create `.short_name_found`) |
| Rust symbol | `rust_fat_next_short_name` |
| Pure helper | `kolibri_utils::fat_next_short_name` / `FatNextShortNameResult` |
| Subsystem | Filesystem / FAT 8.3 short-name collision generation |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `fat_next_short_name` | **Selected** — FAT DF+CF leaf; in-place 8.3 collision mutate |
| `xfs._.extent_unpack` | Rejected — new FS but EBP partition ABI; thinner flag narrative |
| `memmove` | Rejected — memcpy-class interesting but thin + ~24 hot callers |
| `xfs_hashname` | Rejected — too small |
| `blit_clip` | Rejected — video geometry neighbor of Cut H |
| NTFS helpers | Rejected — Cuts I–J already covered NTFS |
| FAT bitfield / `strtoint_dec` / `strrchr` | Rejected — too small / conf / string-family |

---

## Why selected

Cut K’s research question: does Strategy A + C remain viable for a **DF-sensitive FAT 8.3 short-name collision leaf** (`std`/`cld` reverse scan, in-place basename digit arithmetic, CF=exhausted) with zero tables, explicit/volatile byte stores, and a byte-exact differential oracle — without compiler `memset`/`memcpy`/GOT?

| Preference | Result |
|------------|--------|
| Outside NTFS / video geometry | Yes — FAT naming |
| New ABI class | Yes — first leaf that owned temporary `std`/`cld` |
| Strategy A feasible | Zero tables; volatile byte stores |
| Clear ABI | `EDI` → CF; `pushad` preserve; `cld` on exit |
| Testability | Strong oracle (CF + 11-byte buffer) |
| Limited blast radius | 2 callers; independent switch |

---

## Original implementation

FASM leaf in `fat.inc` (retained under `USE_RUST_FAT_NEXT_SHORT_NAME=0`):

* `EDI` → 11-byte 8.3 name; mutates basename `0..7` only  
* Reverse search for `~` via `std` + `repnz scasb` from index 7; always `cld` after  
* No tilde → insert `~1` at end of content (or index 6 if no trailing double-space)  
* Tilde present → walk digits/spaces from end: increment, expand into spaces, or shrink prefix  
* **CF=0** OK / **CF=1** exhausted (`~` at index 1 with no room)  
* `pushad`/`popad`

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/fat_name.rs`](../../rust_kernel/kolibri_utils/src/fat_name.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_fat_next_short_name` |
| Build | [`rust_kernel/kolibri_utils/build-fat-next-short-name.ps1`](../../rust_kernel/kolibri_utils/build-fat-next-short-name.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_fat_next_short_name.bin` |

`#![no_std]` freestanding; `write_volatile` + unrolled basename fills (Cut I lesson — first extract hit `memset`+GOT and was rejected); returns `0`/`1` for trampoline CF mapping (Cut H polarity).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | No-tilde insert; digit `.found`; space expand; `.noplace` shrink; `.err` |
| Loops | Bounded to 8-byte basename |
| Memory | In-place basename mutate; extension untouched |
| vs Cut J | New subsystem (FAT); DF discipline; no USA/stride |

---

## ABI

### FASM `fat_next_short_name` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EDI` → 11-byte 8.3 name |
| Output | basename mutated; **CF=0** OK / **CF=1** exhausted |
| Preserved | all GPRs via `pushad`/`popad` |
| DF | clear on return |

### Rust `rust_fat_next_short_name`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(name: *mut u8)` |
| Return | `u32` in `EAX`: `0` OK / `1` fail |
| Epilogue | `ret 4` |

### Trampoline

```asm
fat_next_short_name:
        pushad
        stdcall rust_fat_next_short_name, edi
        test    eax, eax
        popad
        cld
        jz      .rust_fat_ok
        stc
        ret
.rust_fat_ok:
        clc
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | `fat_gen_short_name`; FAT create `.short_name_found` |
| Upstream | FAT short-name generation / collision on create |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state | none |
| Static data / `.rodata` | none |
| External calls | none |
| Compiler helpers | none observed in final blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Compiler artifact audit

Mandatory after Cut I (`memset`+GOT rejection).

**First extract (rejected):** `.rel.text.rust_fat_next_short_name` contained GOT + two `memset` relocs (fill loops). Implementation rewritten with `write_volatile` + unrolled 8-byte fills.

| Check | Result (final) |
|-------|----------------|
| Section | `.text.rust_fat_next_short_name` |
| Relocations targeting section | **0** |
| Symbol at section offset 0 | **yes** |
| Trailing `ret 4` (`C2 04 00`) | **yes** |
| `CALL rel32` (`E8`) to helpers | **none** |
| `memset`/`memcpy`/`memmove`/`memcmp` / GOT | **none** |
| External symbols | **none** |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_fat_next_short_name` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 4` trailing |
| Blob size | **371** bytes |
| Blob SHA-256 | `E9CFFE656BD72484ED33D6235AB519715E26857443A6E1919DBD14938637A52F` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 4`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-fat-next-short-name.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-k-on.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-k-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-k-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-k-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4510,server,nowait
```

Rollback OFF: set `USE_RUST_FAT_NEXT_SHORT_NAME = 0`, reassemble, CoW/replace, QEMU (port 4511 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **117/117** (was 104; +13 fat_name) |
| Named: insert `~1`; short content; digit + zerofill; expand; shrink; err; ext untouched; chain | **PASS** |
| Named vs separately coded FASM oracle | **PASS** |
| Deterministic PRNG (200 000, seed `0xC07B10EB`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle in `fat_name.rs` tests | **PASS** |
| Compare OK/fail + entire 11-byte buffer | **PASS** |
| Extension bytes never mutated | **PASS** |
| `.found` zero-fills trailing basename bytes (spaces → `'0'`) | **PASS** |

---

## ABI / trampoline / registers

In-kernel smoke `fat_next_short_name_rust_smoke_test` calls real `fat_next_short_name` with:

| Check | Result |
|-------|--------|
| Full basename → `FILENA~1` + CF clear | **PASS** |
| `FILE~1__` → `FILE~200` (zerofill) | **PASS** |
| `FILE~9__` → `FILE~10_` expand | **PASS** |
| `X~999999` → CF set, unchanged | **PASS** |
| EAX/EBX/ECX/EDX/ESI/EDI/EBP preserved on success | **PASS** |
| DF clear after return (`pushfd` bit 10) | **PASS** |
| Public symbol smoke | **PASS** |

---

## Kernel smoke

Called from `high_code` after Cut J smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_FAT_NEXT_SHORT_NAME=1`) | **PASS** — `running`; screendump `tmp_images/cut-k-on.ppm` (2359312 bytes) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `tmp_images/cut-k-off.ppm` (2359312 bytes) |

---

## Regression lock (Cuts A–J)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (117/117) |
| Cut J blob hash `rust_ntfs_restore_usa.bin` | **PASS** — `851FC92B…C417` unchanged |
| Cut I blob hash `rust_ntfs_decode_mcb_entry.bin` | **PASS** — `DA888977…39C9` unchanged |
| Cut H blob hash `rust_block_clip.bin` | **PASS** — `C79E5D83…E5D6` unchanged |
| Cut G blob hash `rust_fs_calculate_time.bin` | **PASS** — `B7B1AB42…1777` unchanged |
| Cut F blob hash `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Cut E blob hash `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut A CRC blob `rust_crc_32.bin` | **PASS** — `E8C42ED0…A6F4` unchanged |
| Full extract of prior blobs after Cut K build | **PASS** |

---

## Determinism

Two independent freestanding rebuilds (each after wiping `i686-kolibri-none`):

| Build | Size | SHA-256 |
|-------|------|---------|
| #1 | 371 | `E9CFFE656BD72484ED33D6235AB519715E26857443A6E1919DBD14938637A52F` |
| #2 | 371 | `E9CFFE656BD72484ED33D6235AB519715E26857443A6E1919DBD14938637A52F` |

---

## Kernel sizes

| Config | `kernel.mnt` bytes |
|--------|-------------------|
| Cut K ON (Rust path) | **228824** |
| Cut K OFF (FASM body; blob still embedded) | **228936** |

OFF is larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut J baseline (228024) ≈ +800 bytes (371-byte blob + trampoline/smoke/data + alignment).

---

## Rollback

| Switch | `USE_RUST_FAT_NEXT_SHORT_NAME` (`1` default / `0` original FASM) |
|--------|------------------------------------------------------------------|
| Verified | OFF QEMU boot **PASS**; switch restored to `1` after audit |

---

## Files touched

* Algorithm: `rust_kernel/kolibri_utils/src/fat_name.rs`  
* FFI / lib exports: `ffi.rs`, `lib.rs`  
* Build: `build-fat-next-short-name.ps1`  
* Embed + smoke: `kernel/rust/fat_next_short_name.inc`  
* Trampoline + original retain: `kernel/fs/fat.inc`  
* Include / call site: `kernel/kernel32.inc`, `kernel/kernel.asm`  
* Docs: `cut-k-plan.md`, `cut-k-implementation.md`, `migration-plan.md`

---

## Proven / not proven / out of scope

### PROVEN

* Candidate audit + plan selecting `fat_next_short_name` over XFS / memmove / blit / NTFS / tiny leaves  
* Freestanding reloc-free blob (371 B, 0 relocs) after rejecting first `memset`+GOT extract  
* Rust unit + named + PRNG vs separately coded FASM-oracle (CF + full 11 bytes)  
* Trampoline CF + `cld` + register preservation via in-kernel smoke  
* Kernel smoke hang-on-fail path  
* QEMU ON/OFF running + screendump  
* Cuts A–J blob hashes unchanged  
* Determinism ×2  

### NOT PROVEN

* Live FAT create/collision on a mounted volume in QEMU  
* Host-assembled FASM binary vs Rust byte differential  
* Pathological FASM OOB (`~` at index 0 with no expand/increment path)  
* All-space basename left-walk OOB  

### OUT OF SCOPE

* Cut L  
* Migrating `fat_gen_short_name` / `memmove` / `xfs._.extent_unpack` / `blit_clip`  
* Allocator / scheduler / IRQ / paging  

---

## Completion gates

| Gate | Result |
|------|--------|
| Candidate audit | PASS |
| New subsystem (FAT, not NTFS/video) | PASS |
| Dependency audit | PASS |
| Compiler artifact audit | PASS (after rewrite) |
| Rust implementation | PASS |
| Reloc-free extraction | PASS |
| Relocations | **0** |
| External symbols | NONE |
| Compiler dependencies | NONE |
| Differential testing | PASS |
| ABI/trampoline | PASS |
| Flag semantics (CF + DF) | PASS |
| Register preservation | PASS |
| Memory mutation | PASS |
| Real caller smoke | PASS (public symbol + smoke vectors) |
| Kernel smoke | PASS |
| QEMU ON | PASS |
| QEMU OFF | PASS |
| Cuts A–J regression | PASS |
| Determinism ×2 | PASS |
| Documentation | COMPLETE |
| Rollback switch | VERIFIED |

---

## Remaining issues

none

**Cut K COMPLETE — STOP. Do not start Cut L.**
