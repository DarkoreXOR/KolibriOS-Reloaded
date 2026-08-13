# Stage-4 Future Rust Replacement Plan — `release_bitmap_page_without_cursor_update`

**Date:** 2026-08-13 (design); **superseded 2026-08-14 by Cut CT**  
**Status:** **SUPERSEDED** — production migration complete: [`cut-ct-plan.md`](cut-ct-plan.md), [`cut-ct-implementation.md`](cut-ct-implementation.md)  
**Inventory after CT:** **100 / 136**  
**Decision:** Implemented as Cut CT (`USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE = 1`).

**Parents:**
- [`stage4-ownership-design.md`](stage4-ownership-design.md) §15–§19
- [`stage4-release-bitmap-contract.md`](stage4-release-bitmap-contract.md)
- [`stage4-release-pages-ab.md`](stage4-release-pages-ab.md)

> Historical pre-migration design retained below for audit. Do **not** treat this
> document as authorizing further allocator cuts.

---

## 1. Current FASM baseline (authoritative)

| Item | Value |
|------|-------|
| Symbol | `release_bitmap_page_without_cursor_update` |
| Source | `kernel/core/memory.inc` |
| Call site | `release_pages` after present-bit + `shr eax,12` |
| Gate | none (always FASM today) |
| `kernel.mnt` | 301864 bytes (unchanged at extract) |
| Memory pack | `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` = `0x8E000` / `0x8E000` / `0x90000` |

### Exact FASM body (LOCAL FACT)

```text
release_bitmap_page_without_cursor_update:
        bts     dword [sys_pgmap], eax  ; CF = OLD (1 = already free)
        cmc                             ; CF = !OLD
        mov     eax, 0
        adc     eax, 0                  ; EAX = delta ∈ {0,1}
        ret
```

### Exact call site (LOCAL FACT)

```text
        shr     eax, 12
        call    release_bitmap_page_without_cursor_update
        add     ebp, eax                ; Mode A batch into local EBP
        ; … loop …
        mov     [pg_data.pages_free], ebp
```

### Critical ownership fact

The helper **does not write `[pages_free]`**.  
It only:

1. mutates `sys_pgmap` via `BTS`, and  
2. returns `delta` in EAX.

`release_pages` owns Mode-A batching (`add ebp,eax`) and the final store.  
Any future Rust body that writes `[pages_free]` would **break** Mode A and
double-count if the caller still batches.

---

## 2. Why this is now a legitimate future Path B target

| Criterion | Status |
|-----------|--------|
| Contiguous mechanical FASM boundary | **Yes** — extracted helper |
| Fixed register ABI | **Yes** — EAX in/out, plain `call`/`ret` |
| Independent host oracle | **Yes** — `pgbm_*` 17/17 + 50 000 §19 |
| Live A/B (`page_start` invariant) | **Yes** — ×4, RESET=0 |
| OFF baseline for future gate | **Yes** — this FASM helper |
| Scope excludes PTE/`invlpg`/mutex/CR3/`page_start` | **Yes** |
| Stronger than migrating monolithic `release_pages` | **Yes** — orchestration stays FASM |

Path A (opportunistic bundling with other allocator leaves) remains **rejected**.  
This leaf is Path B: one helper, one gate, FASM OFF baseline already proven.

**Placeholder future gate name (not created):** `USE_RUST_RELEASE_BITMAP_PAGE`  
**Placeholder future cut name:** do **not** invent Cut CT in-repo until authorized.

---

## 3. Exact public ABI (must match FASM helper)

| | Contract |
|--|----------|
| Convention | plain `call` / `ret` (0) — **not** stdcall at the public symbol |
| **IN** | **EAX** = page index (`phys >> 12`) |
| **OUT** | **EAX** = `delta ∈ {0,1}` |
| **Preserved** | EBX, ECX, EDX, ESI, EDI, EBP |
| **Clobbered** | EAX, EFLAGS |
| **Flags** | **not** a public contract — callers use EAX only (REG-018) |
| **DF** | unchanged |
| **Stack** | no args; no `add esp`; no `ret N` |

Internal freestanding Rust may use `stdcall(page_index, sys_pgmap_ptr) -> u32`
**behind** a FASM trampoline that restores the public ABI above.

---

## 4. Mathematical / bit-level semantics

Derived from the FASM helper (not from abstractions):

```text
OLD = bit[page_index] before BTS          ; Intel BTS: CF ← OLD; bit ← 1
delta = 1 - OLD                           ; cmc + adc eax,0  (OLD∈{0,1})
sys_pgmap bit[page_index] := 1            ; free polarity
; pages_free memory: UNTOUCHED by helper
; page_start: UNTOUCHED
```

| OLD | Meaning | delta | Bitmap after |
|-----|---------|-------|--------------|
| 0 | was allocated | 1 | free |
| 1 | already free | 0 | free (idempotent) |

