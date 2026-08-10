# Cut G Implementation — `fsCalculateTime`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-g-plan.md`](cut-g-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fsCalculateTime` |
| Source | [`kernel/fs/fs_common.inc`](../../kernel/fs/fs_common.inc) |
| Callers | `ntfsCalculateTime` / `ntfs_SetFileInfo`; `ext_SetFileInfo` ×2; fall-through from `fsGetTime` |
| Rust symbol | `rust_fs_calculate_time` |
| Pure helper | `kolibri_utils::fs_calculate_time` / `BdfeTime` |
| Subsystem | Filesystem / calendar |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `fsCalculateTime` | **Selected** — first utility outside string/Unicode/casefold/checksum families |
| `block_clip` | Rejected — CF return ABI risk; weaker calendar-math narrative |
| `fsTime2bdfe` | Rejected — inverse write-back + `EDI+=8`; follow-on pair |
| `strtoint_dec` | Rejected — weaker complexity / conf-only fanout |

---

## Why selected

Cut G’s research question: does the freestanding blob architecture remain viable for a **new family** (FS calendar) with leap/month/`mul` control flow?

| Preference | Result |
|------------|--------|
| Outside proven families | Yes — calendar, not checksum/string/Unicode |
| Real kernel callers | NTFS/ext set-info; `fsGetTime` fall-through |
| Strategy A feasible | Month tables stack-materialized (CP866 lesson) |
| Clear ABI | ESI → block; EAX seconds; plain `ret` |
| Testability | Strong BDFE oracle + grids/PRNG |

---

## Original implementation

FASM leaf in `fs_common.inc` (retained under `USE_RUST_FS_CALCULATE_TIME=0`):

