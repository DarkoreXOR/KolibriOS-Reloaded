# Cut H Implementation — `block_clip`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-h-plan.md`](cut-h-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `block_clip` |
| Source | [`kernel/video/blitter.inc`](../../kernel/video/blitter.inc) |
| Callers | `blit_clip` ×2; production via `blit_32` → `blit_clip` |
| Rust symbol | `rust_block_clip` |
| Pure helper | `kolibri_utils::block_clip` / `Rect` / `BlockClipResult` |
| Subsystem | Video / rectangle geometry |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `block_clip` | **Selected** — first video/geometry leaf; first CF-return ABI |
| `fat_time_to_bdfe` | Rejected — safer EAX but weaker answer to the CF question Cut G deferred |
| `fsTime2bdfe` | Rejected — calendar twin of Cut G |
| `strtoint_dec` | Rejected — weaker complexity / conf-only fanout |
| `blit_clip` | Rejected — depends on `block_clip`; follow-on |
| `antiAliasing` | Rejected — quirky `EBP` ABI |

---

## Why selected

Cut H’s research question: does Strategy A + C remain viable for a **CF-returning, in-place mutate** leaf in a **new subsystem** (video geometry)?

| Preference | Result |
|------------|--------|
| Outside proven families | Yes — geometry, not string/checksum/calendar |
| New ABI class | Yes — CF draw/reject (never proven in A–G) |
| Real kernel callers | `blit_clip` ×2 on blit path |
| Strategy A feasible | Zero tables; no `.rodata` |
| Clear ABI | ESI clip + EDI rect → CF; plain `ret` |
| Testability | Strong RECT oracle + grids/PRNG (CF + 16 bytes) |

---

## Original implementation

FASM leaf in `blitter.inc` (retained under `USE_RUST_BLOCK_CLIP=0`):

* `ESI` → clip `RECT`; `EDI` → mutable target `RECT`  
* Signed compares (`jge`/`jl`/`jle`) on X then Y  
* Clamp edges into clip; **CF=0** draw / **CF=1** reject  
* Preserves `EBX` (push/pop); leaves ESI/EDI pointers unchanged  
* On Y-fail after X clamp, **X fields may already be written**

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/geometry.rs`](../../rust_kernel/kolibri_utils/src/geometry.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_block_clip` |
| Build | [`rust_kernel/kolibri_utils/build-block-clip.ps1`](../../rust_kernel/kolibri_utils/build-block-clip.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_block_clip.bin` |

`#![no_std]` freestanding; signed `i32` compares; mutate via raw pointer stores; returns `0`/`1` for trampoline CF mapping.

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | X reject / X clamp / Y reject / Y clamp |
| Loops | none |
| Arithmetic | Signed compare + conditional stores |
| State | In-place 16-byte RECT mutation |
| vs Cut G | New family + **CF return** (not calendar/`mul`) |

---

## ABI

### FASM `block_clip` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `ESI` → clip RECT; `EDI` → mutable RECT |
| Output | RECT clipped in place; **CF=0** draw / **CF=1** reject |
| Clobbers | `EAX`, `ECX`, `EDX`, flags |
| Preserved | `EBX` (FASM push/pop), `ESI`/`EDI` pointers, `EBP` |

### Rust `rust_block_clip`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(clip: *const u8, rect: *mut u8)` |
| Return | `u32` in `EAX`: `0` draw / `1` reject |
| Epilogue | `ret 8` |

### Trampoline

```asm
block_clip:
        stdcall rust_block_clip, esi, edi
        test    eax, eax
        jnz     .rust_fail
        clc
        ret
.rust_fail:
        stc
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | `blit_clip` (src clip, dst clip) |
| Upstream | `blit_32` → `blit_clip` |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state | none |
| Static data / `.rodata` | none |
| External calls | none |
| Compiler helpers | none observed in blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_block_clip` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 8` (`C2 08 00`) |
| Blob size | **168** bytes |
| Blob SHA-256 | `C79E5D83C6323BF37173370A1A8FDD217D057F6081BCC01E76135E481622E5D6` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 8`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-block-clip.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-h-on.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-h-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-h-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-h-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4496,server,nowait
```

Rollback OFF: set `USE_RUST_BLOCK_CLIP = 0`, reassemble, CoW/replace, QEMU (port 4497 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **79/79** (was 65; +14 geometry) |
| Named edge cases (contain, clamp, reject X/Y, Y-fail after X, negatives, degenerate) | **PASS** |
| Structured 5⁴×5⁴ signed grid (~390k) | **PASS** |
| Deterministic PRNG (200 000, seed `0xC07B10C`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle | **PASS** |
| Compare draw/reject **and** all 16 RECT bytes | **PASS** |
| Partial-mutate-on-Y-fail vectors | **PASS** |

Host differential mirrors `blitter.inc` signed compare / clamp / CF semantics.

---

## ABI/register tests

In-kernel smoke `block_clip_rust_smoke_test` calls real `block_clip` with:

* contain → CF clear, rect unchanged  
* clamp all edges → `{10,10,90,90}`  
* reject X → CF set, rect unchanged  
* Y-fail after X clamp → CF set, left/right mutated  
* `ESI`/`EDI`/`EBP`/`EBX` preservation  

Hang-on-fail marker `0xDEAD0C48` / `'BCLP'` / `'FAIL'`.

| Gate | Result |
|------|--------|
| Trampoline CF via `jc`/`jnc` | **PASS** |
| Register preservation ESI/EDI/EBP/EBX | **PASS** |
| Public symbol smoke | **PASS** |
| Live `blit_32` GUI paint path | **NOT PROVEN** |

---

## Kernel smoke

Called from `high_code` after Cut G smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_BLOCK_CLIP=1`) | **PASS** — `query-status=running`; screendump `tmp_images/cut-h-on-desktop.ppm` |
| Rust OFF (`USE_RUST_BLOCK_CLIP=0`) | **PASS** — `query-status=running`; screendump `tmp_images/cut-h-off-desktop.ppm` |

