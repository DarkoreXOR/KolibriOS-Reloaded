# Stage-4 Next Ownership Audit (Post–Cut CT)

**Date:** 2026-08-14  
**Status:** audit complete (historical) — **Slice E later implemented as Cut CU**  
**Inventory at audit time:** **100 / 136**  
**Inventory after Cut CU:** **103 / 138** — see [`cut-cu-implementation.md`](cut-cu-implementation.md)  
**Cut CT:** COMPLETE — Mode A leaf  
**Cut CU / Slice E:** **COMPLETE** — do **not** re-open this audit as authorization  
**Parent:** [`stage4-ownership-design.md`](stage4-ownership-design.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> Fresh ownership audit of the physical allocator domain after Cut CT.
> Architectural / ownership only at the time of writing. Slice E was later
> authorized and completed as Cut CU.

---

## 0. Verdict (executive)

| Question | Answer |
|----------|--------|
| Does Cut CT complete Stage-4 allocator ownership? | **No** (at audit time) |
| Does CT materially change the ownership graph? | **Yes** — one runtime bitmap writer is now Rust; Mode-A `pages_free` / cursor / alloc* remain FASM |
| Smallest coherent next ownership slice? | **Slice E** — `alloc_page` + `free_page` + `alloc_pages` + Mode-B `pages_free` on the CT release helper |
| Intermediate slices A–D? | **Rejected** — dual ownership of `sys_pgmap` / `pages_free` / `page_start` |
| Path A justified at audit time? | **No** — isolated helper ≠ subsystem |
| Path A shape when Slice E lands? | **Yes** — bitmap domain as sole Rust runtime writer; PTE/fault/CR3 stay FASM |
| Decision (audit) | **NEXT OWNERSHIP SLICE READY** |
| Later outcome | **Cut CU COMPLETE** — Slice E implemented |

---

## 1. Does Cut CT change the architectural ownership graph?

### 1.1 Before CT (historical)

```text
FASM sole runtime writer of sys_pgmap / pages_free / page_start
  ├── alloc_page
  ├── alloc_pages
  ├── free_page
  └── release_pages (inline BTS + Mode-A pages_free)
```

### 1.2 After CT (authoritative now)

```text
sys_pgmap runtime writers (logical):
  ├── Rust: release_bitmap_page_without_cursor_update   [Cut CT]
  ├── FASM: alloc_page
  ├── FASM: alloc_pages
  └── FASM: free_page

pages_free runtime writers:
  ├── FASM: alloc_page (dec / OOM force=1)
  ├── FASM: alloc_pages (sub)
  ├── FASM: free_page (adc)
  └── FASM: release_pages Mode-A final store (uses CT delta)

page_start runtime writers:
  ├── FASM: alloc_page (advance cursor dword)
  ├── FASM: free_page (maybe lower)
  └── (release_pages / CT helper: NEVER)
```

### 1.3 What CT did / did not do

| Effect | Status |
|--------|--------|
| Named production boundary for release ≠ `free_page` | **Done** |
| Rust owns release BTS + delta return | **Done** |
| Mode-A `pages_free` batching still FASM | **Unchanged** |
| `page_start` still FASM-only | **Unchanged** |
| Sole runtime bitmap writer | **Not achieved** (3 FASM co-writers remain) |
| Path A subsystem | **Not achieved** |

**Inference:** CT is a necessary **boundary fixture**, not an ownership completion.
It makes Slice E feasible without inventing the release/free split at migration
time. It does **not** make `alloc_page`-only (or any A–D slice) coherent.

---

## 2. Fresh ownership graph (LOCAL FACT)

Sources: `kernel/core/memory.inc`, `kernel/init.inc`, caller grep under `kernel/`,
[`stage4-bitmap-writers.json`](stage4-bitmap-writers.json), Cut CT docs.

### 2.1 Objects

| Object | Owner today | R | W (runtime) | W (boot) |
|--------|-------------|---|--------------|----------|
| `sys_pgmap` | **split** | many | CT Rust + alloc_page + alloc_pages + free_page | `init_page_map` |
| `pg_data.pages_free` | **FASM** | many (sysfn, taskman, disk_cache, …) | alloc_page, alloc_pages, free_page, release_pages | `init_page_map` |
| `page_start` | **FASM** | alloc* | alloc_page, free_page | `init_page_map` |
| `page_end` | **FASM** | alloc* | — | `init_page_map` |
| `pg_data.mutex` | **FASM** | — | release_pages / commit_pages / heap | — |
| `page_tabs` / PTE / `invlpg` | **FASM** | AQ/BL/CI RO | map_page + many bypasses | — |
| CR3 / fault policy | **FASM** | — | taskman / `page_fault_handler` | — |

### 2.2 Function matrix

| Symbol | Owner | Reads | Writes | Callers (kernel) | Callees | ABI | IRQ | DF |
|--------|-------|-------|--------|------------------|---------|-----|-----|-----|
| `alloc_page` | FASM | `pages_free`, `page_start`, `page_end`, `sys_pgmap` | `pages_free`, `page_start`, `sys_pgmap` (btr) | memory, heap, taskman, dll, framebuffer, ahci, kernel.asm, PF paths | — | plain; out EAX=phys\|0; preserves EBX via push; popfd | `pushfd; cli` | unchanged |
| `free_page` | FASM | `sys_pgmap`, `page_start` | `sys_pgmap` (bts), `pages_free`, maybe `page_start` | heap, sys32, background, taskman, dll, ahci, rd, kernel.asm, memory | — | plain; in EAX=phys; void | `pushfd; cli` | unchanged |
| `alloc_pages` | FASM | `pages_free`, `page_start`, `page_end`, map bytes | map (`rep stosb`), `pages_free`; **not** `page_start` | heap, memory, ntfs, framebuffer, ahci | — | stdcall(count); out EAX=phys\|0 | `pushfd; cli` | DF used by `rep stosb` (must restore if changed) |
| `release_pages` | FASM orch. | PTE, `pages_free` | PTE clear, `invlpg`, Mode-A `pages_free` | heap, ntfs | **CT helper**, mutex | plain; EAX=lin, ECX=count | mutex (not cli) | unchanged |
| `release_bitmap_page_without_cursor_update` | **Rust (CT)** | map | map bit only | `release_pages` only | Rust blob | plain; EAX in/out delta; preserve EBX..EBP | none | unchanged |
| `init_page_map` | FASM boot | e820 | map, `pages_free`, `page_start`, `page_end` | boot | — | boot | N/A | cld |

### 2.3 How the Rust helper connects to FASM

```text
release_pages (FASM)
  mutex_lock
  ebp := [pages_free]                 ; Mode A accumulator — FASM
  loop:
    xchg PTE→0; invlpg                ; FASM virtual domain
    if present:
      page_index := phys>>12
      call release_bitmap_page_without_cursor_update  ; → Rust CT
        ; Rust: BTS map; return delta∈{0,1}; NO pages_free; NO page_start
      ebp += delta                    ; FASM
  [pages_free] := ebp                 ; FASM sole store on this path
  mutex_unlock
```

Trampoline (CT ON): push EBX..EBP → `stdcall rust_*(page_index, sys_pgmap)` →
pop → `ret`. Blob 49 B, 0 relocs. OFF path keeps extracted FASM body.

**Do not call the system Rust-owned** because this one helper is Rust.

### 2.4 PE exports (driver ABI — unchanged)

| Export | Symbol | Convention |
|--------|--------|------------|
| `AllocPage` | `alloc_page` | plain / EAX out |
| `AllocPages` | `alloc_pages` | stdcall / count |
| `FreePage` | `free_page` | plain / EAX in |
| `ReleasePages` | `release_pages` | plain / EAX+ECX |

---

## 3. Candidate ownership slices

### Evaluation legend

- **State Rust-owned** = sole runtime writer after cut  
- **Crossings** = remaining FASM↔Rust state co-ownership edges  
- Prefer smallest **coherent** boundary, not smallest function

### A. `alloc_page` only — REJECTED

| Question | Answer |
|----------|--------|
| Rust state | Partial: would write map/`pages_free`/`page_start` on alloc path only |
| FASM writers remain | `free_page`, `alloc_pages`, release Mode-A |
| Crossings | **High** — 3 co-writers |
| `pages_free` move? | Split (alloc Rust vs free/release FASM) |
| `page_start` move? | Split (alloc advances; free lowers in FASM) |
| `sys_pgmap` exclusive? | **No** |
| `release_pages` coherent? | Yes (unchanged) but dual map ownership |
| Coupling | **Increases** — two allocator semantics |
| Rollback | Gateable but leaves split-brain if half-enabled |
| Oracle / soak | Exists for alloc_page alone, but proves leaf not ownership |
| Needs PTE/CR3? | **No** |
| Preserves free≠release? | Yes (untouched) |

**Quantified problem:** After A, Rust and FASM both mutate the same three
globals on interleaved cli/mutex schedules → dual ownership by definition.

### B. `alloc_page` + `free_page` — REJECTED

| Question | Answer |
|----------|--------|
| `page_start` exclusive? | **Yes** (only runtime writers) |
| `pages_free` exclusive? | **No** — `alloc_pages` + release Mode-A remain FASM |
| `sys_pgmap` exclusive? | **No** — `alloc_pages` + CT already; wait CT is Rust — FASM `alloc_pages` remains |
| Crossings | Medium–high |
| Coupling | Still split on bulk alloc + release counter |

`page_start` invariant alone is insufficient for a coherent subsystem.

### C. `alloc_page` + `alloc_pages` — REJECTED

Leaves `free_page` as FASM owner of cursor lowers and free accounting — worst
split for `page_start` / double-free polarity vs Rust alloc.

### D. `alloc_page` + `free_page` + `alloc_pages` — INCOMPLETE

| Question | Answer |
|----------|--------|
| Map writers | Rust for alloc/free/bulk; CT already Rust for release BTS → **map exclusive** |
| `pages_free` | **Still split** — release Mode-A final `mov [pages_free],ebp` is FASM |
| `page_start` | Rust exclusive |
| Crossings | One critical: Mode-A counter ownership |
| Coherent? | **Almost** — fails sole `pages_free` ownership (§16 invariant 2) |

Without Mode B on the CT helper (or a batch Rust API that owns the counter),
D leaves a permanent FASM writer of `pages_free` on every `ReleasePages` path
(heap free, NTFS, …).

### E. D + Mode-B pages_free on CT release helper — **ACCEPTED**

| Question | Answer |
|----------|--------|
| Rust-owned state | `sys_pgmap`, `pages_free`, `page_start` (runtime) |
| Functions | `alloc_page`, `free_page`, `alloc_pages`, CT helper upgraded to Mode B (or sibling that owns counter) |
| Remaining FASM | `release_pages` orch. (mutex/PTE/`invlpg`/loop), `init_page_map`, `map_page`/PTE/fault/CR3, RO `pages_free` readers |
| Crossings | Call crossings only (symbols + release call-out); **zero** dual state writers |
| `pages_free` move? | **Yes** (sole Rust) |
| `page_start` move? | **Yes** (sole Rust) |
| `sys_pgmap` exclusive? | **Yes** (runtime) |
| `release_pages` coherent? | **Yes** — still ≠ `free_page`; Mode B end-state ≡ Mode A (oracle) |
| Coupling | **Decreases** |
| Rollback | Multi-gate or single ownership gate; OFF restores FASM bodies + Mode A |
| Oracle | PGBM already models alloc/free/alloc_pages + Mode A/B |
| Soak | allocsoak + release/free A/B + pressure/recovery + desktop ON×3 |
| Needs PTE/CR3? | **No** |
| Preserves free≠release? | **Hard requirement** — CT helper must never gain `page_start` stores |

### F. Other combinations considered

| Idea | Verdict |
|------|---------|
| Mode B on CT helper alone | Rejected — worsens `pages_free` split (Rust release vs FASM alloc/free) |
| `free_page` → call CT helper from FASM | Not an ownership transfer; still FASM policy; loses page index in EAX unless saved |
| Bundle `map_page` | Rejected — different domain; PTE bypass writers |
| Full `release_pages` to Rust | Rejected for first slice — imports mutex/PTE/`invlpg` |

---

## 4. `alloc_page` deep audit

### 4.1 Algorithm (LOCAL FACT)

```text
pushfd; cli; push ebx
if pages_free <= 1 → pages_free:=1; EAX:=0; restore; ret     ; early OOM
ebx := page_start; ecx := page_end
scan dwords: bsf [ebx] → if ZF, ebx+=4 until page_end
if miss: EAX:=0; restore; ret                                 ; scan miss: NO force=1
found:
  dec pages_free; if ZF → OOM stub (force=1, EAX=0)           ; after dec
  btr [ebx], bit
  page_start := ebx
  page_index := bit + (ebx-sys_pgmap)*8
  EAX := page_index << 12
restore; ret
```

### 4.2 ABI / flags / DF

| Item | Contract |
|------|----------|
| In | none |
| Out | EAX = phys page base (4 KiB aligned) or 0 |
| Preserved | EBX (explicit); flags via popfd; other GPRs not intentionally clobbered beyond ECX/EAX use |
| Clobbered | EAX, ECX (scan), EFLAGS during body |
| DF | unchanged |
| Stack | plain `ret` |
| Alignment | always `index<<12` |

### 4.3 Interaction with CT helper

None today. Alloc uses **BTR** (allocate); CT uses **BTS** (free). Shared map
polarity only.

### 4.4 If `alloc_page` Rust while `free_page` FASM

| Split | Severity |
|-------|----------|
| `pages_free` | **Critical** — both write; OOM quirk vs free `adc` race under cli vs cli |
| `page_start` | **Critical** — alloc advances; free lowers; dual cursor policy |
| Bitmap logic | Duplicated BTR vs FASM BTS elsewhere |
| Cross-language invariants | Cursor dword alignment, free-bit polarity, OOM force=1 |
| Two allocator semantics | Yes — not a coherent subsystem |

---

## 5. `free_page` deep audit

### 5.1 Algorithm (LOCAL FACT)

```text
pushfd; cli
page_index := EAX >> 12
bts [sys_pgmap], page_index     ; CF=OLD
cmc; adc [pages_free], 0        ; +1 iff OLD==0
dword := sys_pgmap + ((page_index>>3) & ~3)
if page_start > dword: page_start := dword
popfd; ret
```

### 5.2 ABI

| Item | Contract |
|------|----------|
| In | EAX = physical address (any offset in page) |
| Out | void |
| IRQ | cli via pushfd |
| Double-free | OLD=1 → delta 0; silent |
| `page_start` | May lower; never raises |

### 5.3 Can it reuse the CT Rust primitive?

**Not as a drop-in.** CT deliberately omits `pages_free` and `page_start`.

A FASM wrapper would need:

1. Save page index before call (CT returns delta in EAX).  
2. `add [pages_free], delta` (or Mode B inside Rust).  
3. Separate dword/`page_start` update using saved index.

That is still **FASM-owned free_page policy**, not ownership transfer.

### 5.4 Future Rust shape (design only)

| Layer | Role |
|-------|------|
| Internal | Shared BTS+delta (same polarity as CT) |
| Public `free_page` | BTS+delta + `pages_free` + **maybe lower `page_start`** |
| Public CT helper | BTS+delta only; **never** `page_start`; Mode B may add `pages_free` |

Do **not** implement either approach in this audit.

---

## 6. `alloc_pages` deep audit

### 6.1 Semantics (LOCAL FACT)

| Item | Behavior |
|------|----------|
| Granularity | `need_bytes = (count+7)>>3`; charges `need_bytes*8` pages |
| Search | Contiguous run of `0xFF` **bytes** from `page_start` toward `page_end` |
| Clear | `rep stosb` zeros the run |
| `page_start` | **Unchanged** |
| Gate | Needs `pages_free >= 9` and enough FF-bytes after `(pages_free-9)>>3` |
| Fail | EAX=0; does **not** force `pages_free=1` |
| Return | Phys base = `byte_off << 15` (8 pages per byte → `<<15`) |

### 6.2 Relation to `alloc_page`

**Distinct search algorithm**, not a bulk `alloc_page` loop:

- Byte-run / 8-page granularity vs single-bit BSF  
- Does not move cursor  
- Different OOM policy  
- Different PE export convention (stdcall)

Preserve as a separate primitive in any future API.

---

## 7. `pages_free` ownership

### 7.1 Current writers

| Writer | Update style |
|--------|--------------|
| `alloc_page` | `dec`; OOM force `=1` |
| `alloc_pages` | `sub` charged pages |
| `free_page` | `adc` +0/+1 |
| `release_pages` | Mode-A batch then one `mov` |

CT helper: **does not** write `pages_free` (by design).

### 7.2 Options

| Option | Verdict |
|--------|---------|
| A. Only bitmap writes | Status quo after CT for release; insufficient for sole ownership |
| B. Bitmap + `pages_free` without `page_start` | Split-brain — alloc/free couple counter and cursor |
| C. Bitmap + `pages_free` + `page_start` | **Required** for coherent Slice E |
| D. All allocator state including PTE/mutex | Over-scope; imports virtual domain |

### 7.3 Required invariant (future)

> Exactly one runtime owner of `pages_free` updates (Rust), except boot
> `init_page_map` and gated diagnostic save/restore.

Mode B on the CT helper (wrapping `pages_free += delta` under release mutex)
is the release-path half of that invariant. It must land in the **same**
ownership cut as alloc/free/alloc_pages — never Mode A and Mode B together.

### 7.4 Interrupt / atomicity

| Path | Exclusion |
|------|-----------|
| alloc/free/alloc_pages | `cli` via pushfd |
| release_pages | `pg_data.mutex` |

Legacy already races cli vs mutex across paths. Mode B mid-loop counter
visibility may differ from Mode A; **end-state** must match (oracle covered).
Do not “fix” that race in the first ownership cut.

---

## 8. `page_start` ownership

### 8.1 Proven distinction

| Path | `page_start` |
|------|----------------|
| `free_page` | May lower if freed dword &lt; cursor |
| `release_pages` / CT | **Never** stores |
| `alloc_page` | Sets cursor to found dword |
| `alloc_pages` | Does not update |

### 8.2 Invariant

> Every **runtime** writer of `page_start` is Rust-owned.

Runtime writers = `{alloc_page, free_page}` only. Therefore cursor ownership
**requires** both symbols to move together (Slice B minimum for cursor alone).
Cursor-alone is **not** enough for subsystem coherence (§3.B).

Slice E satisfies the invariant and also closes map/`pages_free`.

---

## 9. `release_pages` after CT

### 9.1 Current split

| Layer | Owner |
|-------|-------|
| Mutex / lin→PTE / clear / `invlpg` / loop | FASM |
| Bitmap BTS + delta | Rust CT |
| Mode-A `pages_free` batch + store | FASM |
| `page_start` | untouched |

### 9.2 Effect of CT

- Hardens free≠release as a production API  
- Adds one FASM→Rust crossing per present page  
- Does **not** reduce dual map ownership vs alloc/free  

### 9.3 Implication of next ownership (Slice E)

| Choice | Recommendation |
|--------|----------------|
| Keep Mode A after Rust alloc* | **Forbidden** for sole `pages_free` ownership |
| Mode B: helper updates `pages_free` | **Required** in same cut |
| FASM loop still calls helper | **Yes** — preserve PTE/`invlpg`/mutex in FASM |
| Change `page_start` semantics? | **No** — helper must still never write cursor |
| Accumulate in Rust vs FASM ebp | Prefer Mode B inside helper; delete FASM ebp load/add/store |

Goal: ownership cut must **not** make `release_pages` more fragile — keep
orchestration identical; only relocate counter ownership into the existing
call-out.

---

## 10. Future Rust-owned allocator API (conceptual — do not implement)

### Public (preserve PE / in-kernel names)

| Symbol | Semantics must remain |
|--------|------------------------|
| `alloc_page` | BSF scan; OOM force=1; advances `page_start`; cli |
| `alloc_pages` | 0xFF-run; 8-page charge; no cursor move; stdcall |
| `free_page` | BTS; maybe lower `page_start`; cli |
| `release_bitmap_page_without_cursor_update` | BTS+delta; **no** cursor; Mode B `pages_free` after ownership cut |

### Internal

| Helper | Role |
|--------|------|
| Shared BTS/BTR polarity ops | Avoid divergent bit math |
| Context inject | `sys_pgmap`, `page_start*`, `page_end*`, `pages_free*` |

### Rust owns (runtime)

- `sys_pgmap` writes  
- `pages_free` writes  
- `page_start` writes  

### FASM may

- Read `pages_free` / call public symbols  
- Own `release_pages` PTE/mutex/`invlpg`  
- Own `init_page_map` then hand off  
- Own `map_page`, fault, CR3  

### FASM must not (after Slice E)

- Direct `bts`/`btr`/`stos` into `sys_pgmap`  
- Assign `pages_free` / `page_start` outside Rust API  

**Do not** collapse free/release into one generic free API.

---

## 11. Path A assessment

| Criterion | Today (post-CT) | After Slice E (prospective) |
|-----------|-----------------|-----------------------------|
| Coherent Rust state ownership | **No** | **Yes** (bitmap domain) |
| Multiple related functions | Helper only | alloc/free/bulk + release call-out |
| Reduced dual crossings | One new call crossing; dual writers remain | Dual writers eliminated |
| Real subsystem boundary | **No** | Physical bitmap allocator |
| Subsystem oracle | PGBM exists | Same + Mode B production |
| Production soak | Helper soak done | Full allocator soak required |

**Path A now?** **REJECTED** for opportunistic leaf bundling and for claiming
subsystem ownership from CT alone.

**Path A later?** **Justified as the shape of Slice E** when authorized as a
multi-symbol ownership cut — not as three separate Path B leaves.

---

## 12. Future oracle / soak (next slice)

### Reuse (already exist)

| Asset | Role |
|-------|------|
| `pg_bitmap_oracle.rs` PGBM | IndependentBitmap vs FasmBitmapEmu; 50k + quirks |
| CT `rbpb_*` | Helper differential |
| `asoakdrv` / ledger / QMP sampler | Direct Alloc/Free/AllocPages |
| `qmp_release_free_ab.py` | free≠release `page_start` |
| Pressure/recovery soak | Bounded stress |
| Desktop non-black + RESET=0 | Stability floor |

### New / extended requirements for Slice E

| Area | Requirement |
|------|-------------|
| Host differential | Combined traces: alloc ↔ free ↔ alloc_pages ↔ Mode-B release; assert map digest + `pages_free` + `page_start` |
| Mode B | End-state ≡ Mode A; mid-loop readability may differ |
| ABI smoke | Per public symbol (AllocPage/FreePage/AllocPages + release helper preserve set) |
| Rust blob differential | Each production blob vs oracle / FASM emu |
| FASM OFF / Rust ON | Ownership gate(s) A/B |
| Release/free A/B | Must still prove Δ`page_start`≠0 free vs =0 release |
| Pressure/recovery | Required; **not** live `pages_free<=1` |
| Desktop + ON×3 | Required |
| Early-OOM live ≤1 | **Not** mandatory (oracle covers) |

---

## 13. Memory / blob budget

| Item | Value |
|------|-------|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `0x008E000` |
| `SLOT_BASE` | `0x0090000` |
| CT end `.bss` | `OS_BASE+0x8C543` |
| Assert class | `end+PAGE_SIZE < TMP` → ~6.8 KiB class headroom |
| CT blob | 49 B, 0 relocs |

### Slice E estimate (order-of-magnitude — measure before cut)

| Piece | Likely size |
|-------|-------------|
| `alloc_page` blob | ~80–200 B |
| `free_page` blob | ~60–120 B |
| `alloc_pages` blob | ~150–400 B (byte scan + stos) |
| Mode B helper delta vs CT | small (add pages_free ptr inject) |
| Trampolines | tens of bytes each in FASM text |
| Combined | **~0.5–1.5 KiB** blobs typical |

**REG-012:** Unlikely to threaten the pack if smoke iglobals stay minimal
(omit-smoke pattern allowed). **Measure end `.bss` before authorizing.**
Do **not** move TMP/sys_proc/SLOT_BASE speculatively. If measured growth fails
the assert → block the cut (architectural constraint), do not raise the pack
casually.

---

## 14. Blockers (post-audit)

| Blocker | Status |
|---------|--------|
| Release≠free semantic ambiguity | **Resolved** (live A/B + CT) |
| CT helper leaf | **Done** |
| Coherent next slice definition | **Done** (Slice E) |
| Mode B transition design detail | Needed in next research doc (not blocking “slice ready”) |
| Multi-symbol cut authorization | Future turn — not this audit |
| Production implementation | **Blocked until authorized** |
| Path A claim today | **Rejected** |

---

## 15. Decision

**NEXT OWNERSHIP SLICE READY**

Proposed boundary: **Slice E** — Rust sole runtime writer of `sys_pgmap` /
`pages_free` / `page_start` via `alloc_page` + `free_page` + `alloc_pages` +
Mode-B CT release helper; FASM retains `release_pages` orchestration and all
virtual/fault/CR3 domains.

**Production migration: NONE**  
**Migration count: 100 / 136**  
**Do not start Cut CU from this document alone.**

### Recommended next research/task (exactly one)

~~Draft `docs/migration/stage4-bitmap-ownership-cut-plan.md`~~ — **DONE**
([`stage4-bitmap-ownership-cut-plan.md`](stage4-bitmap-ownership-cut-plan.md);
**SLICE E READY FOR AUTHORIZATION**).

**Next:** only after explicit authorization — implement Slice E. Do not start
Cut CU from the audit alone.

---

## 16. Document history

| Date | Note |
|------|------|
| 2026-08-14 | Post–Cut CT fresh ownership audit; Slice E accepted; Path A rejected today |
