# Post-Cut CW Migration Frontier Audit

**Date:** 2026-08-14  
**Status:** audit complete — **amended** after REG-012 compaction + `ipv4_output` evidence program  
**Inventory:** **105 / 138** (33 pending)  
**Production gates:** **106** `[[rust.migrations]]` all `enabled = true`  
**Cut CW:** **COMPLETE** — do not reopen without a new reproducible regression  
**REG-012 compaction:** **COMPLETE** — slack **2365 B** (see [`reg012-headroom-audit.md`](reg012-headroom-audit.md))  
**IPv4 output evidence:** **COMPLETE** — [`stage4-ipv4-output-oracle.md`](stage4-ipv4-output-oracle.md) (**IPV4_OUTPUT EVIDENCE READY**). Do **not** start Cut CX.

> Fresh post-CW audit of all **33** pending symbols. Selects **exactly one**
> next research/tooling frontier. Does **not** implement Cut CX, change
> inventory/gates, or modify production code.

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Is Cut CW still complete and gated ON? | **Yes** — `USE_RUST_NTFS_SET_FILE_INFO = 1` |
| Did CV+CW unlock filesystem Path A? | **No** — two plugin metadata leaves ≠ FS ownership |
| Did CW unlock a new Path A boundary anywhere? | **No** |
| Does any pending Path B leaf clear the evidence bar **and** fit the pack? | **No** |
| Binding new constraint? | REG-012 slack restored (**2365 B**); memory no longer blocks a ~180–900 B Path B class |
| PTE status changed since CW? | **No** — still blocked |
| Decision | **IPV4_OUTPUT EVIDENCE READY** (research). Next *cut* still unauthorized. |
| Next research task (one) | Future Cut CX **plan** for `ipv4_output` Path B (trampoline + ARP/`eth_output` mocks) — **do not implement in this audit** |

**STAGE-4 POST-CW AUDIT — COMPLETE — STOP**

Do **not** start Cut CX. Do **not** migrate `ntfs_create_partition`, networking, PTE, or process/scheduler. Do **not** move `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE`.

---

## 1. Authoritative repository state

Verified 2026-08-14 from the live tree (not conversation text).

| Item | Value | Source |
|------|--------|--------|
| Completed symbols | **105** | [`migration-todo.md`](migration-todo.md) — 105 `[x]` |
| Pending symbols | **33** | [`migration-todo.md`](migration-todo.md) — 33 `[ ]` |
| Scoped total | **138** | 105 + 33 |
| Migration registry rows | **106** | `project/build.toml` `[[rust.migrations]]` |
| All registry rows enabled | **106 / 106** | no production `enabled = false` |
| Unique `USE_RUST_*` macros | **103** | Cut CU: 4 blobs / 1 gate |
| Cut CW gate | `USE_RUST_NTFS_SET_FILE_INFO = 1` | `kernel/fs/ntfs.inc` |
| Cut CV gate | `USE_RUST_EXT_SET_FILE_INFO = 1` | `kernel/fs/ext.inc` |
| Phys-bitmap gate | `USE_RUST_PHYS_BITMAP_OWNERSHIP = 1` | `kernel/core/memory.inc` |
| Accidental disabled gate | **None** | live FASM assignments match registry |
| Hidden post-CW production leaf | **None in this audit** | CW remains the newest production migration |
| `dev_build/last_image.txt` | `dev_build/test/kernel-20260814-131226.img` | CW final image |
| Soak-validated ON CoW | `dev_build/test/kernel-20260814-130700.img` | CW implementation |
| Regression log | REG-001…019; **no REG-020** | [`regression-log.md`](regression-log.md) |

### 1.1 Cut CW confirmation

| Item | Value |
|------|--------|
| Status | **COMPLETE** |
| Target | `ntfs_SetFileInfo` Path B |
| Blob | **180 B**, 0 relocs |
| SHA-256 | `91d143e331dedf992439d1115b7029bdb8a1cd66897c377938839994b77945b8` |
| QEMU | OFF / ON / A/B / ON×3 + control PASS, RESET=0 |
| Oracle | `$I30` atime/mtime PASS; SI/FN unchanged; USA PASS; MFT 19 / parent 5 |

