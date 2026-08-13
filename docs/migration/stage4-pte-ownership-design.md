# Stage-4 PTE / Virtual-Map Ownership Design

**Date:** 2026-08-14  
**Status:** research complete — **PTE OWNERSHIP STILL BLOCKED** (no Cut CV)  
**Inventory:** **103 / 138** (unchanged)  
**Production gates:** **104** enabled (unchanged)  
**Parent:** [`stage4-post-cu-audit.md`](stage4-post-cu-audit.md)  
**Writer inventory:** [`stage4-pte-writers.json`](stage4-pte-writers.json)  
**Heuristic hits:** [`stage4-pte-writers.hits.json`](stage4-pte-writers.hits.json)  
**Host oracle (research):** `rust_kernel/kolibri_utils/src/pte_oracle.rs` (`pteo_*`, seed `'PTEO'`)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> Virtual-map evidence program after Cut CU. **No** production PTE migration,
> **no** `USE_RUST_MAP_PAGE`, **no** inventory/gate/memory-pack changes.

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Is `map_page` alone a Path A boundary? | **No** — leaf helper; many bypass writers |
| Can Rust own virtual mapping without CR3/fault? | **Not safely today** |
| Why? | `page_tabs` is **dual-use** (hardware PTE + heap `MEM_BLOCK_*`) with many FASM co-writers |
| Smallest coherent sole-writer cluster? | Would pull heap + DLL CoW + fault + PDE growth — imports process/fault |
| Oracle / soak designs? | **Yes** (host oracle stub + soak design below) |
| Decision | **PTE OWNERSHIP STILL BLOCKED** |

**STAGE-4 PTE OWNERSHIP PROGRAM — COMPLETE — STOP**

---

## 1. Architecture facts (LOCAL FACT)

| Item | Value |
|------|-------|
| Recursive PTE window | `page_tabs = 0xFDC00000` (4 MiB) |
| Alias | `app_page_tabs = page_tabs` |
| Kernel half window | `kernel_tabs = page_tabs + (OS_BASE shr 10)` = `0xFDE00000` |
| PDE self-map | `master_tab = page_tabs + (page_tabs shr 10)` |
| Mode | 32-bit non-PAE PDE/PTE; optional `PDE_LARGE`/`CR4_PSE`; optional `CR4_PGE` |
| NX | Not a first-class production PTE bit; `pte_valid_mask` set in `high_code` |
| Present | `PG_READ = 0x001` |
| Writable | `PG_WRITE = 0x002` |
| User | `PG_USER = 0x004` |
| Soft heap tags (share cells) | `MEM_BLOCK_RESERVED=0x02`, `FREE=0x04`, `USED=0x08`, `DONT_FREE=0x10` |

**Critical dual-use rule:** when present=0, low bits may encode **heap soft state**.  
`MEM_BLOCK_RESERVED` equals `PG_WRITE`; fault lazy-alloc tests bit 1 on a non-present entry.

---

## 2. Ownership graph (current)

```text
                    ┌─────────────────────────────────────────┐
                    │ page_tabs[VPN]  (single mutable array)  │
                    └─────────────────────────────────────────┘
                     ▲        ▲        ▲        ▲        ▲
                     │        │        │        │        │
              map_page*  heap soft  dll CoW   v86/BIOS  fault/CoW
              map_io_mem  + hard PTE  HDLL map  sched IO  new_mem_resize
              commit/unmap background create_ring_buffer  IPC clear
              release_pages

* map_page is only one hardware-store helper — not the sole writer.

PDE / master_tab: map_page_table, boot init_mem, error master_tab xchg, LFB PDEs
CR3: do_change_task, v86_*, mtrr flush, framebuffer flush, shutdown
Phys frames: Rust alloc_page/free_page/alloc_pages (Cut CU) — consumed by above
```

| State | Owner today |
|-------|-------------|
| `page_tabs` cells | **FASM multi-writer** |
| PDE / `master_tab` | **FASM** |
| TLB `invlpg` | **FASM** (paired with writers) |
| TLB via `mov cr3` | **FASM** |
| Mapping policy / PG_* choice | **FASM callers** |
| Page-table page alloc | **Rust** `alloc_page` + **FASM** `map_page_table` |
| Fault repair | **FASM** `page_fault_handler` |
| CR3 switch | **FASM** sched / v86 / boot |
| Phys bitmap | **Rust** (CU) |

