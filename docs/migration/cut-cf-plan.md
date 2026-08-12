# Cut CF Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-cf-implementation.md`](cut-cf-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CF** migrates HID mouse aggregator —
> `set_mouse_data` in `kernel/hid/mousedrv.inc`.  
> Cuts A–CE remain complete and must not be redone. **Cut CE is closed — do not
> modify.** Do not start Cut CG in this task.

---

## Fresh post-CE migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **86** |
| `[[rust.migrations]]` production gates | **86** enabled |
| `[ ]` pending | **49** |
| Total scoped | **135** |
| Cut CE (`window._.set_window_clientbox`) | **closed** — untouched |
| Cut CD (`blit_clip`) | **closed** — untouched |
| Cut CC (`process_partition_table_entry`) | **closed** — untouched |
| `TMP_STACK_TOP` | **`0x008D000`** (Cut CE baseline; do not revert) |
| All prior gates | **86/86 enabled** |

Baseline before this cut: **86 / 135**. Target after: **87 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| GUI S+CE | No | Policy leaves; draw/invalidate/server still FASM |
| Video H+CD | No | Geometry leaves; `blit_32` / LFB still FASM |
| HID L+BE (+ aggregator) | No | Accel/hotkey/compose leaves; PS/2–USB PE drivers still FASM |
| IRQ enable/eoi | No | PIC init, ISR dispatch, STI/CLI still FASM |
| FS / net / PE / AHCI / Stage-4 | No | Prior footholds ≠ subsystem ownership |
| `unpack` | No | Single DLL decoder island, not owned subsystem |

Cut CF remains **Path B**.

---

## Special investigations (mandatory)

### `enable_irq` — REJECT

| Item | Finding |
|------|---------|
| Source | `kernel/core/apic.inc` ~391–432 |
| Hardware | PIC `in`/`out` `0x21`/`0xA1` (clear mask bit) **or** IOAPIC MMIO via `IOAPIC_read`/`IOAPIC_write` (clear mask bit 16) |
| STI/CLI | **Does not** touch EFLAGS.IF — mask-only |
| Controllers | Selected by `[irq_mode]` (`IRQ_APIC` vs PIC) |
| Callers | 6 live (boot timer/PIC2/FPU, keyboard, `attach_int_handler`, BIOS disk) — same unmask contract |
| Trampoline safety | Entering Rust with IF clear at some sites is fine (no `sti` in body); compiler must not emit IF-touching sequences — still needs I/O callees |
| Host oracle | **Poor** — PIC port + IOAPIC MMIO + GSI/`ioapic_cur` tables; mockable in principle (CA/CB style) but **QEMU cannot independently observe mask-bit correctness** |
| Verdict | **REJECT for Cut CF** — fails evidence bar; do not weaken for caller count |

### `irq_eoi` — DEFER

| Item | Finding |
|------|---------|
| Source | `apic.inc` ~371–386 (`__fastcall`, `CL` = irq) |
| Hardware | PIC: `out 0xA0`/`0x20` with `AL=0x20`; APIC: store 0 to `[LAPIC_BASE+APIC_EOI]` |
| Callers | 4 (`sched`, `irq.inc` ×2, `v86`) |
| Oracle / QEMU | Same I/O class as `enable_irq` — EOI ordering not host-observable |
| Verdict | **DEFER** with `enable_irq` |

### `unpack` — DEFER

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + `unpack.p` (~32 KiB prob buffer) |
| Boundary | `stdcall unpack(packed, unpacked); ret 8` — KPCK/LZMA + E8/E9 |
| Sub-cuts | Nested decoder locals share `unpack.*` globals — **no meaningful smaller public leaf** |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Verdict | **DEFER** — excellent oracle, disproportionate size/state/blast for one Path B cut |

### `exFAT_find_lfn` — DEFER

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Contract | ESI path UTF-8 in/out; EDI direntry; CF + EAX error; EBP=`exFAT*`; stack callbacks `exFAT_notroot_first/next` |
| Callers | 1 |
| Oracle / soak | Partial (`--disk exfat`); heavy plugin state |
| Verdict | **DEFER** — FS plugin island; not selected solely for disk soak |

### `set_mouse_data` — SELECT

| Item | Finding |
|------|---------|
| Source | `kernel/hid/mousedrv.inc` ~200–268 |
| Flow | PE export `SetMouseData` ← PS/2/USB/COM PE drivers; composes Cut L accel; mutates HID globals; `wakeup_osloop` |
| In-kernel `call` | **0** (export-only fan-in) — production path is PE |
| Globals | `BTN_DOWN`, `MOUSE_X/Y`, `MOUSE_SCROLL_V/H`, `mouse_active`, `_display.width/height`, accel tunables, `osloop_nonperiodic_work` |
| Policy vs compose | Pure composition + deterministic clamps — not opaque driver policy |
| Oracle | **Excellent** — independent FASM-flow mirror over all observable state |
| Smoke risk | REG-003 class — **save/restore** live mouse globals in ABI smoke |
| Verdict | **SELECT** — strongest remaining evidence-quality Path B leaf |

---

## Ranked candidates (49 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`set_mouse_data`** | HID aggregator (L compose) | 0 in-kernel (PE live) | desktop mouse + QMP | **Excellent** | Med | **SELECT** |
| 2 | `enable_irq` | PIC/APIC IRQ unmask | 6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 3 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med–High | Defer with enable_irq |
| 4 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** | Defer — size/state |
| 5 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS island |
| 6 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop | Hard | **High** | Reject — too hot |
| 7 | `tcp_mss` / `ntfs_restore_usa_frs` / `mutex_init` | thin | varies | — | Good | Low | Reject — substance bar |
| 8 | `strchr` / `strnlen` | export-only | 0 kernel | — | Good | Low | Reject — export-only |