CW does **not** migrate `writeRecord`, USA write, `ntfs_find_lfn`, `ntfs_lock`, `ntfsDone`, Delete, SetFileEnd, or PTE.

### 1.2 Memory pack (REG-012) — **compacted**

See [`reg012-headroom-audit.md`](reg012-headroom-audit.md). Pack addresses **unchanged**.

| Symbol | Post-CW (before compaction) | After REG-012 |
|--------|----------------------------|---------------|
| `TMP_STACK_TOP` / `sys_proc` | `0x008E000` | **unchanged** |
| `SLOT_BASE` | `0x0090000` | **unchanged** |
| end `.bss` | `OS_BASE+0x8CFC3` | **`OS_BASE+0x8C6C3`** |
| **Assertion slack** | **61 B** | **2365 B** |
| `kernel.mnt` | 304872 B | 304632 B |

Memory is **no longer** the primary blocker for a ~180–900 B Path B class.

CW itself had to put NSFI fixtures on the **stack** and drop the second kernel smoke vector to assemble. Linear FASM layout still means new `.text`/`.data` before `.bss` consumes assertion slack — measure any future blob.

Do **not** move `TMP_STACK_TOP`, `sys_proc`, or `SLOT_BASE` for a weak candidate.

---

## 2. Current ownership

### 2.1 Rust-owned

| Domain | Status |
|--------|--------|
| Physical bitmap (`sys_pgmap`, `pages_free`, `page_start`) | **Path A COMPLETE** (Cut CU / Slice E) |
| `alloc_page` / `free_page` / `alloc_pages` | **COMPLETE** |
| Mode-B release bitmap helper | **COMPLETE** |
| EXT `ext_SetFileInfo` | **Path B COMPLETE** (Cut CV) |
| NTFS `ntfs_SetFileInfo` | **Path B COMPLETE** (Cut CW) |
| Many Path B footholds | Complete — injection ≠ subsystem ownership |

### 2.2 FASM-owned major domains

| Domain | Status |
|--------|--------|
| Virtual map / PTE / PDE / `invlpg` / `page_tabs` | FASM multi-writer — **blocked** |
| Fault / CR3 / process address spaces | FASM |
| `release_pages` orchestration | FASM (bitmap Rust) |
| NTFS mount + remaining plugins (`Delete`, `SetFileEnd`, Create/Write, `writeRecord`) | FASM |
| TCP/IP output (`tcp_output`, `ipv4_output*`) | FASM |
| Scheduler / `create_process` | FASM (Stage 6–7) |
| IRQ PIC/APIC policy | FASM |
| GUI server (beyond foothold leaves) | FASM |

---

## 3. Complete pending inventory (33 symbols)

`ntfs_SetFileInfo` is **no longer pending**. Classes from live `kernel/` (not memory).

