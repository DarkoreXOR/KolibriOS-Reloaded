# Cut CE Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-ce-implementation.md`](cut-ce-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CE** migrates GUI client-box policy —
> `window._.set_window_clientbox` in `kernel/gui/window.inc`.  
> Cuts A–CD remain complete and must not be redone. **Cut CD is closed — do not
> modify.** Do not start Cut CF in this task.

---

## Fresh post-CD migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **85** |
| `[[rust.migrations]]` production gates | **85** enabled |
| `[ ]` pending | **50** |
| Total scoped | **135** |
| Cut CD (`blit_clip`) | **closed** — untouched |
| Cut CC (`process_partition_table_entry`) | **closed** — untouched |
| All prior gates | **85/85 enabled** |

Baseline before this cut: **85 / 135**. Target after: **86 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| Video H+CD | No | Geometry leaves; `blit_32` / LFB still FASM |
| GUI S + clientbox | No | Policy leaves; window server / draw / invalidate still FASM |
| HID L+BE | No | Accel/hotkey leaves; drivers still FASM |
| IRQ enable/eoi | No | PIC init, ISR dispatch, STI/CLI still FASM |
| FS / net / PE / AHCI / Stage-4 | No | Prior footholds ≠ subsystem ownership |
| `unpack` | No | Single DLL decoder island, not owned subsystem |

Cut CE remains **Path B**.

### Special evaluation: `enable_irq`

| Item | Finding |
|------|---------|
| Source | `kernel/core/apic.inc` ~391–432 |
| I/O | PIC `in`/`out` on `0x21`/`0xA1` (clear mask bit); APIC path MMIO via `IOAPIC_read`/`IOAPIC_write` (unmask bit 16) |
| STI | **Does not** touch EFLAGS.IF — mask-only |
| Callers | 6 live (boot timer/PIC2/FPU, keyboard, `attach_int_handler`, BIOS disk) — same unmask semantics, different IF context |
| Host oracle | **Poor** — needs PIC port + IOAPIC MMIO + GSI tables |
| Reloc-free | Must inject or keep FASM IOAPIC callees |
| Verdict | **REJECT for Cut CE** — novel IRQ class, insufficient evidence bar |

### Special evaluation: `unpack`

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 (~464 nonblank) + `unpack.p` ~32KB |
| Boundary | `stdcall unpack(packed, unpacked); ret 8` — KPCK/LZMA + E8/E9 |
| Sub-cuts | Nested decoder locals share `unpack.*` globals — **no meaningful smaller public leaf** |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Verdict | **REJECT for Cut CE** — excellent oracle, disproportionate size/state/blast |

### Ranked candidates (50 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`window._.set_window_clientbox`** | GUI clientbox + skin inset | 3 | desktop GUI | **Excellent** | Low–Med | **SELECT** |
| 2 | `enable_irq` | PIC/APIC IRQ unmask | 6 | desktop IRQ | Poor (I/O) | Med–High | Reject CE — oracle |
| 3 | `set_mouse_data` | HID aggregator | 0 in-kernel (PE) | weak | Med | Med | Defer |
| 4 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med–High | Defer with enable_irq |
| 5 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** | Reject CE — size |
| 6 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS island |
| 7 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop | Hard | **High** | Reject — too hot after CD |

### Why #1 wins

* After Cut CD, video geometry Path B novelty is spent; strongest next leaf is the **GUI policy sibling of Cut S** (same `EDI→WDATA*` trampoline pattern).
* Three production callers on window create / maximize / state-change paths → high QEMU observability.
* Host differential oracle is realistic (synthetic WDATA + `window_topleft` + `_skinh`).
* Manageable ~48-line body; documented global side-effect (`window_topleft` skin tops) is oracle-friendly.
* No I/O, alloc, IRQ, or mutex.
* Does not reopen Cut CD; does not claim Path A.

### Why alternatives lose

