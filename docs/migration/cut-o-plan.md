# Cut O Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-o-implementation.md`](cut-o-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut O** is the first migration of a **process / MENUET binary-header validation leaf** — magic + memory-budget checks that fill `APP_HDR` for `fs_execute`.  
> Cuts A–N remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `test_app_header` |
| **Source** | [`kernel/core/taskman.inc:206–250`](../../kernel/core/taskman.inc) |
| **Subsystem** | Process / MENUET app header parse (Stage 6 foothold) |
| **Purpose** | Accept `MENUET01`/`MENUET02` headers; enforce `mem_size` vs `i_end` / `OS_BASE` / `pages_free<<12`; fill `APP_HDR` fields used by process create. |

---

## Candidate comparison

### Candidate 1: `test_app_header` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `core/taskman.inc:206–250` |
| Purpose | MENUET header validate + `APP_HDR` fill |
| Complexity | Straight-line magic + three mem checks + six field stores; partial mutate on mid-fail |
| Callers | **1** — `fs_execute` (`taskman.inc:107`) |
| Real QEMU path | **Yes** — every `/sys` app launch during desktop bring-up |
| ABI | Regcall: `EAX`→image, `EBX`→`APP_HDR`; success leaves `EAX`; fail `EAX=0`; clobbers `ECX`/`EDX`; plain `ret` |
| Globals | Reads `[pg_data.pages_free]`; const `OS_BASE` |
| Deps | None (no calls) |
| Reloc risk | **None** if trampoline injects `pages_free` |
| Compiler helper risk | Low (explicit dword stores; avoid bulk clears) |
| Risk | Low–med (must match partial `eip` write before fail) |

### Candidate 2: `is_region_userspace` — rejected for Cut O

| Field | Detail |
|-------|--------|
| Source | `kernel.asm:4481–4497` |
| Why rejected | Strong syscall-helper novelty and ZF-out ABI, but thinner body and ~31 callers (larger blast radius) after selecting the Stage 6 process leaf. |

### Candidate 3: `window._.check_window_position` — rejected for Cut O

| Field | Detail |
|-------|--------|
| Source | `gui/window.inc:1702–1781` |
| Why rejected | Real window-policy novelty, but Cut N already covered GUI arithmetic; display globals + box clamp is a second GUI cut rather than process. |

### Candidate 4: `strtoint_dec` — rejected

| Field | Detail |
|-------|--------|
| Source | `core/conf_lib.inc:116–163` |
| Why rejected | Preferred config-parse subsystem, but **`conf_lib` is not linked** (`if 0` / commented include). No live caller. |

### Candidate 5: `coff_get_align` — rejected

| Field | Detail |
|-------|--------|
| Source | `core/dll.inc:820–839` |
| Why rejected | PE leaf too thin after A–N. |

### Candidate 6: `memmove` — rejected

| Field | Detail |
|-------|--------|
| Source | `kernel.asm` |
| Why rejected | ~23 callers; memcpy-helper-risk probe deferred again. |

### Candidate 7: keyboard / `DummyTest` / timers — rejected

| Field | Detail |
|-------|--------|
| Why rejected | Hotkey predicates too thin; `hotkey_do_test` has fptr table + IRQ adjacency; `DummyTest` non-local `pop`; timers need locks/`cli`. |

### Explicitly excluded by Cut O preference

Filesystem / NTFS / FAT / video blitter / HID mouse / TCP network / GUI font — already covered by Cuts G–N.

---

## Why Cut O is a meaningful next step

Cuts A–N proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry
/ NTFS VLE MCB / NTFS USA / FAT DF+CF short-name / HID mouse accel
/ TCP RTT estimator / GUI font AA (BP/EBP)
```

Cut O must answer a **different** question:

> Does Strategy A + C remain viable for a **process/MENUET binary-format leaf** that validates headers, consults a FASM global memory budget via trampoline injection, partially mutates `APP_HDR` on mid-check fail, and is exercised by the **real `fs_execute` app-launch path** under QEMU?

`test_app_header` is the right probe:

1. **New subsystem** — process / MENUET header parse (Stage 6 foothold)  
2. **New semantic property** — binary-format validation + multi-field structure fill + mid-fail partial mutate  
3. **Real caller path** — `fs_execute` → `test_app_header` on every app launch  
4. **Outside forbidden families** — not FS/NTFS/FAT/video/mouse/TCP/font  
5. **Strategy A+C fit** — no calls; trampoline injects `pages_free`; `OS_BASE` is a constant  
6. **Limited blast radius** — single caller  
7. **Testability** — crafted headers + pages_free grids + PRNG  

---

## Strategy

**A + C** (unchanged architecture):

* Freestanding Rust → reloc-free extract → FASM `file` embed  
* FASM trampoline preserves public `test_app_header` ABI  
* `USE_RUST_TEST_APP_HEADER` rollback switch  

---

## ABI (planned)

### Public FASM `test_app_header`

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` → app image (`APP_HEADER_01_`); `EBX` → `APP_HDR` out |
| Success | `EAX` unchanged (nonzero image pointer); writes `eip`, `_emem`, `esp`, `cmdline`, `path`, `_edata` |
| Fail | `EAX = 0`; may have written `eip` already if magic passed and a later mem check failed |
| Clobbered | `ECX`, `EDX` |
| Preserved | `EBX` (destination pointer) |
| Global | reads `[pg_data.pages_free]` (trampoline-injected into Rust) |
| Constant | `OS_BASE = 0x80000000` |

### Accept rule

```text
banner[0..4) == 'MENU'
banner[4..6) == 'ET'
banner[6..8) ∈ {'01','02'}
mem_size >= i_end
mem_size <  OS_BASE
mem_size <  (pages_free << 12)
```

`version` dword at `+8` is **not** validated.

### Rust `rust_test_app_header`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(header: *const u8, app_hdr: *mut u8, pages_free: u32) -> u32` |
| Return | image pointer (= `header`) on success; `0` on fail |
| Epilogue | `ret 12` |
| Globals | none (`pages_free` is an argument; `OS_BASE` is a const) |

### Trampoline sketch

```asm
test_app_header:
        mov     ecx, [pg_data.pages_free]
        stdcall rust_test_app_header, eax, ebx, ecx
        ret
```

---

## Real caller path

```text
QEMU desktop /sys app launch
  → fs_execute
  → call test_app_header
  → USE_RUST_TEST_APP_HEADER=1 trampoline
  → rust_test_app_header
  → APP_HDR filled / reject
  → process create continues or .err_hdr
```

In-kernel synthetic smoke also exercises the public symbol directly.

---

## Out of scope for Cut O

* Migrating `fs_execute`, `create_process`, `set_app_params`, or scheduler  
* Validating `APP_HEADER` version dword semantics beyond current FASM  
* Re-enabling `conf_lib` / `strtoint_dec`  
* `is_region_userspace` / window clamp / `memmove`  

---

## Completion rule

Complete Cut O gates → document → **STOP**. Do not start Cut P.