| # | Symbol | Primary class | Verdict |
|---|--------|---------------|---------|
| 1 | `strnlen` | thin / export-only | **REJECT** |
| 2 | `strtoint_dec` | dead (`conf_lib.inc` unlinked) | **REJECT** |
| 3 | `ntfs_create_partition` | mount orchestration | **DEFER** — §4 |
| 4 | `ntfs_restore_usa_frs` | fallthrough to Cut J | **REJECT** |
| 5 | `fs_execute` | Stage 6 process create | **DEFER** |
| 6 | `disk_scan_gpt` | disk orchestration | **DEFER** |
| 7 | `disk_scan_partitions` | disk orchestration | **DEFER** |
| 8 | `ipv4_output` | protocol island | **oracle/soak + pack blocked** |
| 9 | `ipv4_output_raw` | protocol island | **oracle/soak + pack blocked** |
| 10 | `net_ptr_to_num` | thin wrapper over AY | **REJECT** |
| 11 | `socket_check_owner` | thin PID compare | **REJECT** |
| 12 | `socket_check_port` | anti-cluster + mutex | **REJECT** |
| 13 | `socket_num_to_ptr` | anti-cluster + mutex | **REJECT** |
| 14 | `socket_ptr_to_num` | anti-cluster | **REJECT** |
| 15 | `tcp_mss` | 3-instruction 1420 clamp | **REJECT** |
| 16 | `tcp_output` | protocol island (~753 lines) | **oracle/soak / blast + pack blocked** |
| 17 | `get_phys_addr` | thin PE glue over AQ | **REJECT** |
| 18 | `map_page` | PTE coupling | **ownership blocked** |
| 19 | `mem_test` | hardware / E820 | **REJECT** |
| 20 | `create_process` | Stage 6 orchestration | **DEFER** |
| 21 | `pid_to_appdata` | dead sibling | **REJECT** |
| 22 | `set_app_params` | Stage 6 | **DEFER** |
| 23 | `mutex_init` | thin primitive, fan-out | **REJECT** |
| 24 | `enable_irq` | hardware-I/O | **REJECT** |
| 25 | `irq_eoi` | hardware-I/O | **REJECT** |
| 26 | `i40` | architecture boundary | **REJECT** (Cut C0) |
| 27 | `syscall_entry` | architecture boundary | **REJECT** (Cut C0) |
| 28 | `sysenter_entry` | architecture boundary | **REJECT** (Cut C0) |
| 29 | `sysfn_getfreemem` | thin façade | **REJECT** |
| 30 | `sysfn_mouse_acceleration` | thin façade over Cut L | **REJECT** |
| 31 | `change_task` | unsuitable/late | **REJECT** |
| 32 | `do_change_task` | boundaries non-cut | **REJECT** |
| 33 | `find_next_task` | Stage 6 scheduler | **DEFER** |

No pending GUI/video/HID symbols remain in the scoped checklist.

Do **not** inflate the inventory with `ntfs_Delete` / `ntfs_SetFileEnd` / `writeRecord` / `ntfs_find_lfn` even though they exist in `kernel/fs/ntfs.inc`.

---

## 4. Filesystem write-path reassessment

Two successful metadata leaves (EXT CV, NTFS CW) share **syscall 70 subfn 6** and a CoW+guest-log+host-parser pattern. They do **not** share:

- lock implementation (`extfsWritingInit` vs `ntfs_lock`)
- on-disk object (inode vs parent `$I30`)
- writeback (`writeInode`+SB vs `writeRecord`+USA)
- time encoding (Unix+offset vs FILETIME)

**Path A FS ownership is not implied by proximity.**

### 4.1 Remaining NTFS / disk symbols **in** the 33

| Symbol | Shared mutable state | Verdict |
|--------|----------------------|---------|
| `ntfs_create_partition` | Constructs the live `NTFS` object (lock, FRS, index bufs, bitmaps, MCB) used by **all** plugins | Mount orchestration — §4.3 |
| `ntfs_restore_usa_frs` | 3-line fallthrough to Rust `ntfs_restore_usa` | **REJECT** |
| `disk_scan_gpt` / `disk_scan_partitions` | MBR/EBR/GPT loop; already composed of Cuts Z/AD/CC | Disk orch — **DEFER**; not a metadata-write cluster |

### 4.2 Out-of-inventory NTFS write siblings (do not count)

`ntfs_Delete`, `ntfs_SetFileEnd`, `ntfs_CreateFile`, `ntfs_WriteFile`, `writeRecord` share `ntfs_lock` / `ntfs_find_lfn` / bitmap / USA writeback with CW. That would be an NTFS **runtime** Path A mega-slice, not a next Path B leaf. No independent Delete/SetFileEnd soak exists. **Do not start that cluster.**

### 4.3 Targeted audit: `ntfs_create_partition`

**LOCAL FACT** — `kernel/fs/ntfs.inc` ~294–540; sole caller `kernel/blkdev/disk.inc` `disk_scan_partitions` FS probe list (alongside FAT/exFAT/EXT/XFS/ISO).

