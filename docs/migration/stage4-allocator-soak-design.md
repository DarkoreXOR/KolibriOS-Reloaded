# Stage-4 Physical Allocator QMP Soak Design

**Date:** 2026-08-13  
**Status:** design-only — **not** a migration cut  
**Parent docs:** [`stage4-ownership-design.md`](stage4-ownership-design.md),  
[`stage4-bitmap-writers.json`](stage4-bitmap-writers.json), [`cut-cs-plan.md`](cut-cs-plan.md)  
**Inventory:** remains **99 / 135**

> This document defines how a future Rust-owned **physical page bitmap**
> domain would be validated under QEMU **before** production ownership moves.
> It does **not** authorize Cut CT, `USE_RUST_ALLOC_*`, trampolines, or any
> change to `alloc_page` / `free_page` / `alloc_pages` / `release_pages`.

---

## 1. Current vs designed ownership

| State | CURRENT owner | DESIGNED future (not implemented) |
|-------|---------------|-----------------------------------|
| `sys_pgmap` | FASM | Rust sole **runtime** writer |
| `pg_data.pages_free` | FASM | Rust |
| `page_start` | FASM (`alloc_page` / `free_page` / boot) | Rust (same writers) |
| `page_end` | Boot `init_page_map` | Stay FASM boot |
| `map_page` / PTE / fault / CR3 | FASM | Stay FASM (outside first slice) |
| `release_pages` | FASM (PTE + bitmap) | FASM PTE/mutex/`invlpg` + Rust bitmap-release |

Path A remains **REJECTED today**. This soak is prerequisite evidence for a
future ownership cut, not the cut itself.

---

## 2. Observability audit (LOCAL FACT)

### 2.1 What exists today

| Capability | Source | Allocator usefulness |
|------------|--------|----------------------|
| QMP connect / `qmp_capabilities` | `scripts/qmp_desktop_smoke.py` | Control plane |
| `RESET` event count | same | Hard fail on boot-loop / triple-fault class |
| `query-status` | same | Must stay `running` |
| Screendump + non-black floor | same | Desktop reachability only — **not** allocator proof |
| `human-monitor-command` `xp /N xw PHYS` | `--xp` in `qmp_desktop_smoke.py` | **Read guest RAM** → can sample `pages_free` / bitmap if VAs→PA known |
| `input-send-event` | Cut CF soak | Launch/click apps for indirect pressure |
| `--disk` / `--bus` | `scripts/run_qemu.py` | FS/DMA buffer pressure |
| Syscall 16 `sysfn_getfreemem` | `kernel.asm` | Returns `pages_free << 2` (KiB free) to apps |
| Syscall 20 `sysfn_meminfo` | `sysfn_meminfo` in `memory.inc` | User buffer: `pages_count`, `pages_free`, `pages_faults`, heap stats |
| `pg_data` / `sys_pgmap` / `page_start` | `data32.inc` | Live globals; readable if host resolves PA |
| Serial console | Commented optional in `kernel.asm` | Not in default QEMU args |
| Alloc success/fail counters | **None** | Must be inferred or added later as **diagnostic-only** |

### 2.2 What cannot be observed without new tooling

| Need | Gap |
|------|-----|
| Periodic `pages_free` during stress | No soak script; only one-shot `--xp` |
| `page_start` / bitmap digest | No automated resolve of symbol→PA + dump |
| Deterministic near-OOM | No guest pressure agent that calls `AllocPage` in a loop |
| Alloc/free event counts | No kernel counters; host must maintain logical ledger |
| `release_pages` isolation | No harness that unmaps known ranges and checks bitmap-only effects |
| Page-fault classification | RESET only; no `#PF` tally export in default soak |

### 2.3 Read-only sampling principle

Prefer **QMP `xp` of known globals** and **syscall 16/20 from a guest test app**
over injecting allocator counters into production paths. Any future diagnostic
counters must be **gate-off by default** and must not change bitmap semantics.

---

## 3. Telemetry design (minimum)

| Signal | Origin | Export | Sample | Perturbs allocator? |
|--------|--------|--------|--------|---------------------|
| `pages_free` | `pg_data.pages_free` | QMP `xp` dword **or** syscall 16 (`<<2` → divide by 4) | Before/after each phase | Read-only |
| `pages_count` | `pg_data.pages_count` | syscall 20 / `xp` | Baseline once | Read-only |
| `pages_faults` | `pg_data.pages_faults` | syscall 20 / `xp` | Baseline + end | Read-only (counter may bump from workload) |
| `page_start` offset | `page_start - sys_pgmap` | QMP `xp` of both symbols | After free-heavy phases | Read-only |
| Bitmap digest | `sys_pgmap[0..N)` | QMP multi-`xp` or chunked dump → FNV | Sparse (phase boundaries) | Read-only; large (128 KiB full map) — prefer sampled windows + free-count check |
| Desktop / RESET | QMP | Existing smoke | Continuous | N/A |
| Host logical free | Soak ledger | Host JSON | Every op | N/A — **independent of kernel counter** |

