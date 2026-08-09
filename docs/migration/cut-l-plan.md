# Cut L Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-l-implementation.md`](cut-l-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut L** is the first migration of a **HID / mouse input leaf** — integer acceleration curve over a motion delta with EAX high-word sign-restore quirks.  
> Cuts A–K remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `mouse_acceleration` |
| **Source** | [`kernel/hid/mousedrv.inc:271–284`](../../kernel/hid/mousedrv.inc) |
| **Subsystem** | HID / mouse input |
| **Purpose** | Map a relative motion delta through KolibriOS’s delay/square/shift acceleration curve. |

---

## Candidate comparison

### Candidate 1: `mouse_acceleration` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `hid/mousedrv.inc:271–284` |
| Purpose | Abs AX → `AL+=delay` → `AL*AL` → `(AX-1)>>factor + 1` → sign restore |
| Complexity | Tiny leaf; subtle EAX high-word signedness; AX-only abs loop |
| Callers | 2 (`set_mouse_data` X and Y relative paths) |
| ABI | Regcall: `AX`/`EAX` in-out; clobbers `CX`; reads `[mouse_delay]`, `[mouse_speed_factor]`; plain `ret` |
| Deps | Two tunables only (also exposed via syscall 18.19 / conf) |
| Reloc risk | **None** if trampoline loads globals and passes immediates |
| Compiler helper risk | Low (pure arithmetic) |
| Risk | Low–med (EAX high-word quirk must be modeled exactly) |

### Candidate 2: `UTF16to8` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Source | `fs/parse_fn.inc:292–331` |
| Why rejected | Attractive **SF** exhaust ABI, but lives under FS parse path with ~7 FS callers; Cut L prefers a genuinely non-FS subsystem after G/I/J/K FS weight. |

### Candidate 3: `memmove` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Source | `kernel.asm:3234–3266` |
| Why rejected | Best memcpy-class helper-risk probe (Cut K deferral), but ~24 hot callers and algorithmically thin vs HID novelty. Defer to a dedicated cut. |

### Candidate 4: `coff_get_align` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Source | `core/dll.inc:820–839` |
| Why rejected | Novel PE/loader leaf, but very small bitfield policy with thin ABI pressure after A–K. |

### Candidate 5: `unpack` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Source | `unpacker.inc` |
| Why rejected | Compression novelty is real, but body is large (LZMA + fixups + BSS scratch) — out of leaf-cut culture; high compiler-helper risk. |

### Candidate 6: `strrchr` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Why rejected | DF already proven by Cut K; string.inc family; weaker new-subsystem story. |

### Candidate 7: `hotkey_do_test` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Why rejected | Keyboard HID adjacency attractive, but depends on `kb_state` + `hotkey_tests` function-pointer table (reloc risk) and sits on scancode/IRQ-adjacent path. |

### Candidate 8: `antiAliasing` — rejected for Cut L

| Field | Detail |
|-------|--------|
| Why rejected | Color blend is not blitter geometry, but quirky EBP contract and GUI-adjacent; deferred again. |

---

## Why Cut L is a meaningful next step

Cuts A–K proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry (CF+mutate)
/ NTFS VLE MCB / NTFS USA / FAT DF+CF short-name
```

Cut L must answer a **different** question:

> Does Strategy A + C remain viable for a **HID mouse acceleration leaf** (input subsystem; AX-only abs; mul-based curve; EAX high-word sign restore; trampoline-injected tunables) as a reloc-free blob with a byte-exact differential oracle?

`mouse_acceleration` is the right probe:

1. **Different subsystem** — HID / mouse (not FS, not video geometry)  
2. **New architectural property** — first input leaf; EAX high-word sign quirk after AX-only mul; trampoline loads BSS/iglobal tunables into stack args (keeps blob reloc-free)  
3. **Strategy A+C fit** — pure arithmetic; no tables; no HW ports  
4. **Limited blast radius** — 2 callers inside `set_mouse_data`  
5. **Testability** — exhaust AX × delay/factor; 200k PRNG; in-kernel smoke without needing live PS/2 traffic  

---

## Strategy

**A + C** (unchanged architecture):

* Freestanding Rust → reloc-free extract → FASM `file` embed  
* FASM trampoline preserves public `mouse_acceleration` ABI  
* `USE_RUST_MOUSE_ACCELERATION` rollback switch  

---

## ABI (planned)

### FASM (callers)

| Item | Contract |
|------|----------|
| Input | `EAX` motion delta (`AX` accelerated; high word affects sign restore) |
| Output | `AX`/`EAX` accelerated delta |
| Clobbers | `CX` (original loads `mouse_speed_factor`) |
| Globals | reads `[mouse_delay]`, `[mouse_speed_factor]` |
| Return | plain `ret` |

### Rust

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(delta, delay, speed_factor)` |
| Return | `u32` in `EAX` |
| Epilogue | `ret 12` |

### Trampoline

Load tunables, `stdcall rust_mouse_acceleration, eax, delay, factor`, `ret`.

---

## Rollback

| Switch | `USE_RUST_MOUSE_ACCELERATION` (`1` default / `0` original FASM) |

---

## Out of scope for Cut L

* Live PS/2 / USB mouse packet path end-to-end in QEMU (may be NOT PROVEN if smoke only)  
* `sysfn_mouse_acceleration` setter/getter migration  
* `set_mouse_data` orchestration  
* Absolute-motion paths (bypass acceleration)  
* `memmove` / `UTF16to8` / Cut M  

---

## Stop rule

Complete Cut L gates → document → **STOP**. Do not start Cut M.