* `ESI` → BDFE `{sec,min,hour,pad,day,month,year}`  
* `years = max(0, year−2001)`; leap table when `(years+1)&3==0`  
* Month-day accumulation loop over `months`/`months2`  
* `seconds = ((days*24+hour)*60+min)*60+sec`  
* Clobbers `EBX`/`ECX`/`EDX`; preserves `ESI`/`EDI`/`EBP`  
* `months`/`months2` iglobal retained for `fsTime2bdfe`

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/time.rs`](../../rust_kernel/kolibri_utils/src/time.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_fs_calculate_time` |
| Build | [`rust_kernel/kolibri_utils/build-fs-calculate-time.ps1`](../../rust_kernel/kolibri_utils/build-fs-calculate-time.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_fs_calculate_time.bin` |

`#![no_std]` freestanding; month tables via `write_volatile` immediates (no `.rodata`); production domain year &lt; 3025 (BH pollution free).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | Year clamp; leap table select; month loop exit |
| Loops | Month-day accumulation |
| Arithmetic | Multi-stage `mul` chain (days→hours→minutes→seconds) |
| State | Local 24-byte month tables on stack |
| vs Cut F | New family + calendar control flow (not checksum fold) |

---

## ABI

### FASM `fsCalculateTime` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `ESI` → 8-byte BDFE block |
| Output | `EAX` = seconds since 2001-01-01 |
| Clobbers | `EBX`, `ECX`, `EDX`, flags (FASM); trampoline may also use them |
| Preserved | `ESI` (**required** — callers `add esi, 8`), `EDI`, `EBP` |

### Rust `rust_fs_calculate_time`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(block: *const u8)` |
| Return | `u32` in `EAX` |
| Epilogue | `ret 4` |

### Trampoline

```asm
fsCalculateTime:
        stdcall rust_fs_calculate_time, esi
        ret
```

`fsGetTime` fall-through into the label is preserved.

---

## Call graph

| Path | Status |
|------|--------|
| Smoke via public `fsCalculateTime` | Exercised |
| `ext_SetFileInfo` / `ntfs_SetFileInfo` under live disk I/O | **NOT PROVEN** |
| `fsGetTime` CMOS → fall-through | **NOT PROVEN** (smoke uses static blocks) |

---

## Link strategy

**Strategy A + C** (reloc-free raw blob + minimal FASM trampoline/switch).

| Rejected | Why |
|----------|-----|
| B `rust-lld` | Not needed — 0 relocs |
| Reject candidate | Not needed — leaf is suitable |

---

## Compiler dependencies

| Item | Result |
|------|--------|
| External calls / helpers | **none** (no `E8` near-calls in blob) |
| `.rodata` refs | **none** (volatile stack tables) |
| Relocations | **0** |
| Panic / bounds-check | **none** in extracted section (volatile indexed reads) |
| Shared epilogue | Internal `jmp` to in-section `ret 4` (extractor note; accepted) |

---

## Section

`.text.rust_fs_calculate_time`

---

## Relocations

**0** (extractor hard-fail if any)

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_fs_calculate_time` |
| `SHT_PROGBITS` | yes |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 4` (`C2 04 00`) present |
| Blob size | **441** bytes |
| Blob SHA-256 | `B7B1AB421964432AA99FCA11101F0E4F54F4FBAF1BB7889C570F5250D2641777` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 4`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-fs-calculate-time.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-g-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-g-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-g-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-g-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4494,server,nowait
```

Rollback OFF: set `USE_RUST_FS_CALCULATE_TIME = 0`, reassemble, CoW/replace, QEMU (port 4495 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **65/65** (was 53; +12 calendar) |
| Named edge cases (epoch, leap, clamp, EOD) | **PASS** |
| Independent calendar spot checks | **PASS** |
| Structured year×month×day×time grid | **PASS** |
| Deterministic PRNG (200 000, seed `0xC07A71E`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle | **PASS** |
| Structured grids 2001–2032 | **PASS** |
| PRNG corpus year∈[2001,3024], month 1–12 | **PASS** |

Host differential mirrors `fs_common.inc` clamp / leap select / month loop / `mul` chain. Year≥3025 BH-quirk domain not claimed.

---

## ABI/register tests

In-kernel smoke `fs_calculate_time_rust_smoke_test` calls real `fsCalculateTime` with:

* epoch → 0  
* 2004-02-29 → 99705600  
* 2010-07-04 12:00 → 299937600  
* 2001-01-01 23:59:59 → 86399  
* year 1999 clamp → 0  
* `ESI`/`EDI`/`EBP` preservation  

Hang-on-fail (`EAX=0xDEAD0C47`) if mismatch. Reaching desktop implies trampoline path matched.

---

## Real caller

| Path | Status |
|------|--------|
| Explicit smoke via public `fsCalculateTime` symbol | **PROVEN** |
| Live NTFS/ext `SetFileInfo` under disk I/O | **NOT PROVEN** |
| Live `fsGetTime` CMOS fall-through | **NOT PROVEN** |

---

## Kernel smoke

Called from `high_code` after Cut F smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_FS_CALCULATE_TIME=1`) | **PASS** — `query-status=running`; screendump `dev_build/cut-g-on-desktop.ppm` |
| Rust OFF (`USE_RUST_FS_CALCULATE_TIME=0`) | **PASS** — `query-status=running`; screendump `dev_build/cut-g-off-desktop.ppm` |

---

## Regression

| Gate | Result |
|------|--------|
| Cut A suite (CRC / UTF-16 / CP866 / UTF-8 in `kolibri_utils`) | **PASS** (included in 65/65) |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut E blob hash `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut F blob hash `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Full extract of prior blobs after Cut G build | **PASS** |

---

## Determinism

Two deep-clean freestanding rebuilds (full `i686-kolibri-none` target wipe):

| Build | Size | SHA-256 |
|------:|-----:|---------|
| #1 | 441 | `B7B1AB421964432AA99FCA11101F0E4F54F4FBAF1BB7889C570F5250D2641777` |
| #2 | 441 | `B7B1AB421964432AA99FCA11101F0E4F54F4FBAF1BB7889C570F5250D2641777` |

**PASS** — identical size and bytes.

---

## Kernel image sizes

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut G ON (default) | **225288** |
| Cut G OFF (FASM body; blob still embedded) | **225368** |
| Cut F baseline (pre–Cut G) | 224696 |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut F baseline ≈ +592 bytes (441-byte blob + trampoline/smoke/data + alignment).

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Rollback

```asm
USE_RUST_FS_CALCULATE_TIME = 0   ; original FASM body
USE_RUST_FS_CALCULATE_TIME = 1   ; Rust trampoline (default)
```

Independent of Cut A–F switches.  
Original FASM body remains in the `else` branch of `fs_common.inc`.  
`months`/`months2` iglobal kept for `fsTime2bdfe`.

Wire-up:

* Trampoline / switch: `kernel/fs/fs_common.inc`  
* Embed + smoke: `kernel/rust/fs_calculate_time.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed BDFE vectors; live FS timestamp write paths are not instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Year ≥ 3025 may diverge from pure math due to FASM `mov bl` / residual `BH` after `shr ebx,2` — documented domain.  
* Proof-of-life remains diagnostic only — not coupled to Cut G.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `fsCalculateTime` over `block_clip` / `fsTime2bdfe` / `strtoint_dec`  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 4`, no external `E8`  
* Rust unit + differential grids / PRNG / named quirks vs separately coded FASM-oracle  
* In-kernel trampoline smoke including ESI/EDI/EBP check (hang-on-fail)  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two full-target-wipe rebuilds  
* Independent rollback switch builds both ways  
* Cut B / C / D / E / F blob hashes unchanged  
* Full `kolibri_utils` suite green (65/65)  
* Reference floppy hash unchanged  

### NOT PROVEN

* Byte-identical behavior of every NTFS/ext set-info timestamp under live disk I/O  
* Live `fsGetTime` CMOS → fall-through path  
* Host-assembled FASM binary vs Rust binary differential  
* Year ≥ 3025 BH-quirk equivalence  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* Migrating `fsTime2bdfe` / `block_clip` / `strtoint_dec`  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut H  

---

## Verification matrix

```text
Candidate audit                         PASS
Candidate comparison                    PASS
Cut G plan                              PASS
Rust implementation                     PASS
Compiler dependencies audited           PASS
Link strategy validation                PASS
Artifact validation                     PASS
Rust tests                              PASS (65/65)
Differential/oracle                     PASS (grids + PRNG + named)
ABI/trampoline                          PASS (in-kernel smoke)
Register preservation                   PASS (ESI/EDI/EBP)
Real caller smoke                       PASS (symbol); live FS NOT PROVEN
Relocation validation                   PASS
Kernel smoke                            PASS
QEMU Rust ON                            PASS
QEMU Rust OFF                           PASS
Deterministic rebuild ×2                PASS
Rollback switch                         PASS
Cut A regression                        PASS (suite + blob extract)
Cut B regression                        PASS (cp866_upper hash)
Cut C regression                        PASS (utf16_upper hash)
Cut D regression                        PASS (strncmp hash)
Cut E regression                        PASS (checksum_1 hash)
Cut F regression                        PASS (checksum_2 hash)
Documentation                           PASS
```

**STOP** — do not start Cut H.
