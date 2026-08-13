# Stage-4 Allocator Soak Tooling

**Date:** 2026-08-13  
**Status:** host/test tooling — **not** a migration cut  
**Parent:** [`stage4-allocator-soak-design.md`](stage4-allocator-soak-design.md)  
**Inventory:** remains **99 / 135** — no production allocator gate  
**Driver tooling decision:** **ALLOCATOR DRIVER TOOLING — PARTIAL** (OOM safety-ceiling BLOCKED; see §8)

This document describes the implemented host QMP sampler, PE export ABI, disposable
PE soak driver, CoW recipe, marker protocol, and artifact layout.

---

## 1. What was implemented

| Piece | Path | Role |
|-------|------|------|
| FAS parser | `scripts/fasm_symbols.py` | Parse FASM `-s` `.fas` dumps |
| Symbol→PA | `scripts/resolve_allocator_symbols.py` | `pages_free`, `page_start`, `sys_pgmap`, `msg_board_*` |
| Assemble `-s` | `scripts/assemble_kernel.py` | Emits `kernel/bin/kernel.fas` |
| Sampler | `scripts/qmp_allocator_soak.py` | Boot/QMP/desktop/xp/board scrape/JSON |
| PE driver | `tools/allocsoak/asoakdrv.asm` | Direct `AllocPage`/`FreePage`/`AllocPages` |
| Build PE | `scripts/build_asoakdrv.py` | → `dev_build/allocsoak/ASOAKDRV` |
| MENUET loader | `tools/allocsoak/allocsoak.asm` | Syscall 68.21 load + start LAUNCHER |
| Build loader | `scripts/build_allocsoak.py` | → `dev_build/allocsoak/ALLOCSOK` |
| CoW recipe | `scripts/prepare_allocsoak_image.py` | Image + put + firstapp patch |

**Production changes: NONE** (no `USE_RUST_ALLOC_*`, no allocator body edits).

---

## 2. PE export ABI (verified from FASM sources)

Sources: `kernel/core/exports.inc`, `kernel/core/memory.inc` (`alloc_page` /
`alloc_pages` / `free_page`), `kernel/core/peload.inc` (`map_PE` /
`load_pe_driver`), `docs/compatibility/driver-binary-contract.md`.

| Export | Impl label | Convention | Args | Return | Failure |
|--------|------------|------------|------|--------|---------|
| `AllocPage` | `alloc_page` | plain `call` / `ret` (comment: gcc ABI) | none | **EAX** = physical page address (4 KiB aligned) | **EAX = 0**; early OOM forces `pages_free = 1` without bitmap/`page_start` mutation |
| `AllocPages` | `alloc_pages` | **stdcall** (`ret 4`) | **count** = requested page count | **EAX** = physical base of run | **EAX = 0** |
| `FreePage` | `free_page` | plain `call` / `ret`; **EAX** in = physical page address | EAX | void | double-free: `BTS` polarity → `pages_free` does **not** inflate |

### AllocPages semantics (LOCAL FACT)

1. Count is converted: `n = (count + 7) >> 3` (units of **8-page / 0xFF-byte** runs).
2. Allocates `n * 8` pages; clears that many contiguous `0xFF` bytes in `sys_pgmap`.
3. **Does not** update `page_start`.
4. OOM / no-run path returns 0 without claiming to force `pages_free = 1` (unlike `alloc_page`).

### FreePage / page_start

- `free_page` may **lower** `page_start` toward the freed bit’s dword.
- `alloc_pages` does not touch `page_start`.
- `release_pages` (not exercised by the main soak PE) — **DONE** via
  `asoakdrv_ab.asm` / [`stage4-release-pages-ab.md`](stage4-release-pages-ab.md)

### Load / entry ABI

