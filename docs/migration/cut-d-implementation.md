# Cut D Implementation — `strncmp`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-d-plan.md`](cut-d-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `strncmp` |
| Source | [`kernel/core/string.inc`](../../kernel/core/string.inc) |
| Callers | `dll.inc`, `heap.inc`, `peload.inc`, `ext_lib.inc`; PE export `strncmp` |
| Rust symbol | `rust_strncmp` |
| Pure helper | `kolibri_utils::strncmp` |

---

## Original implementation

FASM `proc strncmp stdcall` (retained under `USE_RUST_STRNCMP=0`):

* If `n == 0` → return 0  
* Else `cmpsb` loop; on inequality → `seta`/`setb`/`movsx` → −1/0/+1  
* On equality: stop at NUL → 0; else `dec n` until exhausted → 0  
* Saves/restores `ESI`/`EDI`; uses `cld`

No callees, no globals, no memory writes.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/string.rs`](../../rust_kernel/kolibri_utils/src/string.rs) `strncmp` |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_strncmp` |
| Build | [`rust_kernel/kolibri_utils/build-strncmp.ps1`](../../rust_kernel/kolibri_utils/build-strncmp.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_strncmp.bin` |

`#![no_std]` freestanding; `#[inline(always)]` into FFI so the dedicated section stays self-contained.

---

## Why this target

Cuts B and C already migrated register-leaf casefold from `parse_fn.inc`. Cut D deliberately moves to **`core/string.inc`** to prove Strategy A+C generalizes to:

* a different include / subsystem  
* stdcall stack arguments (`ret 12`)  
* read-only memory walks  
* a PE-exported symbol  

Remaining `parse_fn` leaves were audited and deferred (tables, flags ABI, or weaker generalization value). See [`cut-d-plan.md`](cut-d-plan.md).

---

## ABI

When replacing a legacy FASM routine with Rust, the **effective ABI** includes
not only arguments, return value, stack cleanup, and explicitly documented
callee-saved registers, but also **observable legacy register preservation
relied upon by existing callers**. That preservation is an **ABI compatibility
requirement**, not an optimization detail.

### FASM `strncmp` (caller-visible contract)

| Item | Contract |
|------|----------|
| Convention | `stdcall` (`proc … stdcall`) |
| Inputs | `s1`, `s2`, `n` — three stack dwords |
| Output | `EAX` ∈ {−1, 0, +1} (unsigned byte order) |
| Stack cleanup | callee `ret 12` |
| Callee-saved (documented) | `ESI`, `EDI` (FASM body); Rust stdcall preserves `EBX`/`ESI`/`EDI`/`EBP` |
| **Legacy EDX** | **`EDX` survives the call** (FASM body never touched it) |
| Flags / DF | FASM body executes `cld` (forces DF=0); other flags clobbered |
| Memory | read-only |

### Legacy EDX preservation (required)

```text
Legacy behavior:
    EDX survives the call.

Reason:
    get_service (kernel/core/dll.inc) keeps SRV* in EDX across strncmp,
    then uses EDX after the call (return path / list walk).

Rust integration requirement:
    FASM trampoline must preserve EDX across rust_strncmp.
    rust_strncmp itself may clobber EDX (uses it as s2); that is fine
    behind the trampoline.

Current solution:
    push edx
    stdcall rust_strncmp, [s1], [s2], [n]
    pop edx
```

This is **ABI compatibility**, not a micro-optimization.

### Potential legacy-ABI difference: DF / `cld` (unchanged)

```text
Potential legacy-ABI difference:
    FASM strncmp forces DF=0 via CLD.
    Rust strncmp currently leaves DF unchanged.

Status:
    Not demonstrated to cause the observed Cut D regression.
    Not changed as part of the Cut D fix.
    Requires separate investigation if ABI completeness is required.
```

### Rust `rust_strncmp`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(s1: *const u8, s2: *const u8, n: u32)` |
| Return | `i32` in `EAX` |
| Epilogue | `ret 12` |
| EDX | may be clobbered (trampoline restores) |
| DF | not forced; see note above |

### Trampoline (production)

```asm
proc strncmp stdcall, s1:dword, s2:dword, n:dword
        push    edx
        stdcall rust_strncmp, [s1], [s2], [n]
        pop     edx
        ret
endp
```

---

## Post-bisect status (2026-08-09)

```text
Cut D:
    target: strncmp
    status: COMPLETE — FIXED

Regression:
    Desktop remained functional.
    Network connectivity was lost.

Root cause:
    Rust strncmp clobbered EDX.

Critical caller:
    get_service.

Fix:
    EDX-preserving FASM trampoline.

Validation:
    Desktop OK.
    Internet OK.
```

Investigation log: [`black-screen-investigation.md`](black-screen-investigation.md).

---

## Link strategy

**Strategy A + C** (reloc-free raw blob + minimal FASM trampoline/switch).

| Rejected | Why |
|----------|-----|
| B `rust-lld` | No `.rodata` / relocs / helpers needed |
| Reject candidate | Not needed — leaf is suitable |

---

## Rust section

`.text.rust_strncmp`

---

## Blob size

**84** bytes

---

## Relocations

**0** (extractor hard-fail if any)

---

## Extractor validation

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_strncmp` |
| `SHT_PROGBITS` | yes |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 12` (`C2 0C 00`) present |
| Blob SHA-256 | `F9158B384DDBB4FB4BAAF8DD6E5669957C48E26ABA1C59B6D67E433F16312259` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 12`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-strncmp.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-d-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-d-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-d-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-d-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4480,server,nowait
```

Rollback OFF: set `USE_RUST_STRNCMP = 0`, reassemble, CoW/replace, QEMU (port 4481 used in audit).

---

## Rust tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **38/38** (was 34; +4 strncmp) |
| Named vectors (equal / unequal / empty / unsigned / `n=-1`) | **PASS** |
| `n == 0` always equal | **PASS** |
| Sign values exactly −1 / 0 / +1 | **PASS** |

---

## Differential/oracle tests

| Suite | Result |
|-------|--------|
| Named corpus vs FASM-faithful host oracle | **PASS** |
| Deterministic PRNG corpus (10 000 vectors, fixed seed) | **PASS** |

Host differential is algorithm-oracle level (mirrors `string.inc` control flow), not a host-assembled FASM binary. Domain is not exhaustively enumerable.

---

## ABI test

In-kernel smoke `strncmp_rust_smoke_test` calls real `strncmp` trampoline with:

* equal `"kolibri"` / `"kolibri"` (`n=7`) → 0  
* unequal `"kolibri"` / `"kolibrj"` → −1  
* `n=0` → 0  
* `n=-1` equal → 0  
* empty / empty → 0  
* `ESI`/`EDI` preservation check around the calls  

Hang-on-fail (`EAX=0xDEAD57C3`) if mismatch. Reaching desktop implies trampoline path matched.

---

## Real caller validation

| Path | Status |
|------|--------|
| Explicit smoke via public `strncmp` symbol | **PROVEN** |
| PE import resolution / DLL / heap name compares under real workloads | **NOT PROVEN** (not separately instrumented) |

Boot to desktop with a working icon layout makes PE/`strncmp` use **plausible**, but is not treated as a dedicated real-caller proof.

---

## Kernel smoke

Called from `high_code` after Cut C smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump with non-trivial framebuffer content).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_STRNCMP=1`) | **PASS** — `query-status=running`; screendump `dev_build/cut-d-on-desktop.png` |
| Rust OFF (`USE_RUST_STRNCMP=0`) | **PASS** — `query-status=running`; screendump `dev_build/cut-d-off-desktop.png` |

---

## Regression tests

| Gate | Result |
|------|--------|
| Cut A suite (CRC / UTF-16 / CP866 / UTF-8 in `kolibri_utils`) | **PASS** (included in 38/38) |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Full extract of prior blobs after Cut D build | **PASS** |

---

## Determinism

Two deep-clean freestanding rebuilds (delete `i686-kolibri-none/release`, recompile `core`+crate):

| Build | Size | SHA-256 |
|------:|-----:|---------|
| #1 | 84 | `F9158B384DDBB4FB4BAAF8DD6E5669957C48E26ABA1C59B6D67E433F16312259` |
| #2 | 84 | `F9158B384DDBB4FB4BAAF8DD6E5669957C48E26ABA1C59B6D67E433F16312259` |

**PASS** — identical size and bytes.

---

## Image size

| Config | `kernel.mnt` size |
|--------|------------------:|
| Cut D ON (default) | **223544** |
| Cut D OFF (FASM body; blob still embedded) | **223576** |
| Cut C baseline (pre–Cut D) | 223288 |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob (same Cut A/B/C pattern). Size delta itself is not a failure.

ON − Cut C baseline ≈ +256 bytes (84-byte blob + trampoline/smoke/data + alignment).

Reference floppy SHA-256 unchanged:

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Rollback

```asm
USE_RUST_STRNCMP = 0   ; original FASM body
USE_RUST_STRNCMP = 1   ; Rust trampoline (default)
```

Independent of Cut A (`USE_RUST_CRC` / `UTF16` / `CP866` / `UTF8`), Cut B (`USE_RUST_CP866_UPPER`), and Cut C (`USE_RUST_UTF16_UPPER`).  
Original FASM body remains in the `else` branch of `string.inc`.

Wire-up:

* Trampoline / switch: `kernel/core/string.inc`  
* Embed + smoke: `kernel/rust/strncmp.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed vectors; PE/DLL/heap callers are not separately instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Proof-of-life remains on branch `tmp/rust_pof` — not coupled to Cut D.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `strncmp` over remaining `parse_fn` / other leaves  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 12`  
* Rust unit + differential corpus / PRNG vs FASM-oracle  
* In-kernel trampoline smoke including ESI/EDI check (hang-on-fail)  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two deep-clean rebuilds  
* Independent rollback switch builds both ways  
* Cut B / Cut C blob hashes unchanged  
* Full `kolibri_utils` suite green (38/38)  
* Reference floppy hash unchanged  

### NOT PROVEN

* Byte-identical behavior of every PE/DLL/heap caller under real app workloads  
* Host-assembled FASM binary vs Rust binary differential  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* Other `string.inc` helpers (`strnlen`, `strncpy`, …)  
* Remaining `parse_fn.inc` leaves  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut E  

---

## Verification matrix

```text
Candidate audit                         PASS
Cut D plan                              PASS
Rust implementation                     PASS
Rust tests                              PASS (38/38)
Differential/oracle                     PASS (corpus + PRNG)
ABI/trampoline                          PASS (ESI/EDI + EDX preserve via trampoline)
Relocation validation                   PASS
Kernel smoke                            PASS (early); D–O diagnostic smokes OFF post-bisect
QEMU Rust ON                            PASS (desktop + network after EDX fix)
QEMU Rust OFF                           PASS
Deterministic rebuild ×2                PASS
Rollback switch                         PASS
Cut A regression                        PASS (suite + blob extract)
Cut B regression                        PASS (suite + cp866_upper hash)
Cut C regression                        PASS (suite + utf16_upper hash)
Post-bisect EDX ABI fix                 PASS (get_service / network)
Documentation                           PASS
```

Stage 2 continued through Cut AB after this write-up — see [`migration-plan.md`](migration-plan.md). DF/`cld` for `strncmp` remains an open separate question.
