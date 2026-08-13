# Stage-4 Bitmap Ownership Cut Plan (Slice E)

**Date:** 2026-08-14  
**Status:** **IMPLEMENTED** — Cut CU COMPLETE ([`cut-cu-implementation.md`](cut-cu-implementation.md))  
**Inventory:** **103 / 138**  
**Production gates:** **104** enabled (`USE_RUST_PHYS_BITMAP_OWNERSHIP = 1`)  
**Cut CT:** COMPLETE — Mode A leaf retained for OFF rollback  
**Cut CU / Slice E:** **COMPLETE** — do **not** treat this plan as open work  
**Parents:**
[`stage4-next-ownership-audit.md`](stage4-next-ownership-audit.md),
[`stage4-ownership-design.md`](stage4-ownership-design.md),
[`cut-ct-implementation.md`](cut-ct-implementation.md),
[`cut-cu-plan.md`](cut-cu-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> Pre-migration plan for **Slice E** (now executed as **Cut CU**): sole Rust
> runtime ownership of the physical page bitmap domain (`sys_pgmap` /
> `pages_free` / `page_start`) via `alloc_page` + `free_page` + `alloc_pages` +
> Mode-B CT release helper.

---

## 0. Decision (executive)

| Item | Value |
|------|-------|
| Decision | **SLICE E / CUT CU COMPLETE** |
| Path A (live) | **Accepted** — sole runtime ownership transferred |
| Path A (cut kind) | **Yes** — one multi-symbol ownership cut, not four Path B leaves |
| Implementation | **Done** — [`cut-cu-implementation.md`](cut-cu-implementation.md) |
| Inventory / gates | **103 / 138**; master gate ON |

---

## 1. Ownership transition

### 1.1 CURRENT (post–Cut CT)

| Domain | Owner |
|--------|-------|
| `sys_pgmap` runtime writes | **Split:** Rust CT helper + FASM `alloc_page` / `alloc_pages` / `free_page` |
| `pages_free` runtime writes | **FASM:** alloc/free/alloc_pages + `release_pages` Mode-A store |
| `page_start` runtime writes | **FASM:** `alloc_page`, `free_page` |
| `page_end` | **FASM** boot (`init_page_map`); runtime RO |
| `alloc_page` / `free_page` / `alloc_pages` | **FASM** bodies |
| `release_bitmap_page_without_cursor_update` | **Rust (CT)** — BTS + delta; no counter; no cursor |
| `release_pages` orchestration | **FASM** — mutex / PTE / `invlpg` / Mode-A batch |
| `map_page` / fault / CR3 / PTE bypasses | **FASM** |

### 1.2 FUTURE (Slice E — design target)

| Domain | Owner |
|--------|-------|
| `sys_pgmap` runtime writes | **Rust only** (boot `init_page_map` remains FASM) |
| `pages_free` runtime writes | **Rust only** (Mode B; no FASM Mode-A store) |
| `page_start` runtime writes | **Rust only** (`alloc_page` + `free_page`) |
| `page_end` | **FASM** boot; Rust **reads** only |
| `alloc_page` / `free_page` / `alloc_pages` | **Rust** (public symbols preserved) |
| CT release helper | **Rust Mode B** — BTS + `pages_free += delta`; still **no** `page_start` |
| `release_pages` orchestration | **FASM** — calls Mode-B helper; **no** competing counter store |
| Virtual / fault / CR3 | **FASM** unchanged |

### 1.3 Sole-writer invariants (post–Slice E)

1. Exactly **one** runtime writer family for `sys_pgmap`: the Slice E Rust API.  
2. Exactly **one** runtime writer family for `pages_free`: the Slice E Rust API (including Mode-B helper).  
3. Exactly **one** runtime writer family for `page_start`: Rust `alloc_page` + `free_page`.  
4. **Never** Mode A and Mode B simultaneously for `pages_free`.  
5. Boot `init_page_map` may write all three once; afterward Rust owns runtime mutation.  
6. Diagnostic save/restore of `pages_free` (existing smoke pattern) must save/restore around tests only — not a policy writer.

### 1.4 Transition diagram

```text
BEFORE (CT):
  release_pages ──call──► CT helper (map only) ──delta──► FASM ebp ──store──► pages_free
  alloc*/free* ──FASM──► map + pages_free + page_start

AFTER (Slice E):
  release_pages ──call──► CT helper Mode B (map + pages_free)     ; no FASM store
  alloc*/free* ──Rust──► map + pages_free + page_start (as applicable)
```

---

## 2. Mode A → Mode B

### 2.1 Definitions

| Mode | Who writes `pages_free` on release path | Batching |
|------|------------------------------------------|----------|
| **A (current)** | FASM `release_pages` final `mov [pages_free], ebp` after summing CT deltas | Local EBP under mutex |
| **B (Slice E)** | Rust CT helper (or internal sibling) does wrapping `pages_free += delta` per successful free | Optional internal accumulate; **FASM must not store** |

End-state equivalence (map + final `pages_free` + unchanged `page_start`) is already covered by PGBM Mode A/B tests. Mid-loop observability of `[pages_free]` may differ under Mode B; that is accepted under `pg_data.mutex` (same race class as today’s cli vs mutex).

### 2.2 Per-writer `pages_free` ownership after Slice E

| Symbol | Who updates `pages_free` | When | How | Batching | Wrapping | Old-bit |
|--------|--------------------------|------|-----|----------|----------|---------|
| `alloc_page` | Rust | On success: after `dec` check passes; on early/post-dec OOM: force `=1` | `dec` then maybe force; scan-miss: **no** write | N/A | wrapping `dec` / force | N/A (BTR path) |
| `free_page` | Rust | After BTS | `+= 1` iff OLD==0 (`cmc`/`adc` polarity) | N/A | wrapping add | OLD from BTS |
| `alloc_pages` | Rust | On success after map clear | `-= need_bytes*8` | N/A | wrapping sub | N/A |
| `release_bitmap_page_without_cursor_update` | Rust Mode B | Per present free | `+= delta` with delta=`!OLD` | Prefer **per-call** update (simplest; mutex held) | wrapping add | OLD from BTS |
| `release_pages` | **Nobody** for counter | — | Remove EBP load / `add ebp,eax` / final store | Deleted | — | — |

### 2.3 Preferred Mode B helper shape

Keep the **public** plain-call ABI shape familiar to `release_pages`:

| Channel | Mode A (CT today) | Mode B (Slice E) |
|---------|-------------------|------------------|
| IN EAX | page_index | page_index (**unchanged**) |
| OUT EAX | delta ∈ {0,1} | delta ∈ {0,1} (**unchanged** — still useful for debug; caller ignores) |
| Map | BTS free polarity | same |
| `pages_free` | none | wrapping `+= delta` |
| `page_start` | never | **never** |
| Preserve | EBX, ECX, EDX, ESI, EDI, EBP | **same** (EBP no longer accumulator, but still preserve) |

Internal freestanding body (reloc-free):  
`stdcall(page_index, map*, pages_free*) -> delta` behind the plain-call trampoline (inject `sys_pgmap` + `&pg_data.pages_free`).

**Do not** change the public name to imply “free_page”.

---

## 3. ABI audit — `alloc_page`

### 3.1 Legacy FASM (LOCAL FACT — `memory.inc`)

```text
proc alloc_page
  pushfd; cli; push ebx
  if [pages_free] <= 1 → [pages_free]:=1; EAX:=0; pop ebx; popfd; ret
  ebx := [page_start]; ecx := [page_end]
  scan: bsf eax,[ebx]; if ZF: ebx+=4; until ebx>=ecx → miss
  miss: pop ebx; popfd; EAX:=0; ret          ; pages_free UNCHANGED
  found:
    dec [pages_free]; if ZF → OOM stub
    btr [ebx], eax
    [page_start] := ebx
    EAX := ((ebx-sys_pgmap)*8 + bit) << 12
    pop ebx; popfd; ret
  OOM stub: [pages_free]:=1; EAX:=0; pop ebx; popfd; ret
```

| Item | Contract |
|------|----------|
| Convention | plain `call` / `ret` |
| Input | none |
| Output | EAX = phys page base (`index<<12`) or **0** |
| Preserved | EBX (push/pop); EFLAGS restored via `popfd` |
| Clobbered during body | EAX, ECX; EFLAGS until `popfd` |
| Caller-visible flags | **Restored** to entry via `popfd` (not a return channel) |
| DF | unchanged |
| Stack | no args |
| IRQ | `cli` for body; restore IF via `popfd` |
| Map | BTR clear bit (allocate) |
| `pages_free` | early OOM / post-dec OOM force `=1`; success `dec`; scan-miss untouched |
| `page_start` | set to found dword on success |

### 3.2 Future Rust ABI

| Layer | Design |
|-------|--------|
| Public symbol | `alloc_page` — same plain ABI as above |
| Trampoline | `pushfd; cli; push ebx` (or equivalent preserve) → stdcall Rust → restore → `ret` |
| Freestanding | `stdcall(map*, page_start*, page_end*, pages_free*) -> phys_or_0` reloc-free inject |
| Gate | future `USE_RUST_ALLOC_PAGE` (name illustrative; not created now) under a **single Slice E ownership master** or coordinated gates — see §15 |

Do **not** reuse the CT helper ABI (page_index → delta). Alloc is a different operation (BTR + cursor + OOM).

### 3.3 Risks

- OOM force=`1` vs scan-miss no-force must not be conflated.  
- `cli`/`popfd` discipline (REG lessons: do not invent CF return).  
- Dual ownership if Slice E gates are half-enabled — forbidden (§15).

---

## 4. ABI audit — `free_page`

### 4.1 Legacy FASM

```text
proc free_page          ; EAX = phys (any offset in page)
  pushfd; cli
  page_index := EAX >> 12
  bts [sys_pgmap], page_index     ; CF=OLD
  cmc; adc [pages_free], 0        ; +1 iff OLD==0
  dword := sys_pgmap + ((page_index>>3) & ~3)
  if [page_start] > dword: [page_start] := dword
  popfd; ret
```

| Item | Contract |
|------|----------|
| Convention | plain `call` / `ret` |
| Input | EAX = physical address |
| Output | void (EAX clobbered) |
| Preserved | none guaranteed except via pushfd for flags; typical callers do not rely on other regs |
| Clobbered | EAX, EFLAGS (restored by popfd) |
| DF | unchanged |
| IRQ | `cli` / restore via `popfd` |
| Double-free | OLD=1 → no `pages_free` inflate |
| `page_start` | **may lower**; never raise |

### 4.2 Future Rust architecture (preferred)

**Dedicated public `free_page`** that owns BTS + counter + cursor.

Internal sharing (allowed):

```text
delta = bitmap_bts_free(map, page_index)     ; shared polarity with CT
pages_free += delta                          ; free_page path
maybe_lower_page_start(page_start*, map, page_index)
```

**Must not** implement public `free_page` as “call CT helper then hope”:

- CT Mode B would update `pages_free` but **not** cursor — incomplete.  
- CT returns delta in EAX, destroying page_index unless saved.  
- Calling CT from free_page would blur the hard free≠release invariant in call graphs.

CT helper remains **only** for the release path (no cursor).

### 4.3 Hard invariant

`free_page` **may** lower `page_start`.  
`release_bitmap_page_without_cursor_update` **must never** write `page_start`.  
Live proof: [`stage4-release-pages-ab.md`](stage4-release-pages-ab.md).

---

## 5. ABI audit — `alloc_pages`

### 5.1 Legacy FASM

| Item | Contract |
|------|----------|
| Convention | **stdcall** `alloc_pages(count)` → `ret 4` |
| Input | `[esp+4]` = requested page count |
| Output | EAX = phys base of run or **0** |
| IRQ | `pushfd; cli` … `popfd` |
| DF | `rep stosb` requires DF=0; body uses `cld` implicitly via house style — Rust must leave DF as on entry (prefer force DF=0 for stos then restore if changed) |
| Preserved | EBX, EDI (push/pop); ESI pushed only on success path |
| Granularity | `need_bytes = (count+7)>>3`; charge `need_bytes<<3` pages |
| Search | contiguous `0xFF` bytes from `[page_start]` to `[page_end]` |
| Clear | `rep stosb` zeros the run |
| `page_start` | **unchanged** |
| Fail | EAX=0; **no** force `pages_free=1`; needs `pages_free>=9` and enough FF capacity |
| Return encoding | `byte_off << 15` (= 8 pages per byte → `<<12` × 8) |

### 5.2 Relation to other primitives

| Share? | Verdict |
|--------|---------|
| Bitmap clear polarity | Yes — free bit = 1; allocate clears to 0 |
| `pages_free` update helper | Optional thin wrapping sub |
| Cursor adjust | **No** — must not touch `page_start` |
| Collapse to N×`alloc_page` | **Forbidden** — different search, charge, OOM, cursor |

### 5.3 Future Rust ABI

Public stdcall preserved. Freestanding inject:  
`stdcall(count, map*, page_start*, page_end*, pages_free*) -> phys_or_0`  
(`page_start*` is **read-only** for this symbol).

---

## 6. `page_start` ownership

### 6.1 Invariant

> Every **runtime** writer of `page_start` is Rust-owned after Slice E.

### 6.2 Writer inventory (verified)

| Symbol | Writes `page_start`? |
|--------|----------------------|
| `alloc_page` | **Yes** — set to found dword |
| `free_page` | **Yes** — maybe lower |
| `alloc_pages` | **No** |
| `release_pages` | **No** |
| CT helper | **No** |
| `init_page_map` | **Yes** — boot only |

Source: `memory.inc` + [`stage4-bitmap-writers.json`](stage4-bitmap-writers.json).

### 6.3 Future boundary

- Rust `alloc_page` / `free_page` are the only runtime writers.  
- Rust `alloc_pages` / Mode-B helper must assert (tests) cursor unchanged.  
- FASM must not `mov [page_start], …` after boot.

### 6.4 Test matrix

| Case | Expect |
|------|--------|
| Successful `alloc_page` | cursor dword = allocated bit’s dword |
| `free_page` below cursor | `page_start` decreases (or equal if same dword rules) |
| `free_page` at/above cursor | unchanged |
| `AllocPages` | Δ`page_start` = 0 |
| `ReleasePages` | Δ`page_start` = 0 (A/B harness) |
| Mixed alloc/free/release | digest + cursor match oracle |

---

## 7. `pages_free` ownership

### 7.1 Invariant

> Exactly one runtime ownership model for `pages_free` — Rust Slice E API.  
> No competing FASM Mode-A store after activation.

### 7.2 Logical layers (keep distinct)

| Layer | Meaning |
|-------|---------|
| Bitmap mutation | BTS/BTR/`stos` of free bits |
| Counter mutation | `pages_free` arithmetic |
| Allocation charge | How many pages the op claims (1 vs `need*8`) |

### 7.3 Semantics table

| Op | Bitmap | Counter |
|----|--------|---------|
| `alloc_page` success | BTR 1→0 | `dec`; refuse if would hit 0 → OOM force 1 |
| `alloc_page` early OOM | none | force `=1` |
| `alloc_page` scan miss | none | **unchanged** |
| `free_page` | BTS | `+= !OLD` |
| `alloc_pages` success | clear FF run | `-= need_bytes*8` |
| `alloc_pages` fail | none | unchanged |
| Mode-B release helper | BTS | `+= !OLD` |
| Boot init | build map | set absolute count |

Wrapping: match FASM `adc`/`sub`/`dec` 32-bit wrap (oracle `wrapping_*`).

---

## 8. `release_pages` transition (highest risk)

### 8.1 CURRENT sequence (exact)

```text
release_pages:                    ; EAX=lin base, ECX=count
  push ebp,esi,edi,ebx
  esi := &page_tabs[lin>>12]; edi := lin
  mutex_lock(pg_data.mutex)
  ebp := [pages_free]             ; <<< Mode A LOAD — REMOVE in Slice E
@@:
  eax := 0; xchg [esi], eax       ; clear PTE
  invlpg [edi]
  test eax, 1
  jz .next
  shr eax, 12
  call release_bitmap_page_without_cursor_update   ; CT / Mode B later
  add ebp, eax                    ; <<< Mode A ACCUM — REMOVE in Slice E
.next:
  edi += 4096; esi += 4; loop
  mov [pages_free], ebp           ; <<< Mode A STORE — REMOVE in Slice E
  mutex_unlock
  pop …; ret
```

### 8.2 FUTURE sequence

```text
release_pages:
  push ebp,esi,edi,ebx            ; keep frame for ABI stability (ebp unused for counter)
  … same PTE setup …
  mutex_lock(pg_data.mutex)
  ; NO ebp := [pages_free]
@@:
  … same xchg / invlpg / present test …
  shr eax, 12
  call release_bitmap_page_without_cursor_update   ; Mode B updates pages_free
  ; NO add ebp, eax
.next:
  …
  ; NO mov [pages_free], ebp
  mutex_unlock
  pop …; ret
```

### 8.3 Exact FASM edits later (checklist — do not apply now)

1. Delete `mov ebp, [pg_data.pages_free]`.  
2. Delete `add ebp, eax` after helper call.  
3. Delete `mov [pg_data.pages_free], ebp`.  
4. Keep mutex / PTE / `invlpg` / loop / register frame.  
5. Keep `call release_bitmap_page_without_cursor_update`.  
6. Update comments: Mode B; MUST NOT call `free_page`.  
7. Optionally leave EBP push/pop for stack symmetry / future use — or drop if proven unused (preserve EBX.. for helper ABI).

### 8.4 Atomicity / synchronization

| Concern | Resolution |
|---------|------------|
| Per-page vs batched counter | Per-page Mode B under **same mutex** is end-state equivalent; preferred for simplicity |
| Mid-loop `[pages_free]` visibility | May update earlier than Mode A; accepted |
| `cli` alloc vs mutex release | Legacy race class; do not expand scope to “fix” it in Slice E |
| Double-count | Gate design must make Mode A OFF when Mode B ON (§15) |
| Interrupt in helper | Helper itself need not `cli`; caller holds mutex; alloc paths use `cli` separately |

### 8.5 Public vs internal call

`release_pages` continues to call the **public** plain-call helper (same symbol).  
Internal freestanding Mode B body is trampoline-private.  
Do not have `release_pages` call `free_page`.

---

## 9. Shared internal Rust primitives

### 9.1 Allowed (narrow)

| Internal | Used by |
|----------|---------|
| `bitmap_btr_alloc` / dword BSF scan | `alloc_page` |
| `bitmap_bts_free` → delta | `free_page`, Mode-B helper |
| `pages_free_add_delta` / `sub_charge` / `force_oom_one` | alloc/free/bulk/helper |
| `page_start_set` / `page_start_lower_to_dword` | `alloc_page` / `free_page` only |
| `find_ff_run` + `clear_ff_run` | `alloc_pages` only |

### 9.2 Forbidden over-generalization

- No generic `Allocator` trait / framework in kernel.  
- No single `free(page, flags)` that merges release+free.  
- No collapsing `alloc_pages` into a loop of `alloc_page`.  
- Each public symbol keeps independent host tests.

---

## 10. State ownership matrix

| State | Current owner | Future owner | Allowed readers | Allowed writers (runtime) | Boot writer |
|-------|---------------|--------------|-----------------|---------------------------|-------------|
| `sys_pgmap` | split | **Rust** | many (implicit via API) | `alloc_page`, `free_page`, `alloc_pages`, Mode-B helper | `init_page_map` |
| `pages_free` | FASM | **Rust** | sysfn 16, taskman, disk_cache, getcache, Cut O smoke (RO/save-restore), … | same four Rust ops | `init_page_map` |
| `page_start` | FASM | **Rust** | `alloc_page`, `alloc_pages` (RO) | `alloc_page`, `free_page` only | `init_page_map` |
| `page_end` | FASM | **FASM** (RO at runtime) | Rust alloc* (RO inject) | **none** at runtime | `init_page_map` |

Unauthorized writer = any FASM `bts`/`btr`/`stos` into map or `mov` to `pages_free`/`page_start` outside boot / gated diagnostics after Slice E ON.

---

## 11. Oracle design

### 11.1 Base

| Item | Value |
|------|-------|
| File | `rust_kernel/kolibri_utils/src/pg_bitmap_oracle.rs` |
| Models | `IndependentBitmap` vs `FasmBitmapEmu` |
| Seed | `0x5047424D` (`PGBM`) |
| Floor | ≥ **50 000** randomized combined traces |

Oracle remains **independent** of production Rust blobs (emu + set model, not the cut implementation).

### 11.2 Required coverage

**alloc_page:** success; early OOM (`pages_free<=1`); post-dec OOM; scan miss; cursor move; bitmap; counter.  

**free_page:** allocated; already-free; cursor lower / no-lower; counter.  

**alloc_pages:** counts 0/1/8/16; FF-run hit; fragmented miss; fail; counter; cursor unchanged.  

**release bitmap Mode B:** allocated; already-free; counter; cursor unchanged; ≡ Mode A end-state.  

**Combined traces:** alloc→free; alloc→release; alloc_pages→release; fragmented alloc/free; repeated free; mixed; boundary indices; wrapping counter stress.

### 11.3 Diff strategy

Identical op streams → identical `(map digest, pages_free, page_start_off)`.  
Fail closed on any divergence before production gate ON.

---

## 12. ABI smoke plan

| Symbol | Marker (illustrative) | Checks |
|--------|----------------------|--------|
| `alloc_page` | `ALPG` | EAX phys\|0; EBX preserved; EFLAGS restored; DF; SP; `cli` body (IF restored); OOM canary `pages_free` |
| `free_page` | `FRPG` | void; `page_start` may drop; `pages_free` +0/+1; DF; SP |
| `alloc_pages` | `ALPS` | stdcall cleanup; EAX; cursor unchanged; charge; DF |
| Mode-B helper | `RBPB` / `RPBM` | delta; preserve EBX..EBP; `page_start` canary; `pages_free` += delta; DF; SP |

Rules:

- No public CF return contract (REG-018).  
- Prefer save/restore disposable map bits or spare pages.  
- Omit heavy `.bss` smoke iglobals if REG-012 tight (Cut CN/CO pattern) — host differentials remain mandatory.

---

## 13. QEMU soak plan

### 13.1 Reuse

| Tool | Path / doc |
|------|------------|
| Allocsoak PE | `tools/allocsoak/asoakdrv.asm` |
| QMP sampler | `scripts/qmp_allocator_soak.py` |
| Release/free A/B | `scripts/qmp_release_free_ab.py` |
| Pressure/recovery | CT soak pattern / [`stage4-allocator-soak-*.md`](stage4-allocator-soak-design.md) |
| Desktop | existing QMP non-black + RESET=0 |

### 13.2 Required production soak (Slice E ON)

1. Boot baseline (desktop non-black, RESET=0)  
2. Repeated `AllocPage`  
3. `FreePage`  
4. `AllocPages` (incl. 8/32-class sizes used by heap/AHCI)  
5. Fragmentation / reuse  
6. `ReleasePages` path (KernelAlloc/free or A/B driver)  
7. Pressure → recovery (safe ceiling; **not** live `pages_free<=1`)  
8. Filesystem activity (`--disk` optional but recommended)  
9. GUI / desktop stability  
10. ON ×3  
11. A/B: Slice E OFF (full FASM Mode A) vs ON (Rust Mode B)

### 13.3 Explicit non-requirements

- Live early-OOM `pages_free<=1` — **not** a gate ([`stage4-early-oom-experiment.md`](stage4-early-oom-experiment.md)).  
- CR3 / TLB proof — out of scope.  
- Migrating `map_page` — out of scope.

### 13.4 Release/free A/B must still prove

| Path | Δ`pages_free` | Δ`page_start` |
|------|---------------|---------------|
| FreePage | +1 on success | may be ≠ 0 |
| ReleasePages | +1 on success | **0** |

---

## 14. Memory / blob budget

### 14.1 Fixed pack (do not move)

| Symbol | Value |
|--------|-------|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `0x008E000` |
| `SLOT_BASE` | `0x0090000` |
| CT end `.bss` | `OS_BASE+0x8C543` |

### 14.2 Estimate (non-binding)

Combined Rust blobs **~0.5–1.5 KiB** class; CT helper already 49 B.

### 14.3 Mandatory pre-activation measurements

Before any production gate ON:

1. Compile each blob; record **exact byte size**.  
2. Record **relocation count** (target **0** reloc-free inject style).  
3. Measure combined `.text` / image growth (`kernel.mnt` size).  
4. Measure end `.bss`; verify `end + PAGE_SIZE < TMP_STACK_TOP`.  
5. Confirm REG-012 pack unchanged.  
6. If assert fails → **block cut**; do **not** raise TMP/sys_proc/SLOT_BASE without a separate memory-layout decision.

---

## 15. Rollback design

### 15.1 Principle

Rollback must restore **one** `pages_free` ownership model — never leave Mode A and Mode B both live.

### 15.2 Recommended gate structure (design only)

Prefer a **master ownership gate** (illustrative name: `USE_RUST_PHYS_BITMAP_OWNERSHIP`) that switches **all** of:

- `alloc_page`  
- `free_page`  
- `alloc_pages`  
- CT helper Mode B vs Mode A  
- `release_pages` Mode-A ebp path present vs absent  

Alternatively: four coordinated gates that CI/`build.toml` enables atomically — same rule: no partial ON.

### 15.3 OFF behavior

| Piece | OFF |
|-------|-----|
| `alloc_page` / `free_page` / `alloc_pages` | Original FASM bodies |
| CT helper | FASM or CT Rust Mode **A** (map-only) — must match `release_pages` Mode A store |
| `release_pages` | Mode A ebp load/add/store restored |

### 15.4 ON behavior

| Piece | ON |
|-------|-----|
| Three alloc symbols | Rust |
| CT helper | Mode B (`pages_free` write) |
| `release_pages` | No Mode A counter ops |

**Illegal:** Rust alloc ON + FASM Mode A release store still present.  
**Illegal:** Mode B helper ON + FASM Mode A store still present.

---

## 16. Cut boundary

### 16.1 IN Slice E

| Symbol | Role |
|--------|------|
| `alloc_page` | single-page alloc + cursor + OOM |
| `free_page` | single-page free + maybe cursor |
| `alloc_pages` | FF-run bulk alloc |
| `release_bitmap_page_without_cursor_update` | Mode B release bitmap + counter |

### 16.2 OUT of Slice E

`map_page`, `init_page_map`, PTE writers, `invlpg`, `pg_data.mutex` ownership, `page_fault_handler`, CR3 / process / scheduler, heap VA policy, `commit_pages` / `unmap_pages` orchestration (except they **call** the public API).

### 16.3 Boundary change rule

Only revise the IN set if a new **runtime** writer of map/`pages_free`/`page_start` is discovered. Current inventory: **none unresolved**.

---

## 17. Path A reassessment

| Criterion | Slice E design |
|-----------|----------------|
| Coherent Rust-owned shared state | **Yes** — map + counter + cursor |
| Multiple related functions | **Yes** — four symbols, one domain |
| Meaningful state ownership | **Yes** — sole runtime writers |
| Reduced FASM↔Rust dual crossings | **Yes** — dual writers eliminated; call crossings remain at public symbols |
| Subsystem oracle | **Yes** — extended PGBM |
| Subsystem production soak | **Yes** — §13 |
| Coherent rollback | **Yes** — §15 single ownership model |

**Verdict:** Slice E, **when authorized and completed as one ownership cut**, is genuine **Path A** for the physical bitmap allocator subsystem — **not** four opportunistic Path B leaves.

**Today:** Path A is **not** live (state still mostly FASM). Do not claim Path A completion until Slice E production evidence lands.

Cut naming (future): may be Cut CU or a multi-symbol Path A cut id — **not created by this document**.

---

## 18. Prerequisites checklist (before implementation authorization)

- [x] Writer inventory complete  
- [x] free≠release proven live  
- [x] CT Mode A helper in production  
- [x] Post-CT ownership audit (Slice E selected)  
- [x] This cut plan (ABI / Mode B / soak / rollback)  
- [ ] Explicit human/agent authorization to implement Slice E  
- [ ] Extended PGBM combined oracle PASS  
- [ ] Blobs measured; REG-012 assert PASS  
- [ ] ABI smokes / host differentials PASS  
- [ ] QEMU OFF/ON/A-B/ON×3 + release/free A/B + pressure/recovery PASS  

---

## 19. Known risks

1. Partial gate enable → dual `pages_free` writers (catastrophic).  
2. Accidental `free_page` alias for release path → `page_start` ABI break.  
3. Mode B mid-loop counter visibility surprises for RO readers (rare).  
4. `alloc_pages` DF / `rep stos` fidelity.  
5. OOM quirk vs scan-miss conflation.  
6. Blob/`.bss` growth vs REG-012 (measure; don’t raise pack casually).  
7. Claiming Path A before soak evidence.  
8. Expanding scope into `map_page`/fault “while we’re here.”

---

## 20. Document history

| Date | Note |
|------|------|
| 2026-08-14 | Slice E ownership cut plan drafted — **READY FOR AUTHORIZATION**; production migration NONE |