| Item | Fact |
|------|------|
| Load | Syscall **68.21** → `load_pe_driver(path, cmdline)` |
| Entry args | `push cmdline; push DRV_ENTRY(=1); call START` then **three pops** (cdecl) |
| Success return | Non-zero `SRV*` (normal drivers via `RegService`) |
| This soak driver | Runs soak in `START`, returns **0** so `fail_init` frees the image (disposable) |
| Imports | Module name `'KERNEL'`; resolved against `__exports` |
| **Fixups** | **Mandatory** — Kolibri maps the PE away from FASM default `ImageBase=0x400000`. Without `data fixups`, `call [Import]` uses stale `0x40xxxx` and hangs |

Bring-up proof: stub PE with imports but no calls PASSes; same PE calling imports without fixups HANGS; with `data fixups`, `AllocPage`/`FreePage`/`SysMsgBoardStr` work (`ALLOCSOK AP-OK` / full soak markers).

Usable by arbitrary user MENUET apps? **No** — only PE drivers mapped into kernel space via 68.21 / `load_PE`. Hidden init beyond a successful PE map + resolved imports: **none** for AllocPage/FreePage/AllocPages.

---

## 3. Driver design

| Item | Value |
|------|-------|
| Source | `tools/allocsoak/asoakdrv.asm` |
| Binary | `dev_build/allocsoak/ASOAKDRV` |
| Loader | `tools/allocsoak/allocsoak.asm` → `ALLOCSOK` |
| Seed | `0x5047424D` (`PGBM`) |
| Ledger cap | `MAX_LEDGER = 2048` |
| Pressure | `PRESSURE_TARGET = 512` retained `AllocPage`s |
| OOM ceiling | `MAX_OOM_EXTRA = 256` additional attempts after pressure |
| Frag pattern | `FRAG_N = 64` singles; free odds; try `AllocPages(8)` |

Phases (guest):

1. **A** — `ALLOCSOK START` / `ALLOCSOK A` + Delay (host baseline window)
2. **DF** — allocate, free, free again (`ALLOCSOK DF`)
3. **B** — AllocPage hammer (`ALLOCSOK B` / `B DONE`) + Delay
4. **AP** — AllocPages table N∈{0,1,8,16,9,7} (`ALLOCSOK AP`)
5. **FRAG** — hole pattern + AllocPages(8) (`ALLOCSOK FRAG`)
6. **OOM** — bounded extra AllocPage (`ALLOCSOK OOM HIT` or `OOM BLOCKED`)
7. **RECOVER** — free ledger; AllocPage+FreePage prove usability
8. **PASS/FAIL** — `ALLOCSOK PASS` or `ALLOCSOK FAIL`

Independent guest ledger: phys PAs in `ledger[]`; never trusts `pages_free`.

---

## 4. Marker protocol

Compact ASCIIZ lines via `SysMsgBoardStr` → `msg_board_data` (host scrapes with QMP `xp`):

```text
ALLOCSOK START
ALLOCSOK A
ALLOCSOK DF
ALLOCSOK B
ALLOCSOK B DONE
ALLOCSOK AP
ALLOCSOK FRAG
ALLOCSOK OOM
ALLOCSOK OOM HIT | ALLOCSOK OOM BLOCKED
ALLOCSOK RECOVER
ALLOCSOK STAT <flags> <ap_ok> <apages_ok> <oom_ops>
ALLOCSOK PASS | ALLOCSOK FAIL
```

Do **not** use pixels as correctness proof.

---

## 5. Host / QMP correlation

```text
python scripts/prepare_allocsoak_image.py
python scripts/qmp_allocator_soak.py --wait 120 --artifact-name soak-driver-final1.json
```

Host ledger is independent: intended PE retain counts + boot `pages_free` series +
final baseline/recovery samples + guest marker classification.

| Correlate | How |
|-----------|-----|
| Pressure | Boot xp `pages_free` dip; guest `B DONE` |
| OOM | Guest `OOM HIT` / `OOM BLOCKED` (authoritative); host `pages_free<=1` secondary |
| AllocPages / frag | Markers `AP` / `FRAG` |
| Recovery | Post-desktop `pages_free` vs baseline; marker `RECOVER` + `PASS` |
| Phys PA | Guest-only in driver ledger; host sees bitmap digest / freelist deltas |