**Do not trust `pages_free` alone as oracle.** Host ledger:

```text
logical_free ≈ pages_count_managed − Σ(outstanding host-tracked allocations)
```

For natural workloads (app launch), the ledger is approximate (kernel also
allocates for caches). For a **dedicated AllocPage hammer** (PE export or
diagnostic), the ledger can be exact.

**Optional later (not required to start FASM baseline soak):** gated diagnostic
tallies (`alloc_ok`, `alloc_fail`, `free_ok`) behind a non-production flag —
only if `xp`+syscall sampling proves insufficient.

---

## 4. Pressure workload design (FASM allocator)

Workloads must exercise **current FASM** `alloc_page` / `alloc_pages` /
`free_page` / `release_pages` without changing them.

### 4.1 Mechanisms (ranked)

| Rank | Mechanism | Pros | Cons |
|------|-----------|------|------|
| 1 | **Guest PE/driver or MENUET app** calling exported `AllocPage`/`FreePage`/`AllocPages` in a seeded loop | Exact ledger; can hit OOM | Needs disposable CoW-installed test binary (not production kernel change) |
| 2 | Repeated GUI app launch/close via QMP input | Uses existing paths | Indirect; hard near-OOM; noisy ledger |
| 3 | `--disk` browse + AHCI | Cache/DMA pages | Weak control of counts |
| 4 | Touch large reserved user mappings (fault→`alloc_page`) | Hits fault path | Couples PTE/fault domain |

**Preferred primary:** (1) disposable test app on CoW image.  
**Secondary soak:** (2)+(3) for stability after pressure.

### 4.2 Phases

| Phase | Goal | Actions | Samples |
|-------|------|---------|---------|
| **A — Baseline** | Boot health | Boot → desktop; record `pages_free`, `pages_count`, `pages_faults`, digest window | T0 |
| **B — Progressive pressure** | Free pages decrease | Deterministic AllocPage batches (seed `PGBM`); or staged app launches | After each batch |
| **C — Near OOM** | Hit legacy floor | Continue until `alloc_page` returns 0 with `pages_free<=1` semantics | On each failure |
| **D — Recovery** | Free restores usability | FreePage all ledger pages; confirm `pages_free` rises; AllocPage succeeds again | Pre/post free |
| **E — Fragmentation/reuse** | Cursor + reuse | Alternate single free holes + `alloc_pages` FF-run requests; free; realloc same sizes | After pattern |
| **F — Stability** | No latent corruption | Close hammer; desktop interaction; optional `--disk`; screendump; RESET=0 | Final |

Seed: reuse `'PGBM'` (`0x5047_424D`) for guest hammer PRNG so host oracle and soak share lineage.

---

## 5. OOM semantics (production FASM — preserve)

From `kernel/core/memory.inc` `alloc_page` (LOCAL FACT):

| Item | Behavior |
|------|----------|
| Entry guard | `cmp [pages_free], 1` / `jle .out_of_memory` |
| Return | EAX = **0** |
| `pages_free` on OOM path | Forced to **1** (not 0) |
| Bitmap / `page_start` on early OOM | **Unchanged** |
| Scan miss (`pages_free>=2` but no free bit) | EAX=0; **`pages_free` unchanged** (distinct from OOM path) |
| Flags | Restored via `popfd` (CLI window only) |
| Recoverable? | **Yes** if later `free_page` raises free bits and `pages_free` |
| Caller behavior | Callers typically `test eax,eax` / jz fail (fault handler, heap, FB, …) |
| Scheduler | No direct OOM→sched coupling in the leaf |

Soak must distinguish **OOM path** vs **scan-miss path** when forcing failures.

`alloc_pages` OOM/fail: EAX=0; **no** force-`pages_free=1`; requires `pages_free>=9` and FF-run availability.

---

## 6. Free / recovery invariants

| Invariant | Check |
|-----------|-------|
| Ledger match (hammer mode) | `pages_free_observed == pages_free_at_T0 − outstanding + known_adjustments` within documented noise floor |
| Realloc after free | Pages previously freed can be allocated again (PA may differ — only required for exact hammer if tracking bit indices) |
| Double-free | Second FreePage on same PA: `pages_free` does **not** increase (BTS polarity) |
| No RESET | Throughout |
| Cursor after free | Freeing a page below `page_start` **lowers** `page_start` (dword align) |
| Cursor after release_pages | Bulk release **must not** change `page_start` (legacy quirk) |

