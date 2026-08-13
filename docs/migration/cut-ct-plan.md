# Cut CT Plan — `release_bitmap_page_without_cursor_update`

**Date:** 2026-08-14  
**Status:** complete — see [`cut-ct-implementation.md`](cut-ct-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md).

> **Nomenclature:** **Cut CT** migrates the Stage-4 physical bitmap release
> helper `release_bitmap_page_without_cursor_update` in `kernel/core/memory.inc`.  
> Do **not** migrate `release_pages`, `free_page`, `alloc_page`, `alloc_pages`,
> `page_start`, PTE/`invlpg`/mutex/CR3/fault ownership. Do not start Cut CU.

---

## Fresh repository audit

| Check | Result |
|-------|--------|
| Inventory | **99 / 135** (`migration-todo.md`) |
| Production gates | **99** `[[rust.migrations]]` `enabled = true` |
| Cut CS | **BLOCKED** (no Path A / opportunistic leaf) |
| Stage-4 research | complete — oracle, writers inventory, release/free A/B, FASM extract |
| `kernel.mnt` (pre-CT) | **301864** bytes |
| `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` | **`0x8E000` / `0x8E000` / `0x90000`** (REG-012) |

### Path A: **REJECTED**

Opportunistic bundling with `alloc_page` / `free_page` / `release_pages` /
`map_page` would claim Stage-4 allocator ownership without an ownership handoff.
Those symbols remain deferred. Cut CT is **Path B**: one extracted helper, one
gate, proven OFF baseline.

### Selection rationale

| Criterion | Status |
|-----------|--------|
| Contiguous FASM boundary | extracted helper in `memory.inc` |
| Fixed public ABI | EAX in/out, plain `call`/`ret` |
| Independent host oracle | `pgbm_*` + PGBM 50 000 |
| Live free vs release A/B | ×4, RESET=0 |
| OFF baseline | extracted FASM helper |
| Scope excludes PTE/`invlpg`/mutex/`page_start` | yes |

---

## Target

| Field | Value |
|-------|-------|
| Cut | **CT** |
| FASM symbol | `release_bitmap_page_without_cursor_update` |
| Source | `kernel/core/memory.inc` |
| Rust symbol | `rust_release_bitmap_page_without_cursor_update` |
| Gate | `USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE` |
| Path | **B** |
| Subsystem | Stage-4 physical bitmap release (helper only) |

---

## Current FASM boundary (OFF baseline)

```text
release_bitmap_page_without_cursor_update:
        bts     dword [sys_pgmap], eax
        cmc
        mov     eax, 0
        adc     eax, 0
        ret
```

`release_pages` call site (unchanged semantics):

```text
        shr     eax, 12
        call    release_bitmap_page_without_cursor_update
        add     ebp, eax
        ; … later: mov [pg_data.pages_free], ebp
```

---

## Approved Rust ABI

### Public (FASM-facing)

| | Contract |
|--|----------|
| Convention | plain `call` / `ret` (0) |
| IN | EAX = page index (`phys >> 12`) |
| OUT | EAX = delta ∈ {0,1} |
| Preserved | EBX, ECX, EDX, ESI, EDI, EBP |
| Clobbered | EAX, EFLAGS |
| Flags | **not** a public contract (REG-018) |
| DF | unchanged |
| Stack | no args; no `ret N` |

### Internal (blob)

| | Contract |
|--|----------|
| Convention | `stdcall` |
| Args | `(page_index: u32, map: *mut u8) -> u32` |
| Epilogue | `ret 8` |
| Relocs | **0** mandatory |

### Trampoline (Cut AA inject style)

```text
push ebx ecx edx esi edi ebp
stdcall rust_release_bitmap_page_without_cursor_update, eax, sys_pgmap
pop ebp edi esi edx ecx ebx
ret
```

Exactly one cleanup owner (`ret 8` in Rust). No extra `add esp` (REG-009).

---

## Ownership scope

| Owned by Rust (this cut) | Remains FASM |
|--------------------------|--------------|
| `sys_pgmap` BTS + delta | Mode-A `add ebp,eax` + final `pages_free` store |
| | `page_start` |
| | PTE clear / `invlpg` / mutex / CR3 / faults |
| | `alloc_page` / `free_page` / `alloc_pages` / `release_pages` loop |

**pages_free rule:** Rust must not read or write `pages_free`.  
**page_start rule:** Rust must not access `page_start`.

---

## Validation plan

1. Host: fix/align §19 helper oracle to production (pages_free untouched by helper).
2. Host: Rust differential vs IndependentBitmap / FasmBitmapEmu (map + delta).
3. ABI smoke `RBPB`: public trampoline when gate ON; register/DF/stack/canaries.
4. QEMU: OFF → ON → A/B → ON×3; release/free A/B; allocator pressure/recovery;
   exFAT/FS as available; final desktop (baseline non-black **779380**, RESET=0).
5. Memory asserts: pack unchanged; blob 0 relocs; no TMP_STACK_TOP raise.
6. Enable gate only after all criteria pass. Inventory +1. **Stop** (no Cut CU).

---

## Rollback

```text
USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE = 0
```

Extracted FASM helper body remains under the `else` branch.

---

## Cross-references

- [`stage4-ownership-design.md`](stage4-ownership-design.md)
- [`stage4-release-bitmap-contract.md`](stage4-release-bitmap-contract.md)
- [`stage4-release-pages-ab.md`](stage4-release-pages-ab.md)
- [`stage4-release-bitmap-rust-plan.md`](stage4-release-bitmap-rust-plan.md)
