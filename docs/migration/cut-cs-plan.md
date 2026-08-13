# Cut CS Plan

**Date:** 2026-08-13  
**Status:** **BLOCKED — STOP** (no migration selected)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CS** was the next production migration after Cut CR
> (`drawChar`). Fresh post-CR audit found **no** pending symbol that clears the
> evidence bar. Do **not** invent a thin wrapper cut to advance `99/135`.
> Cut CR remains complete and must not be modified.

---

## Fresh post-CR repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **99 / 135** (`migration-todo.md`; 99 `[x]` + 36 `[ ]`) |
| Production gates | **99** `[[rust.migrations]]` with `enabled = true` |
| Cut CR | **complete** — `drawChar` SHA `9fe4d8e9636149563e1f0b65cfa9b0df76453abe3a61e0dcf10e56c864e6f15f` (**1958 B / 0 reloc**; 1× path only) |
| `USE_RUST_DRAW_CHAR` | **1** |
| REG-012 pack | `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` = **`0x8E000` / `0x8E000` / `0x90000`** |
| End `.bss` (CR ON) | **`OS_BASE+0x8C3C3`** |
| Early-stack assert | `0x8D3C3 < 0x8E000` → **~4.9 KiB** headroom |
| Final CR image | `dev_build/test/kernel-20260813-162513.img` |
| Desktop baseline | **779380** non-black, `resets=0` |

**Do not restore stale `0x008F000` / `SLOT_BASE=0x91000`.** Do not raise `TMP_STACK_TOP` to force an oversized blob. Do not reopen REG-016/017/018/019 without new evidence. Do not modify Cut CR (including expanding scaled `esi!=1` into Rust without a memory-architecture re-audit).

### Path A decision: **REJECTED**

No pending cluster establishes genuine Rust-owned subsystem state, a coherent
multi-function boundary, fewer FASM↔Rust crossings, a subsystem oracle, and a
production soak that is stronger than a collection of adjacent leaves.