---

## 3. Writer inventory summary

Sources: hand audit + `scripts/inventory_pte_writers.py` +
`scripts/merge_pte_writers_inventory.py`.

| Class | Count | Notes |
|-------|------:|-------|
| Heuristic raw hits | 167 | scanner aid |
| Heuristic write-class | 117 | includes invlpg/CR3 |
| Audited runtime `page_tabs`/PDE writers (records) | **23** | authority; heap group expands to 30+ loci |
| Audited boot writers | **3** | `init_mem`, `init_page_map`, `pte_valid_mask` |
| Fault-path writer | **1** | `page_fault_handler` |
| Indirect (call `map_page` only) | several | `safe_map_page`, taskman, kernel.asm TSS maps |
| Unresolved suspicious | **1** | `PCIe.inc` `sys_pgdir` / `PG_LARGE` undeclared in-tree |
| Unexplained after classification | **0** | PCIe classified as unresolved |

### 3.1 Completeness cross-checks performed

- Token scan: `page_tabs`, `app_page_tabs`, `invlpg`, `cr3`, `pte_valid_mask`
- Store patterns: `mov [page_tabs…]`, `xchg … page_tabs`, `stosd` after `lea edi,[page_tabs…]`
- Callers: `map_page`, `commit_pages`, `unmap_pages`, `map_io_mem`, `map_page_table`
- CR3 writes / PDE via `master_tab` / `PROC.pdt_0`
- Soft-tag stores: `MEM_BLOCK_*` in `heap.inc`, `dll.inc`, `background.inc`
- Boot: `init.inc` / `high_code`

### 3.2 Sites that bypass `map_page`

Listed in JSON `bypasses_of_map_page`. Major: heap, dll, background, v86,
`commit_pages`/`unmap_pages`/`map_io_mem`/`release_pages`, ring buffer, IPC,
`new_mem_resize`, framebuffer PT fill, `map_page_table`.

---

## 4. `map_page` deep audit

### 4.1 ABI (verified from `memory.inc`)

```text
map_page:   ; not a FASM `proc` — manual stdcall
  in stack:  [esp+4]=ret, [esp+8]=lin, [esp+12]=phys, [esp+16]=flags
  body: push ebx
        eax := (phys | flags) & [pte_valid_mask]
        [page_tabs + (lin>>12)*4] := eax
        invlpg [lin]
        pop ebx
        ret 12
  preserves: EBX (explicit); other GPRs clobbered as used (EAX, flags)
  DF: unchanged
  IRQ: no cli
  CR3: no
  alloc: no
```

PE export: `MapPage`. Illustrative signature `stdcall(lin,phys,flags); ret 12`
is **authoritative**.

### 4.2 Ownership verdict

| Option | Verdict |
|--------|---------|
| A. True subsystem boundary | **No** |
| B. Leaf wrapper | **Yes** (thin hardware store + invlpg) |
| C. Helper bypassed by many writers | **Yes** |
| D. Orchestration importing FASM state | Partially — uses global `page_tabs` + `pte_valid_mask` |

---

## 5. Path A boundary analysis

### 5.1 Rejected clusters

| Cluster | Why rejected |
|---------|--------------|
| `map_page` only | Dual writers remain; false ownership |
| `map_page` + `map_io_mem` + `commit_pages` + `unmap_pages` | Still dual with heap soft/hard + dll + v86 + fault |
| Above + `release_pages` PTE half | Bitmap already Rust; PTE array still shared with heap |
| Hardware PTE helpers + `invlpg` only | Same shared array; soft writers remain |
| Full `page_tabs` sole writer | Must include heap + DLL CoW + fault + PDE growth + v86 + sched I/O → **imports fault/CR3/process** |

### 5.2 Conditional future Path A (not ready)

**Only if** a prior architectural change separates soft heap metadata from the
hardware PTE window (or proves soft stores are a distinct owned protocol with
zero hardware aliasing risk under a Rust hardware-PTE API), then a candidate
hardware cluster could be:

