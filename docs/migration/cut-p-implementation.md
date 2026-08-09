# Cut P Implementation — `is_region_userspace`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-p-plan.md`](cut-p-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `is_region_userspace` |
| Source | [`kernel/kernel.asm`](../../kernel/kernel.asm) |
| Callers | ~30 live stdcall sites (syscalls, GUI, net, FS, posix, …) |
| Rust symbol | `rust_is_region_userspace` |
| Pure helper | `kolibri_utils::is_region_userspace` |
| Subsystem | Syscall pointer / user-memory gate |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `is_region_userspace` | **Selected** — Stage 3 foothold; ZF-out ABI; real syscall fanout |
| `window._.check_window_position` | Rejected — second GUI leaf after Cut N |
| `memmove` | Rejected — memcpy helper-risk deferred again |
| `strtoint_dec` | Rejected — `conf_lib` not linked; no live caller |

---

## Why selected

Cut P’s research question: does Strategy A + C remain viable for a **flags-only security gate** where the result is **ZF**, callers require **EAX/ECX/EDX** preservation (Cut D class), and FASM’s overflow-to-zero quirk must be preserved bit-exactly?

---

## Special ABI handling

`is_region_userspace` does **not** return its result through EAX. Its legacy ABI uses **ZF** as the result while preserving **EAX**, **ECX**, and **EDX**. The Rust trampoline therefore reconstructs ZF after the Rust call (`cmp eax, 1`) and restores the caller-visible registers with flag-neutral `pop`s without modifying the reconstructed flags.

This is different from ordinary return-value migrations (Cuts A–O scalar EAX / CF leaves).

---

## Original implementation

FASM leaf retained under `USE_RUST_IS_REGION_USERSPACE=0`:

```asm
push eax
mov  eax, [base]
cmp  eax, OS_BASE-1
ja   @fail          ; ZF=0
add  eax, [len]
jc   @fail          ; ZF from ADD (1 iff sum_mod==0)
cmp  eax, OS_BASE
ja   @fail          ; ZF=0
cmp  eax, eax       ; ZF=1
@fail:
pop  eax            ; flags unchanged
ret 8
```

### Overflow-to-zero quirk (deliberate compatibility)

```text
base + len wraps with sum_mod == 0
→ JC taken, ZF=1
→ callers with jnz-reject treat as ACCEPT
```

Do **not** normalize this to documented “overflow ⇒ reject”. It is legacy observable ABI.

`OS_BASE = 0x80000000`. `end == OS_BASE` is accepted.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/userspace.rs`](../../rust_kernel/kolibri_utils/src/userspace.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_is_region_userspace` |
| Build | [`rust_kernel/kolibri_utils/build-is-region-userspace.ps1`](../../rust_kernel/kolibri_utils/build-is-region-userspace.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_is_region_userspace.bin` |
| Embed / smoke | [`kernel/rust/is_region_userspace.inc`](../../kernel/rust/is_region_userspace.inc) |

`#![no_std]` freestanding; pure integer arithmetic; no helpers/GOT/rodata.

Return: `1` = legacy ZF=1, `0` = legacy ZF=0.

---

## ABI

### FASM public (callers unchanged)

| Item | Contract |
|------|----------|
| Convention | stdcall, `ret 8` |
| Result | ZF only |
| Preserved | EAX, ECX, EDX (required); EBX/ESI/EDI/EBP |
| DF | unchanged |
| CF/SF/OF | not required for Cut P |

### Rust

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(base, len) -> u32` |
| Epilogue | `ret 8` |

### Trampoline (`USE_RUST_IS_REGION_USERSPACE=1`)

```asm
proc is_region_userspace stdcall, base:dword, len:dword
        push    ecx
        push    edx
        push    eax
        stdcall rust_is_region_userspace, [base], [len]
        cmp     eax, 1
        pop     eax
        pop     edx
        pop     ecx
        ret
endp
```

---

## Compiler artifact audit

| Check | Result |
|-------|--------|
| Section | `.text.rust_is_region_userspace` |
| Relocations | **0** |
| `CALL` / GOT / PLT | **none** |
| `.rodata` | **none** |
| Epilogue | `ret 8` (`C2 08 00`) |
| Blob size | **45** bytes |
| Blob SHA-256 | `958AFB3AB5F3677397459FFB5187C009F7CACCA20DB1FF5FDDB827C39C26FDAA` |

LLVM implements the same predicate via `test`/`js`, `ADD`+`sete` (overflow ZF quirk), and `cmp`/`setb` for `sum <= OS_BASE`.

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **158/158** (+6 userspace) |
| Required vectors + boundary grid | **PASS** |
| Overflow-to-zero / non-zero | **PASS** |
| Deterministic PRNG (200 000, seed `0x43555450`) | **PASS** |
| Trampoline ZF model (`cmp == 1`) | **PASS** |

Oracle models the **actual FASM instruction sequence**, not the intended high-level “overflow always reject” docs.

---

## In-kernel smoke

`is_region_userspace_rust_smoke_test` (wired after Cut O smoke):

* Public symbol → trampoline → Rust  
* Vectors: `(0,0)`, `(0,1)`, `OS_BASE±`, `end==OS_BASE`, overflow±zero  
* ZF via immediate `jz`/`jnz`; EAX/ECX/EDX sentinels  
* Fail hang: `EAX=0xDEAD0C50`, `EBX='SURI'`, `ECX='FAIL'`  

---

## QEMU validation

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| ON | `USE_RUST_IS_REGION_USERSPACE=1` | **OK** (manual + screendump `tmp_images/cut-p-on.ppm`) | **OK** (manual) |
| OFF | `=0` (original FASM body) | **OK** (screendump `tmp_images/cut-p-off.ppm`) | **OK** (FASM = A–O baseline; desktop reached) |

Smoke (ON diagnostic): **PASS** (boot continued; no `0xDEAD0C50` hang).

Production default after completion: **`USE_RUST_IS_REGION_USERSPACE = 1`**.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel-cut-p-on.mnt` | 231384 | `A82559245E7DCB93E3198CDD862557F0E26F462E107D83B0DB27F146ADAFE0A0` |
| `kernel-cut-p-off.mnt` | 231384 | `88BFB472804FD475394875F9FB910674405BB3F1A7608A4D23D773CCFE640CFC` |

Same padded size; different hashes (trampoline vs FASM body). Size delta alone is not a failure criterion.

---

## Rollback

```text
USE_RUST_IS_REGION_USERSPACE = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/is_region_userspace.inc`. Independent of Cuts A–O switches. Cut D EDX fix and Cut M smoke fix untouched.

---

## Evidence summary

### PROVEN

* Candidate audit selecting syscall ZF-gate over window/memmove/conf alternates  
* FASM truth table including overflow-to-zero → ZF=1  
* Caller audit: ~30 sites; EAX/ECX/EDX preservation required; dual ZF polarity (posix inverted)  
* Freestanding 45-byte blob, 0 relocs, no CALL/GOT  
* Host differential + 200k PRNG  
* Trampoline ZF reconstruction + register preserve  
* In-kernel smoke hang-on-fail  
* QEMU ON/OFF desktop; ON internet manual; OFF FASM baseline  

### NOT PROVEN

* CF bit-exact match on overflow path (not caller-required)  
* Exhaustive syscall matrix under QEMU  

### OUT OF SCOPE

* Migrating syscall bodies / Stage 3 façade wholesale  
* `memmove` / window clamp / Cut Q  

---

## Cut P status

**COMPLETE.** Do not start Cut Q until explicitly instructed.