| Cluster | Why not Path A |
|---------|----------------|
| AQ+BL+CI + `alloc_page` / `map_page` / `get_phys_addr` | Translate footholds ≠ paging/allocator ownership (CLI, `sys_pgmap`, `invlpg`, page-fault / CR3 still FASM) |
| AS/AY + `socket_*` siblings | Cut AS is lock-free membership; `socket_num_to_ptr` / `socket_check_port` take `socket_mutex` and mutate ports — list lifecycle still FASM |
| AC/M/V/BD + `tcp_output` / `ipv4_output*` | Timer/route/flags leaves ≠ protocol-stack ownership |
| AH+AI+CL+CQ exFAT | Hash/sector/lookup leaves ≠ plugin ownership |
| AL+BR+BS+CM EXT + `ext_SetFileInfo` | Time/address leaves ≠ EXT write-path ownership |
| J + `ntfs_restore_usa_frs` / `ntfs_SetFileInfo` | USA restore is Rust; FRS is fallthrough; SetFileInfo is write orchestration |
| Video H+CD+CP | Clip/blit leaves; LFB / win_map / cursor policy still FASM |
| GUI S+CE+N+CR | Geometry + AA + 1× glyph ≠ GUI server ownership |
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader orchestration stays FASM |
| D+BB+BF+BH+CN + `strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ `enable_irq` / `irq_eoi` | Mask/EOI leaves; PIC/APIC init + ISR still FASM |
| Stage 6 `create_process` / `fs_execute` / `find_next_task` | Process/sched ownership explicitly late / unsuitable |
| CO `unpack` | Decoder island — done; still not subsystem ownership |

Cut CS has **no Path A target**. Manufacturing a Path A from unrelated leaves is forbidden.

---

## Complete candidate ranking (36 pending)

| Rank | Symbol(s) | FASM locus | Callers / blast | Oracle | Verdict |
|------|-----------|------------|-----------------|--------|---------|
| 1 | `alloc_page` / `map_page` | `memory.inc` | Broad (heap, taskman, AHCI, FB, fault path) | Strong synthetic bitmap/PTE, weak live TLB/CLI | **DEFER** — Stage 4; catastrophic blast; no Rust allocator ownership |
| 2 | `tcp_output` / `ipv4_output` / `ipv4_output_raw` | `tcp_output.inc` / `IPv4.inc` | Many TCP/UDP/ICMP send sites | Needs independent packet oracle + net soak | **DEFER** — Stage 5 protocol island |
| 3 | `ntfs_SetFileInfo` / `ext_SetFileInfo` | `ntfs.inc` / `ext.inc` | FS plugin write vtable | Metadata write + disk A/B required | **DEFER** — FS write-path blast |
| 4 | `disk_scan_gpt` / `disk_scan_partitions` | `disk.inc` | Mount orchestration | Needs GPT/MBR fixture soak beyond CC/Z/AD | **DEFER** — disk orchestration |
| 5 | `ntfs_create_partition` | `ntfs.inc` | Mount | Mount orchestration | **DEFER** |
| 6 | `create_process` / `fs_execute` / `set_app_params` | `taskman.inc` | Process create | Stage 6 | **DEFER** / unsuitable early |
| 7 | `find_next_task` / `change_task` / `do_change_task` | `sched.inc` | Scheduler | Boundaries non-cut / late | **REJECT** unsuitable / late |
| 8 | `i40` / `syscall_entry` / `sysenter_entry` | syscall entry | Every syscall | Boundaries Cut C0 | **REJECT** — preserve entry asm |
| 9 | `ntfs_restore_usa_frs` | `ntfs.inc` ~1118 | **4** live (`ntfs_restore_usa_frs` → fallthrough J) | Cut J replay only | **REJECT** — `mov eax,[ebp+NTFS.frs_size]` + fallthrough; zero new semantics |
| 10 | `socket_num_to_ptr` / `socket_check_port` | `socket.inc` | Many / bind path | List walk + **mutex** | **REJECT** — AS/AY anti-cluster; mutex/list lifecycle FASM |
| 11 | `socket_ptr_to_num` | `socket.inc` | **0** in-kernel | AS compose | **REJECT** — dead / thin wrapper |
| 12 | `socket_check_owner` | `socket.inc` | **0** in-kernel | TID cmp | **REJECT** — dead thin |
| 13 | `net_ptr_to_num` | `stack.inc` | **0** (wrapper over AY) | `ror edi,2` | **REJECT** — thin wrapper |
| 14 | `get_phys_addr` | `usb/hccommon.inc` | **0** in-kernel; PE `GetPhysAddr` only | AQ + page offset | **REJECT** — thin glue / PE-only |
| 15 | `pid_to_appdata` | `taskman.inc` | Commented-only (`debug.inc`) | AA sibling walk | **REJECT** — dead |
| 16 | `strnlen` | `string.inc` | PE export only; `_strnlen` is **private** taskman copy | `repne scasb` | **REJECT** — thin export; do not migrate private twin as inventory inflation |
| 17 | `tcp_mss` | `tcp_subr.inc` | **1** (`tcp_input`) | clamp+store 1420 | **REJECT** — thin |
| 18 | `mutex_init` | `sync.inc` | ~33 | circular list + count=1 | **REJECT** — thin + fan-out |
| 19 | `sysfn_getfreemem` / `sysfn_mouse_acceleration` | `kernel.asm` | Syscall façade | Load/store | **REJECT** — thin façade |
| 20 | `enable_irq` / `irq_eoi` | `apic.inc` | IRQ path | PIC/IOAPIC port MMIO | **REJECT** — no deterministic hardware-independent oracle |
| 21 | `mem_test` | `init.inc` | Boot once | CR0/cache/`wbinvd`; **skipped when E820 present** | **REJECT** — QEMU E820 skips body |
| 22 | `strtoint_dec` | `conf_lib.inc` | Unlinked | N/A | **REJECT** — dead |

### Special scrutiny (serious candidates)

#### `alloc_page` / `map_page` — DEFER (not SELECT)

| Field | `alloc_page` | `map_page` |
|-------|--------------|------------|
| ABI | `proc`; CLI via `pushfd`/`cli`; EAX=PA or 0; preserves EBX via push | stack `lin,phys,flags`; `ret 12`; writes `page_tabs`; `invlpg` |
| Side effects | `sys_pgmap` BTR, `pages_free`, `page_start` | PTE mutate + TLB shootdown |
| Independence | Semantically clear bitmap scan | Semantically clear PTE store |
| Why not now | Migrating either claims Stage-4 allocator/paging without owning fault / `free_page` / `commit_pages` / CR3. Live soak is every allocation — failure mode is immediate hang/black screen. Evidence bar requires explicit Stage-4 ownership acceptance, not leaf opportunism. | Same |

#### `tcp_output` / `ipv4_output*` — DEFER

Large send-path state machines (socket queues, timers, ARP, eth_output). Cut BD/AC/M/V are footholds only. Independent packet-level oracle + realistic network soak are missing. Caller count alone is not selection.

#### `*_SetFileInfo` — DEFER

`ntfs_SetFileInfo` locks, finds LFN, mutates index/FRS flags+timestamps, `writeRecord`. Needs exact metadata oracle and safe CoW disk A/B. Not a Path B leaf.

#### `ntfs_restore_usa_frs` — REJECT (unchanged)

```text
ntfs_restore_usa_frs:
        mov     eax, [ebp+NTFS.frs_size]
