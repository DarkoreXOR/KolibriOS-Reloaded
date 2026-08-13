# Cut CU Plan — Slice E Physical Bitmap Ownership

**Date:** 2026-08-14  
**Status:** COMPLETE — see [`cut-cu-implementation.md`](cut-cu-implementation.md)  
**Parent:** [`stage4-bitmap-ownership-cut-plan.md`](stage4-bitmap-ownership-cut-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

---

## Target

| Field | Value |
|-------|--------|
| Cut | **CU** (Slice E) |
| Kind | **Path A** multi-symbol ownership cut (one coherent boundary) |
| Symbols | `alloc_page`, `free_page`, `alloc_pages`, Mode-B `release_bitmap_page_without_cursor_update` |
| Gate | `USE_RUST_PHYS_BITMAP_OWNERSHIP` (master — atomic) |
| Source | `kernel/core/memory.inc` |
| Rust | `rust_kernel/kolibri_utils/src/phys_bitmap.rs` |

---

## Path A justification

Rust becomes sole **runtime** writer of `sys_pgmap`, `pages_free`, and `page_start`.
Four related public operations share that state; FASM dual writers are eliminated.
PTE / mutex / `invlpg` / CR3 / fault remain FASM. Not four Path B leaves.

---

## Scope

**IN:** bitmap alloc/free/bulk + Mode-B release helper + `pages_free`/`page_start` ownership.  
**OUT:** `map_page`, `init_page_map`, PTE writers, fault, CR3, mutex ownership, `release_pages` orchestration.

---

## ABI summary (public kernel symbols)

### `alloc_page` (plain)

| | |
|--|--|
| In | none (state via globals) |
| Out | EAX = physical page address, or 0 |
| Preserve | EBX; DF |
| CLI | `pushfd; cli` … restore via `popfd` |
| State | may advance `page_start`; may decrement `pages_free`; BTR bitmap |
| Stack | plain `call`/`ret` |

### `free_page` (plain)

| | |
|--|--|
| In | EAX = physical page address |
| Out | void (EAX clobbered) |
| Preserve | DF; interrupt masking via CLI window matching legacy |
| State | BTS; `pages_free += !OLD`; **may lower** `page_start`; never raise |
| Stack | plain `call`/`ret` |

### `alloc_pages` (stdcall)

| | |
|--|--|
| In | `[esp+4]` = count |
| Out | EAX = physical base or 0; `ret 4` |
| State | 0xFF-run; charge `need*8`; **`page_start` unchanged** |
| Not | N× `alloc_page` |

### Release bitmap helper (Mode B)

| | |
|--|--|
| In | EAX = page_index |
| Out | EAX = delta ∈ {0,1} |
| Preserve | EBX/ECX/EDX/ESI/EDI/EBP; DF |
| State | BTS; **`pages_free += delta`**; **never** `page_start` |
| Stack | plain `call`/`ret` |

---

## Master gate / rollback

`USE_RUST_PHYS_BITMAP_OWNERSHIP = 1` enables all Slice E paths together.  
`= 0` restores FASM alloc* + Mode-A release + CT Mode-A helper.  
No mixed Mode A/B when ON.

---

## Completion

All criteria in the Slice E authorization brief are met — see implementation report.