* `enable_irq` / `irq_eoi`: interrupt I/O + weak host oracle (post-CC evidence bar).
* `unpack`: strongest FASM reduction but ~32KB `unpack.p` + LZMA is not one safe cut.
* `set_mouse_data`: PE-export aggregator; REG-003-class global side-effects.
* Thin rejects (`tcp_mss`, `ntfs_restore_usa_frs`, export-only strings): fail substance bar.
* `blit_32`: natural video follow-on but LFB hot path + mouse-under + bpp forks — too large.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `window._.set_window_clientbox` |
| **Source** | [`kernel/gui/window.inc:1562–1609`](../../kernel/gui/window.inc) |
| **Subsystem** | GUI / window management |
| **Stage** | Stage 2 / Stage 7 soft foothold (Cut S follow-on) |
| **Path** | B (Path A REJECTED) |
| **Purpose** | Derive `WDATA.clientbox` from window box + style insets; refresh skin tops in `window_topleft` |

### Callers (3)

| Site | Context |
|------|---------|
| `window.inc:690` | Maximize / screen-workarea layout loop |
| `window.inc:1525` | After Cut S on window state change |
| `window.inc:1690` | `sys_set_window` first-draw client box |

---

## Legacy ABI (locked from FASM)

```text
window._.set_window_clientbox() → void; plain ret
in:  EDI → WDATA*
     reads [_skinh]
     reads/writes window_topleft[] (5 × {left,top} dwords)
out: always writes window_topleft[3].top and [4].top := [_skinh]
     mutates WDATA.clientbox.{left,top,width,height}
preserves: EAX, ECX, EDI (push/pop)
clobbers: flags; may touch EDX/EBX/ESI in theory (FASM body does not use them)
flags: unused by callers
DF: unused
stack: owned by caller (plain ret)
interrupt state: untouched
```

### Semantics

1. `window_topleft[3].top = window_topleft[4].top = [_skinh]` (always).
2. If `(fl_wstyle & WSTYLE_CLIENTRELATIVE) == 0` (`0x20`):
   - `clientbox = {0, 0, box.width, box.height}`
3. Else (`style = fl_wstyle & 0x0F`):
   - `left = window_topleft[style].left`
   - `top  = window_topleft[style].top` (post-skinh write for styles 3/4)
   - `clientbox.left = left`
   - `clientbox.width = box.width - 2*left + 1` (Leency +1)
   - `clientbox.top = top`
   - `clientbox.height = box.height - top - left + 1` (Leency +1)

### WDATA layout (const.inc)

| Field | Offset |
|-------|--------|
| `box` | 0 (16 B) |
| `fl_wstyle` | 19 (`cl_workarea+3`) |
| `clientbox` | 32 (16 B) |

---

## Rust ABI

```text
stdcall rust_set_window_clientbox(wdata, skinh, window_topleft) → void; ret 12
wdata          → writable WDATA* (needs box + fl_wstyle + clientbox)
skinh          = [_skinh] injected by trampoline
window_topleft → writable base of 5×{left,top} table
```

No globals inside the blob — reloc-free via trampoline injection (Cut S pattern).

### Trampoline / stack ownership

```text
push eax ecx edi
FASM stdcall rust_set_window_clientbox, edi, [_skinh], window_topleft
; callee cleans 12 (ret 12) — NO add esp (REG-009)
pop edi ecx eax
ret
```

### Production gate

`USE_RUST_SET_WINDOW_CLIENTBOX = 1` in `kernel/gui/window.inc` (independent of `USE_RUST_CHECK_WINDOW_POSITION`).

### Oracle / tests / QEMU

| Item | Plan |
|------|------|
| Oracle | Independent FASM-flow mirror `fasm_oracle_set_window_clientbox` |
| PRNG | seed `0x53574342` (`'SWCB'`), ≥50_000 cases |
| Host | focused window clientbox tests + full `kolibri_utils` suite |
| ABI smoke | synthetic WDATA: whole-window / client-relative styles 0–4; skin-top mutation; EAX/ECX/EDI canaries |
| QEMU | OFF then ON desktop; A/B; repeated ON ×3; GUI soak (fn0 / maximize / clientbox path) |

### Rollback

`USE_RUST_SET_WINDOW_CLIENTBOX = 0` → original FASM body.
