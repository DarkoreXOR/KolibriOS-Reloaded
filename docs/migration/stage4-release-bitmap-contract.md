# Stage-4 Release Bitmap Contract (§19)

**Date:** 2026-08-13  
**Status:** Cut CT production migration complete — see [`cut-ct-implementation.md`](cut-ct-implementation.md)  
**Parent:** [`stage4-ownership-design.md`](stage4-ownership-design.md) §19  
**Live A/B:** [`stage4-release-pages-ab.md`](stage4-release-pages-ab.md)  
**Inventory after CT:** **100 / 136**  
**Decisions:**
- **RELEASE BITMAP CONTRACT — READY** (host + disposable smoke)
- **Cut CT COMPLETE** — Rust owns the extracted helper; `release_pages` Mode A remains FASM

---

## 1. Purpose

1. Validate the future `release_bitmap_page_without_cursor_update` **contract**
   (host oracle + disposable RBPB smoke).
2. Mechanically extract that body into a **production FASM helper** called by
   `release_pages`, without Rust ownership or gates.

---

## 2. Semantic contract

| Item | Rule |
|------|------|
| Input | page index (`phys >> 12`) |
| Output | `delta ∈ {0,1}` |
| Bitmap | BTS polarity: bit=1 free; set bit free |
| Delta | `1` iff OLD bit was `0` (allocated→free); `0` if already free |
| `pages_free` | Mode A: caller batches `add ebp,eax` then one store |
| `page_start` | **unchanged** (helper never writes it) |
| Not owned by helper | PTE, `invlpg`, mutex, CR3, faults |

≠ `free_page` (which may lower `page_start`).

---

## 3. FASM helper (production boundary — still FASM-owned)

| Item | Value |
|------|-------|
| Name | `release_bitmap_page_without_cursor_update` |
| Source | `kernel/core/memory.inc` |
| Call site | `release_pages` after present-bit test + `shr eax,12` |
| Rust / gate | **none** |

### ABI

| | |
|--|--|
| Convention | plain `call` / `ret` (0) |
| IN | EAX = page index |
| OUT | EAX = delta ∈ {0,1} |
| Preserved | EBX, ECX, EDX, ESI, EDI, EBP |
| Clobbered | EAX, EFLAGS |
| Flags | not a public contract (REG-018) |
| DF | unchanged |

### Body (mechanical)

```text
bts dword [sys_pgmap], eax   ; CF = OLD
cmc                          ; CF = !OLD
mov eax, 0
adc eax, 0                   ; EAX = delta
ret
```

Caller (`release_pages`): `add ebp, eax` (Mode A batching) then
`mov [pages_free], ebp` after the loop.

Dead local `page_start` candidate (legacy EBX) omitted — never stored.

---

## 4. Host §19 oracle

| Item | Value |
|------|-------|
| File | `rust_kernel/kolibri_utils/src/pg_bitmap_oracle.rs` |
| PRNG | `0x5047424D` (`PGBM`) |
| Limitation | No Unicorn — `FasmBitmapEmu` is the FASM reference |

**Result:** `pgbm_*` **17/17 PASS** (includes Mode A/B end-state, 50 000 §19 cases).

---

## 5. Disposable ABI smoke (RBPB)

| Piece | Path |
|-------|------|
| PE | `tools/allocsoak/asoakdrv_rbpb.asm` |
| Runner | `scripts/qmp_release_bitmap_contract.py` |

Validates **contract shape** via a test-only PE shim (not the kernel helper).
**RBPB PASS**, RESET=0.

---

## 6. Live validation after FASM extract

| Test | Result |
|------|--------|
| Release/free A/B ×4 (`qmp_release_free_ab.py`) | PROVEN; FreePage Δps=−4 Δpf=+1; ReleasePages Δps=0 Δpf=+1 |
| Identical to pre-extract baseline deltas | yes |
| Desktop smoke | non-black **779380**, RESET=0 |
| `kernel.mnt` size | **301864** bytes (unchanged vs prior build) |
| Helper present in `.fas` | yes |

Original-vs-extracted comparison uses pre-extract A/B artifacts as baseline A
and post-extract ×4 as B (same invariants; not byte-identical machine code).

---

## 7. Future production prerequisites (Rust still blocked)

1. Host §19 differential — **DONE**
2. Disposable ABI smoke — **DONE**
3. Mode A/B bitmap-domain equivalence — **DONE**
4. FASM mechanical helper extract — **DONE**
5. Real Rust trampoline / gate — **NOT DONE**
6. Live production soak beyond disposable PE — still required before ownership cut
7. Desktop + ON×3 when gated — N/A until gate exists

Do **not** require live `pages_free<=1` OOM as a soak gate.

---

## 8. Decision

**FASM BITMAP BOUNDARY READY — RUST MIGRATION STILL BLOCKED**

The named FASM helper is the production preparation boundary. Inventory stays
**99 / 135**. Cut CT does not exist. No `USE_RUST_*`.

Future Rust replacement plan (docs only):
[`stage4-release-bitmap-rust-plan.md`](stage4-release-bitmap-rust-plan.md)
(**FUTURE RUST REPLACEMENT PLAN READY — MIGRATION STILL BLOCKED**).