Rust may implement this with load/mask/store + integer arithmetic **if and only
if** it matches BTS polarity and the delta table for every page index in range.
Do **not** “normalize” wrapping at the call site: caller `add ebp,eax` already
wraps like `adc ebp,0`.

Out-of-range page indices: FASM `bts` on `sys_pgmap` still runs; keep the same
unchecked behavior unless a separate ABI decision documents otherwise (default:
preserve).

---

## 5. Relocation / addressing strategy

### Problem

`sys_pgmap` is a kernel global (`data32.inc`). A freestanding Rust blob must not
emit ELF/COFF relocations against it.

### Selected strategy (Cut AA / AQ class)

**Trampoline-injected pointer; public ABI stays EAX-only.**

```text
Public (gate ON):
  release_bitmap_page_without_cursor_update:
        push ebx ecx edx esi edi ebp
        ; EAX = page_index
        stdcall rust_release_bitmap_page_without_cursor_update, eax, sys_pgmap
        ; EAX = delta  — do not pop into EAX; restore other regs only
        pop ebp edi esi edx ecx ebx
        ret

Freestanding:
  rust_*(page_index: u32, map: *mut u8) -> u32
        ; reloc-free: only uses stack args
```

| Approach | Verdict |
|----------|---------|
| Absolute imm32 of `sys_pgmap` baked into blob | Possible but brittle (link address / extract patch); **not preferred** |
| Extra public register arg for map base | Breaks proven EAX-only loop ABI | **reject** |
| Injected stdcall args via trampoline | **select** — 0 relocs, matches Cut AA |
| Touch `[pages_free]` from Rust | **reject** — not part of current helper |

`pages_free` strategy: **none in Rust** — FASM `release_pages` keeps Mode A.

---

## 6. Planned Rust surface (design only — do not implement)

| Item | Plan |
|------|------|
| Public FASM symbol | still `release_bitmap_page_without_cursor_update` (trampoline when gated) |
| Freestanding symbol | `rust_release_bitmap_page_without_cursor_update` |
| Section | `.text.rust_release_bitmap_page_without_cursor_update` (project naming) |
| Extract | `rust_kernel/kolibri_utils/out/rust_release_bitmap_page_without_cursor_update.bin` |
| Embed | `kernel/rust/release_bitmap_page_without_cursor_update.inc` + `file` |
| Gate | `USE_RUST_RELEASE_BITMAP_PAGE` (placeholder — **not added**) |

### Outline

```rust
// Freestanding, no_std, no allocator, no panic
// stdcall(page_index, map_base) -> delta
pub unsafe extern "stdcall" fn rust_release_bitmap_page_without_cursor_update(
    page_index: u32,
    map: *mut u8,
) -> u32 {
    // byte = page_index >> 3; bit = page_index & 7
    // OLD = (map[byte] >> bit) & 1
    // map[byte] |= 1 << bit
    // return 1 - OLD   // as u32
}
```

Preserve DF: avoid `std` / libc; no implicit direction-flag ops; trampoline may
`cld` only if smoke requires a known DF (prefer leave DF untouched like FASM).

---

## 7. FASM/Rust boundary

```text
release_pages                    [FASM — forever in this slice]
  ├── mutex / lin→PTE / xchg / invlpg / present / loop / Mode A store
  └── release_bitmap_page_without_cursor_update
        ├── OFF: current FASM body (bts/cmc/adc→delta)
        └── ON:  trampoline → rust_* (map injected); same EAX delta
```

| Layer | Owner after future ON |
|-------|------------------------|
| `release_pages` orchestration | FASM |
| PTE / `invlpg` / mutex | FASM |
| Mode A `ebp` + final `pages_free` store | FASM |
| Bitmap BTS + delta | **Rust** (gated) |
| `page_start` | untouched (neither path writes it here) |
| `free_page` / `alloc_page` / `alloc_pages` | **out of scope** |

---

## 8. Oracle reuse

| Existing | Reuse |
|----------|-------|
| `IndependentBitmap::release_bitmap_page_without_cursor_update` | **Yes** — primary independent model |
| `FasmBitmapEmu::release_bitmap_page_without_cursor_update` | **Yes** — FASM-faithful reference |
| `pgbm_s19_*` + `pgbm_s19_prng_50000` | **Yes** — seed `0x5047424D` |
| `pgbm_s19_mode_a_vs_mode_b_end_state` | **Yes** — Mode A batching stays FASM |
| `pgbm_s19_ne_free_page_for_cursor` | **Yes** — must not converge with `free_page` |
| Prior alloc/free/OOM `pgbm_*` | Unchanged background |

### New Rust-specific differentials (when implementing)

| Test | Purpose |
|------|---------|
| `rbpu_rust_vs_fasm_emu` | Extracted blob / emulator vs `FasmBitmapEmu` on identical maps |
| Preserve canaries on trampoline path | EBX…EBP + DF in host or smoke |
| Map pointer injection | Wrong base must fail loudly in unit tests |
| Assert Rust does **not** write a fake `pages_free` field | API shape |

