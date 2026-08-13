# Post-Cut CV Migration Frontier Audit

**Date:** 2026-08-14  
**Status:** audit complete — Cut CW **COMPLETE** ([`cut-cw-implementation.md`](cut-cw-implementation.md)). Do **not** start Cut CX.  
**Inventory:** **105 / 138** (33 pending)  
**Production gates:** **106** enabled `[[rust.migrations]]`  
**Cut CV:** **COMPLETE** — do not reopen without a new reproducible regression  
**Parent:** [`cut-cv-implementation.md`](cut-cv-implementation.md), [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> Fresh post-CV audit of all **34** pending symbols. Selects **exactly one** next
> research/tooling frontier. Does **not** implement Cut CW, change inventory/gates,
> or modify production code.

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Is Cut CV still complete and gated ON? | **Yes** — `USE_RUST_EXT_SET_FILE_INFO = 1`; doctor PASS |
| Did CV unlock a new Path A boundary? | **No** |
| Does any pending Path B leaf clear the evidence bar today? | Cut CW **COMPLETE**; remaining pending are REJECT/DEFER/blocked |
| Strongest substantive next domain? | **NTFS `SetFileInfo`** (filesystem write Path B) |
| What blocks an immediate cut? | Cut CW **plan exists** — implementation not authorized |
| PTE status changed since CV? | **No** — still blocked |
| Decision | **NTFS SETFILEINFO EVIDENCE READY — SECONDARY TOOLING GAP** (debugfs absent; primary `$I30` oracle live) |
| Next research task (one) | Implement Cut CW **only when authorized** — plan: [`cut-cw-plan.md`](cut-cw-plan.md) |

**STAGE-4 POST-CV AUDIT — COMPLETE — STOP**

**Update 2026-08-14 (NTFS oracle program):** See [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md). Host `$I30` parser, guest `NTFSOAK1`/`NSFI.LOG`, and QMP harness are in-tree.

**Update 2026-08-14 (NTFS boot/attach stall):** See [`stage4-ntfs-boot-stall.md`](stage4-ntfs-boot-stall.md). Pre-firstapp hang was invalid FILE records (USA overlaying `0xFFFFFFFF`). Recipe fixed. Not a kernel regression.

**Update 2026-08-14 (Cut CW):** [`cut-cw-implementation.md`](cut-cw-implementation.md) — production ON. Do **not** start Cut CX.

---

## 1. Authoritative repository state

Verified 2026-08-14 from live tree (git clean at `5c9b7b4 cut cv`).

| Item | Value | Source |
|------|-------|--------|
| Completed symbols | **104** | [`migration-todo.md`](migration-todo.md) — 104 `[x]` |
| Pending symbols | **34** | [`migration-todo.md`](migration-todo.md) — 34 `[ ]` |
| Scoped total | **138** | 104 + 34 |
| Migration registry rows | **105** | `project/build.toml` `[[rust.migrations]]` (production section) |
| All registry rows enabled | **105 / 105** | no `enabled = false` in production registry |
| Unique `USE_RUST_*` gates ON | **102** | shared gates: Cut CU (4 blobs), Cut A (4 symbols) |
| Doctor | **PASS** | `python scripts/doctor.py` |
| Accidental disabled gate | **None** | live FASM gates match registry |
| Post-CV uncommitted production drift | **None** | `git status` clean |

### 1.1 Cut CV verification

| Item | Value |
|------|--------|
| Status | **COMPLETE** |
| Gate | `USE_RUST_EXT_SET_FILE_INFO = 1` (`kernel/fs/ext.inc`) |
| Blob | **398 B**, 0 relocs |
| SHA-256 | `b10b847708cdfce77dfbb5589fbdfb32f30c7eab2bb09198778f1feb1df46d27` |
| Context | 44 bytes (`ExtSetFileInfoCtx`) |
| Final image (CV doc) | `dev_build/test/kernel-20260813-220901.img` |
| `dev_build/last_image.txt` | `dev_build/test/kernel-20260813-221509.img` (newer local build pointer) |
| Known pre-enable issue (EDI/`findInode`) | Fixed before gate ON — **do not reopen** without new reproduction |

Cut CV does **not** migrate `writeInode`, `writeSuperblock`, `disk_sync`, `findInode`,
`extfsWritingInit`, other plugins, or PTE.

### 1.2 Memory pack (unchanged — REG-012)

| Symbol | Value |
|--------|--------|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `OS_BASE + 0x008E000` |
| `SLOT_BASE` | `OS_BASE + 0x0090000` |
| Post-CV end `.bss` | `OS_BASE + 0x8CD03` (Cut CV build log) |
| Headroom to `TMP_STACK_TOP` | **0x12FD (4861 B ≈ 4.7 KiB)** |
| `kernel.mnt` (user baseline) | **304136 B** |

Do **not** move `TMP_STACK_TOP`, `sys_proc`, or `SLOT_BASE` for a weak candidate.

---

## 2. Current ownership summary

### 2.1 Rust-owned (runtime / documented ownership)

| Domain | Status |
|--------|--------|
| Physical bitmap (`sys_pgmap`, `pages_free`, `page_start`) | **Path A COMPLETE** (Cut CU / Slice E) |
| `alloc_page` / `free_page` / `alloc_pages` | **COMPLETE** |
| Mode-B `release_bitmap_page_without_cursor_update` | **COMPLETE** (CU) |
| EXT metadata write leaf (`ext_SetFileInfo`) | **Path B COMPLETE** (Cut CV) |
| Many Path B footholds (calendar, blit, PE, strings, …) | Complete — injection ≠ ownership |

### 2.2 FASM-owned major domains

| Domain | Status |
|--------|--------|
| Virtual map / PTE / PDE / `invlpg` | FASM multi-writer — **blocked** |
| Fault / CR3 / process address spaces | FASM |
| `release_pages` orchestration (mutex + PTE) | FASM (bitmap Rust) |
| NTFS/other FS write plugins (`ntfs_SetFileInfo`, Delete, …) | FASM |
| TCP/IP output (`tcp_output`, `ipv4_output*`) | FASM |
| Scheduler / `create_process` | FASM (Stage 6–7) |
| IRQ PIC/APIC policy | FASM |

---

## 3. Complete pending inventory (34 symbols)

| # | Symbol | Primary class | Path | Notes |
|---|--------|---------------|------|-------|
| 1 | `strnlen` | thin / export-only | — | **REJECT** — PE export only; zero in-kernel callers |
| 2 | `strtoint_dec` | dead | — | **REJECT** — `conf_lib.inc` not linked |
| 3 | `ntfs_create_partition` | mount orchestration | — | **DEFER** Stage 5+ |
| 4 | `ntfs_restore_usa_frs` | fallthrough / wrapper | — | **REJECT** — 3-line fallthrough to Rust `ntfs_restore_usa` (Cut J) |
| 5 | `ntfs_SetFileInfo` | FS write leaf | **B (Cut CW COMPLETE)** | [`cut-cw-implementation.md`](cut-cw-implementation.md) |
| 6 | `fs_execute` | process create | — | **DEFER** Stage 6 |
| 7 | `disk_scan_gpt` | disk orchestration | — | **DEFER** |
| 8 | `disk_scan_partitions` | disk orchestration | — | **DEFER** |
| 9 | `ipv4_output` | protocol island | — | **oracle/soak blocked** |
| 10 | `ipv4_output_raw` | protocol island | — | **oracle/soak blocked** |
| 11 | `net_ptr_to_num` | thin wrapper | — | **REJECT** — `ror edi,2` over migrated `net_ptr_to_num4` |
| 12 | `socket_check_owner` | thin / dead | — | **REJECT/DEFER** |
| 13 | `socket_check_port` | anti-cluster + mutex | — | **REJECT** |
| 14 | `socket_num_to_ptr` | anti-cluster + mutex | — | **REJECT** |
| 15 | `socket_ptr_to_num` | anti-cluster | — | **DEFER/REJECT** |
| 16 | `tcp_mss` | thin clamp | — | **REJECT** — one caller; 3-instruction clamp |
| 17 | `tcp_output` | protocol island | — | **oracle/soak / blast blocked** |
| 18 | `get_phys_addr` | thin PE glue | — | **REJECT** — AQ compose |
| 19 | `map_page` | PTE coupling | — | **ownership blocked** |
| 20 | `mem_test` | hardware probe | — | **REJECT** |
| 21 | `create_process` | Stage 6 orchestration | — | **DEFER** |
| 22 | `pid_to_appdata` | dead sibling | — | **REJECT** — only commented caller in `debug.inc` |
| 23 | `set_app_params` | Stage 6 | — | **DEFER** |
| 24 | `mutex_init` | thin primitive | — | **REJECT** — fan-out without ownership |
| 25 | `enable_irq` | hardware-I/O | — | **REJECT** |
| 26 | `irq_eoi` | hardware-I/O | — | **REJECT** |
| 27 | `i40` | architecture boundary | — | **REJECT** (Cut C0) |
| 28 | `syscall_entry` | architecture boundary | — | **REJECT** (Cut C0) |
| 29 | `sysenter_entry` | architecture boundary | — | **REJECT** (Cut C0) |
| 30 | `sysfn_getfreemem` | thin façade | — | **REJECT** |
| 31 | `sysfn_mouse_acceleration` | thin façade | — | **REJECT** — Cut L delegate |
| 32 | `change_task` | unsuitable/late | — | **REJECT** |
| 33 | `do_change_task` | boundaries non-cut | — | **REJECT** |
| 34 | `find_next_task` | Stage 6 scheduler | — | **DEFER** |

No pending GUI/video symbols remain in the scoped checklist.

---

## 4. NTFS `SetFileInfo` reassessment

### 4.1 Source and syscall path

| Item | Detail |
|------|--------|
| Implementation | `kernel/fs/ntfs.inc` ~lines 4294–4339 (~46 lines) |
| Plugin vtable | `ntfs_user_functions` idx 6 (`GetFileInfo` idx 5) |
| Syscall | sysfn **70** / subfn **6** (same f.70 block as EXT) |
| Safety gate | Cut AZ `file_system_is_operation_safe` (32-byte SetFileInfo buffer) |
| Entry sequence | `ntfs_lock` → `ntfs_find_lfn` → index/FRS target selection → mutate → `writeRecord` → `ntfs_unlock` |

### 4.2 Fields mutated

| Field | Source | Encoding |
|-------|--------|----------|
| `fileFlags` (attrs) | BDFE `[ebx+16]` low byte masked `0x27` | merged into index entry or `$STANDARD_INFORMATION` |
| `fileCreated` | BDFE +8 | FILETIME via `ntfsCalculateTime` (Cut AF — **Rust when gated**) |
| `fileAccessed` | BDFE +16 | FILETIME via `ntfsCalculateTime` |
| `fileModified` | BDFE +24 | FILETIME via `ntfsCalculateTime` |
| Not mutated directly | ctime BDFE slot, size, MCB/data runs | ctime follows existing `writeRecord`/FRS rules |

### 4.3 Index vs FRS path (blast radius)

```
ntfs_find_lfn success
  → fragmentCount must be 1 else ERROR_UNSUPPORTED_FS
  → if cur_index_buf == 'INDX': mutate index entry at computed offset
  → else: mutate $STANDARD_INFORMATION in FRS (frs_buffer or index-relative)
  → writeRecord(frs_or_index_buf, LastRead sector)
```

**USA / fixup:** `writeRecord` increments update-sequence, applies USA words across
512-byte sectors, then `fs_write64_sys` — same class as Cut J `ntfs_restore_usa`
(Rust) but **write direction** with live MFT persistence.

**Locks:** partition-level `ntfs_lock` / `ntfs_unlock` (not the same as EXT mutex path).

**Sync:** implicit via `writeRecord` → disk write; no separate guest sync API required.

### 4.4 Failure semantics

| Case | EAX |
|------|-----|
| Not found | `ERROR_FILE_NOT_FOUND` |
| Fragmented | `ERROR_UNSUPPORTED_FS` |
| find_lfn fail | `ERROR_FS_FAIL` |
| Success | 0 |

### 4.5 Existing Rust NTFS footholds (compose, not ownership)

| Symbol | Cut | Relevance to SetFileInfo |
|--------|-----|--------------------------|
| `ntfsCalculateTime` | AF | **Direct** — 3× per call |
| `ntfsGetTime` | BT | inverse path only |
| `ntfs_restore_usa` | J | read-side USA; write uses `writeRecord` |
| `createMcbEntry` | AX | not on SetFileInfo path |
| `ntfs_decode_mcb_entry` | I | not on SetFileInfo path |

### 4.6 Test tooling today

| Tool | EXT (CV) | NTFS |
|------|----------|------|
| CoW attach soak | `scripts/qmp_ext_setfileinfo_soak.py` | **Missing** |
| Host metadata oracle | `scripts/ext_setfileinfo_oracle.py` + mini EXT2 walker | **Missing** |
| Guest driver | `tools/extsoak/extsoak.asm` | **Missing** |
| Image recipe | `images/ext-image.img`, `scripts/mkfs.py ext` | `scripts/mkfs.py ntfs` + `tools/mkfs_utils/ntfs_minimal.py` (create only) |
| Secondary cross-check | Docker `debugfs` | **No** NTFS equivalent wired |
| `--disk ntfs` attach | yes (`run_qemu.py`) | attach-only soak culture exists (Cut BT) |

### 4.7 Can EXT oracle architecture be reused safely?

| EXT pattern | NTFS reuse |
|-------------|------------|
| sysfn70 subfn 6 guest driver | **Yes** — same f.70 ABI |
| CoW disposable image + recipe JSON | **Yes** — pattern transfers |
| Durable guest log on test volume | **Yes** — e.g. `/hd1/N/NSFI.LOG` |
| Host independent parser | **Needs new work** — MFT record / `$STANDARD_INFORMATION` / `$I30` index entry parser |
| BDFE→on-disk time model | **Different** — FILETIME 1601 epoch, not Unix+978307200 |
| USA write verification | **Needs new work** — post-write sector fixup check |
| debugfs-like tool | **Gap** — no wired `ntfsfix`/`icat` harness |

**Verdict:** NTFS is **not** equivalent to EXT. The **workflow** reuses; the **oracle**
must be NTFS-specific. Cut CV success **does not** automatically qualify NTFS for cut.

### 4.8 NTFS SetFileInfo status

| Criterion | Status |
|-----------|--------|
| Path B boundary | **Clear** (single plugin vtable leaf) |
| ABI clarity | **Good** (same f.70 as EXT) |
| Independent host oracle | **Missing** |
| Guest soak | **Missing** |
| Ownership | **Plugin leaf only** — not Path A FS ownership |
| Memory (est.) | ~600–900 B blob + ~48–56 B ctx + callbacks (fits ~4.7 KiB if authorized) |
| **Overall** | **NOT CUT READY** — tooling gap |

---

## 5. Thin / wrapper reassessment

Verified from live `kernel/` call graph (not memory):

| Symbol | In-kernel callers | Verdict |
|--------|-------------------|---------|
| `strnlen` | **0** (PE export only) | **REJECT** — same class as pre-CN `strchr` |
| `net_ptr_to_num` | **0** direct; wrapper over Rust `net_ptr_to_num4` | **REJECT** |
| `pid_to_appdata` | **0** live (`debug.inc` commented) | **REJECT** |
| `tcp_mss` | **1** (`tcp_input.inc:342`) — 3-instruction clamp to 1420 | **REJECT** — no new oracle |

No new live caller or subsystem boundary appeared since post-PTE audit.

---

## 6. Network reassessment

Infrastructure **unchanged** since prior audits:

| Capability | Status |
|------------|--------|
| Rust footholds | checksums, route, fragment slot, TCP timers/flags, `socket_check`, `net_ptr_to_num4` |
| `scripts/run_qemu.py` netdev/user-net/pcap | **Absent** — no packet capture hooks |
| Deterministic packet oracle | **Missing** |
| Host packet parser for output path | **Missing** |
| Network soak scripts | Desktop smoke only; no output-byte A/B |
| Pending leaves | `tcp_output` (~700+ lines), `ipv4_output`, `ipv4_output_raw` |

**Verdict:** Network remains **oracle/soak blocked**. Stage-5 value is high but effort
exceeds filesystem write tooling. Rank **#2** practical frontier behind NTFS write oracle.

---

## 7. Filesystem write-path cluster

| Question | Answer |
|----------|--------|
| Path A FS write ownership? | **No** — no Rust-owned shared FS plugin state |
| EXT write cluster | **Complete at leaf level** — `ext_write_time` (BS) + `ext_SetFileInfo` (CV) remain separate Path B leaves |
| Next plugin write leaf | **`ntfs_SetFileInfo`** |
| Other pending write symbols in checklist | **None** (`fat_SetFileInfo` / `exFAT_SetFileInfo` exist in tree but are **out of scope**) |
| Fake Path A cluster? | **Forbidden** — adjacent plugin leaves ≠ ownership |

Cut CV proved the **EXT oracle → Path B cut** pipeline. NTFS is the natural **next
plugin write** target, not a bundled Path A program.

---

## 8. PTE status (no new evidence)

Cut CV touched EXT inode write only — **no change** to:

- dual-use `page_tabs` (hardware PTE + soft `MEM_BLOCK_*`)
- ~23 audited runtime PTE writers (+ boot/fault/unresolved PCIe)
- missing PDE oracle v1 / live PTE soak
- ~4.7 KiB REG-012 headroom (insufficient for mega-slice)

**Verdict:** **PTE OWNERSHIP STILL BLOCKED** — do not restart full PTE audit.

---

## 9. Path A assessment

| Claim | Result |
|-------|--------|
| New Path A ready after CV | **No** |
| Phys-bitmap Path A | Done (CU) |
| EXT SetFileInfo ownership | **No** — leaf + FASM callbacks |
| Virtual-map Path A | Blocked (PTE program complete, decision unchanged) |
| Network Path A | Needs output-queue ownership + packet soak |
| Process/sched Path A | Stage 6 — deferred |
| Manufactured bundles (thin wrappers, adjacent FS leaves) | **Forbidden** |

---

## 10. Rust subsystem leverage (post-CV)

| Leverage | Unlocks pending cut? |
|----------|---------------------|
| EXT SetFileInfo pattern (ctx + callbacks) | **Template only** for NTFS — not evidence |
| `fsCalculateTime` / calendar Rust leaves | Already composed in CV; NTFS uses `ntfsCalculateTime` (AF) |
| Phys allocator ownership | **No** — pending consumers still touch PTE/process |
| NTFS time/USA/MCB Rust footholds | Reduces **future** NTFS blob size; **not** oracle substitute |

---

## 11. Oracle / soak score (serious candidates)

| Rank | Candidate | ABI | Oracle | Soak | Blast | Memory | Ownership | Ready? |
|------|-----------|-----|--------|------|-------|--------|-----------|--------|
| 1 | `ntfs_SetFileInfo` | Good | **Missing** | **Missing** | Medium | ~OK | Plugin B | **No** |
| 2 | `tcp_output` | Hard | **Missing** | **Missing** | **High** | Risky | None | **No** |
| 3 | `ipv4_output` | Medium | **Missing** | **Missing** | High | Risky | None | **No** |
| 4 | `ipv4_output_raw` | Medium | **Missing** | **Missing** | High | Risky | None | **No** |
| 5 | `map_page` | Known | Partial (pteo host) | **Missing** | **Extreme** | Blocked | PTE blocked | **No** |

All thin/dead/IRQ/orch symbols score below the evidence bar by design.

---

## 12. Top candidates (ranked)

1. **`ntfs_SetFileInfo`** — strongest pending Path B; same sysfn70 write class as CV; needs NTFS MFT oracle
2. **`tcp_output`** — largest Stage-5 island; blocked on packet netdev/oracle stack
3. **`ipv4_output`** — egress compose; same network tooling gap
4. **`ipv4_output_raw`** — raw egress variant; same gap
5. **`map_page`** — architecturally central but **ownership blocked** (not a near cut)

**Rejected for ranking uplift:** `strnlen`, `net_ptr_to_num`, `pid_to_appdata`, `tcp_mss` — thin/dead/wrapper class unchanged.

---

## 13. Candidate comparison

| Dimension | NTFS SetFileInfo | tcp_output | Thin leaves |
|-----------|------------------|------------|-------------|
| Semantic substance | High (metadata + USA persist) | High | None |
| Independent oracle | **Live `$I30` PASS** (debugfs absent) | **Gap** | N/A |
| Existing infra | corrected `ntfs_minimal.py` + NTFSOAK + QMP soak | checksum/route footholds | None |
| CV precedent | EXT oracle pipeline **proven** | None | N/A |
| Effort to unlock | **Done** (live ×3 + control) | **High** (netdev + pcap + state) | N/A |
| Path A? | No | No | No |
| Inventory inflation risk | Low (if evidence-first) | High | **High** |

---

## 14. Selected next frontier

### CoW NTFS `SetFileInfo` — evidence complete; Cut CW **plan** is next

**Type:** **PLANNING** (research only — **not Cut CW implementation**)

Live evidence (2026-08-14) is in [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md):

1. Corrected `ntfs_minimal.py` fixture under `dev_build/ntfssoak/` (not `images/ntfs-image.img`).
2. Guest `NTFSOAK1` — sysfn 70 subfn 5/6, durable `NSFI.LOG`.
3. Host FILETIME model includes Cut-AF `ntfsCalculateTime` ADC carry.
4. Host parser: parent `$I30` + USA + file `$STANDARD_INFORMATION` / `$FILE_NAME` unchanged.
5. ×3 mutate + 1 GetFileInfo-only control, RESET=0.
6. debugfs/ntfsprogs **not installed** (secondary gap only).

**Next concrete task:** implement Cut CW **only when authorized**. Plan: [`cut-cw-plan.md`](cut-cw-plan.md).

**Explicitly out of scope now:** Rust `ntfs_SetFileInfo`, any new `USE_RUST_*` gate, inventory change, PTE, network netdev.

**Why selected:**

- Strongest **substantive** pending symbol after CV — not adjacent-inventory inflation.
- EXT CV proved oracle-first gating works; NTFS is the direct next filesystem **write** leaf in checklist scope.
- Smaller blast and tooling surface than `tcp_output` + netdev program.
- Fits REG-012 memory headroom **if** later authorized (~600–900 B class estimate).

**Why not NEXT CUT READY:** Live ×3 soak **PASS**. Cut CW **plan** authored. Implementation not started. debugfs is optional secondary.

**Why not PATH A RESEARCH:** No coherent Rust-owned FS plugin state — only individual leaves qualify.

**Why not STILL BLOCKED:** Guest + custom `$I30` oracle + writeback are live and repeatable.

---

## 15. Memory / blob constraints (serious candidates)

| Candidate | Est. blob | Est. ctx | Relocs | `.bss` | Fits REG-012? |
|-----------|-----------|----------|--------|--------|---------------|
| `ntfs_SetFileInfo` | 600–900 B | 48–56 B | 0 (callback model) | 0 | **Likely** (~4.7 KiB headroom) |
| `tcp_output` | multi-KiB | large | unknown | risk | **Unlikely** without pack move |
| `map_page` | N/A | N/A | N/A | N/A | **Blocked** (ownership) |

Do **not** raise `TMP_STACK_TOP` / move `SLOT_BASE` for network or weak leaves.

---

## 16. Remaining blockers (summary)

| Blocker | Affected domain |
|---------|-----------------|
| NTFS live SetFileInfo soak ×3 | **Closed**; Cut CW **PLANNED** ([`cut-cw-plan.md`](cut-cw-plan.md)); not implemented |
| Packet capture + byte oracle + netdev | `tcp_output`, `ipv4_output*` |
| PTE dual-use + multi-writer + soak | `map_page`, Path A virtual map |
| Stage 6 lifecycle oracle | `create_process`, `find_next_task`, … |
| Hardware/IRQ oracle | `enable_irq`, `irq_eoi` |
| Thin/dead/export-only policy | `strnlen`, `net_ptr_to_num`, `pid_to_appdata`, `tcp_mss`, … |

---

## 17. Documentation touchpoints

| Path | Action |
|------|--------|
| This file | Created (post-CV audit); boot-stall update 2026-08-14 |
| [`stage4-ntfs-boot-stall.md`](stage4-ntfs-boot-stall.md) | Boot/attach isolation — **COMPLETE** |
| [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md) | Live ×3 soak + control **PASS** (debugfs absent) |
| [`migration-plan.md`](migration-plan.md) | Pointer added |
| [`migration-todo.md`](migration-todo.md) | **Unchanged** (inventory correct) |
| Registry / gates / production code | **Unchanged** |

---

## 18. Decision

**TOOLING / EVIDENCE GAP**

**NTFS SETFILEINFO EVIDENCE READY — SECONDARY TOOLING GAP**

Live ×3 soak + control **PASS**. Cut CW **plan** authored ([`cut-cw-plan.md`](cut-cw-plan.md)). Do **not** implement Cut CW until authorized.