| Question | Answer |
|----------|--------|
| Role | Mount: validate bootsec, allocate `NTFS`, load `$MFT`, decode MCB, map `$Bitmap` / MFT bitmap |
| Boot vs runtime | Disk attach / partition scan (boot and hot-plug) |
| Superblock | Boot sector via `ntfs_test_bootsec` (Cut AG already Rust) |
| MFT bootstrap | `fs_read64_sys` + `ntfs_restore_usa_frs` + unnamed `$DATA` scan + `ntfs_decode_mcb_entry` (Cut I) |
| Locks | `mutex_init` on `NTFS.Lock` |
| Caches | `frs_buffer`, two 4 KiB index buffers, `BitmapBuffer` via `alloc_kernel_space` + **`alloc_pages` + `commit_pages`** |
| Disk sync | Reads only at mount; no `writeRecord` |
| Error handling | Multi-stage `kernel_free` unwind; EAX=0 if not NTFS |
| Leaf vs orch | **Orchestration** (~246 lines + helpers) |
| Shared state | **Yes** — the object every NTFS plugin later mutates |

Classification vs the requested A/B/C/D:

- **Not A** (not a Path B leaf)
- **B-shaped** internally (Stage-5 mount cluster) but **must remain C**: mount subsystem boundary, deferred
- **D as next cut:** not useful — would pull allocator commit, mutex, attr read, USA, MCB, bitmap window; blob would blow REG-012 slack

Uses Rust-owned `alloc_pages` only as a **consumer**. That does **not** make mount Rust-ready (PTE `commit_pages` still FASM).

**Filesystem verdict:** keep remaining FS work Path B if ever authorized; **no** FS Path A; **no** Cut CX from NTFS adjacency.

---

## 5. Network reassessment

**Amendment:** user-net + `filter-dump` harness now exists (`scripts/qmp_ipv4_output_soak.py`). Default `[qemu].args` still have no NIC (desktop-only). Do not treat default `run_qemu.py` as a net soak.

| Symbol | Size / role | Oracle | Soak |
|--------|-------------|--------|------|
| `ipv4_output` | ~100 lines; route (Cut AC) + ARP + `eth_output` / loopback | **Ready** (RFC 791/1071, 50k) | **Ready** (DHCP/UDP ×3, RESET=0) |
| `ipv4_output_raw` | socket+copy variant; documented caller quirk | Not this program | Missing |
| `tcp_output` | `proc`, ~753 lines, socket mutex, window, `TCP_BIT_SENDALOT` | **Missing** | **Missing** |

Live capture (guest MAC `52:54:00:12:34:56`): TTL 128, ID 0, IHL `0x45`, TOS 0, flags 0, proto 17, checksum OK — matches FASM `ipv4_output` constants. QEMU slirp replies (TTL 64, ID≠0) excluded.

Memory fit is no longer the blocker (2365 B slack). **Do not start Cut CX from this audit.**

---

## 6. PTE reassessment (delta only)

No new `page_tabs` sole-writer, no PDE oracle, no fault soak, no mapping-boundary cleanup since CW. CW did not touch paging. Headroom **tightened** (61 B slack). **`map_page` remains rejected.** PTE OWNERSHIP STILL BLOCKED.

---

## 7. Process / scheduler

`create_process`, `find_next_task`, `fs_execute`, `set_app_params` still require process-table / scheduler / CR3 / address-space lifecycle. Multiple callers do not create a small coherent state boundary. **DEFER Stage 6.**

---

## 8. GUI / HID / video

Zero pending checklist symbols. Existing Rust leaves (`blit_clip`, `blit_32`, `drawChar`, `set_window_clientbox`, `set_mouse_data`, …) are footholds, not a Rust-owned graphics/input subsystem. **Reject further leaf inflation.**

---

## 9. Thin / dead / hardware

Policy unchanged. Confirmed live:

- `tcp_mss`: clamp to 1420, one caller
- `socket_check_owner`: PID compare vs `current_slot`
- `strnlen`: export-only
- `mutex_init`: primitive without lock-subsystem ownership
- `enable_irq` / `irq_eoi`: hardware I/O without deterministic oracle

Do **not** pick a 20-byte wrapper merely because it might fit the 61 B slack.

---

## 10–11. Path A search and Rust-ownership leverage

