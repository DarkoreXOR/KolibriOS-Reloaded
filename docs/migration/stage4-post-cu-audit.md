# Stage-4 Post–Cut CU Audit

**Date:** 2026-08-14  
**Status:** audit complete — **STOP** (no Cut CV / no production migration)  
**Inventory:** **103 / 138**  
**Production gates:** **104** enabled `[[rust.migrations]]`  
**Cut CU / Slice E:** **COMPLETE** — do not reopen without a new reproducible regression  
**Parent:** [`stage4-ownership-design.md`](stage4-ownership-design.md)  
**Prior ownership audit (historical):** [`stage4-next-ownership-audit.md`](stage4-next-ownership-audit.md)  
**CU evidence:** [`cut-cu-implementation.md`](cut-cu-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> Fresh post-CU audit of remaining Path A / Path B opportunities after Rust took
> sole **runtime** ownership of the physical page bitmap. This document does
> **not** authorize Cut CV or any production code change.

---

## 0. Verdict (executive)

| Question | Answer |
|----------|--------|
| Does Cut CU complete phys-bitmap runtime ownership? | **Yes** |
| Does CU unlock automatic `map_page` / fault / CR3 migration? | **No** |
| Does a new Path A subsystem boundary emerge ready to cut? | **No** — research only |
| Does any pending Path B leaf clear the evidence bar? | **No** |
| What is the strongest next engineering boundary? | Virtual `page_tabs` / PTE ownership research |
| What blocks that boundary? | PTE writer inventory + independent PTE/`invlpg` oracle + map/fault soak |
| Decision (audit-time) | **TOOLING / EVIDENCE GAP** |
| Follow-on (evidence program) | [`stage4-pte-ownership-design.md`](stage4-pte-ownership-design.md) → **PTE OWNERSHIP STILL BLOCKED** |

**STAGE-4 POST-CU AUDIT — COMPLETE — STOP**

---

## 1. Authoritative post-CU inventory (repository)

| Item | Value | Source |
|------|-------|--------|
| Completed symbols | **103** | `migration-todo.md` `[x]` |
| Pending symbols | **35** | `migration-todo.md` `[ ]` |
| Scoped total | **138** | 103 + 35 |
| Production gates | **104** enabled | `project/build.toml` `[[rust.migrations]]` |
| Gate math | 104 gates ≠ 103 symbols | Cut A = 4 gates; Cut CU = 4 blob registrations sharing one master gate; CT Mode-A gate retained |
| Cut CU symbols | `alloc_page`, `free_page`, `alloc_pages`, Mode-B release helper | [`cut-cu-implementation.md`](cut-cu-implementation.md) |
| Cut CU gate | `USE_RUST_PHYS_BITMAP_OWNERSHIP = 1` | `kernel/core/memory.inc` |
| CT Mode-A gate | `USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE = 1` (OFF/rollback path) | retained |

### 1.1 Cut CU final inventory contribution

| Before CU | After CU |
|-----------|----------|
| **100 / 136** | **103 / 138** |

- Completed `alloc_page` (was pending).
- Expanded scoped set with `free_page` + `alloc_pages` as one Path A ownership cut (not three Path B leaves).
- Mode-B upgrades the CT release helper in place (same named boundary; Mode-A blob retained for rollback).

### 1.2 Cut CU verified artifacts

| Artifact | Value |
|----------|-------|
| Combined Slice E blobs | **546 B**, 0 relocs each |
| `rust_alloc_page.bin` | 123 B — SHA `ee6bd568…6687e` |
| `rust_free_page.bin` | 83 B — SHA `158b1bd7…e582` |
| `rust_alloc_pages.bin` | 281 B — SHA `d2fc2459…c3e4` |
| `rust_release_bitmap_page_mode_b.bin` | 59 B — SHA `9b0281a2…15e0` |
| CT Mode-A blob (rollback) | 49 B |
| `kernel.mnt` (ON) | **302824** bytes (live tree matches) |
| Recorded final image | `dev_build/test/kernel-20260813-201323.img` |
| `dev_build/last_image.txt` | `dev_build/test/kernel-20260813-204140.img` (later confirmation pointer; not a new cut) |
| Host suite | **836/836** at CU close |
| Allocator soak | `dev_build/allocsoak/soak-cut-cu.json` present |
| QEMU ON / OFF / A/B | non-black **779380**, RESET=0 (recorded) |
| ON ×3 | PASS |
| ON + exFAT | PASS 779380 |
| Confirmation jobs (post-record) | ON desktop PASS 779380; OFF baseline PASS 779380; ON+exFAT PASS 779380 |

No new Cut CU regression was discovered in this audit. Do **not** reopen CU.

---

## 2. Memory baseline (verified live)

| Symbol | Value | Sources |
|--------|-------|---------|
| `TMP_STACK_TOP` | **`0x008E000`** | `kernel/const.inc`, `docs/compatibility/fixed-addresses.md`, `docs/architecture/memory-model.md` |
| `sys_proc` | **`OS_BASE+0x008E000`** | same |
| `SLOT_BASE` | **`OS_BASE+0x0090000`** | same (REG-012: must end at `VGABasePtr`) |
| CU ON end `.bss` | **`OS_BASE+0x8C7C3`** | [`cut-cu-implementation.md`](cut-cu-implementation.md) |
| Early-stack assert | `0x8D7C3 < 0x8E000` → **PASS** (~2.1 KiB headroom) | CU report |
| Pack policy | Do **not** move `sys_proc` / `SLOT_BASE`; raise TMP only with measured need | REG-012 |

CU did **not** change the REG-012 pack. Headroom is real but insufficient justification to force a weak leaf or an oversized PTE ownership blob.

---

## 3. Current ownership (LOCAL FACT after CU)

### 3.1 Rust sole runtime ownership (phys bitmap)

| Object / API | Owner | Notes |
|--------------|-------|-------|
| `sys_pgmap` runtime writes | **Rust** | Boot `init_page_map` still FASM |
| `pages_free` runtime writes | **Rust** | Mode B release helper; no FASM Mode-A store when CU ON |
| `page_start` runtime writes | **Rust** | Only via `alloc_page` / `free_page` |
| `alloc_page` | **Rust** | plain; EAX out; pushfd/cli; EBX preserve |
| `free_page` | **Rust** | phys in; BTS free polarity; may lower `page_start` |
| `alloc_pages` | **Rust** | stdcall(count); 0xFF-run; cursor unchanged |
| `release_bitmap_page_without_cursor_update` | **Rust Mode B** | BTS + `pages_free += delta`; never `page_start` |

### 3.2 FASM retained (explicitly out of CU)

| Domain | Owner | Notes |
|--------|-------|-------|
| `release_pages` orchestration | **FASM** | mutex / PTE clear / `invlpg` / loop; calls Rust Mode-B helper |
| `map_page` | **FASM** | `stdcall(lin,phys,flags); ret 12`; PTE store + `invlpg` |
| Direct PTE writers | **FASM** | heap, dll, v86, sched I/O maps, `commit_pages`, `unmap_pages`, `map_io_mem`, fault paths, … |
| `page_tabs` / `app_page_tabs` | **FASM** | multi-writer; recursive PT window |
| `pte_valid_mask` | **FASM** | boot feature mask |
| `page_fault_handler` | **FASM** | calls Rust `alloc_page` + FASM `map_page` |
| CR3 / `PROC.pdt_0` / process create | **FASM** | Stage 6 adjacent |
| `pg_data.mutex` | **FASM** | `release_pages` / `commit_pages` / heap |
| `init_page_map` / `page_end` | **FASM** | boot |

### 3.3 Rust read-only footholds (injection ≠ ownership)

| Cut | Symbol | Sees | Owns? |
|-----|--------|------|-------|
| AQ | `get_pg_addr` | injected `page_tabs` | No |
| BL | `v86_get_lin_addr` | injected `page_tabs` | No |
| CI | `usb_td_to_virt` | AQ compose | No |
| O | `test_app_header` | injected `pages_free` value | No |

### 3.4 Ownership graph (post-CU)

```text
Boot FASM                    Phys bitmap (Rust runtime)         Virtual map (FASM)
─────────────                ──────────────────────────         ──────────────────
init_page_map ──writes──►    sys_pgmap / pages_free /           page_tabs multi-writer
                             page_start                         ├── map_page
                             ▲                                  ├── commit/unmap/map_io/…
                    ┌────────┴────────┐                         ├── fault handler
                    │                 │                         └── invlpg sites
              alloc_page/pages   free_page
                    │                 │
                    └──── Rust CU ────┘

release_pages (FASM orch)
  mutex → PTE xchg 0 → invlpg → Rust Mode-B bitmap helper → unlock
```

**Hard inference:** Rust-owned physical allocator ≠ Rust-owned paging.

---

## 4. Path A reassessment

### 4.1 Does CU create a new ready Path A cut?

**No.**

Path A requires:

1. Rust owns meaningful shared subsystem state — **true for phys bitmap (already cut)**.
2. Multiple functions share one coherent boundary.
3. Moving them together reduces FASM↔Rust co-ownership crossings.
4. Independent subsystem oracle exists.
5. Production soak exists or can be created.
6. Rollback is coherent.
7. Scope does not silently import unrelated CR3/fault/process ownership.

CU satisfied (1)–(7) for the **bitmap domain**. That cut is finished. A *new* Path A must identify a **different** coherent state domain.

### 4.2 Candidate Path A clusters examined

| Cluster | Consumes Rust allocator? | Additional state needed | Already Rust-owned? | Verdict |
|---------|--------------------------|-------------------------|---------------------|---------|
| `map_page` alone | Calls `alloc_page` elsewhere, not inside | Sole `page_tabs` writer + `invlpg` contract | **No** — many FASM PTE writers | **REJECT** as Path A (false ownership) |
| `map_page` + `commit_pages` + `unmap_pages` + `map_io_mem` | Indirect | Full PTE writer set + TLB policy | **No** | **DEFER research** — possible future Path A shape |
| `release_pages` full body | Already calls Mode-B helper | mutex + PTE + `invlpg` ownership | Mutex/PTE FASM | **REJECT now** — would create Rust orch / FASM PTE split-brain unless PTE domain moves first |
| `page_fault_handler` | Yes (`alloc_page`) | fault policy, CoW, CR3/AS | **No** | **REJECT** — imports process/CR3 |
| Heap / buffer allocators | Yes | heap free-lists, mutexes, mapping | **No** | **REJECT** — consumer layer ≠ bitmap ownership extension |
| FS/net consumers of pages | Yes | FS/net subsystem state | **No** | **REJECT** — not unlocked by allocator ownership |
| AQ+BL+CI + `get_phys_addr` | Translate only | still RO | **No** | **REJECT** — footholds ≠ ownership |

### 4.3 Newly unlocked by CU (genuine, but not cut-ready)

1. **Virtual-map ownership research** — allocator dual-write risk is gone; paging is now the remaining Stage-4 ownership question.
2. **Fault-path analysis that treats `alloc_page` as a stable Rust ABI** — still cannot migrate fault without PTE/CR3 policy.
3. **`sysfn_getfreemem` reads Rust-owned `pages_free`** — still a thin syscall façade (not a Path A or Path B win).

None of these are automatically unlocked *migrations*.

### 4.4 Path A decision

**Path A for a new production cut: REJECTED (not ready).**  
**Path A research direction: ACCEPT** — treat `page_tabs`/PTE as the next ownership domain to *study*, mirroring the pre-CT bitmap research sequence.

---

## 5. Complete pending-symbol ranking (35)

| Rank | Symbol(s) | Class | Oracle / soak | Verdict |
|------|-----------|-------|---------------|---------|
| 1 | `map_page` (+ eventual PTE cluster) | Stage-4 virtual map | Needs PTE writer inventory + PTE/`invlpg` oracle + map/fault soak | **DEFER — evidence gap** |
| 2 | `tcp_output` / `ipv4_output*` | Stage-5 protocol | Packet oracle + net soak missing | **DEFER** |
| 3 | `ntfs_SetFileInfo` / `ext_SetFileInfo` | FS write path | Metadata write/readback oracle missing | **DEFER** |
| 4 | `disk_scan_gpt` / `disk_scan_partitions` / `ntfs_create_partition` | Mount orch. | Beyond Z/AD/CC | **DEFER** |
| 5 | `create_process` / `fs_execute` / `set_app_params` | Stage 6 | Process lifecycle | **DEFER** / late |
| 6 | `find_next_task` / `change_task` / `do_change_task` | Sched | Boundaries | **REJECT** unsuitable/late |
| 7 | `i40` / `syscall_entry` / `sysenter_entry` | Entry asm | — | **REJECT** Cut C0 |
| 8 | `release_pages` (full) | Stage-4 orch. | Bitmap owned; PTE not | **REJECT now** (premature Path A) |
| 9 | `ntfs_restore_usa_frs` | Fallthrough | Zero new semantics | **REJECT** |
| 10 | `socket_num_to_ptr` / `socket_check_port` | Anti-cluster | Mutex/list FASM | **REJECT** |
| 11 | `socket_ptr_to_num` / `socket_check_owner` | Dead/thin | — | **REJECT** |
| 12 | `net_ptr_to_num` | Thin wrapper | AY | **REJECT** |
| 13 | `get_phys_addr` | PE-only glue | AQ + offset | **REJECT** |
| 14 | `pid_to_appdata` | Dead | Commented caller only | **REJECT** |
| 15 | `strnlen` | Export-only | — | **REJECT** |
| 16 | `tcp_mss` | Thin clamp | 1420 store | **REJECT** |
| 17 | `mutex_init` | Thin + fan-out | circular list init | **REJECT** |
| 18 | `sysfn_getfreemem` / `sysfn_mouse_acceleration` | Thin façade | load/store | **REJECT** (even though `pages_free` is Rust-owned) |
| 19 | `enable_irq` / `irq_eoi` | Hardware I/O | No deterministic oracle | **REJECT** |
| 20 | `mem_test` | Boot probe | Skipped under E820 | **REJECT** |
| 21 | `strtoint_dec` | Dead/unlinked | — | **REJECT** |

### 5.1 Deferred-class re-evaluation (prior rejects)

| Prior conclusion | Still valid? | Why |
|------------------|--------------|-----|
| `map_page` deferred (no Rust allocator) | **Partially aged** | Allocator ownership **done**; `map_page` still blocked by **PTE multi-writer** + missing PTE oracle/soak |
| Fault/CR3 deferred | **Yes** | Still process/AS ownership |
| `tcp_output` / `ipv4_output*` deferred | **Yes** | Still no packet oracle / net soak |
| `*_SetFileInfo` deferred | **Yes** | Still no write/readback oracle |
| Socket anti-cluster | **Yes** | Mutex/list lifecycle still FASM |
| Thin exports / façades | **Yes** | CU does not add substance |
| IRQ / `mem_test` | **Yes** | Oracle gaps unchanged |
| NTFS fallthrough | **Yes** | Still zero new semantics |
| Process/sched boundaries | **Yes** | Still late / unsuitable |

---

## 6. Deep dive — strongest architectural next target

### 6.1 `map_page` (not selected for Cut CV)

| Field | Fact |
|-------|------|
| Source | `kernel/core/memory.inc` |
| ABI | stack `lin, phys, flags`; `ret 12`; preserves via push ebx; writes `page_tabs[lin>>12] = (phys\|flags) & pte_valid_mask`; `invlpg [lin]` |
| Callers | Broad: heap, framebuffer, taskman, kernel.asm TSS/I/O maps, fault/IPC map helpers |
| Globals | `page_tabs`, `pte_valid_mask` |
| IRQ / DF | no cli in helper; `invlpg` side effect |
| Oracle today | None independent for live TLB + multi-writer PTE state |
| Soak today | Desktop/allocator soaks do **not** prove PTE semantics |
| Why CU helps | Fault/`commit` paths can call Rust `alloc_page` without dual bitmap writers |
| Why CU is insufficient | Ownership of mapping state remains FASM-split across many stores |

Migrating `map_page` alone would **increase** FASM↔Rust confusion: one helper in Rust while heap/dll/v86/commit/unmap keep writing `page_tabs`.

### 6.2 What a coherent virtual-map Path A would require (research, not a cut)

Minimum coherent future shape (illustrative — **not authorized**):

1. Complete runtime writer inventory of `page_tabs` / `app_page_tabs` / `invlpg` sites (bitmap-writers analog).
2. Independent PTE oracle: address→PTE bits, `pte_valid_mask`, unmap polarity, failure cases — stronger than desktop non-black.
3. Defined sole-writer boundary (which helpers move together; which remain FASM).
4. Dedicated map/unmap/fault soak (allocator soak is necessary but not sufficient).
5. Explicit exclusion of CR3 / `do_change_task` / `create_process` unless a separate Stage-6 acceptance exists.
6. Memory budget under REG-012 without speculative TMP moves.

Until (1)–(4) exist: **TOOLING / EVIDENCE GAP**.

### 6.3 Path B — no select

Post-CU pending set contains **no** Path B leaf with all of: deterministic semantics, strong independent oracle, real production callers, clear ABI, manageable blast, existing soak, and meaningful FASM-ownership reduction. Selecting `tcp_mss`, `mutex_init`, `strnlen`, `get_phys_addr`, or `sysfn_getfreemem` would be inventory inflation.

---

## 7. Candidate comparison (top remaining)

| Candidate | Path | Arch value | Evidence | Blast | Fit after CU | Rank |
|-----------|------|------------|----------|-------|--------------|------|
| PTE/`page_tabs` ownership research → eventual `map_page` cluster | A (future) | Highest Stage-4 residual | **Missing** inventory/oracle/soak | High | Enabled as *research* only | **1 (research)** |
| `tcp_output` / `ipv4_output*` | B→island | High Stage-5 | Missing packet oracle/net soak | High | Unchanged by CU | 2 (blocked) |
| `*_SetFileInfo` | B→island | High FS | Missing metadata A/B | High | Unchanged | 3 (blocked) |
| Disk scan / mount orch. | B | Medium | Partial fixtures only | High | Unchanged | 4 (blocked) |
| Thin/dead/IRQ/fallthrough | — | Low | N/A | Varies | Still reject | — |

---

## 8. Recommended next target (not a cut)

**Recommended next target:** Stage-4 **virtual-map evidence program**

1. Generate `page_tabs` / PTE / `invlpg` runtime writer inventory (JSON + script, analogous to [`stage4-bitmap-writers.json`](stage4-bitmap-writers.json)).
2. Host independent PTE oracle (mask, store, unmap, present-bit, failure semantics).
3. Design map/unmap/fault soak stronger than desktop non-black / non-RESET.
4. Only then propose a coherent Path A ownership slice (likely multi-symbol; **not** `map_page`-only) for authorization as a future cut.

**Recommended Path:** **A (research)** — not Path B leaf hunting.

**Selection rationale:** CU completed the only coherent phys-bitmap Path A. The highest remaining architectural value in Stage-4 is virtual mapping ownership. That boundary is real, but its missing evidence is specific and analogous to the successful pre-CT bitmap research sequence. No pending Path B symbol clears the bar.

### Future ABI (illustrative only — not authorized)

```text
map_page (legacy public ABI unchanged if/when migrated):
  stdcall(lin_addr, phys_addr, flags) → ret 12
  effect: page_tabs[lin>>12] := (phys|flags) & pte_valid_mask
          invlpg [lin]
```

Any Rust implementation must preserve that ABI and must not claim ownership while bypass writers remain FASM.

### Oracle / soak (required before any Cut CV-class migration)

| Need | Status |
|------|--------|
| Independent PTE state oracle | **Missing** |
| Writer inventory for `page_tabs` | **Missing** (ad-hoc greps only) |
| Map/unmap/fault soak | **Missing** (allocator soak exists; insufficient) |
| Desktop non-black | Exists — **not** primary semantic oracle |

### Memory / blob

| Item | Status |
|------|--------|
| Current headroom | ~2.1 KiB to TMP after CU `.bss` |
| Speculative TMP raise | **Forbidden** for weak candidates |
| Estimate for full PTE cluster | Unknown until inventory + prototype blobs |

If a future excellent PTE ownership cut cannot fit REG-012 safely, document **memory-blocked** rather than move `sys_proc`/`SLOT_BASE`.

---

## 9. Explicit blockers

1. No PTE/`page_tabs` sole-writer inventory.
2. No independent PTE/`invlpg` oracle.
3. No dedicated virtual-map / fault soak.
4. No packet-level net oracle (blocks protocol islands).
5. No SetFileInfo metadata write/readback oracle (blocks FS write path).
6. Remaining Path B checklist items are thin/dead/anti-cluster/unsuitable.

---

## 10. Decision

**TOOLING / EVIDENCE GAP** (this audit)

Follow-on evidence program:
[`stage4-pte-ownership-design.md`](stage4-pte-ownership-design.md) —
**PTE OWNERSHIP STILL BLOCKED**. Inventory + host `pteo_*` oracle + soak design
landed; no coherent Path A virtual-map slice without heap/fault/CR3 entanglement.

- Not **NEXT CUT READY**
- Not **NEW PATH A READY**
- Do not start Cut CV. Do not implement a migration from this audit.

---

## 11. Documentation / production

| Action | Status |
|--------|--------|
| This audit | Created |
| [`stage4-ownership-design.md`](stage4-ownership-design.md) | Updated for post-CU ownership graph + next research pointer |
| Production code / gates / blobs | **NONE** |

---

## 12. Rollback reminder (CU only)

```text
USE_RUST_PHYS_BITMAP_OWNERSHIP = 0
```

Restores FASM alloc* + Mode-A release + CT Mode-A helper. Not invoked by this audit.