```text
map_page, map_io_mem, commit_pages, unmap_pages,
release_pages PTE clear+invlpg (bitmap stays Rust Mode-B),
create_ring_buffer PTE half, IPC temp clears
```

State owned: hardware-present PTE cells + paired `invlpg`.  
Remaining FASM: soft heap tags, DLL CoW, fault policy, CR3, PDE/`map_page_table`,
v86, sched I/O maps — **still dual writers of the array** unless soft tags move.

**Today:** that separation does **not** exist → Path A **blocked**.

### 5.3 Answer to the key question

> Can Rust own virtual mapping state without simultaneously owning process/CR3/fault state?

**No — not with the current `page_tabs` dual-use design.**

Minimum prerequisite boundary (research, not a cut):

1. Formal split or contract: soft `MEM_BLOCK_*` vs hardware PTE cells, **or**
2. Accept a mega-slice that owns heap+DLL+fault+PDE (imports CR3/process) — Stage-6 scale.

---

## 6. Independent PTE oracle

### 6.1 Model

Implemented host-only in `pte_oracle.rs`:

| Engine | Role |
|--------|------|
| `IndependentPageMap` | VPN → `{raw, kind∈{Hardware,SoftHeap}}` + expected invlpg VPN set |
| `FasmPteEmu` | Flat `tabs[VPN]` + `(phys\|flags)&mask` + invlpg set + `xchg` clear |

Seed: `'PTEO'` (`0x5054_454F`). Tests: `pteo_*` (7), including **50 000** randomized map/unmap/remap/soft/xchg cases.

### 6.2 Operations covered

- map / unmap / remap  
- unmap absent  
- `pte_valid_mask`  
- present / writable / user bits  
- soft `MEM_BLOCK_RESERVED` (lazy-fault encoding)  
- release-style `xchg` clear polarity  

### 6.3 Explicit limitations

- No Unicorn/assembled FASM execution  
- No multi-CR3 / per-process PDT model  
- No real TLB; invlpg is a recorded set only  
- No PDE/`master_tab` hierarchy in v1 (documented gap for next tooling)  
- No page-table exhaustion / alloc_page failure coupling in v1  

### 6.4 Edge-case matrix (design)

| Case | Host oracle | Live soak needed? |
|------|-------------|-------------------|
| Map new | yes | yes |
| Remap | yes | yes |
| Unmap existing / absent | yes | yes |
| Soft reserved → fault alloc | partial (encoding) | **yes** (fault) |
| Permission user/kernel / RO/RW | flags bits | yes |
| PT creation / `map_page_table` | **not yet** | yes |
| PT exhaustion | **not yet** | yes |
| CR3 switch visibility | **out of scope** | process soak |

---

## 7. FASM differential

| Item | Status |
|------|--------|
| Independent vs emu store shape | **PASS** (`pteo_randomized_50k_map_unmap`) |
| Assembled FASM under Unicorn | **Not used** (same limitation class as PGBM) |
| Soft vs hardware kind tracking | Independent only; emu stores raw |
| Claim | Store-shape equivalence for `map_page`/`unmap`/`xchg` — **not** full kernel paging equivalence |

---

## 8. Map / unmap / fault soak design (not implemented)

Desktop non-black is **not** the semantic oracle.

| Phase | Intent | Evidence |
|-------|--------|----------|
| A Map baseline | Create known VAs; QMP/`xp` PTE raw + AQ translate | board markers + xp |
| B Unmap | Clear; verify #PF or absent PTE | markers |
| C Remap | Replace frame; verify new phys | xp + translate |
| D Permissions | RO vs RW; user vs supervisor where safe | fault vs success |
| E PT pressure | Grow via `new_mem_resize` / many maps | PDE present + alloc |
| F Fault-driven | Touch reserved/unmapped; distinguish recover vs kill | `pages_faults` + markers |
| G Process/CR3 | Minimal: only if boundary includes it — **not** for blocked Path A | optional |
| H Recovery | Free/unmap; desktop + FS/GUI; repeat ×3 | stability |

### 8.1 Tooling

| Existing | Missing |
|----------|---------|
| QMP connect / screendump / `xp` (`qmp_desktop_smoke`, allocator soak) | Guest VA→PTE dump helper for arbitrary VPN |
| `SysMsgBoardStr` scrape | Dedicated `pte soak` PE driver / recipe |
| Allocator soak pressure | Fault inject markers (safe reserved touch) |
| AQ translate foothold | Host script comparing expected PTE raw vs xp |
| CR3 observe via QMP regs (possible) | Automated multi-AS correlation |