| Candidate grouping | Coherent shared state? | Oracle/soak? | Verdict |
|--------------------|------------------------|--------------|---------|
| EXT+NTFS SetFileInfo | **No** — different FS objects/locks/writeback | Each leaf already done | Not Path A |
| NTFS mount + plugins | Yes (`NTFS` struct) | Mount+Delete+Write soaks missing; huge blast | Mega-slice — **not ready** |
| Network output buffers | Partial (`net_device_list`, sockets) | pcap + packet oracle ready for `ipv4_output` | Island — Path B leaf evidence done; no gate |
| Allocator consumers (`commit_pages`, mount) | Still touch FASM PTE | — | Consumer ≠ owner |
| Graphics/input | No remaining pending cluster | — | Footholds only |

**No valid new Path A.** Using the Rust allocator does not make a consumer cut-ready.

---

## 12. Memory constraint (selection filter)

| Next-cut class | Est. growth | Fits 2365 B slack? |
|----------------|-------------|---------------------|
| Thin wrapper | ~20–40 B | Physically yes — **policy REJECT** |
| CW-class Path B leaf | ~180 B blob + trampoline + smoke | **Yes** |
| `ipv4_output` | hundreds of B + ctx | **Likely yes — still measure blob** |
| `ntfs_create_partition` / `tcp_output` | multi-KiB | **No** without pack move |

REG-012 compaction is **complete**. Do not move the pack.

---

## 13. Candidate scorecard

| # | Candidate | ABI | Semantic oracle | Host oracle | Live callers | Subsystem soak | Ownership | Blast | Memory | Rollback | Payoff | Cut now? |
|---|-----------|-----|-----------------|-------------|--------------|----------------|-----------|-------|--------|----------|--------|----------|
| 1 | `ipv4_output` | Good (TTL in `AL`) | Header fields | **50k RFC 791/1071** | Yes | **user-net + filter-dump ×3** | Path B island | Med (ARP/eth FASM) | Slack 2365 B | Gate OK later | Med | **No this turn** |
| 2 | `ipv4_output_raw` | Quirky | Same | Not this program | Yes | Missing | Path B | Med | Slack OK | Gate OK | Low | **No** |
| 3 | `disk_scan_partitions` | Loop orch | Partition list | Weak | Yes | Attach soak only | Orch | High | Fail size | Hard | Low | **No** |
| 4 | `ntfs_create_partition` | Mount orch | Volume object | Partial (bootsec AG) | 1 | Attach ≠ mount unit | Orch / future Path A | **Very high** | Fail size | Hard | High later | **No** |
| 5 | `tcp_output` | Complex | TCP segments | **Missing** | Yes | **Missing** | Island | **Very high** | Fail size | Hard | High later | **No** |

Do not rank by caller count. Thin/dead/IRQ/C0 symbols score zero and are omitted.

---

## 14. Selected next frontier

**Type:** **IPV4_OUTPUT EVIDENCE** (this program) — then a future Cut CX **plan** only.

Completed this turn: independent packet oracle, host parser, QEMU user-net/`filter-dump`, guest firstapp stimulus, FASM capture. See [`stage4-ipv4-output-oracle.md`](stage4-ipv4-output-oracle.md).

**Do not start Cut CX. Do not add `USE_RUST_IPV4_OUTPUT`.**

**Why not NEXT CUT READY:** evidence ≠ authorized migration. Trampoline, blob fit measurement, and rollback gate are still a *plan*.

**Why not PATH A RESEARCH READY:** no new sole-writer cluster.

**Why not STILL BLOCKED (memory):** REG-012 restored 2365 B slack.

---

## 15. Documentation touchpoints

| Path | Action |
|------|--------|
| This file | Amended after REG-012 + ipv4_output evidence |
| [`stage4-ipv4-output-oracle.md`](stage4-ipv4-output-oracle.md) | **Created** |
| [`migration-plan.md`](migration-plan.md) | Frontier pointer |
| Inventory / gates / production networking | **Unchanged** |

---

## 16. Explicit non-goals (this audit)

- No Rust source, no new gate, no Cut CX
- No edits to `ntfs_create_partition`, networking, PTE, scheduler
- No speculative memory-pack move
- No inventory increment
- No reopening Cut CW without a new reproducible regression
