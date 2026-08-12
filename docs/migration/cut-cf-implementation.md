# Cut CF Implementation — `set_mouse_data`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-cf-plan.md`](cut-cf-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CF** |
| FASM symbol | `set_mouse_data` |
| Source | [`kernel/hid/mousedrv.inc`](../../kernel/hid/mousedrv.inc) |
| Callers | PE export `SetMouseData` (PS/2/USB/COM drivers); 0 in-kernel `call` |
| Rust symbol | `rust_set_mouse_data` |
| Pure helper | `kolibri_utils::set_mouse_data` / `MouseDataState` |
| Subsystem | HID / mouse input |
| Stage | Stage 2 / Stage 5 soft HID deepen (Cut L follow-on) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — HID L+BE+CF are leaves; PE mouse drivers / cursor draw remain FASM.

Selected `set_mouse_data` over `enable_irq` (poor I/O oracle — **REJECT**), `irq_eoi` (same class), `unpack` (~32KB `unpack.p` + LZMA), and `exFAT_find_lfn` (FS plugin island) after fresh post-CE audit.

Also required: raise `TMP_STACK_TOP` `0x008D000` → `0x008D800` (+2 KiB). Proof: end `.bss` @ `OS_BASE+0x8C243`; assert needs `0x8D243 < TMP_STACK_TOP`. Gap to `sys_proc` @ `0x8E000` remains (`0x800`).

---

## Legacy ABI

```text
set_mouse_data stdcall(BtnState, XMoving, YMoving, VScroll, HScroll) → void; ret 20
preserves: ECX, EDX (FASM `uses ecx edx`)
mutates: BTN_DOWN, MOUSE_X/Y, MOUSE_SCROLL_*, mouse_active, osloop_nonperiodic_work
reads: _display.width/height, mouse_delay, mouse_speed_factor
flags / IF: unused / untouched
```

Relative motion composes Cut L accel; absolute uses `(moving * dim) >> 15`; signed `jl` clamp; scroll `bts` bits 15/23.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_set_mouse_data` |
| Blob | **531** bytes, **0 relocations** |
| SHA-256 | `a2a4bbaea4cb5afea1c2f3f191b1e05e97cf623ca8a3be01da18bf6bf8a00532` |
| Trampoline | naked stdcall: build 44 B `SetMouseDataCtx` on stack; `stdcall rust_set_mouse_data, …, esp` (`ret 24`); `add esp,44` local only; `ret 20` |
| Gate | `USE_RUST_SET_MOUSE_DATA` (prod 1) |
| Rust ABI | `stdcall rust_set_mouse_data(btn,x,y,vs,hs,ctx); ret 24` |
| Accel | Inlined Cut L `mouse_acceleration` (no cross-blob reloc) |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_set_mouse_data` + independent accel oracle) |
| PRNG seed | `0x534D4454` (`'SMDT'`) |
| PRNG cases | 50,000 |
| Cut CF host tests | focused `smdt_*` **9/9 PASS** |
| Full host suite | **676/676 PASS** |
| ABI smoke | **PASS** — marker `'SMDT'` (gated `if USE_RUST_SET_MOUSE_DATA`; hang=`DEAD0CF0` on fail); save/restore live HID globals (REG-003) |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_SET_MOUSE_DATA=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_SET_MOUSE_DATA=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-165948.img` (post-REG-011)

---

## Subsystem soak

QMP `input-send-event` relative moves + left click after desktop; `query-status: running`, non-black=779380, resets=0. Exercises PE `SetMouseData` path when mouse driver is loaded. ABI smoke covers relative / absolute / scroll vectors before desktop.

---

## Regressions

**REG-010** (fixed this cut): trampoline arg offset omitted return-address (+4); smoke hung (`jmp $`) → black “running” desktop.

**REG-011** (fixed post-completion): trampoline clobbered **EBX/ESI/EDI/EBP**; PE `SetMouseData` drivers lost live state → frozen mouse cursor. Trampoline now preserves all six; smoke canaries them.

See [`regression-log.md`](regression-log.md).

Applied prior lessons: FASM `stdcall` only (no double cleanup); HID global save/restore in smoke; QMP `RESET` checked.

---

## Rollback

```text
USE_RUST_SET_MOUSE_DATA = 0
```

in `kernel/hid/mousedrv.inc` (or `enabled = false` for Cut CF in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/mouse.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_set_mouse_data` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/set_mouse_data.inc` | Blob embed + ABI smoke |
| `kernel/hid/mousedrv.inc` | Gate + trampoline + legacy body |
| `kernel/kernel32.inc` | Include |
| `kernel/kernel.asm` | Smoke call |
| `kernel/const.inc` | `TMP_STACK_TOP` +0x800 |
| `project/build.toml` | Blob + migration CF |
| `docs/compatibility/fixed-addresses.md` | TMP_STACK address |
| `docs/architecture/memory-model.md` | TMP_STACK note |
| `docs/migration/regression-log.md` | REG-010 |

---

## Stop

**Cut CF complete. Do not start Cut CG in this task.**
