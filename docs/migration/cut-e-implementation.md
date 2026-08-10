# Cut E Implementation — `checksum_1`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-e-plan.md`](cut-e-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `checksum_1` |
| Source | [`kernel/network/stack.inc`](../../kernel/network/stack.inc) |
| Callers | `icmp.inc` (5×); via `udp_checksum` / `tcp_checksum` macros |
| Rust symbol | `rust_checksum_1` |
| Pure helper | `kolibri_utils::checksum_1` |
| Twin (not migrated) | `checksum_2` remains FASM |

---

## Why selected

Cuts B/C were `parse_fn` casefold leaves; Cut D was `string.inc` `strncmp`. Cut E deliberately steps into the **network** subsystem with a leaf that has:

* multi-stage `shr`/`pushf` length decode  
* an 8-byte ADC stride loop  
* 4/2/1 remnant paths  
* carry-chain internal state  

while remaining pure (no HW/IRQ/sched/allocator/globals/tables).

Rejected alternates (see plan): `fsCalculateTime` (table reloc), `strtoint_dec` (weaker complexity step-up), `checksum_2` alone (too small), further `string.inc` / `parse_fn` leaves.

---

## Original implementation

FASM leaf in `stack.inc` (retained under `USE_RUST_CHECKSUM_1=0`):

* `EDX` seed, `ESI` data, `ECX` length → `EDX` partial sum  
* Network-order word accumulation via `add dl,[esi+1]` / `adc dh,[esi+0]`  
* Remnant CF bits saved with `pushf`/`popf`  
* Plain `ret`; clobbers `ESI`/`ECX`/flags; leaves `EBX`/`EDI`/`EBP` alone  

