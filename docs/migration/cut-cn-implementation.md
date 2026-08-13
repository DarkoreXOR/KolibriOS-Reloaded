# Cut CN Implementation — `strchr`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-cn-plan.md`](cut-cn-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CN** |
| FASM symbol | `strchr` |
| Source | [`kernel/core/string.inc`](../../kernel/core/string.inc) |
| Callers | **0 in-kernel**; PE export (`exports.inc`) |
| Rust symbol | `rust_strchr` |
| Pure helper | `kolibri_utils::string::strchr` |
| Subsystem | core/string forward character search (Stage-2 leaf) |
| Stage | Stage 2 / string leaf (complement to Cut BB `strrchr`) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — same post-CM vacuum as Cut CM; no pending symbol clears subsystem-ownership bar.

Selected `strchr` over `unpack` (Stage-2 LZMA architecture), `exFAT_find_lfn` (plugin/callback island), `blit_32` (LFB blast), `drawChar` (Stage 7), `tcp_mss` (thin TCP deepen), and IRQ leaves (I/O oracle). Post-CM `.bss` headroom (~61 B to assert) blocked separate `kernel/rust/strchr.inc` smoke iglobals — blob inlined in `string.inc` beside gate; **no in-kernel ABI smoke hook** (host `schrc_*` + export contract; REG-012 pack unchanged).

**Memory:** End `.bss` remains **`OS_BASE+0x8CFC3`**; assert **`0x8DFC3 < 0x8E000`** PASS. `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` unchanged (`0x8E000` / `0x8E000` / `0x90000`).

---

## Legacy ABI

```text
strchr  stdcall(s, c)
  in:  stack — s (ptr), c (int; low byte via scasb)
  out: EAX = ptr to first c or NULL
  preserves: EDI (push/pop in FASM body)
  clobbers: ECX, EDX, flags
  DF: cld at entry; unchanged at return
  stack: ret 8
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_strchr` (embedded in `string.inc` when gated) |
| Blob | **37** bytes, **0 relocations** |
| SHA-256 | `e01986525b60bba7fc73747c7dddf3bf3f3e1832296e7f3139f3d407f9b5914b` |
| Epilogue | `ret 8` |
| Trampoline | `stdcall rust_strchr`; `cld`; no double stack cleanup |
| Gate | `USE_RUST_STRCHR` (prod **1**) |
| Rust ABI | `stdcall rust_strchr(s, c); ret 8` → EAX = ptr/`0` |

Production Rust uses a compact forward scan (semantically equivalent to the chunk-doubling FASM body). Differential oracle mirrors FASM chunk flow independently.

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent chunk-growth FASM-flow oracle vs Rust body |
| PRNG seed | `0x53434852` (`'SCHR'`) |
| PRNG cases | 50,000 |
| Cut CN host tests | focused `schrc_*` **6/6 PASS** |
| Full host suite | **751/751 PASS** |
| ABI smoke | **N/A in-kernel** (REG-012 headroom); host `schrc_*` + trampoline contract |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_STRCHR=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_STRCHR=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260813-102638.img`

---

## Subsystem soak

PE-export class — **no in-kernel callers**. Desktop gate ON/OFF/A/B parity is the production soak (same evidence class as export-adjacent string leaves). Host oracle is primary correctness evidence.

---

## Regressions

None.

---

## Rollback

```text
USE_RUST_STRCHR = 0
```

in `kernel/core/string.inc` (or `enabled = false` for Cut CN in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/string.rs` | `strchr` + chunk oracle + `schrc_*` tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_strchr` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | module note |
| `kernel/core/string.inc` | gate + trampoline + inline blob embed |
| `project/build.toml` | blob + migration CN |
| `docs/migration/cut-cn-*.md` | plan + impl |
| `docs/migration/migration-todo.md` / `migration-plan.md` | inventory |

---

## Inventory after Cut CN

`95 / 135` completed; `40` pending; `95 / 95` production gates enabled.
