# Cut F Implementation — `checksum_2`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-f-plan.md`](cut-f-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `checksum_2` |
| Source | [`kernel/network/stack.inc`](../../kernel/network/stack.inc) |
| Callers | `icmp.inc` (5×); via `udp_checksum` / `tcp_checksum` macros |
| Rust symbol | `rust_checksum_2` |
| Pure helper | `kolibri_utils::checksum_2` |
| Sibling (already migrated) | `checksum_1` (Cut E) — **not modified** |

---

## Why selected

Cut F validates **checksum-family independence**: can the Cut E pipeline land a related sibling without coupling the two migrations?

| Preference | Result |
|------------|--------|
| Family repeatability | Direct twin of `checksum_1` in `stack.inc` |
| Independent ABI/oracle/blob/switch | Yes |
| Strategy A feasible | Pure register arithmetic — no buffers |
| Not a complexity cut | Intentionally small; Cut E already proved ADC/loop complexity |

Rejected alternates (see plan): `fsCalculateTime` (wrong research question / table reloc), `strchr` (string.inc again).

---

## Relationship to checksum_1

| Item | Detail |
|------|--------|
| Shared | Same include; production callers always finalize with `checksum_2` after a partial sum |
| Different | No memory walk; fold / `not` / zero→`0xFFFE` quirk / `xchg`; `DX` out |
| Isolation | No shared Rust helpers with Cut E; separate section, blob, `USE_RUST_CHECKSUM_2` |

---

## Original implementation

FASM leaf in `stack.inc` (retained under `USE_RUST_CHECKSUM_2=0`):

* `EDX` semi-checksum → `DX` INET-order final checksum  
* Two-stage 16-bit fold; `test`/`not`/conditional `dec`; `xchg dl,dh`  
* Plain `ret`; clobbers `ECX`/flags; leaves `EAX`/`EBX`/`ESI`/`EDI`/`EBP` alone  
* `DEBUGF` compiles out when `DEBUG_NETWORK_VERBOSE=0`  

No callees, no globals, no memory access.

