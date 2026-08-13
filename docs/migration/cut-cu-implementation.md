# Cut CU Implementation — Slice E Physical Bitmap Ownership

**Date:** 2026-08-14  
**Status:** complete  
**Plan:** [`cut-cu-plan.md`](cut-cu-plan.md)  
**Ownership plan:** [`stage4-bitmap-ownership-cut-plan.md`](stage4-bitmap-ownership-cut-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

---

## Target

| Field | Value |
|-------|--------|
| Cut | **CU** / **Slice E** |
| Path | **A** (sole runtime ownership of phys bitmap state) |
| Symbols | `alloc_page`, `free_page`, `alloc_pages`, Mode-B release helper |
| Gate | `USE_RUST_PHYS_BITMAP_OWNERSHIP = 1` |
| Inventory | **100 / 136 → 103 / 138** (+`free_page`, +`alloc_pages` scoped; three symbols completed) |

---

## Ownership

| Domain | Owner after CU ON |
|--------|-------------------|
| `sys_pgmap` runtime writes | **Rust** |
| `pages_free` runtime writes | **Rust** |
| `page_start` runtime writes | **Rust** (`alloc_page` / `free_page` only) |
| `page_end` | FASM boot; runtime RO |
| `release_pages` orch. | **FASM** (mutex / PTE / `invlpg`; no Mode-A counter) |
| `map_page` / fault / CR3 | **FASM** |
| Boot `init_page_map` | **FASM** |

---

## Mode A → Mode B

| | Mode A (OFF / pre-CU) | Mode B (ON) |
|--|----------------------|-------------|
| Helper | BTS + delta; no `pages_free` | BTS + `pages_free += delta` |
| `release_pages` | EBP load/add/store | no counter ops |
| `page_start` | never touched by helper | never touched by helper |

---

## Blobs (0 relocations each)

| Blob | Bytes | SHA-256 |
|------|------:|---------|
| `rust_alloc_page.bin` | 123 | `ee6bd568731aaa1911a97eb36a8b707b3d3d605bd50c53fcfb04c41a67b6687e` |
| `rust_free_page.bin` | 83 | `158b1bd78ad252b98d5c5e1fb7cbd55393327ee7fe0ef618af825799130ee582` |
| `rust_alloc_pages.bin` | 281 | `d2fc2459920368beaee3227034b231b2bcd305cc70789ececcc597257315c3e4` |
| `rust_release_bitmap_page_mode_b.bin` | 59 | `9b0281a21d36edf3ac08ab7c9c72819e0b6e42de3a7f22f9e12ed687c8ee15e0` |
| Combined | **546** | |

CT Mode-A blob retained for OFF path (49 B).

---

## ABI (public symbols unchanged)

| Symbol | Convention | Notes |
|--------|------------|-------|
| `alloc_page` | plain; EAX=phys\|0; pushfd/cli; EBX preserve | trampoline converts absolute↔offset |
| `free_page` | plain; EAX=phys in; void; cli | may lower `page_start` |
| `alloc_pages` | stdcall(count); ret 4 | 0xFF-run; cursor unchanged |
| release helper | plain; EAX page_index→delta | Mode B updates `pages_free` |

Freestanding Rust uses **byte offsets** from `sys_pgmap` for cursor args (trampoline converts).

---

## Validation

| Test | Result |
|------|--------|
| Host `slce_*` + PGBM | PASS (suite **836/836**) |
| Blob extract | 0 relocs each |
| QEMU OFF | non-black **779380**, RESET=0 |
| QEMU ON | **779380**, RESET=0 |
| A/B OFF vs ON | match 779380 |
| ON ×3 | PASS ×3 |
| ON + exFAT | PASS 779380 |
| Allocator soak | PASS (`soak-cut-cu.json`) |
| Release/free A/B ×4 | **PROVEN** (free may Δps; release Δps=0) |
| Memory | `.bss` `OS_BASE+0x8C7C3`; assert PASS; pack unchanged |
| Image ON | **302824** bytes |

---

## Memory

| Item | Value |
|------|-------|
| `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` | `0x8E000` / `0x8E000` / `0x90000` |
| End `.bss` | `OS_BASE+0x8C7C3` |
| Assert | `0x8D7C3 < 0x8E000` PASS |
| `kernel.mnt` (ON) | **302824** bytes |
| Final image | `dev_build/test/kernel-20260813-201323.img` |

---

## Rollback

```text
USE_RUST_PHYS_BITMAP_OWNERSHIP = 0
```
(or `enabled = false` for all Cut CU `[[rust.migrations]]` entries). Restores FASM alloc* + Mode-A release + CT Mode-A helper.

---

## Files

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/phys_bitmap.rs` | production + `slce_*` |
| `rust_kernel/kolibri_utils/src/ffi.rs` | stdcall exports |
| `kernel/core/memory.inc` | master gate + trampolines + Mode B release |
| `kernel/rust/phys_bitmap.inc` | blob embeds |
| `project/build.toml` | blobs + CU migrations |
| `docs/migration/cut-cu-*.md` | plan + report |

---

## Known limitations

* Page phys `0` remains ambiguous with OOM (legacy FASM).  
* Mid-loop `[pages_free]` may update earlier under Mode B (accepted under mutex).  
* Live `pages_free<=1` OOM not required (oracle covers).  
* PTE/CR3/fault not migrated.
* FASM trampolines convert absolute↔offset and store absolute `page_start` after Rust mutates the offset out-param — logical ownership remains Rust.
