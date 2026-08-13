# Cut CT Implementation — `release_bitmap_page_without_cursor_update`

**Date:** 2026-08-14  
**Status:** complete  
**Plan:** [`cut-ct-plan.md`](cut-ct-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|--------|
| Cut identifier | **CT** |
| FASM symbol | `release_bitmap_page_without_cursor_update` |
| Source | [`kernel/core/memory.inc`](../../kernel/core/memory.inc) |
| Rust symbol | `rust_release_bitmap_page_without_cursor_update` |
| Pure helper | `kolibri_utils::release_bitmap_page_without_cursor_update` |
| Subsystem | Stage-4 physical bitmap release (helper only) |
| Stage | Stage 4 Path B leaf |
| Migration kind | **Single-function cut** (Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — bundling with `alloc_page` / `free_page` / `release_pages` /
`map_page` would claim Stage-4 allocator ownership without an ownership handoff.
Selected the extracted §19 helper after Stage-4 research (oracle, writers inventory,
release/free A/B, FASM extract). **Not Path A.** Allocator policy / PTE / `invlpg` /
mutex / CR3 / `page_start` remain FASM.

**Memory:** Blob **49 B**; end `.bss` **`OS_BASE+0x8C543`**; assert
`0x8D543 < 0x8E000` PASS. **`TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` unchanged**
(`0x8E000` / `0x8E000` / `0x90000`). Image size **302248** (was 301864 pre-CT).

---

## Legacy ABI

```text
release_bitmap_page_without_cursor_update  (plain call / ret)
  in:  EAX = page index (phys >> 12)
  out: EAX = delta ∈ {0,1}
  preserves: EBX, ECX, EDX, ESI, EDI, EBP
  clobbers: EAX, EFLAGS
  DF: unchanged
  flags: not an observable return (REG-018)
  stack: no args; ret 0
```

Semantics: `bts` free-polarity on `sys_pgmap`; delta = !OLD. Does **not** write
`pages_free` or `page_start`. Caller (`release_pages`): `add ebp,eax` then final
`mov [pg_data.pages_free], ebp`.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_release_bitmap_page_without_cursor_update` |
| Blob | **49** bytes, **0** relocations |
| SHA-256 | `6f00e207e049daa62f86099ed2eac578546e9180a46bd9b17454538c18d039d6` |
| Epilogue | `ret 8` (`c20800`) |
| Trampoline | push EBX..EBP; `stdcall rust_*, eax, sys_pgmap`; pop; `ret` |
| Gate | `USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE` (prod **1**) |
| Rust ABI | `stdcall(page_index, map) -> delta` |
| OFF | extracted FASM helper body intact |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | PGBM IndependentBitmap + FasmBitmapEmu (helper does **not** write `pages_free`) |
| PRNG seed | `0x52504242` (`'RBPB'`) / PGBM `0x5047424D` |
| Cases | 50 000 Rust vs oracle; 500 Rust vs PGBM; existing `pgbm_*` 17/17 |
| Host focused | `rbpb_*` **7/7 PASS** |
| Full host suite | **830/830 PASS** |
| ABI smoke | **PASS** — marker `'RBPB'` / fail `DEAD0C54`; public trampoline when ON |

---

## QEMU validation

| Config | Gate | Image | non-black | resets | Result |
|--------|------|-------|-----------|--------|--------|
| OFF | FASM helper | `kernel-20260813-190727.img` | 779380 | 0 | PASS |
| ON | Rust | `kernel-20260813-190918.img` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 | 1 | 779380 ×3 | 0 | PASS |
| Final + exFAT | 1 | `kernel-20260813-191702.img` | 779380 | 0 | PASS |

Release/free A/B ×4: **RELEASE/FREE PAGE_START DIFFERENCE PROVEN** (free Δpage_start≠0;
release Δpage_start=0; pages_free +1 both). Allocator soak: pressure **ok**, recovery
**ok**, RESET=0 (`soak-cut-ct.json`).

---

## Regressions

None new. Applied REG-009 (no extra `add esp` after `ret 8`), REG-010 (no stack-arg
confusion), REG-012 (pack unchanged), REG-018 (no CF public contract).

---

## Production gate

`USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE = 1` in `kernel/core/memory.inc`
(via `project/build.toml` Cut CT `enabled = true`).

---

## Rollback

```text
USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE = 0
```

(or `enabled = false` for Cut CT in `project/build.toml` then rebuild). Extracted FASM
helper remains under the `else` branch.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/release_bitmap_page.rs` | production + `rbpb_*` |
| `rust_kernel/kolibri_utils/src/ffi.rs` | stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | mod / re-export |
| `rust_kernel/kolibri_utils/src/pg_bitmap_oracle.rs` | helper leaves `pages_free` untouched |
| `kernel/core/memory.inc` | gate + trampoline + FASM OFF body |
| `kernel/rust/release_bitmap_page_without_cursor_update.inc` | blob + RBPB smoke |
| `kernel/kernel32.inc` / `kernel/kernel.asm` | include + smoke call |
| `project/build.toml` | blob + Cut CT migration |
| `docs/migration/cut-ct-plan.md` / `cut-ct-implementation.md` | plan + report |
| `docs/migration/migration-todo.md` / `migration-plan.md` | inventory |

---

## Known limitations

* Rust owns **only** the bitmap-release helper. `release_pages` orchestration,
  `pages_free` Mode-A store, `page_start`, PTE/`invlpg`/mutex remain FASM.
* Live trampoline smoke uses an already-free `sys_pgmap` bit (delta=0) so boot
  does not free a live allocated page; full map/delta coverage is host + direct
  `rust_*` synthetic map.
* Stage-4 allocator ownership is **not** complete after this cut.

---

## Updated inventory

**100 / 136** (`100` completed checklist leaves including Cut CT; `36` still pending;
`100` production gates enabled).

---

## Next candidates (Cut CU — do not start)

1. Further Stage-4 research toward a next proven leaf (not opportunistic)
2. `alloc_page` — still deferred pending ownership design
3. `free_page` — deferred; must preserve ≠ release bitmap contract
4. `strnlen` / thin export leaves — weak evidence bar
5. `ntfs_restore_usa_frs` — fallthrough sibling, not ownership