**Smallest additions (future tooling only):**

1. Disposable PE/`asoak`-style driver exporting map/unmap/touch markers.  
2. Host `scripts/qmp_pte_soak.py` reusing allocator QMP helpers.  
3. Extend host oracle with PDE/`master_tab` level.

Do **not** build a huge paging framework.

---

## 9. Future ABI prerequisites (discovery only)

| Symbol | ABI |
|--------|-----|
| `map_page` | stdcall(lin,phys,flags); ret 12; EBX preserved; invlpg; no cli |
| `map_io_mem` | stdcall(base,size,flags); returns kVA or 0 |
| `commit_pages` | eax=phys\|flags, ebx=lin, ecx=count; mutex; void |
| `unmap_pages` | eax=base, ecx=count; void; no free |
| `release_pages` | eax=lin, ecx=count; mutex; PTE clear + Rust bitmap |
| `map_page_table` | stdcall(lin,phys); PDE PG_UWR; invlpg window |

No new ABI invented. No Rust gate.

---

## 10. Future ownership invariants (required if ever unblocked)

For every mutable runtime cell in the recursive window:

1. Exactly one writer owner (hardware **or** soft protocol — not both unmanaged).  
2. Explicit readers (AQ/BL/CI OK).  
3. Explicit boundary + oracle + soak.  
4. No silent CR3/fault import.  
5. Phys frames remain Rust CU APIs.

---

## 11. Memory / blob feasibility

| Item | Value |
|------|-------|
| Current headroom | ~2.1 KiB (`0x8E000 - 0x8D7C3`) |
| Tiny `map_page`-only blob | ~tens of bytes — **still forbidden** (false ownership) |
| Coherent sole-writer cluster | Likely **many KiB–tens of KiB** (heap+dll+fault) — **exceeds pack** |
| REG-012 | Do **not** move TMP/`sys_proc`/`SLOT_BASE` to force fit |

**Memory is an additional blocker** even if ownership were solved.

---

## 12. Comparison with other unblock paths

| Path | Arch value after CU | Evidence gap | Blast | Uses Rust allocator? |
|------|---------------------|--------------|-------|----------------------|
| **PTE / virtual map** | Highest Stage-4 residual | Ownership entanglement + PDE oracle + soak | Catastrophic if wrong | Yes (consumer) |
| **Network** `tcp_output`/`ipv4_output*` | High Stage-5 | Packet oracle + net soak | High | Weak |
| **FS write** `*_SetFileInfo` | High FS | Metadata write/readback | High | Weak |

**Recommendation:** PTE research was the correct next program; it **proves Path A blocked**.  
Do **not** pivot to inventory-inflation Path B leaves. Parallel **tooling** for net/FS oracles remains valuable but is **not** “stronger architecture” than resolving (or consciously deferring) Stage-4 virtual map.

---

## 13. Remaining blockers

1. Dual-use `page_tabs` (hardware + heap soft).  
2. Many bypass writers (dll, v86, sched, fault, …).  
3. Fault/CR3 coupling for any sole-writer story.  
4. PDE/`master_tab` not in host oracle v1.  
5. No live map/unmap/fault soak harness.  
6. ~2.1 KiB REG-012 headroom insufficient for mega-slice.  
7. Unresolved `PCIe.inc` `sys_pgdir` symbol audit.

---

## 14. Decision

**PTE OWNERSHIP STILL BLOCKED**

Research deliverables (inventory, oracle stub, soak design, Path A analysis) are
complete. **Do not start Cut CV.**

---

## 15. Documentation / production

| Artifact | Role |
|----------|------|
| This file | Ownership design |
| `stage4-pte-writers.json` | Audited inventory |
| `stage4-pte-writers.hits.json` | Heuristic scan |
| `scripts/inventory_pte_writers.py` | Scanner |
| `scripts/merge_pte_writers_inventory.py` | Authority merge |
| `pte_oracle.rs` | Host-only research oracle |
| Production code / gates / inventory / memory pack | **NONE** |