### Why #1 wins

* Post-CE, GUI clientbox Path B novelty is spent; strongest next leaf with a **strong independent oracle** is the HID aggregator that composes Cut L.
* Deterministic global mutations are oracle-friendly (unlike IRQ I/O masks).
* Manageable ~68-line body; no alloc/mutex/IRQ; accel inlined from existing Rust leaf (reloc-free).
* REG-003 lesson applied: smoke saves/restores live HID globals.
* Does not reopen Cuts CC/CD/CE; does not claim Path A.

### Why alternatives lose

* `enable_irq` / `irq_eoi`: interrupt I/O without a QEMU-visible mask/EOI oracle.
* `unpack`: strongest FASM reduction but ~32KB `unpack.p` + LZMA is not one safe cut.
* `exFAT_find_lfn`: plugin island with stack callbacks + CF/EBP.
* Thin / export-only rejects fail the substance bar.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `set_mouse_data` |
| **Source** | [`kernel/hid/mousedrv.inc:200–268`](../../kernel/hid/mousedrv.inc) |
| **Subsystem** | HID / mouse input |
| **Stage** | Stage 2 / Stage 5 soft HID deepen (Cut L follow-on) |
| **Path** | B (Path A REJECTED) |
| **Purpose** | Aggregate button/motion/scroll into HID globals; accelerate relative deltas; wake osloop |

### Callers

| Site | Context |
|------|---------|
| PE export `SetMouseData` | External PS/2 / USB / COM mouse drivers |
| (no in-kernel `call`) | Syscall mouse paths mutate coords directly |

---

## Legacy ABI (locked from FASM)

```text
set_mouse_data stdcall(BtnState, XMoving, YMoving, VScroll, HScroll) → void; ret 20
uses: ECX, EDX preserved (FASM `uses ecx edx`)
in:   stack args as above
      reads [_display.width], [_display.height]
      reads [mouse_delay], [mouse_speed_factor] (via mouse_acceleration)
      reads/writes MOUSE_X, MOUSE_Y, MOUSE_SCROLL_*, BTN_DOWN, mouse_active
      writes osloop_nonperiodic_work via wakeup_osloop
out:  mutated HID globals; always mouse_active=1 and osloop wake
flags: unused by callers (internal jns/jl only)
DF: unused
interrupt state: untouched
```

### Semantics (summary)

1. `BTN_DOWN = BtnState & 0x3FFFFFFF`.
2. X: absolute (`BtnState&0x80000000`) → `(XMoving * width) >> 15`; else if nonzero relative → accel + `MOUSE_X`, clamp ≥0; clamp to `width-1`.
3. Y: absolute (`BtnState&0x40000000`) → `(YMoving * height) >> 15`; else if nonzero relative → accel(`-YMoving`) + `MOUSE_Y`, clamp ≥0; clamp to `height-1`.
4. Nonzero `VScroll`/`HScroll`: add to scroll words; `bts` BTN_DOWN bits 15 / 23.
5. `mouse_active = 1`; `osloop_nonperiodic_work = 1`.

---

## Rust ABI

```text
stdcall rust_set_mouse_data(btn, x, y, vscroll, hscroll, ctx) → void; ret 24
ctx → SetMouseDataCtx {
  mouse_x, mouse_y, scroll_h, scroll_v,   // *mut u16
  btn_down, mouse_active,                 // *mut u32
  display_width, display_height,          // u32
  mouse_delay, mouse_speed_factor,        // u32 (low bytes used)
  osloop_nonperiodic_work                 // *mut u32
}
```

Accel is **inlined** via `mouse_acceleration` (Cut L) — no cross-blob call. Blob stays reloc-free.

### Trampoline / stack ownership

```text
set_mouse_data:                    ; stdcall ret 20
  push ecx edx
  sub esp, sizeof.ctx / fill ctx from globals
  stdcall rust_set_mouse_data, btn, x, y, vs, hs, esp
  ; callee cleans 24 — NO add esp for those args (REG-009)
  add esp, sizeof.ctx              ; local frame only
  pop edx ecx
  ret 20
```

### Production gate

`USE_RUST_SET_MOUSE_DATA = 1` in `kernel/hid/mousedrv.inc` (independent of `USE_RUST_MOUSE_ACCELERATION`).

### Oracle / tests / QEMU

| Item | Plan |
|------|------|
| Oracle | Independent FASM-flow mirror `fasm_oracle_set_mouse_data` |
| PRNG | seed `0x534D4454` (`'SMDT'`), ≥50_000 cases |
| Host | focused mouse aggregator tests + full `kolibri_utils` suite |
| ABI smoke | save/restore live HID globals; relative/absolute/scroll vectors; ECX/EDX canaries |
| QEMU | OFF then ON desktop; A/B; ON ×3; HID soak (QMP mouse events after desktop) |

### Memory

Raised `TMP_STACK_TOP` `0x008D000` → `0x008D800` (+2 KiB) after CF blob+smoke
failed the early-stack memmap assert (`$-OS_BASE+PAGE_SIZE = 0x8D243`).
Documented in `const.inc` / fixed-addresses / memory-model. Gap to `sys_proc`
@ `0x8E000` remains (`0x800`).

### Rollback

`USE_RUST_SET_MOUSE_DATA = 0` → original FASM body.