Independent check (strong): dump bitmap window + popcount free bits ≈ `pages_free` for the managed range (allow known reserved low pages).

---

## 7. `alloc_pages` soak cases

| Case | Expectation |
|------|-------------|
| N=8 (one FF byte) | Success when run exists; charge 8 to `pages_free` |
| N=16 (two FF bytes) | Success; charge 16 |
| N=0 | Fail (need_bytes=0 never completes) |
| `pages_free < 9` | Fail |
| Fragmented map (no FF byte) | Fail even if many free single bits |
| After free reconstituting 0xFF bytes | Success again |
| `page_start` | Unchanged across successful `alloc_pages` |

---

## 8. `release_pages` split validation

```text
release_pages
  ├── FASM: mutex + PTE xchg + invlpg + VA walk
  └── future Rust: bitmap BTS + pages_free  [MUST NOT store page_start]
```

**Observable invariants for the bitmap half (testable once split exists; for FASM baseline, observe whole function):**

1. Present PTEs → corresponding phys pages become free bits; `pages_free` increases by newly freed count only.
2. `page_start` **unchanged** across the call (contrast with equal number of `free_page` calls).
3. Absent PTEs → no bitmap change for those slots.
4. Mutex / TLB correctness remain FASM responsibility — soak treats RESET/hang as orchestration failure, not bitmap mismatch.

**Baseline (pre-split):** run process teardown / `release_pages` callers and assert (2)+(1) via `xp`.  
**Post-split (future):** same asserts; additionally host oracle can replay phys-page list through `bitmap_release_phys_pages`.

---

## 9. `page_start` semantics

| Event | `page_start` |
|-------|----------------|
| Boot `init_page_map` | Set to first free dword |
| `alloc_page` success | Set to dword containing allocated bit |
| `free_page` | May move **down** to dword of freed bit |
| `alloc_pages` | **Unchanged** |
| `release_pages` | **Unchanged** (local EBX discarded) |

Soak must record `page_start` at phase boundaries and flag violations of the table above.

---

## 10. QMP protocol (host control flow)

```text
1.  build + prepare_image (CoW; optional install pressure app)
2.  launch QEMU (existing run_qemu / qmp port)
3.  wait desktop (screendump floor + RESET=0) — stability gate only
4.  resolve symbol PAs (pg_data.pages_free, page_start, sys_pgmap) once
5.  Phase A sample → JSON artifact
6.  trigger workload (QMP input and/or guest app already auto-running)
7.  Phase B–E sample loop (poll interval e.g. 250–1000 ms; also on barriers)
8.  detect OOM (EAX=0 from hammer log **or** pages_free<=1 + failed alloc marker)
9.  Phase D recovery
10. Phase F desktop + optional --disk
11. final sample + screendump
12. require status=running, RESET=0, no unexpected SHUTDOWN
13. compare ledger vs observed; classify FAIL
14. save artifacts under dev_build/ (delete when done per dev-build rule)
```

| Concern | Policy |
|---------|--------|
| Timeout | Per-phase deadlines; global e.g. 10–15 min stress budget |
| RESET | Any RESET → FAIL (`boot-loop` / `panic-class`) |
| Shutdown | Unexpected → FAIL |
| Pixel count | Desktop gate only — **never** allocator pass criterion |
| Seed | `PGBM` logged in artifact header |
| Failure classes | `desktop`, `reset`, `oom-missing`, `oom-wrong-semantics`, `recovery-leak`, `cursor`, `alloc-pages`, `release-cursor`, `ledger-mismatch`, `timeout` |

---

## 11. Guest control (minimal)

| Option | Production kernel change? | Verdict |
|--------|---------------------------|---------|
| QMP keyboard/mouse to open apps | No | Secondary pressure only |
| Disposable MENUET/PE **test app** on CoW calling `AllocPage`/`FreePage` | No kernel change | **Preferred** |
| New syscall | Forbidden for this research | Reject |
| Always-on kernel alloc hammer | Changes production behavior | Reject |
| Gate-off diagnostic thread | Future optional | Only if app path insufficient |

**Smallest harness:** CoW-installed `allocsoak` app + host script that waits for desktop, samples via `xp`, signals the app (file drop / shared mailbox / simple “start” click), reads a results file from the CoW FS after shutdown **or** live via msg board dump if enabled.

Default QEMU has **no serial**; prefer `xp` + optional results sector/file on disposable disk.

---

## 12. Success criteria (future ownership gate)

### Boot / stability
- Desktop reached (existing floor); `resets=0`; status `running`
- Phase F still interactive / non-black desktop class
- ON×3 stress boots with same seed class

