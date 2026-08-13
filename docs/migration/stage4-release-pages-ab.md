# Stage-4 `release_pages` vs `free_page` `page_start` A/B

**Date:** 2026-08-13  
**Status:** research/validation only — **not** a migration cut  
**Parent:** [`stage4-allocator-soak-design.md`](stage4-allocator-soak-design.md),  
[`stage4-allocator-soak-tooling.md`](stage4-allocator-soak-tooling.md)  
**Inventory:** remains **99 / 135**  
**Decision:** **RELEASE/FREE PAGE_START DIFFERENCE PROVEN**

---

## 1. Question

Does live FASM preserve the oracle’s `page_start` divergence?

| Path | Bitmap | `pages_free` | `page_start` |
|------|--------|--------------|--------------|
| `free_page` | free (BTS) | +1 if bit was clear→set | may **lower** if freed dword &lt; cursor |
| `release_pages` | free (BTS) after PTE clear | +1 per present page | **unchanged** (local EBX discarded) |

---

## 2. Exact legacy semantics (FASM — unchanged)

Source: `kernel/core/memory.inc`.

### `free_page` (`FreePage`)

| Item | Value |
|------|-------|
| Input | `EAX` = physical page address |
| Flags | `pushfd` / `cli` / `popfd` |
| Bitmap | `bts dword [sys_pgmap], page_index` |
| `pages_free` | `cmc` / `adc [pages_free], 0` |
| `page_start` | If freed dword addr **&lt;** `[page_start]`, `mov [page_start], eax` |
| PTE / mutex / invlpg | none |

### `release_pages` (`ReleasePages`)

| Item | Value |
|------|-------|
| Input | `EAX` = linear base, `ECX` = page count |
| Sync | `pg_data.mutex` lock/unlock |
| Per page | `xchg` clear PTE, `invlpg`, if present: `bts` + update **local** EBX / EBP |
| `pages_free` | written once at end from EBP |
| `page_start` | **never stored** (EBX candidate discarded) |
| Relation to `free_page` | **not** a loop over `free_page` |

`kernel_free` = `release_pages` then `free_kernel_space` (VA heap only).  
`FreeKernelSpace` alone does **not** free physical pages.

Host oracle: `pgbm_release_does_not_update_page_start` in `pg_bitmap_oracle.rs`.

---

## 3. Isolation strategy

Process-exit / desktop teardown was rejected (allocator noise).

Disposable PE driver `tools/allocsoak/asoakdrv_ab.asm`:

1. **Case A:** `AllocPage` target A → `FILL_N=40` fillers (force cursor past A’s bitmap dword) → host latch → real `FreePage(A)` → latch → free fillers.
2. **Case B:** `KernelAlloc(4096)` → mapped lin/phys A′ → 40 fillers → latch → real `ReleasePages(lin,1)` → latch → `FreeKernelSpace` (VA only) → free fillers.
3. Recovery `AllocPage`/`FreePage` → `ALLOCSOK PASS`.

`FILL_N>32` matters: `free_page` only lowers `page_start` when the freed dword is **strictly below** the cursor; two consecutive allocs often share a dword.

Loaded via existing `ALLOCSOK` firstapp + `68.21` (same as soak tooling). No production syscall, no allocator patches.

---

## 4. Telemetry

| Piece | Path |
|-------|------|
| Driver | `tools/allocsoak/asoakdrv_ab.asm` |
| Host | `scripts/qmp_release_free_ab.py` |
| Symbols | `scripts/resolve_allocator_symbols.py` |
| Samples | QMP `xp` of `pages_free`, `page_start`, bitmap digest |
| Markers | `ALLOCSOK FREE BEFORE/AFTER`, `REL BEFORE/AFTER` |

First host sample after each marker is latched (avoids filler cleanup / case-B setup contamination).

**Digest caveat:** digest region is `[page_start, page_end)`. Freeing a page **below** the cursor (the interesting case) may leave that digest unchanged while `pages_free` still increments — observed on Case B. Do not treat digest-alone as the free oracle; use `pages_free` + markers + Case A digest shift.

---

## 5. Results (seed `0x5047424D` / `PGBM`)

Four independent CoW boots (1 + 3 repeats). All identical:

| Case | `pages_free` Δ | `page_start` | Digest `[page_start,page_end)` |
|------|----------------|--------------|--------------------------------|
| `free_page` | **+1** | **−4** (one dword lower) | changed |
| `release_pages` | **+1** | **0** (unchanged) | unchanged (bit below cursor) |

Example (all runs):

| | before | after |
|--|--------|-------|
| FreePage `page_start` | `0x802A00EC` (2150240492) | `0x802A00E8` (2150240488) |
| FreePage `pages_free` | 63492 | 63493 |
| ReleasePages `page_start` | `0x802A00EC` | `0x802A00EC` |
| ReleasePages `pages_free` | 63492 | 63493 |

Guest: `ALLOCSOK PASS`, `STAT 00000007`, phys reuse `00751000` after Case A free.  
QEMU: **0** RESET, **0** shutdown across all runs. Desktop reached after PASS.

Artifacts:

- `dev_build/allocsoak/release-free-page-ab.json`
- `dev_build/allocsoak/release-free-page-ab-run{2,3,4}.json`
- `dev_build/allocsoak/release-free-page-ab-summary.json`

---

## 6. Decision

**RELEASE/FREE PAGE_START DIFFERENCE PROVEN**

Revalidated after FASM helper extract (2026-08-13): FreePage Δps=−4 Δpf=+1;
ReleasePages Δps=0 Δpf=+1; ×4; RESET=0 — identical invariants to pre-extract.

Folded into ownership design: [`stage4-ownership-design.md`](stage4-ownership-design.md)
§15–§19; helper details in [`stage4-release-bitmap-contract.md`](stage4-release-bitmap-contract.md).

---

## 7. Recommended next research task

~~Host §19 oracle + RBPB smoke + FASM helper extract~~ — **DONE**.

~~Future Rust replacement plan (docs)~~ — **DONE**
([`stage4-release-bitmap-rust-plan.md`](stage4-release-bitmap-rust-plan.md)).

**Next:** strengthen production soak checklist for Stage-4 activation, or await
explicit authorization to start the gated helper cut (still no Cut CT unless
directed). Until then: no gate, no Rust blob.