Do **not** duplicate the entire 50 000-case suite; call the existing §19 ops from
new wrappers that also exercise the freestanding entry once it exists.

**Limitation today:** no Unicorn assembled-FASM replay — `FasmBitmapEmu` remains
the instruction-faithful host reference.

---

## 9. Future ABI smoke (production, when gated)

Reuse RBPB conceptual vectors; smoke must call the **public** symbol (trampoline),
not only `rust_*`:

1. Canaries: EBX/ECX/EDX/ESI/EDI/EBP (+ DF bit).
2. Synthetic or save/restore `sys_pgmap` region for planted bits.
3. Allocated → EAX=1; already-free → EAX=0; repeat.
4. `page_start` canary unchanged.
5. Confirm `[pages_free]` unchanged across the helper alone (Mode A owned by caller).
6. Stack ESP unchanged; plain `ret`.
7. Marker e.g. `RBPB` / cut-specific smoke result dword.
8. **No** live `pages_free<=1` OOM requirement.

Disposable PE `asoakdrv_rbpb.asm` already validated the **ABI shape**; production
smoke must target the **kernel** public entry after the gate exists.

---

## 10. Future QEMU validation

| Step | Build |
|------|-------|
| OFF | Extracted FASM helper (current tree) |
| ON | Gate ON + Rust blob |
| A/B | OFF vs ON desktop + release/free harness |
| ON×3 | Repeated ON boots |
| Release/free A/B | `qmp_release_free_ab.py` — FreePage may lower cursor; ReleasePages must not |
| Allocator pressure | Existing `asoakdrv` soak / recovery (not early-OOM≤1 gate) |
| Optional | `--disk` / GUI light soak |

Expected invariants (both OFF and ON):

- FreePage: `pages_free +1`; `page_start` may drop  
- ReleasePages: `pages_free +1`; `page_start` unchanged; PTE clear + `invlpg` still FASM  

---

## 11. Memory / blob budget

| Constraint | Value |
|------------|-------|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `0x008E000` |
| `SLOT_BASE` | `0x0090000` |
| Current `kernel.mnt` | 301864 bytes |

Before activation **require**:

1. Blob size measurement (expect tens of bytes — tiny leaf).  
2. **0** relocations in extracted `.bin`.  
3. Fixed-address / `.bss` assert still green.  
4. `kernel.mnt` size delta recorded.  

If the blob or trampoline threatens REG-012 headroom: **block the cut** — do not
move TMP/sys_proc/SLOT_BASE for this leaf.

---

## 12. Regression risks (activation)

1. Wrong `sys_pgmap` base (injection / absolute mismatch)  
2. Accidental `[pages_free]` write inside Rust (Mode A break / double-count)  
3. Accidental `page_start` write  
4. Wrong BTS polarity / already-free delta  
5. Wrapping mismatch at `add ebp,eax` boundary (helper must still return 0/1 only)  
6. Clobber EBX/ECX/EDX/ESI/EDI/EBP (REG-001 class)  
7. DF mutation  
8. CF treated as public result across pops (REG-018)  
9. Relocations in blob  
10. stdcall double cleanup / `ret N` on public path (REG-009)  
11. Changing PTE/`invlpg`/mutex ordering in `release_pages` while wiring the gate  
12. Sharing code with `free_page` and importing cursor mutation  

---

## 13. Rollback

| Gate OFF | Behavior |
|----------|----------|
| `USE_RUST_RELEASE_BITMAP_PAGE = 0` (future) | Public symbol = current FASM `bts/cmc/adc` body |

FASM helper remains the authoritative OFF baseline. No dual writers: one
implementation of the public symbol at a time.

---

## 14. Cut boundary (future)

**In scope:** gated replacement of `release_bitmap_page_without_cursor_update` only.

**Out of scope:**

- `release_pages` orchestration  
- `free_page` / `alloc_page` / `alloc_pages`  
- `page_start` policy  
- PTE / `invlpg` / mutex / CR3 / fault  
- `map_page` / Path A bitmap-domain ownership claim  

---

## 15. Activation blockers (must clear before any cut)

- [ ] Freestanding Rust + extract + 0 reloc (not started)  
- [ ] FASM trampoline + gate + rollback (not started)  
- [ ] Host `rbpu_*` / reuse `pgbm_s19_*` against real blob  
- [ ] In-kernel public-entry ABI smoke  
- [ ] QEMU OFF / ON / A/B / ON×3  
- [ ] Release/free A/B on ON build  
- [ ] Blob size + REG-012 assert  
- [ ] Explicit cut authorization (inventory still 99/135 until then)  

**Not a blocker:** live `pages_free<=1` OOM soak
([`stage4-early-oom-experiment.md`](stage4-early-oom-experiment.md)).

---

## 16. Decision

**FUTURE RUST REPLACEMENT PLAN READY — MIGRATION STILL BLOCKED**

Production changes from this document: **NONE**.  
Cut CT: **does not exist**.  
Inventory: **99 / 135**.