### Allocator
- Baseline `pages_free` recorded
- Phase B: free pages **decrease** under pressure
- Phase C: failure matches legacy OOM **or** documented scan-miss
- Phase D: free pages **recover**; subsequent alloc succeeds
- Hammer ledger matches observed within tolerance
- Bitmap popcount sanity vs `pages_free`
- `alloc_pages` edge table passes
- Double-free does not inflate `pages_free`
- `release_pages` leaves `page_start` unchanged while freeing present pages

### Explicitly insufficient alone
- Single desktop boot
- Matching 779380 pixels
- Host `pgbm_*` tests without QEMU (necessary but not sufficient)

---

## 13. Would this justify Rust sole runtime bitmap ownership?

**If the soak above PASSes on FASM baseline and later PASSes on a gated Rust bitmap implementation with A/B ledger parity:**  
**Yes** — for the **bitmap-only** slice (`alloc_page` / `free_page` / `alloc_pages` / release bitmap primitive), because PTE/fault/CR3 remain FASM and are out of scope.

**Remaining gaps (ownership still blocked until closed):**

| Gap | Why it matters |
|-----|----------------|
| No pressure app / sampler script yet | Cannot run Phases B–E |
| Symbol→PA resolution not automated | `xp` sampling fragile |
| Natural app-launch pressure may never hit OOM | Need hammer for Phase C |
| `release_pages` split not implemented | Can validate FASM quirk now; Rust half only after split |
| Full 128 KiB bitmap dump costly | Use digests + popcount windows |
| Fault/CR3 not covered | Acceptable for bitmap-only claim; document limitation |

---

## 14. Tooling gap (minimal) — **CLOSED for PE driver path (PARTIAL OOM)**

**No production kernel change required.**

Implemented (see [`stage4-allocator-soak-tooling.md`](stage4-allocator-soak-tooling.md)):

1. **`scripts/qmp_allocator_soak.py`** — desktop wait, RESET/shutdown, `xp`
   sampling, msg_board scrape, host ledger, JSON under `dev_build/allocsoak/`
2. **`scripts/resolve_allocator_symbols.py`** + FASM `-s` (`kernel/bin/kernel.fas`)
3. **Disposable PE driver** — `tools/allocsoak/asoakdrv.asm` (`AllocPage` /
   `FreePage` / `AllocPages`) + MENUET loader + CoW firstapp recipe
4. **PE fixups** required (`data fixups`) — without them IAT calls hang

Still open:

* Live `pages_free<=1` early-OOM — **SAFETY BOUNDARY PROVEN** under AllocPage-only
  PE retain (scan-miss/`page_start` wall); see
  [`stage4-early-oom-experiment.md`](stage4-early-oom-experiment.md)
* Isolated `release_pages` vs `free_page` cursor A/B without process-exit noise —
  **DONE** ([`stage4-release-pages-ab.md`](stage4-release-pages-ab.md);
  decision **RELEASE/FREE PAGE_START DIFFERENCE PROVEN**)
* Per-op host xp samples of every AllocPages table row (markers prove execution)

---

## 15. Prerequisites checklist (before production ownership cut)

- [x] Host bitmap oracle (`pgbm_*`)
- [x] Writer inventory (4 runtime writers)
- [x] QMP sampler script
- [x] Guest pressure via PE `AllocPage`/`FreePage`/`AllocPages` on CoW
- [x] Early-OOM live reachability classified (boundary: scan-miss before `pages_free<=1`)
- [ ] FASM baseline soak PASS adjusted for scan-miss vs early-OOM distinction
- [ ] `release_pages` split design implemented in FASM (bitmap call-out) — still not Rust
- [ ] Rust bitmap domain gated + A/B soak PASS
- [ ] ON×3 + optional `--disk` stability

---

## 16. Conclusion

**ALLOCATOR DRIVER TOOLING — PARTIAL**; **EARLY-OOM SAFETY BOUNDARY PROVEN**;  
**RELEASE/FREE PAGE_START DIFFERENCE PROVEN**

Next research task: follow
[`stage4-release-bitmap-rust-plan.md`](stage4-release-bitmap-rust-plan.md) only
when a Stage-4 helper cut is authorized; until then strengthen production soak
checklist. Still no `USE_RUST_*` / no Cut CT by default.

Usage: [`stage4-allocator-soak-tooling.md`](stage4-allocator-soak-tooling.md),
[`stage4-early-oom-experiment.md`](stage4-early-oom-experiment.md),
[`stage4-release-pages-ab.md`](stage4-release-pages-ab.md),
[`stage4-ownership-design.md`](stage4-ownership-design.md) §15–§20,
[`stage4-release-bitmap-contract.md`](stage4-release-bitmap-contract.md),
[`stage4-release-bitmap-rust-plan.md`](stage4-release-bitmap-rust-plan.md)