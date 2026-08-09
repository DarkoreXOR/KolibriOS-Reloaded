# Cut S Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-s-implementation.md`](cut-s-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut S** is the first migration of a **GUI window screen-fit policy leaf** — `window._.check_window_position`, which clamps `WDATA.box` into the visible screen using `EDI → WDATA*` and `_display.{width,height}` while preserving the caller register set.  
> Cuts A–R remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `window._.check_window_position` |
| **Source** | [`kernel/gui/window.inc:1702–1781`](../../kernel/gui/window.inc) |
| **Subsystem** | GUI / window management |
| **Purpose** | Keep a window box inside the screen (shrink oversize, nudge off-screen edges) |

---

## Post-R candidate audit (live tree)

### Coverage already proven (A–R)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string | A–D, Q |
| Net checksum / TCP RTT | E, F, M |
| Calendar BDFE→secs | G |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 / XFS BE unpack | I–K, R |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |

### Ranked remaining candidates

| Candidate | Callers | Novelty | Differential | Smoke | QEMU | Blast | Risk | Verdict |
|-----------|---------|---------|--------------|-------|------|-------|------|---------|
| `window._.check_window_position` | 2 | **High** — GUI screen-fit; EDI→WDATA*; display globals via trampoline | Excellent | Easy | **Strong** (fn0/fn67) | Low | Low–med | **SELECT** |
| `fsTime2bdfe` | 4+ | High — EDI+=8 calendar inverse | Excellent | Easy | Weak stock FAT | Low | Med | Defer |
| `fat_gen_short_name` | 1 | Med — compose Cut K/B | Excellent | Easy | Strong FAT create | Low | Med | Defer |
| `tcp_set_persist` | 2 | Med — timer arming after M | Excellent | Easy | Strong net | Low | Low | Defer |
| `blit_clip` | 1 | Low–med — H composition | Excellent | Easy | Weak fn73 | Low | Low | Defer |
| `set_window_clientbox` | 3 | Med — client inset + skin | Good | Med | Strong | Low | Med | Later |
| `memmove` | ~24 | Low algo / high fanout | Easy | Easy | Everywhere | **High** | Med–high | Stage-4 |

### Why this target beats the alternatives

* After Cut R, the deferred “live desktop but less novel ABI” gap is closed — screen-fit policy is a **new GUI class**, not another MOVBE/SHRD/EBP bitfield leaf.
* Stronger QEMU observability than `fsTime2bdfe` / `blit_clip` / XFS soak.
* Contained blast (2 production call sites on every window create/move path).
* Reloc-free strategy matches Cut L/O: trampoline injects `_display.width` / `_display.height` so Rust never references iglobals.

---

## Implementation plan

```text
Selected target:
    window._.check_window_position

Why it is stronger than the alternatives:
    New GUI policy class + strongest stock-image live path among post-R candidates;
    not calendar-family (fsTime2bdfe), not Cut-H composition (blit_clip),
    not Cut-M sibling (tcp_set_persist), not Stage-4 memmove.

Legacy ABI:
    EDI → WDATA* (box at +0); reads [_display.width/height];
    mutates box.{left,top,width,height}; push/pop EAX EBX ECX EDX ESI; plain ret;
    flags unused.

Critical invariants:
    Unsigned jae size clamp to display-1;
    signed jl / jge position nudge so left+width < display (and Y analog);
    wrapping ADD before signed compare; EDI pointer identity preserved.

Rust strategy:
    Freestanding Box clamp; stdcall (box*, width, height); no globals.

Trampoline strategy:
    push five regs; stdcall rust_check_window_position, edi, width, height;
    pop five regs; ret. USE_RUST_CHECK_WINDOW_POSITION gate.

Differential strategy:
    Independent FASM-flow oracle vs Rust; boundary grid + large PRNG corpus.

Smoke strategy:
    Save/restore _display dims; public ABI vectors; EDI/EAX–ESI preserve.

QEMU strategy:
    cut-r-final.img lineage; OFF then ON desktop regression (fn0 path).

Rollback gate:
    USE_RUST_CHECK_WINDOW_POSITION = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register ABI trampoline; `USE_RUST_CHECK_WINDOW_POSITION` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | Regcall leaf, plain `ret` |
| Register in | **EDI → `WDATA*`** (box at offset 0) |
| Implicit in | `[_display.width]`, `[_display.height]` |
| Out | Mutates `WDATA.box.{left,top,width,height}` in place |
| Preserved | EAX, EBX, ECX, EDX, ESI (FASM `push`/`pop`); **EDI** |
| Flags | unspecified / unused by callers |

### Layout (`BOX` / `WDATA.box`)

```text
+0  left     (i32)
+4  top      (i32)
+8  width    (i32; compared unsigned vs display)
+12 height   (i32; compared unsigned vs display)
sizeof.BOX = 16
WDATA.box = 0  → EDI may address WDATA or BOX interchangeably for this leaf
```

---

## Out of scope

* Migrating `set_window_clientbox` / `check_window_draw` / other window helpers  
* Migrating `blit_clip` / `fsTime2bdfe` / `memmove`  
* Cut T  

---

## Completion rule

Complete Cut S gates → document → **STOP**. Do not start Cut T.
