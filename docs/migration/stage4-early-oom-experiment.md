# Stage-4 Early-OOM Observation Experiment

**Date:** 2026-08-13  
**Status:** research/validation only — **not** a migration cut  
**Parent tooling:** [`stage4-allocator-soak-tooling.md`](stage4-allocator-soak-tooling.md)  
**Inventory:** remains **99 / 135**  
**Decision:** **EARLY-OOM SAFETY BOUNDARY PROVEN**

---

## 1. Question

Can the real FASM `alloc_page` early-OOM path (`pages_free <= 1` at entry → EAX=0,
force `pages_free=1`, no bitmap/`page_start` mutation) be driven safely and
reproducibly in a disposable CoW guest under QEMU?

---

## 2. Exact early-OOM semantics (FASM — unchanged)

Source: `kernel/core/memory.inc` `alloc_page`.

| Step | Behavior |
|------|----------|
| Entry | `cmp [pg_data.pages_free], 1` / `jle .out_of_memory` |
| Early OOM | `mov [pages_free], 1`; `xor eax,eax`; **no** `btr`; **no** `page_start` write |
| Success path | `dec pages_free`; if result 0 → same OOM stub (forces 1); else `btr` + update `page_start` |
| Scan miss | Range `[page_start, page_end)` has no free bit while `pages_free` may still be `>= 2` → EAX=0, **`pages_free` unchanged** |

Oracle: `pgbm_oom_pages_free_le_1` (host) matches the early path.  
`alloc_pages` fail path does **not** force `pages_free=1`.

---

## 3. Physical memory budget (measured)

| Quantity | Value |
|----------|-------|
| QEMU `-m` | 256 MiB |
| `pages_count` | 65407 |
| Firstapp-era `pages_free` (live xp) | ~63470–64352 |
| Post-desktop `pages_free` | ~48600–49000 |
| Bitmap window `page_end - page_start` | ~7952 bytes ≈ 63616 bits |

**Implication:** Post-desktop ~48.9k free is **not** the firstapp pressure budget.
A PE driver loaded as firstapp sees ~64k free. Retaining 50k leaves ~13k free
(proven: `B DONE 0xC350` with probe still succeeding).

AllocPage returns **physical** addresses only; the disposable driver does **not**
map pages into a process VA. Address-space growth is **not** the bottleneck.

---

## 4. Experiment design

| Piece | Path |
|-------|------|
| Driver | `tools/allocsoak/asoakdrv_oom.asm` |
| Limits | `tools/allocsoak/oom_limits.inc` (generated) |
| Host runner | `scripts/qmp_early_oom_experiment.py` |
| Seed | `0x5047424D` (`PGBM`) |

Protocol:

1. **A** — baseline Delay + marker  
2. **B** — retain until `ledger_count == MAX_RETAIN` (net outstanding, not op count)  
3. On AllocPage EAX=0 — **scan-miss rewind**: FreePage oldest ledger entry (may lower
   `page_start`), retry once  
4. Cap rewinds at 4096 → `ALLOCSOK SCANMISS CAP` (not early-OOM)  
5. At most one classified OOM HIT if EAX=0 without cap  
6. Hold 2s for host xp → free-all → AllocPage prove recovery → PASS  

Safety: 0 RESET required; Delay/ChangeTask yields; clean FreePage recovery.

---

## 5. Progressive pressure results

| Retain ceiling | Class | Host min live `pages_free` | Notes |
|----------------|-------|----------------------------|-------|
| 2048…50000 | CEILING_BLOCKED | (earlier runs) | 50000 retain succeeded; ~13k free left |
| 60000 | CEILING_BLOCKED | 3474 | Still allocatable |
| 63000 | CEILING_BLOCKED | **471** | Closest host sample to floor |
| 64000–65200 | CEILING_BLOCKED / prior false OOM HIT | — | Op-count vs net-retain bug fixed |
| **64300** | **SCANMISS_CAP** ×1+×3 | 10223–12271 during those boots | Rewind wall before `pages_free<=1` |

Canonical confirmation (ret=64300):

- Markers: `… SCANMISS CAP` → `OOM BLOCKED` → `RECOVER` → `PASS`
- STAT: `flags` includes scanmiss-cap bit; `scanmiss=0x1000` (4096); `pressure_ok≈0xF7EE` (~63470 net retained)
- QEMU resets: **0** on all runs
- Recovery: post-desktop `pages_free` restored (~48–49k); subsequent AllocPage OK

---

## 6. Why early-OOM was not observed

1. **Scan miss precedes early OOM.** Sequential AllocPage advances `page_start`.
   When the searchable range is exhausted, EAX=0 while `pages_free` can remain
   hundreds–thousands (counter still counts bits not found from the current cursor,
   and/or bits outside the active window).
2. **Oldest-page rewind does not drain the counter.** Free+realloc of ledger[0]
   restores findability of *held* pages but does not force `pages_free` to 1;
   after 4096 rewinds the driver hits the safety cap.
3. **Host undersamples the nadir** during fast guest loops; best reliable live
   minimum across the campaign was **471** (not ≤1). No run captured
   `pages_free<=1` concurrent with a verified early-OOM probe.

Therefore: labeling any EAX=0 as early-OOM would be **incorrect**. The oracle
still defines early-OOM; this CoW/PE retain harness cannot safely reach it.

---

## 7. Classification (user taxonomy)

**C — Cannot approach `pages_free<=1` because another limit is reached first**
(scan-miss / `page_start` wall), with elements of **B** (closest host sample 471).

Not **D** (instability): all high-pressure runs recovered with 0 RESET/shutdown.

**Final decision: EARLY-OOM SAFETY BOUNDARY PROVEN**

Production soak must **not** require live `pages_free<=1` as a gate criterion
under AllocPage-only PE retain. Host oracle + scan-miss distinction remain valid.

---

## 8. Artifacts

Under `dev_build/allocsoak/`:

- `early-oom-summary.json`
- `early-oom-step1-ret64300.json`
- `early-oom-repeat{1,2,3}-ret64300.json`
- prior progressive `early-oom-step*-ret*.json`
- `early-oom-recipe.json`

---

## 9. Explicit non-claims

- No production allocator edits  
- No Rust gates / Cut CT  
- Early-OOM leaf semantics remain as FASM + oracle describe  
- `release_pages` vs `free_page` cursor A/B **not** solved here  
