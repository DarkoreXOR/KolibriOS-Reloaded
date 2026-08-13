# Post-PTE Migration Frontier Audit

**Date:** 2026-08-14  
**Status:** audit complete — **STOP** (no Cut CV / no production migration)  
**Inventory:** **103 / 138** (35 pending)  
**Production gates:** **104** enabled  
**PTE program:** COMPLETE but **BLOCKED** — [`stage4-pte-ownership-design.md`](stage4-pte-ownership-design.md)  
**Parent:** [`stage4-post-cu-audit.md`](stage4-post-cu-audit.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> Fresh architectural unblock audit after the PTE evidence program. Selects
> **exactly one** next research/tooling task. Does **not** implement it.
> Does **not** reopen PTE ownership, migrate leaves, or change inventory/gates/memory.

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Is PTE Path A unblocked? | **No** — dual-use `page_tabs` + multi-writer + fault/CR3 |
| Did CU unlock a pending consumer Path A? | **No** — consumers still touch PTE/heap/process |
| Is a Path B leaf ready to cut? | **No** — thin/dead/orch/protocol/write still fail the bar |
| Strongest practical frontier? | **Filesystem write-path evidence** (EXT first) |
| Decision | **TOOLING / EVIDENCE GAP** |
| Next research task (one) | CoW **EXT** `SetFileInfo` → metadata readback oracle harness |

**POST-PTE MIGRATION FRONTIER — COMPLETE — STOP**

---

## 1. Authoritative inventory (repository)

| Item | Value | Source |
|------|------:|--------|
| Completed | **103** | `migration-todo.md` `[x]` |
| Pending | **35** | `migration-todo.md` `[ ]` |
| Total | **138** | 103 + 35 |
| Gates enabled | **104** | `project/build.toml` `[[rust.migrations]]` |

Do **not** “repair” to 103/136. Gate≠symbol count is expected (Cut A multi-gate; Cut CU multi-blob / one master gate).

### 1.1 All 35 pending — classification

| # | Symbol | Class |
|---|--------|-------|
| 1 | `strnlen` | thin / export-only **REJECT** |
| 2 | `strtoint_dec` | dead / unlinked **REJECT** |
| 3 | `ntfs_create_partition` | mount orch **DEFER** |
| 4 | `ntfs_restore_usa_frs` | fallthrough **REJECT** |
| 5 | `ntfs_SetFileInfo` | FS write — **oracle/soak blocked** |
| 6 | `ext_SetFileInfo` | FS write — **oracle/soak blocked** (smallest write surface) |
| 7 | `fs_execute` | Stage 6 **DEFER** |
| 8 | `disk_scan_gpt` | disk orch **DEFER** |
| 9 | `disk_scan_partitions` | disk orch **DEFER** |
| 10 | `ipv4_output` | protocol island — **oracle/soak blocked** |
| 11 | `ipv4_output_raw` | protocol island — **oracle/soak blocked** |
| 12 | `net_ptr_to_num` | thin wrapper **REJECT** |
| 13 | `socket_check_owner` | thin/dead **REJECT** |
| 14 | `socket_check_port` | anti-cluster + mutex **REJECT** |
| 15 | `socket_num_to_ptr` | anti-cluster + mutex **REJECT** |
| 16 | `socket_ptr_to_num` | thin/dead **REJECT** |
| 17 | `tcp_mss` | thin clamp **REJECT** |
| 18 | `tcp_output` | protocol island — **oracle/soak / blast blocked** |
| 19 | `get_phys_addr` | thin PE glue **REJECT** |
| 20 | `map_page` | PTE — **ownership blocked** |
| 21 | `mem_test` | hardware / E820 skip **REJECT** |
| 22 | `create_process` | Stage 6 **DEFER** |
| 23 | `pid_to_appdata` | dead (commented caller) **REJECT** |
| 24 | `set_app_params` | Stage 6 **DEFER** |
| 25 | `mutex_init` | thin + fan-out **REJECT** |
| 26 | `enable_irq` | hardware oracle **REJECT** |
| 27 | `irq_eoi` | hardware oracle **REJECT** |
| 28 | `i40` | boundaries Cut C0 **REJECT** |
| 29 | `syscall_entry` | boundaries Cut C0 **REJECT** |
| 30 | `sysenter_entry` | boundaries Cut C0 **REJECT** |
| 31 | `sysfn_getfreemem` | thin façade **REJECT** |
| 32 | `sysfn_mouse_acceleration` | thin façade **REJECT** |
| 33 | `change_task` | unsuitable/late **REJECT** |
| 34 | `do_change_task` | boundaries non-cut **REJECT** |
| 35 | `find_next_task` | Stage 6 late **DEFER** |

No pending GUI/video symbols remain in the scoped checklist.

---

## 2. PTE status (unchanged — blocked)

| Item | Value |
|------|-------|
| Ownership | FASM multi-writer; dual-use hardware PTE + soft `MEM_BLOCK_*` |
| Writers | heuristic 167 / write-class 117; audited runtime records **23** (heap → 30+ loci); boot 3; fault 1; unresolved PCIe 1 |
| Oracle | host `pteo_*` 7/7; store-shape PASS; no Unicorn; no PDE v1 |
| Soak | phases A–H designed; **not** implemented |
| Verdict | **PTE OWNERSHIP STILL BLOCKED** — do not restart as Cut CV |

---

## 3. Current ownership summary

### Rust-owned subsystems / footholds

| Domain | Status |
|--------|--------|
| Phys bitmap runtime (`sys_pgmap` / `pages_free` / `page_start`) | **Path A COMPLETE** (CU) |
| Alloc APIs + Mode-B release helper | **COMPLETE** |
| Many Path B leaves (FS calendar, blit, PE, strings, …) | Complete footholds |
| AQ/BL/CI/O | Injection / RO — **not** owners |

### FASM-owned major domains

| Domain | Status |
|--------|--------|
| Virtual map / PTE / PDE / `invlpg` | FASM multi-writer |
| Fault / CR3 / process AS | FASM |
| Heap soft+hard `page_tabs` protocol | FASM |
| `release_pages` orch (mutex/PTE) | FASM (bitmap Rust) |
| TCP/IP output / eth / ARP | FASM |
| FS write plugins (`*_SetFileInfo`, Delete, …) | FASM |
| Scheduler / create_process | FASM |
| IRQ PIC/APIC policy | FASM |

---

## 4. Domain comparison

### 4.1 PTE / virtual memory

| Field | Assessment |
|-------|------------|
| Status | **BLOCKED** (ownership + memory) |
| Missing | soft/hardware separation **or** mega-slice; PDE oracle; live soak; >>2.1 KiB |
| Path A? | Not without importing heap/fault/CR3 |
| Next? | Do **not** select — program already complete |

### 4.2 Network output

| Field | Assessment |
|-------|------------|
| Status | Architecturally valuable Stage-5 island; **tooling gap** |
| Strongest symbols | `tcp_output`, `ipv4_output`, `ipv4_output_raw` |
| Ownership today | FASM socket mutex, queues, ARP, `eth_output`, device list |
| Existing Rust footholds | checksums, route, fragment slot, TCP timers/flags, `socket_check` |
| Missing | Independent **packet-byte oracle**; QEMU **netdev/user-net/pcap** (absent from `run_qemu.py`); output-buffer soak; retransmission/state model |
| Effort | **High** — netdev plumbing + large `tcp_output` blast |
| Memory | Likely large blob if migrated later |
| Path A? | Only if packet/output queue ownership is accepted — not today |
| Rank | #2 practical frontier (behind FS write tooling size) |

### 4.3 Filesystem write

| Field | Assessment |
|-------|------------|
| Status | Promising Stage-5 plugin deepen; **tooling gap** |
| Strongest symbols | **`ext_SetFileInfo`** (small), then `ntfs_SetFileInfo` |
| Why EXT first | ~40-line body; uses Rust `fsCalculateTime`; `writeInode` + `writeSuperblock` + `disk_sync`; `--disk ext` + `images/` + `mkfs_utils` already exist |
| NTFS | Larger (lock/find LFN/index/FRS/`writeRecord`); still valuable later |
| Existing tooling | `scripts/mkfs.py`, CoW attach soaks, disk A/B culture |
| Missing | Deterministic **SetFileInfo → sync → GetFileInfo and/or host inode timestamp readback** harness; guest trigger path (sysfn70) automation |
| Effort | **Medium** — smaller than netdev + tcp_output |
| Memory | Likely fit under REG-012 if ever cut (small leaf + existing compose) |
| Path A? | No — single plugin write vtable ≠ FS ownership |
| Path B later? | Yes, **after** oracle+soak exist |
| Rank | **#1** next research task |

Note: `fat_SetFileInfo` / `exFAT_SetFileInfo` exist in tree but are **not** in the 35 pending checklist — do not invent scope.

### 4.4 Process / scheduler

| Field | Assessment |
|-------|------------|
| Status | **REJECT / late** |
| CU unlock? | Alloc calls exist in `create_process` / taskman, but AS/CR3/PTE/SLOT ownership remain FASM |
| Missing | Lifecycle oracle + Stage-6 acceptance |
| Path A? | No coherent small boundary |

### 4.5 Remaining FS read / plugin

| Field | Assessment |
|-------|------------|
| Status | Read/calendar/sector math largely exhausted in checklist |
| Remaining orch | `disk_scan_*`, `ntfs_create_partition` — mount orchestration **DEFER** |
| No strong Path B leaf left without write-path or orch |

### 4.6 GUI / video

| Field | Assessment |
|-------|------------|
| Status | **No pending checklist symbols** |
| Rust footholds | H/CD/CP/N/CR/S/CE |
| Shared Rust framebuffer ownership? | **No** — LFB/win_map/cursor still FASM |
| Path A? | **REJECT** — further leaf inflation forbidden |

### 4.7 Allocator consumers

| Consumer area | Touches PTE/`page_tabs`? | Pending symbol? | Newly unlocked Path A? |
|---------------|--------------------------|-----------------|------------------------|
| Heap `user_*` | **Yes** (dual-use) | No (not pending) | **No** |
| DLL load maps | **Yes** | No | **No** |
| AHCI DMA maps | map_io_mem + alloc | No | **No** |
| NTFS `alloc_pages` buffers | then mapped/committed | No SetFileInfo unlock | **No** |
| Framebuffer | **Yes** | No | **No** |
| `create_ring_buffer` | **Yes** dual map | Not pending | **No** |
| `sysfn_getfreemem` | reads `pages_free` only | Thin façade | **No** |
| `get_phys_addr` | AQ + offset | Thin PE | **No** |

**Conclusion:** Cut CU does **not** newly unlock a pending coherent consumer subsystem that avoids PTE/CR3/fault.

### 4.8 Thin / dead / hardware

Status: **REJECT** unchanged (table §1.1). Do not migrate to inflate 103→104.

---

## 5. Path A assessment

| Claim | Result |
|-------|--------|
| New Path A ready now | **No** |
| Phys-bitmap Path A | Already done (CU) |
| Virtual-map Path A | Blocked (PTE program) |
| Network Path A | Not without output/queue ownership + packet soak |
| FS write Path A | Not — plugin write ≠ FS ownership |
| GUI Path A | No shared Rust render state |
| Manufactured leaf bundles | **Forbidden** |

---

## 6. Path B assessment

No pending Path B leaf clears: independent oracle + live callers + manageable blast + soak + meaningful FASM reduction + REG-012 fit.

Closest **future** Path B after tooling: `ext_SetFileInfo` (compose-heavy, small ABI, existing EXT disk soak culture).

---

## 7. Memory constraint

| Item | Value |
|------|-------|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `OS_BASE+0x008E000` |
| `SLOT_BASE` | `OS_BASE+0x0090000` |
| CU end `.bss` | `OS_BASE+0x8C7C3` |
| Headroom | ~2.1 KiB |

Influences ranking: PTE mega-slice and large `tcp_output` are weak under current pack; small FS write leaf is compatible **if** later authorized. This audit moves **no** addresses.

---

## 8. Selected next research task (exactly one)

### CoW EXT `SetFileInfo` → metadata readback oracle harness

**Goal (research/tooling only — not a cut):**

1. Define a deterministic fixture file on a CoW copy of an EXT regression disk
   (`images/ext-image.img` via `python scripts/mkfs.py ext …` if missing).  
2. Drive a guest `SetFileInfo` (sysfn70/80) for atime/mtime (and document attribute bits if any).  
3. Force `disk_sync` / clean shutdown or documented flush.  
4. Independent expected model: BDFE/KOS times → EXT inode `aTime`/`mTime` (+ superblock side effects if observable), composing already-Rust `fsCalculateTime` / known EXT epoch offset.  
5. Verify via **at least one** of:
   - guest `GetFileInfo` readback A/B, and/or  
   - host-side EXT inode field parse from the CoW image.  
6. Produce a scripted soak recipe under `scripts/` / `tools/` (QMP optional) with PASS/FAIL JSON — stronger than desktop non-black.  
7. Document how the same pattern extends to `ntfs_SetFileInfo` later.

**Explicitly out of scope for this task:** Rust `ext_SetFileInfo`, any `USE_RUST_*` gate, inventory change, PTE work, network netdev.

**Why this one (not network, not PTE soft-split):**

| Alternative | Why not selected now |
|-------------|----------------------|
| Packet oracle + QEMU user-net | Higher Stage-5 value but **larger** missing stack (`run_qemu.py` has **no** netdev/pcap today; `tcp_output` blast) |
| PTE soft/hardware separation design | Restarts blocked PTE program; likely memory/arch flag-day |
| Thin leaf cut | Inventory inflation |
| Process/sched research | Still Stage-6 unsuitable |

---

## 9. Rejected alternatives (ranked)

1. PTE soft-state separation / mega-slice — blocked; do not restart.  
2. Network packet+netdev program — strong #2; defer until after FS write harness or explicit Stage-5 pivot.  
3. NTFS SetFileInfo harness first — harder than EXT; do EXT first.  
4. Disk scan / mount orch — no oracle win.  
5. Path B thin rejects — forbidden.  
6. GUI leaf hunting — no pending symbols; no Path A state.

---

## 10. Decision

**TOOLING / EVIDENCE GAP** (at audit time)

Filesystem write (EXT) was the clearest domain where existing CoW/mkfs/attach infrastructure could be extended by **one** concrete harness.

---

## 12. Oracle result (2026-08-14) — COMPLETE

Harness implemented and hardened. See [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md).

| Item | Result |
|------|--------|
| Decision | **EXT SETFILEINFO EVIDENCE READY** |
| Mutation | `ROOT.TXT` inode 12 atime/mtime |
| Guest log | Durable `/hd0/1/ESFI.LOG` on EXT CoW (v2) |
| Host | Python EXT2 mini + Docker `debugfs` PASS |
| QEMU ×3 hardened | PASS, `RESET=0` |
| Production | **NONE** (no Cut CV, no gate, no SetFileInfo migration) |

**Next research task (one):** Implement Cut CV **only when authorized** — see [`cut-cv-plan.md`](cut-cv-plan.md) (plan complete; gate not added).

Do **not** start Cut CV implementation. Inventory remains **103 / 138**.

---

## 11. Documentation touchpoints

| Path | Action |
|------|--------|
| This file | Created (audit) + §12 oracle result |
| [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md) | Oracle complete |
| [`migration-plan.md`](migration-plan.md) | Pointer to frontier |
| Inventory / gates / memory / production code | **Unchanged** |