No callees, no globals, no memory writes.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/checksum.rs`](../../rust_kernel/kolibri_utils/src/checksum.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_checksum_1` |
| Build | [`rust_kernel/kolibri_utils/build-checksum1.ps1`](../../rust_kernel/kolibri_utils/build-checksum1.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_checksum_1.bin` |

`#![no_std]` freestanding; `#[inline(always)]` into FFI; **raw `*const u8` walks** (no slice indexing) so the dedicated section stays reloc-free.

---

## Complexity increase

| Metric | Cuts B/C/D | Cut E |
|--------|------------|-------|
| Control flow | linear / few branches | length decode + loop + 3 remnant paths |
| Internal state | none / index | CF carry across DL/DH/EDX |
| Blob size | 71–84 B | **652 B** |
| Subsystem | parse_fn / string | **network** |

---

## ABI

### FASM `checksum_1` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Inputs | `EDX` seed, `ESI` data, `ECX` length |
| Output | `EDX` partial sum |
| Clobbers | `ESI`, `ECX`, flags; trampoline also clobbers `EAX` |
| Preserved | `EBX`, `EDI`, `EBP` (tested) |

### Rust `rust_checksum_1`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(seed: u32, data: *const u8, length: u32)` |
| Return | `u32` in `EAX` |
| Epilogue | `ret 12` |

### Trampoline

```asm
checksum_1:
        stdcall rust_checksum_1, edx, esi, ecx
        mov     edx, eax
        ret
```

EAX liveness across callers: **audited** — ICMP/UDP/TCP paths use `EDX`/`checksum_2` next; none require EAX preserved.

---

## Link strategy

**Strategy A + C** (reloc-free raw blob + minimal FASM trampoline/switch).

| Rejected | Why |
|----------|-----|
| B `rust-lld` | Not needed after panic-path removal |
| Reject candidate | Not needed — leaf is suitable |

---

## Compiler-generated dependencies

### First attempt (REJECTED for extract)

Slice indexing (`data[i]`) in release still emitted bounds-check → panic/rodata:

* section size ~1270 B  
* `.rel.text.rust_checksum_1` present  
* many `.data.rel.ro..Lanon.*` sections  

**Classification:** compiler panic machinery, not algorithmic.

### Final artifact (ACCEPTED)

| Item | Result |
|------|--------|
| External calls / helpers | **none** in the extracted section |
| `.rodata` refs | **none** |
| Relocations | **0** |
| Panic / bounds-check | **eliminated** via raw pointer reads |

---

## Section

`.text.rust_checksum_1`

---

## Blob/object size

**652** bytes

---

## Relocations

**0** (extractor hard-fail if any)

---

## Extractor/link validation

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_checksum_1` |
| `SHT_PROGBITS` | yes |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 12` (`C2 0C 00`) present |
| Blob SHA-256 | `83D3FDDB9B0F2F55C586E90FCA3F5F86BB08A58FA91976C955D2C27CB4C5ED18` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 12`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-checksum1.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-e-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-e-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-e-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-e-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4490,server,nowait
```

Rollback OFF: set `USE_RUST_CHECKSUM_1 = 0`, reassemble, CoW/replace, QEMU (port 4491 used in audit).

---

## Rust tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **44/44** (was 38; +6 checksum) |
| Empty / named vectors | **PASS** |
| Exhaustive lengths `0..=64` × seeds | **PASS** |
| Remnant class `0..=24` | **PASS** |
| Carry stress (all `0xFF`) | **PASS** |

---

## Differential/oracle tests

| Suite | Result |
|-------|--------|
| Named corpus vs FASM-faithful host oracle | **PASS** |
| Length-stratified exhaustive `0..=64` | **PASS** |
| Deterministic PRNG corpus (20 000 vectors, fixed seed) | **PASS** |

Host differential mirrors `stack.inc` control flow (byte ADC + 3-deep CF stack). Domain is not fully enumerable over all seeds×buffers.

---

## ABI/register tests

In-kernel smoke `checksum1_rust_smoke_test` calls real `checksum_1` trampoline with:

* empty → 0  
* 8-byte → `0x1A35`  
* 2-byte → `0xDEAD`  
* odd + seed → `0x12345579`  
* 5-byte → `0x0906`  
* 9-byte + seed → `0x54AB`  
* `EBX`/`EDI`/`EBP` preservation  

Hang-on-fail (`EAX=0xDEAD0C51`) if mismatch. Reaching desktop implies trampoline path matched.

---

## Real caller test

| Path | Status |
|------|--------|
| Explicit smoke via public `checksum_1` symbol | **PROVEN** |
| Live ICMP/TCP/UDP packet checksum under real network traffic | **NOT PROVEN** |

Boot does not exercise the network stack; smoke is the deterministic Cut E execution proof.

---

## Kernel smoke

Called from `high_code` after Cut D smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_CHECKSUM_1=1`) | **PASS** — `query-status=running`; screendump `dev_build/cut-e-on-desktop.png` |
| Rust OFF (`USE_RUST_CHECKSUM_1=0`) | **PASS** — `query-status=running`; screendump `dev_build/cut-e-off-desktop.png` |

---

## Regression

| Gate | Result |
|------|--------|
| Cut A suite (CRC / UTF-16 / CP866 / UTF-8 in `kolibri_utils`) | **PASS** (included in 44/44) |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Full extract of prior blobs after Cut E build | **PASS** |

---

## Determinism

Two deep-clean freestanding rebuilds:

| Build | Size | SHA-256 |
|------:|-----:|---------|
| #1 | 652 | `83D3FDDB9B0F2F55C586E90FCA3F5F86BB08A58FA91976C955D2C27CB4C5ED18` |
| #2 | 652 | `83D3FDDB9B0F2F55C586E90FCA3F5F86BB08A58FA91976C955D2C27CB4C5ED18` |

**PASS** — identical size and bytes.

---

## Image sizes

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut E ON (default) | **224408** |
| Cut E OFF (FASM body; blob still embedded) | **224488** |
| Cut D baseline (pre–Cut E) | 223544 |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut D baseline ≈ +864 bytes (652-byte blob + trampoline/smoke/data + alignment).

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Rollback

```asm
USE_RUST_CHECKSUM_1 = 0   ; original FASM body
USE_RUST_CHECKSUM_1 = 1   ; Rust trampoline (default)
```

Independent of Cut A–D switches.  
Original FASM body remains in the `else` branch of `stack.inc`.

Wire-up:

* Trampoline / switch: `kernel/network/stack.inc`  
* Embed + smoke: `kernel/rust/checksum1.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed vectors; live network packet paths are not instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Blob is larger than the FASM leaf (LLVM expands byte-wise ADC simulation) — acceptable for Cut E complexity probe.  
* `checksum_2` intentionally not migrated (one function per cut).  
* Proof-of-life remains diagnostic only — not coupled to Cut E.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `checksum_1` over FS calendar / conf_lib / string leaves  
* Compiler-dependency gate: slice panic relocs classified and eliminated  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 12`  
* Rust unit + differential corpus / PRNG / length-exhaustive vs FASM-oracle  
* In-kernel trampoline smoke including EBX/EDI/EBP check (hang-on-fail)  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two deep-clean rebuilds  
* Independent rollback switch builds both ways  
* Cut B / C / D blob hashes unchanged  
* Full `kolibri_utils` suite green (44/44)  
* Reference floppy hash unchanged  

### NOT PROVEN

* Byte-identical behavior of every ICMP/TCP/UDP checksum under live network traffic  
* Host-assembled FASM binary vs Rust binary differential  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* `checksum_2` migration  
* `fsCalculateTime` / other deferred candidates  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut F  

---

## Verification matrix

```text
Candidate audit                         PASS
Cut E plan                              PASS
Rust implementation                     PASS
Compiler dependencies audited           PASS
Rust tests                              PASS (44/44)
Differential/oracle                     PASS (corpus + PRNG + length-exhaustive)
ABI/trampoline                          PASS (in-kernel smoke)
Register preservation                   PASS (EBX/EDI/EBP)
Real caller smoke                       PASS (symbol path); live net NOT PROVEN
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
Documentation                           PASS
```

**STOP** — do not start Cut F.