**Zero quirk (precise):** pre-`not` DX == 0 → after `not`/`dec` → `0xFFFE` → after swap → `0xFEFF`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/checksum.rs`](../../rust_kernel/kolibri_utils/src/checksum.rs) `checksum_2` |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_checksum_2` |
| Build | [`rust_kernel/kolibri_utils/build-checksum2.ps1`](../../rust_kernel/kolibri_utils/build-checksum2.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_checksum_2.bin` |

`#![no_std]` freestanding; `#[inline(always)]` into FFI; scalar `u32` only (no slices). Independent of `checksum_1` ADC helpers.

---

## ABI

### FASM `checksum_2` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EDX` semi-checksum |
| Output | `DX` final checksum (INET order); high half of `EDX` clear |
| Clobbers | `ECX`, flags; trampoline also clobbers `EAX` |
| Preserved | `EBX`, `ESI`, `EDI`, `EBP` (tested); FASM body also left `EAX` alone |

### Rust `rust_checksum_2`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(sum: u32)` |
| Return | `u32` in `EAX` (low 16 = final DX) |
| Epilogue | `ret 4` |

### Trampoline

```asm
checksum_2:
        stdcall rust_checksum_2, edx
        mov     edx, eax
        ret
```

EAX/ECX liveness across callers: **audited** — ICMP/UDP/TCP paths use `DX` next; none require EAX/ECX preserved across `checksum_2`.

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
| External calls / helpers | **none** in the extracted section |
| `.rodata` refs | **none** |
| Relocations | **0** |
| Panic / bounds-check | **none** (no indexing) |

Compiler optimized the zero path to an early `0xFEFF` constant load; still reloc-free and semantically equivalent on the valid fold domain.

---

## Section

`.text.rust_checksum_2`

---

## Blob/object size

**43** bytes

---

## Relocations

**0** (extractor hard-fail if any)

---

## Extraction/link validation

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_checksum_2` |
| `SHT_PROGBITS` | yes |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 4` (`C2 04 00`) present |
| Blob SHA-256 | `2086790423DB13AF9BC136CDB33F9038ED5CBD7918A09E47E80C1DEB90F20C0C` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 4`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-checksum2.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-f-on.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-f-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-f-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-f-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4492,server,nowait
```

Rollback OFF: set `USE_RUST_CHECKSUM_2 = 0`, reassemble, CoW/replace, QEMU (port 4493 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **53/53** (was 44; +9 checksum_2) |
| Named edge cases (zero quirk, folds, swaps) | **PASS** |
| Exhaustive low-16 / high-16 | **PASS** |
| Structured grids (all low×11 highs; all high×11 lows) | **PASS** |
| Carry-producing folds | **PASS** |
| `checksum_1`→`checksum_2` chain vector | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Named corpus vs FASM-faithful host oracle | **PASS** |
| Structured exhaustive grids | **PASS** (~1.4M+ comparisons) |
| Deterministic PRNG corpus (200 000 `u32`, seed `0xF00D_C0DE`) | **PASS** |

Host differential mirrors `stack.inc` fold/`test`/`not`/`dec`/`xchg`. Full 2³² domain not enumerated in CI.

---

## ABI/register preservation

In-kernel smoke `checksum2_rust_smoke_test` calls real `checksum_2` trampoline with:

* zero → `0xFEFF`  
* `0xFFFF` → 0  
* `0x1A35` → `0xCAE5`  
* `0xFFFFFFFF` → 0  
* `0x12345678` → `0x5397`  
* `checksum_1`→`checksum_2` chain on static ICMP bytes  
* `EBX`/`ESI`/`EDI`/`EBP` preservation on pure `checksum_2`  

Hang-on-fail (`EAX=0xDEAD0C52`) if mismatch. Reaching desktop implies trampoline path matched.

---

## Real caller

| Path | Status |
|------|--------|
| Explicit smoke via public `checksum_2` symbol | **PROVEN** |
| Production-style `checksum_1`→`checksum_2` chain on static buffer | **PROVEN** (smoke) |
| Live ICMP/TCP/UDP packet checksum under real network traffic | **NOT PROVEN** |

Boot does not exercise the network stack; smoke is the deterministic Cut F execution proof.

---

## Network validation

| Item | Status |
|------|--------|
| New drivers / packet generators | **OUT OF SCOPE** (not introduced) |
| Live traffic | **NOT PROVEN** |

---

## Kernel smoke

Called from `high_code` after Cut E smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_CHECKSUM_2=1`) | **PASS** — `query-status=running`; screendump `tmp_images/cut-f-on-desktop.ppm` |
| Rust OFF (`USE_RUST_CHECKSUM_2=0`) | **PASS** — `query-status=running`; screendump `tmp_images/cut-f-off-desktop.ppm` |

---

## Regression

| Gate | Result |
|------|--------|
| Cut A suite (CRC / UTF-16 / CP866 / UTF-8 in `kolibri_utils`) | **PASS** (included in 53/53) |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut E blob hash `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Full extract of prior blobs after Cut F build | **PASS** |

---

## Determinism

Two deep-clean freestanding rebuilds (full `i686-kolibri-none` target wipe):

| Build | Size | SHA-256 |
|------:|-----:|---------|
| #1 | 43 | `2086790423DB13AF9BC136CDB33F9038ED5CBD7918A09E47E80C1DEB90F20C0C` |
| #2 | 43 | `2086790423DB13AF9BC136CDB33F9038ED5CBD7918A09E47E80C1DEB90F20C0C` |

**PASS** — identical size and bytes.

---

## Image sizes

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut F ON (default) | **224696** |
| Cut F OFF (FASM body; blob still embedded) | **224728** |
| Cut E baseline (pre–Cut F) | 224408 |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut E baseline ≈ +288 bytes (43-byte blob + trampoline/smoke/data + alignment).

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Rollback

```asm
USE_RUST_CHECKSUM_2 = 0   ; original FASM body
USE_RUST_CHECKSUM_2 = 1   ; Rust trampoline (default)
```

Independent of `USE_RUST_CHECKSUM_1` and Cut A–D switches.  
Original FASM body remains in the `else` branch of `stack.inc`.

Wire-up:

* Trampoline / switch: `kernel/network/stack.inc`  
* Embed + smoke: `kernel/rust/checksum2.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed vectors; live network packet paths are not instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Under verbose network debug (`DEBUG_NETWORK_VERBOSE` enabled), FASM prints a checksum debug line; Rust path does not — delta only in debug builds.  
* Proof-of-life remains diagnostic only — not coupled to Cut F.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `checksum_2` over FS calendar / string leaves for family independence  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 4`  
* Rust unit + differential grids / PRNG / named quirks vs FASM-oracle  
* In-kernel trampoline smoke including EBX/ESI/EDI/EBP check + `checksum_1`→`checksum_2` chain (hang-on-fail)  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two full-target-wipe rebuilds  
* Independent rollback switch builds both ways  
* Cut B / C / D / E blob hashes unchanged  
* Full `kolibri_utils` suite green (53/53)  
* Reference floppy hash unchanged  

### NOT PROVEN

* Byte-identical behavior of every ICMP/TCP/UDP checksum under live network traffic  
* Host-assembled FASM binary vs Rust binary differential  
* Exhaustive coverage of all 2³² input values in CI  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* Refactoring `checksum_1`/`checksum_2` into a shared Rust helper  
* `fsCalculateTime` / other deferred candidates  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut G  

---

## Verification matrix

```text
Candidate audit                         PASS
Cut F plan                              PASS
Rust implementation                     PASS
Compiler dependencies audited           PASS
Link strategy validation                PASS
Rust tests                              PASS (53/53)
Differential/oracle                     PASS (grids + PRNG + named)
ABI/trampoline                          PASS (in-kernel smoke)
Register preservation                   PASS (EBX/ESI/EDI/EBP)
Real caller smoke                       PASS (symbol + chain); live net NOT PROVEN
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
Documentation                           PASS
```

**STOP** — do not start Cut G.
