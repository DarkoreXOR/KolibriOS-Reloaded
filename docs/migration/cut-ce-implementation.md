# Cut CE Implementation — `window._.set_window_clientbox`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-ce-plan.md`](cut-ce-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CE** |
| FASM symbol | `window._.set_window_clientbox` |
| Source | [`kernel/gui/window.inc`](../../kernel/gui/window.inc) |
| Callers | 3× (maximize layout; state change after Cut S; `sys_set_window`) |
| Rust symbol | `rust_set_window_clientbox` |
| Pure helper | `kolibri_utils::set_window_clientbox` / `SetWindowClientboxResult` |
| Subsystem | GUI / window clientbox policy |
| Stage | Stage 2 / Stage 7 soft foothold (Cut S follow-on) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — GUI S+CE are policy leaves; window server / draw / invalidate remain FASM.

Selected `window._.set_window_clientbox` over `enable_irq` (poor I/O oracle), `unpack` (~32KB `unpack.p` + LZMA), and IRQ/HID deepeners after fresh post-CD audit.

Also required: raise `TMP_STACK_TOP` `0x008CC00` → `0x008D000` (+1 KiB) so cumulative Stage-2 blob/smoke growth still fits the early-stack memmap assert (`data32.inc`). Gap to `sys_proc` @ `0x8E000` remains.

---

## Legacy ABI

```text
window._.set_window_clientbox() → void; plain ret
in:  EDI → WDATA*
     reads [_skinh]; reads/writes window_topleft[]
out: always window_topleft[3].top = window_topleft[4].top = [_skinh]
     mutates WDATA.clientbox.{left,top,width,height}
preserves: EAX, ECX, EDI
flags: unused
```

Client-relative (`fl_wstyle & 0x20`): insets from `window_topleft[style&0x0F]` with Leency `width/height + 1`. Whole-window: `clientbox = {0,0,box.width,box.height}`.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_set_window_clientbox` |
| Blob | **83** bytes, **0 relocations** |
| SHA-256 | `1363efb5a32ec941f1010f8da4e868d590e3935e94c8c8e98aebaa677c150b1e` |
| Trampoline | `push eax/ecx/edi`; FASM **`stdcall rust_set_window_clientbox, edi, [_skinh], window_topleft`** (no `add esp`); pops |
| Gate | `USE_RUST_SET_WINDOW_CLIENTBOX` (prod 1) |
| Rust ABI | `stdcall rust_set_window_clientbox(wdata, skinh, window_topleft); ret 12` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_set_window_clientbox`) |
| PRNG seed | `0x53574342` (`'SWCB'`) |
| PRNG cases | 50,000 |
| Cut CE host tests | focused `swcb_*` **9/9 PASS** |
| Full host suite | **667/667 PASS** |
| ABI smoke | **PASS** — marker `'SWCB'` (gated `if USE_RUST_SET_WINDOW_CLIENTBOX`; hang=`DEAD0CE0` on fail) |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_SET_WINDOW_CLIENTBOX=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_SET_WINDOW_CLIENTBOX=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-161542.img`

---

## Subsystem soak

Desktop GUI path (`sys_set_window` / maximize / state change → `set_window_clientbox`) exercised on every successful desktop smoke. OFF/ON non-black pixel counts match. ABI smoke vectors cover whole-window + style0 + style3/skinh before desktop.

---

## Regressions

None new. Applied prior lessons: FASM `stdcall` only (no double cleanup); EAX/ECX/EDI preserve; QMP `RESET` checked (`resets=0`).

---

## Rollback

```text
USE_RUST_SET_WINDOW_CLIENTBOX = 0
```

in `kernel/gui/window.inc` (or `enabled = false` for Cut CE in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/window.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_set_window_clientbox` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/set_window_clientbox.inc` | Blob embed + ABI smoke |
| `kernel/gui/window.inc` | Gate + trampoline + legacy body |
| `kernel/kernel32.inc` | Include |
| `kernel/kernel.asm` | Smoke call |
| `kernel/const.inc` | `TMP_STACK_TOP` +0x400 |
| `project/build.toml` | Blob + migration CE |
| `docs/compatibility/fixed-addresses.md` | TMP_STACK address |
| `docs/architecture/memory-model.md` | TMP_STACK note |

---

## Stop

**Cut CE complete. Do not start Cut CF in this task.**
