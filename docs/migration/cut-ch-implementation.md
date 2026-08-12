# Cut CH Implementation — `rebase_coff`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-ch-plan.md`](cut-ch-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CH** |
| FASM symbol | `rebase_coff` |
| Source | [`kernel/core/dll.inc`](../../kernel/core/dll.inc) |
| Callers | `load_library` when usermode base ≠ preferred |
| Rust symbol | `rust_rebase_coff` |
| Pure helper | `kolibri_utils::rebase_coff` / `rebase_coff_buffered` |
| Subsystem | PE/COFF preferred-base DIR32 rebase |
| Stage | Stage 8 PE loader foothold (after Y/AT/BK/BU/CG) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — PE Y+AT+BK+BU+CG+CH leaves do not establish Rust-owned PE loader.

Selected `rebase_coff` over `enable_irq` (poor I/O oracle — **REJECT**), `irq_eoi` (same class), `unpack` (~32KB `unpack.p` + LZMA), `exFAT_find_lfn` (FS plugin island), `blit_32` (LFB hot-path blast), and `memmove` (high fan-out blast) after fresh post-CG audit. Y-mutate ban stretch is stale with AT+BK+BU+CG complete.

Memory: end `.bss` @ `OS_BASE+0x8C783`; assert needs `0x8D783 < TMP_STACK_TOP`. Existing **`TMP_STACK_TOP = 0x008D800`** unchanged (do not lower).

---

## Legacy ABI

```text
rebase_coff stdcall(coff, sym, delta) → void; ret 12
  edx = delta for entire walk
  walk nSections from coff+20 (FASM do-while; Rust while-guard on 0)
  for each reloc: Type==6 only
    eax = reloc.VA + sec.VA
    add [eax + edx], edx     ; patch at VA+secVA+delta; addend = delta
  sym: UNUSED (ABI parity with load_library)
preserves: EBX, ESI (explicit uses); EDI/EBP untouched by body
clobbers: EAX, ECX, EDX
flags / IF: unused / untouched
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_rebase_coff` |
| Blob | **203** bytes, **0 relocations** |
| SHA-256 | `4aa5ae57e774e2c7f3b84c120ef04ee2711b74d2f01a521835a049b588e76c04` |
| Trampoline | `proc … uses ebx ecx edx esi edi ebp` → `stdcall rust_rebase_coff`; outer `ret` cleans caller |
| Gate | `USE_RUST_REBASE_COFF` (prod 1) |
| Rust ABI | `stdcall rust_rebase_coff(coff, sym, delta); ret 12` |
| vs Cut Y | Y patches DIR32/REL32 with **symbol Value**; CH is Type 6 only, addend = **delta** |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow arena mirror (`rebase_coff_oracle`) |
| PRNG seed | `0x52424346` (`'RBCF'`) |
| PRNG cases | 50,000 |
| Cut CH host tests | focused `rbcf_*` **9/9 PASS** |
| Full host suite | **694/694 PASS** |
| ABI smoke | **PASS** — marker `'RBCF'` (gated `if USE_RUST_REBASE_COFF`; hang=`DEAD0C61` on fail); synthetic COFF |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_REBASE_COFF=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_REBASE_COFF=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-174909.img`

---

## Subsystem soak

Desktop boot loads PE drivers / libraries through `load_library`. When usermode base ≠ preferred, `rebase_coff` patches DIR32 sites. ON desktop reached with `query-status: running`, non-black=779380, resets=0, pixel-identical to OFF. ABI smoke covers Type 6 delta-add / Type≠6 skip / register canaries / unused-sym before desktop.

---

## Regressions

None this cut.

Applied prior lessons: FASM `stdcall` only (no double cleanup); REG-001/011 preserve EBX/ESI/EDI/EBP; QMP `RESET` checked; smoke uses valid hex fail marker (`DEAD0C61`).

---

## Rollback

```text
USE_RUST_REBASE_COFF = 0
```

in `kernel/core/dll.inc` (or `enabled = false` for Cut CH in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/rebase_coff.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_rebase_coff` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/rebase_coff.inc` | Blob embed + ABI smoke |
| `kernel/core/dll.inc` | Gate + trampoline + legacy body |
| `kernel/kernel32.inc` | Include |
| `kernel/kernel.asm` | Smoke call |
| `project/build.toml` | Blob + migration CH |
| `docs/migration/cut-ch-plan.md` | Plan |
| `docs/migration/migration-todo.md` | Inventory 89/135 |
| `docs/migration/migration-plan.md` | Cut CH entry |

---

## Stop

**Cut CH complete. Do not start Cut CI.**