---

## Regression

| Gate | Result |
|------|--------|
| Cut A suite (CRC / UTF-16 / CP866 / UTF-8 in `kolibri_utils`) | **PASS** (included in 79/79) |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut E blob hash `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut F blob hash `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Cut G blob hash `rust_fs_calculate_time.bin` | **PASS** — `B7B1AB42…1777` unchanged |
| Full extract of prior blobs after Cut H build | **PASS** |

---

## Determinism

Two deep-clean freestanding rebuilds (full `i686-kolibri-none` target wipe):

| Build | Size | SHA-256 |
|------:|-----:|---------|
| #1 | 168 | `C79E5D83C6323BF37173370A1A8FDD217D057F6081BCC01E76135E481622E5D6` |
| #2 | 168 | `C79E5D83C6323BF37173370A1A8FDD217D057F6081BCC01E76135E481622E5D6` |

**PASS** — identical size and bytes.

---

## Kernel image sizes

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut H ON (default) | **225896** |
| Cut H OFF (FASM body; blob still embedded) | **225960** |
| Cut G baseline (pre–Cut H) | 225288 |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut G baseline ≈ +608 bytes (168-byte blob + trampoline/smoke/data + alignment).

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Rollback

```asm
USE_RUST_BLOCK_CLIP = 0   ; original FASM body
USE_RUST_BLOCK_CLIP = 1   ; Rust trampoline (default)
```

Independent of Cut A–G switches.  
Original FASM body remains in the `else` branch of `blitter.inc`.

Wire-up:

* Trampoline / switch: `kernel/video/blitter.inc`  
* Embed + smoke: `kernel/rust/block_clip.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed RECT vectors; live `blit_32` paint under GUI I/O is not instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Proof-of-life remains diagnostic only — not coupled to Cut H.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `block_clip` over `fat_time_to_bdfe` / `fsTime2bdfe` / `strtoint_dec` / `blit_clip` / `antiAliasing`  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 8`, no external deps  
* Rust unit + differential grids / PRNG / named quirks vs separately coded FASM-oracle (CF + RECT bytes)  
* In-kernel trampoline smoke including CF checks and ESI/EDI/EBP/EBX preservation  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two full-target-wipe rebuilds  
* Independent rollback switch builds both ways  
* Cut B / C / D / E / F / G blob hashes unchanged  
* Full `kolibri_utils` suite green (79/79)  
* Reference floppy hash unchanged  

### NOT PROVEN

* Byte-identical behavior of every live `blit_32` / window paint under GUI load  
* Host-assembled FASM binary vs Rust binary differential  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* Migrating `blit_clip` / `fsTime2bdfe` / `fat_time_to_bdfe` / `strtoint_dec`  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut I  

---

## Verification matrix

```text
Candidate audit                         PASS
Candidate comparison                    PASS
Cut H plan                              PASS
Rust implementation                     PASS
Compiler dependencies audited           PASS
Link strategy validation                PASS
Artifact validation                     PASS
Rust tests                              PASS (79/79)
Differential/oracle                     PASS (grids + PRNG + named)
ABI/trampoline                          PASS (in-kernel smoke + CF)
Register preservation                   PASS (ESI/EDI/EBP/EBX)
Real caller smoke                       PASS (symbol); live blit NOT PROVEN
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
Cut G regression                        PASS (fs_calculate_time hash)
Documentation                           PASS
```

**STOP** — do not start Cut I.
