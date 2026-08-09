# Cut P Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-p-implementation.md`](cut-p-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut P** is the first migration of a **syscall user-memory / pointer gate** — `is_region_userspace`, whose legacy ABI returns via **ZF** while preserving **EAX/ECX/EDX**.  
> Cuts A–O remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `is_region_userspace` |
| **Source** | [`kernel/kernel.asm`](../../kernel/kernel.asm) |
| **Subsystem** | Syscall pointer / user-memory gate (Stage 3 foothold) |
| **Purpose** | Accept regions entirely below `OS_BASE` (with `end == OS_BASE` allowed); reject kernel addresses and out-of-range ends; **bit-exact** FASM ZF including overflow-to-zero quirk. |

---

## Candidate comparison

### Candidate 1: `is_region_userspace` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `kernel.asm` |
| Purpose | Userspace region gate; ZF-out ABI |
| Complexity | Tiny arithmetic; ABI novelty (ZF + register preserve) |
| Callers | **~30** live stdcall sites across syscalls / GUI / net / FS |
| Real QEMU path | **Yes** — nearly every user-buffer syscall |
| ABI | stdcall `ret 8`; result in **ZF**; EAX/ECX/EDX preserved |
| Reloc risk | **None** — `OS_BASE` constant |
| Risk | Med blast radius; mitigated by tiny body + differential + smoke |

### Candidate 2: `window._.check_window_position` — rejected

Second GUI leaf after Cut N; only 2 callers.

### Candidate 3: `memmove` — rejected

Memcpy helper-risk probe; ~24 hot callers; deferred again.

### Candidate 4: `strtoint_dec` — rejected

`conf_lib` not linked; no live caller.

---

## Why Cut P is a meaningful next step

Cuts A–O proved utils / FS / video / HID / TCP / GUI font / process header leaves.

Cut P answers:

> Does Strategy A + C remain viable for a **flags-only syscall security gate** used by ~30 callers, where Rust returns a scalar but the trampoline must reconstruct **ZF** and preserve **EAX/ECX/EDX** (Cut D class), including the FASM overflow-to-zero quirk?

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; trampoline preserves public ABI; `USE_RUST_IS_REGION_USERSPACE` rollback switch.

---

## ABI (planned / locked)

### Public FASM `is_region_userspace`

| Item | Contract |
|------|----------|
| Convention | stdcall, `ret 8` |
| Inputs | `(base, len)` stack dwords |
| Result | **ZF=1** accept / **ZF=0** reject (not EAX) |
| Preserved | **EAX, ECX, EDX** (proven by callers); EBX/ESI/EDI/EBP untouched |
| DF | unchanged |
| CF/SF/OF | unspecified for Cut P (no caller dependency) |

### Overflow-to-zero quirk (compatibility, not a bugfix)

When `base + len` unsigned-overflows and the wrapped sum is **0**, FASM takes `JC` with **ZF=1** from `ADD`. Callers using `jnz` reject therefore **accept**. Rust + trampoline must reproduce **ZF=1**.

### Rust `rust_is_region_userspace`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(base: u32, len: u32) -> u32` |
| Return | `1` = legacy ZF=1; `0` = legacy ZF=0 |
| Epilogue | `ret 8` |

### Trampoline

```asm
push ecx / push edx / push eax
stdcall rust_is_region_userspace, [base], [len]
cmp eax, 1          ; ZF = (ret == 1)
pop eax / pop edx / pop ecx   ; flag-neutral
ret 8
```

---

## Out of scope

* Migrating callers or syscall bodies  
* “Fixing” overflow-to-zero to documented reject  
* Matching CF on overflow  
* `memmove` / window clamp / Cut Q  

---

## Completion rule

Complete Cut P gates → document → **STOP**. Do not start Cut Q.