---

## 6. Safety limits

| Limit | Value | On hit |
|-------|-------|--------|
| Retained pages | 2048 | stop retaining / free overflow |
| OOM extra | 256 | `ALLOCSOK OOM BLOCKED` (not allocator FAIL) |
| Ops / runtime | Delay+ChangeTask yields; desktop must return | RESET/shutdown → sampler FAIL |
| Clean exit | START returns 0 | image freed by `load_pe_driver` |

Full freelist drain to `pages_free<=1` is **not** forced under the current ceiling
(~48k free at baseline) — classified **OOM BLOCKED**, not fabricated PASS.

---

## 7. Recipe / artifacts

`dev_build/allocsoak/`:

| File | Role |
|------|------|
| `ASOAKDRV` | Disposable PE binary |
| `ALLOCSOK` | MENUET loader |
| `recipe.json` | Image path, seed, ABI, pressure_target |
| `soak-driver-final{1,2,3}.json` | Repeat-run evidence |
| `allocator-soak.ppm` | Diagnostic screendump only |
| `KERNEL.MNT.patched` | Firstapp redirect host copy |

Firstapp patch is **idempotent** (`/sys/LAUNCHER` → `/sys/ALLOCSOK`).

---

## 8. Phase coverage (current)

| Phase | Status |
|-------|--------|
| A Baseline | PASS |
| B AllocPage hammer | PASS (guest ledger + host freelist dip) |
| C OOM | **BLOCKED** by safety ceiling (`ALLOCSOK OOM BLOCKED`) — not allocator failure |
| AllocPages table | PASS (guest executed; N=0.. cases recorded in driver) |
| Double-free | PASS (executed; legacy BTS polarity — observe, do not assume) |
| Fragmentation / reuse | PASS |
| Recovery | PASS (freelist returns; AllocPage works after free-all) |
| Stability / RESET | PASS ×3 repeats |
| `release_pages` vs `free_page` | **PROVEN** — [`stage4-release-pages-ab.md`](stage4-release-pages-ab.md) |

---

**Driver tooling decision:** **ALLOCATOR DRIVER TOOLING — PARTIAL** (see soak doc)  
**Early-OOM experiment decision:** **EARLY-OOM SAFETY BOUNDARY PROVEN** — see
[`stage4-early-oom-experiment.md`](stage4-early-oom-experiment.md)  
**Release/free A/B decision:** **RELEASE/FREE PAGE_START DIFFERENCE PROVEN** — see
[`stage4-release-pages-ab.md`](stage4-release-pages-ab.md)

Runner: `python scripts/qmp_release_free_ab.py --repeats 3`

---

## 10. Early-OOM experiment (bounded)

Runner: `python scripts/qmp_early_oom_experiment.py --single 64300 --repeats 3`

| Item | Result |
|------|--------|
| Physical retain | Safe to ~63k pages (firstapp freelist ~64k; no VA mapping) |
| Best host `pages_free` min | **471** (retain 63000 campaign) |
| Live `pages_free<=1` | **Never observed** |
| Blocker | `ALLOCSOK SCANMISS CAP` — `page_start` scan-miss wall before early-OOM |
| QEMU RESET | 0 across confirmation ×4 |
| Decision | **EARLY-OOM SAFETY BOUNDARY PROVEN** |

Do **not** use live `pages_free<=1` as a production soak gate under AllocPage-only
PE retain. Distinguish early-OOM (oracle) from scan-miss (EAX=0, counter unchanged).

---

## 11. Explicit non-claims

Passing this tooling does **not** mean:

* Stage-4 ownership is ready
* `alloc_page` migration is ready
* Rust owns the allocator
* Cut CT may start
* Full OOM (`pages_free<=1`) was proven live under AllocPage-only PE retain
