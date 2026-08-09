# Cut N Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-n-implementation.md`](cut-n-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut N** is the first migration of a **GUI font-smoothing color-blend leaf** — RGB channel blend with a **16-bit `BP` loop counter sharing the `EBP` register** used as a 32-bit background color.  
> Cuts A–M remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `antiAliasing` |
| **Source** | [`kernel/gui/font.inc:846–862`](../../kernel/gui/font.inc) |
| **Subsystem** | GUI / font smoothing (not video blitter geometry) |
| **Purpose** | Blend each of the low three RGB bytes of a foreground color toward a background: `(3·fg + bg) >> 2`. |

---

## Candidate comparison

### Candidate 1: `antiAliasing` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `gui/font.inc:846–862` |
| Purpose | Font AA RGB blend when `fontSmoothing == 1` |
| Complexity | 3-channel rotate loop; BP as 16-bit counter; final `mov ebp, ebx` |
| Callers | 2 (`drawChar` left/right AA paths) |
| ABI | Regcall: `EAX`=fg, `EBX`=bg; out `EAX`=blend, `EBP`=`EBX`; clobbers `ECX`/`EDX`/`BP`; plain `ret` |
| Deps | None (pure arithmetic) |
| Reloc risk | **None** |
| Compiler helper risk | Low (byte arithmetic + rotates; no fills) |
| Risk | Low–med (must match 3-byte-only blend + EBP restore contract) |

### Candidate 2: `test_app_header` — rejected for Cut N

| Field | Detail |
|-------|--------|
| Source | `core/taskman.inc:206–250` |
| Why rejected | Strong process/MENUET leaf and Stage 6 foothold, but couples to `pages_free` / `OS_BASE`; larger than a controlled ABI probe. Deferred after GUI BP/EBP proof. |

### Candidate 3: `strtoint_dec` — rejected for Cut N

| Field | Detail |
|-------|--------|
| Source | `core/conf_lib.inc:116–163` |
| Why rejected | Preferred config-parse subsystem and quirky place-value algorithm, but narrative-thinner after A–D string work; kept as fallback if GUI EBP proved unsafe. |

### Candidate 4: `memmove` — rejected for Cut N

| Field | Detail |
|-------|--------|
| Source | `kernel.asm` |
| Why rejected | ~23 callers; memcpy-helper-risk probe deferred again (same reason as Cuts B–M). |

### Candidate 5: `coff_get_align` — rejected for Cut N

| Field | Detail |
|-------|--------|
| Source | `core/dll.inc:820–839` |
| Why rejected | PE/loader novelty real but thin bitfield→mask ABI after A–M. |

### Candidate 6: `DummyTest` — rejected for Cut N

| Field | Detail |
|-------|--------|
| Source | `gui/event.inc:228–244` |
| Why rejected | Fail path `pop`s return address (non-local return); trampoline risk outweighs IPC novelty for this cut. |

### Candidate 7: `hotkey_test*` / `hotkey_do_test` — rejected for Cut N

| Field | Detail |
|-------|--------|
| Source | `hid/keyboard.inc` |
| Why rejected | Keyboard preferred, but predicates are too thin; dispatcher has `kb_state` + relocating function-pointer table / IRQ adjacency after Cut L HID. |

### Candidate 8: timers / clipboard / sound / sync / unpack — rejected

| Field | Detail |
|-------|--------|
| Why rejected | Locks, ports, allocators, or oversized LZMA — not Strategy A+C leaves. |

### Explicitly excluded by Cut N preference

Filesystem / NTFS / FAT / video blitter geometry / HID mouse / TCP network — already covered by Cuts G–M.

---

## Why Cut N is a meaningful next step

Cuts A–M proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry
/ NTFS VLE MCB / NTFS USA / FAT DF+CF short-name / HID mouse accel
/ TCP RTT estimator
```

Cut N must answer a **different** question:

> Does Strategy A + C remain viable for a **GUI font color-blend leaf** whose FASM body uses a **16-bit `BP` counter inside the same register later restored as 32-bit `EBP` background color** — reloc-free, byte-exact, smokeable via font AA?

`antiAliasing` is the right probe:

1. **New subsystem** — GUI font smoothing (distinct from Cut H blitter geometry)  
2. **New ABI property** — first BP/EBP dual-use contract  
3. **Outside forbidden families** — not FS/NTFS/FAT/video-clip/mouse/TCP  
4. **Strategy A+C fit** — pure arithmetic; no globals; no tables; no HW  
5. **Limited blast radius** — 2 callers inside `drawChar`  
6. **Testability** — exhaustive/grid RGB pairs + large PRNG; clear oracle  

Deferred in Cuts H–M as “quirky EBP”; now the intentional novelty after TCP depth.

---

## Strategy

**A + C** (unchanged architecture):

* Freestanding Rust → reloc-free extract → FASM `file` embed  
* FASM trampoline preserves public `antiAliasing` ABI  
* `USE_RUST_ANTI_ALIASING` rollback switch  

---

## ABI (planned)

### Public FASM `antiAliasing`

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` = foreground RGB dword; `EBX` = background RGB dword |
| Output | `EAX` = blended (low 3 bytes); high byte of `EAX` unchanged |
| Side effect | `EBP := EBX` (matches FASM `mov ebp, ebx` after 4×`ror`) |
| Clobbered | `ECX`, `EDX` (and transient `BP` in original body) |
| Preserved | `EBX` (after 4 rotates returns to original); callers also push/pop `EBX`/`EDX` |

### Blend rule (per low byte, three times)

```text
out_byte = (3 * fg_byte + bg_byte) >> 2
```

Bytes are processed via `ror` by 8; a fourth `ror` restores dword lane order. The high byte is **not** blended.

### Rust `rust_anti_aliasing`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(fg: u32, bg: u32) -> u32` |
| Epilogue | `ret 8` |
| Globals | none |

### Trampoline sketch

```asm
antiAliasing:
        stdcall rust_anti_aliasing, eax, ebx
        mov     ebp, ebx
        ret
```

---

## Out of scope for Cut N

* Migrating subpixel paths (`.subpixelLeft` / `.subpixelRight`)  
* Migrating `drawChar` or other font renderer logic  
* Live proof that every desktop glyph uses AA under QEMU (depends on `fontSmoothing`)  
* `test_app_header` / `strtoint_dec` / `memmove` / keyboard hotkeys  

---

## Completion rule

Complete Cut N gates → document → **STOP**. Do not start Cut O.