; fallthrough → ntfs_restore_usa (Cut J / Rust)
```

Four live callers, zero new semantics. Structural USA oracle alone does not justify wrapping an already-migrated leaf.

#### Thin / dead / I/O-oracle rejects — REJECT (unchanged)

`strnlen`, `tcp_mss`, `mutex_init`, `get_phys_addr`, `net_ptr_to_num`, `sysfn_*`, `pid_to_appdata`, `socket_check_owner`, `socket_ptr_to_num`, `enable_irq`, `irq_eoi`, `mem_test`, `strtoint_dec`, boundaries entry/sched symbols.

#### Socket mutex siblings — REJECT (anti-cluster)

`socket_num_to_ptr` / `socket_check_port` have live callers and clear oracles, but take `socket_mutex`, mutate port fields, and do not transfer socket-list ownership to Rust. Selecting them would inflate inventory and re-open the AS/AY anti-cluster without Path A justification.

---

## Selection decision

**No target selected.**

Post-CR, the last named Path B leaf that combined independent semantic substance, live production callers, reloc-free fit under REG-012, and a host-checkable oracle was **`drawChar`** (Cut CR). The remaining 36 checklist items are exclusively:

* Stage 4–7 orchestration / write-path / protocol islands (**DEFER** until ownership + oracle + soak exist),
* thin wrappers / clamps / façades / dead / PE-export-only (**REJECT**),
* boundaries unsuitable entry/sched symbols (**REJECT**),
* hardware I/O without deterministic oracles (**REJECT**),
* fallthrough / anti-cluster siblings (**REJECT**).

Advancing `99 → 100` by migrating `tcp_mss`, `mutex_init`, `strnlen`, or `ntfs_restore_usa_frs` would violate the selection standard (confidence and FASM-ownership reduction over completion count).

---

## Unblock criteria (future Cuts)

Cut CS (or a renamed successor) may proceed only when **at least one** of the following is true and documented in a fresh audit:

1. **Stage 4 ownership accepted** for a paging/allocator cut (`alloc_page` / `free_page` / `map_page` cluster) with CLI/`invlpg`/`sys_pgmap` contracts, independent bitmap/PTE oracle, and a dedicated fault/alloc soak plan that is stronger than desktop non-black.
2. **Protocol island** (`tcp_output` or `ipv4_output*`) gains an independent packet oracle and a realistic network soak harness.
3. **FS write-path** (`*_SetFileInfo`) gains a deterministic metadata oracle and safe disk-image A/B.
4. **Genuine Path A** emerges (Rust-owned subsystem state + multi-function boundary + fewer crossings) — not a manufactured leaf bundle.
5. **New evidence** changes a prior REJECT (e.g. `ntfs_restore_usa_frs` gains real semantics; a “dead” symbol gains live callers).

Until then: **do not start implementation work under the Cut CS label.**

---

## Unblock re-audit (2026-08-13, post-CS)

**Outcome: STILL BLOCKED.** Repository re-verification + pending-symbol
reclassification + test/tooling scan found **no** new ownership boundary,
oracle, or soak capability since the CS blocked audit.

| Check | Result |
|-------|--------|
| Inventory | **99 / 135** unchanged |
| Gates | **99** enabled / **0** disabled |
| Cut CT | **does not exist** |
| CR blob / gate | 1958 B; SHA `9fe4d8e9…f15f`; `USE_RUST_DRAW_CHAR = 1` |
| REG-012 pack | unchanged `0x8E000` / `0x8E000` / `0x90000` |
| Path A | still **REJECTED** — AQ/BL/CI inject `page_tabs`; AS injects `net_sockets`; neither owns allocator or socket lifecycle |
| Network soak | `scripts/run_qemu.py` has `--disk` / `--bus` only — **no** packet/netdev soak |
| FS write oracle | `tools/mkfs_utils` + attach soaks exist; **no** SetFileInfo write/readback oracle |
| Paging harness | host PTE/bitmap oracles possible in isolation; **no** CLI/`invlpg`/fault soak ownership plan |
| Dead rejects | `pid_to_appdata` still commented-only; no new live callers for thin/dead class |

**Note:** Cut CO `unpack` is **already complete** — not a pending blocker. REG-012
headroom after CR is ~4.9 KiB (`end .bss` `0x8C3C3`); that does not unlock any
of the deferred Stage 4–7 orchestration targets.

**Recommended research (not cuts):** (1) Stage-4 ownership + bitmap oracle —
**done** (`stage4-ownership-design.md`, `stage4-bitmap-writers.json`,
`pgbm_*`). (2) Allocator QMP soak **design** — **done:**
[`stage4-allocator-soak-design.md`](stage4-allocator-soak-design.md);
next: host QMP sampler + CoW `allocsoak` recipe (still not a migration).
(3) Packet-buffer oracle + net soak. (4) CoW SetFileInfo→GetFileInfo
readback. Do **not** invent Cut CT.

---

## Memory baseline (unchanged)

| Item | Value |
|------|-------|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `0x008E000` |
| `SLOT_BASE` | `0x0090000` |
| CR ON end `.bss` | `OS_BASE+0x8C3C3` |
| Headroom to TMP | assert needs `end+PAGE_SIZE < TMP` → ~4.9 KiB |

No blob was embedded. Memory docs unchanged.

---

## Inventory / gates

Unchanged: **99 / 135**, **99** production gates enabled. Cut CR final image and rollback (`USE_RUST_DRAW_CHAR = 0`) remain authoritative for the last completed cut.

---

## Ranked reminders for a future unblocked cut

| Priority | If unblocked by… | Candidate |
|----------|------------------|-----------|
| Highest architectural value | Stage 4 ownership | `alloc_page` (+ eventually `map_page` / `free_page`) |
| Highest protocol value | Packet oracle + net soak | `tcp_output` / `ipv4_output*` |
| Highest FS value | Metadata write oracle + disk A/B | `ntfs_SetFileInfo` / `ext_SetFileInfo` |
| Still reject by default | — | thin/dead/IRQ/`mem_test`/fallthrough/anti-cluster |

---

**CUT CS — BLOCKED — STOP**
)
