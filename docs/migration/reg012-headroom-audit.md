# REG-012 Headroom Compaction Audit

**Date:** 2026-08-14  
**Status:** **COMPLETE** — production-image ABI-smoke compaction only  
**Inventory:** **105 / 138** (unchanged)  
**Production gates:** **106** `[[rust.migrations]]` all `enabled = true`  
**Production migration:** **NONE** (not a cut; do not start Cut CX)  
**Parent:** [`post-cw-next-frontier.md`](post-cw-next-frontier.md), [`regression-log.md`](regression-log.md) REG-012  
**Inventory JSON:** [`../../dev_build/memory/reclaim-inventory.json`](../../dev_build/memory/reclaim-inventory.json)

This task reclaims historical ABI-smoke storage so future substantive
blobs can assemble against the fixed memory pack. It does **not** move
`TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE`, change completed Rust
semantics, disable gates, or migrate a pending symbol.

---

## 1. Memory accounting (why 4157 B raw vs 61 B slack)

Linear FASM layout (low → high, all below `TMP_STACK_TOP`):

```text
.text (code + Rust blobs)
.data (IncludeIGlobals — initialized smoke markers/fixtures)
endofcode:
align 16
.bss  (cur_saved_data 4096, FPU, kernel globals, IncludeUGlobals)
assert:  $-OS_BASE + PAGE_SIZE  <  TMP_STACK_TOP
TMP_STACK_TOP = sys_proc = 0x008E000
SLOT_BASE                       = 0x0090000
VGABasePtr                      = 0x00A0000
```

| Quantity | Before | Formula |
|----------|--------|---------|
| `TMP_STACK_TOP` / `sys_proc` | `0x008E000` | `kernel/const.inc` — **do not move** |
| `SLOT_BASE` | `0x0090000` | packed against VGA — **do not move** |
| end `.bss` | `OS_BASE+0x8CFC3` | FASM `diff16 "end of .bss"` |
| Raw gap `.bss`→TMP | `0x8E000-0x8CFC3` = **0x103D (4157 B)** | unused bytes before `sys_proc` |
| Assert | `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP` | `data32.inc` |
| Assert value | `0x8DFC3 < 0x8E000` | `0x8CFC3+4096` |
| **Effective slack** | **0x3D (61 B)** | `0x8E000-0x8DFC3` |

The extra ~4 KiB in the raw gap **is** the `PAGE_SIZE` term in the
assert. Early boot (`B32` / `high_code`) sets `esp = TMP_STACK_TOP` and
the stack grows **down** toward `.bss`. The assert reserves one page of
TMP stack so `.bss` cannot collide with the early stack. New `.text` /
`.data` / `.bss` **before** that assert consume the **61 B slack**, not
the 4157 B raw gap.

`cur_saved_data rb 4096` lives **inside** `.bss` (after `endofcode`) and
is already counted in `0x8CFC3`. It is not the assert's extra page.

`IncludeUGlobals` emits `rb size` (uninitialized). Shrinking uglobals
improves the assert without shrinking `kernel.mnt`. Shrinking iglobals
improves both.

Early ABI smokes (Cut J/CV/AQ, …) run **before** `kernel_alloc` of the
`RING0_STACK_SIZE` (8 KiB) stack. Stack-local fixtures on that path must
fit in the reserved 4096 B TMP page (plus remaining slack). Cut AR
(`r_f_port_area` 4096 B iglobal) already documented init-stage stack
overlap corruption — that buffer stays global.

---

## 2. Candidates audited

Smoke-only storage under `kernel/rust/*.inc`. Production FS/allocator/PTE
runtime structs were rejected without size-hunting.

| Symbol | Section | Bytes | Refs | Production | Tests | Rollback | Cut/gate | Method | Status |
|--------|---------|------:|------|------------|-------|----------|----------|--------|--------|
| `esfi_smoke_inode/bdfe/f70/disk/extfs` | `.data` iglobal | 244 | CV smoke + `fill_ctx` only | none | ESFI 4 vectors | OFF smoke is `ret` | CV `USE_RUST_EXT_SET_FILE_INFO` | stack-local 288 B frame | **COMPACTED** |
| `esfi_smoke_path` | `.data` | 9 | `fill_ctx` | none | path string | n/a | CV | keep tiny constant | **KEEP** |
| `rust_ext_set_file_info_smoke_result` / `esfi_smoke_seq` / `esfi_smoke_calc_count` | `.data` | 12 | callbacks + marker | none | seq/canary/hang | n/a | CV | keep (callbacks need stable abs) | **KEEP** |
| `usa_smoke_buf2` | `.bss` uglobal | 1024 | vector 2 only, sequential | none | USA CF-mismatch | FASM public path uses same buf | J `USE_RUST_NTFS_RESTORE_USA` | reuse `usa_smoke_buf` | **COMPACTED** |
| `usa_smoke_buf` | `.bss` | 1024 | vectors 1–4 | none | USA restore | needed OFF | J | keep one record | **KEEP** |
| `get_pg_addr_smoke_ptes` | `.bss` | 5120→4120 | indices ≤ `0x405` | none | GPAD high-path | rust_* still uses table | AQ `USE_RUST_GET_PG_ADDR` | shrink unused tail | **COMPACTED** |
| `nsfi_*` fixtures | stack (pre-existing) | 208 | CW smoke | none | NSFI | OFF smoke is `ret` | CW | already stack | **KEEP** (prior) |
| `r_f_port_area_smoke_reserved_ports` | `.data` | 4096 | AR smoke + `.fail` | none | FPAR | — | AR | **REJECTED** (explicit stack-overlap) | **KEEP** |
| `pid_slot_smoke_table` | `.bss` | 1024 | AA synthetic APPDATA×4 | none | PTSL | after RING0 | AA | stack-localizable later | **KEEP** |
| `v86_get_lin_addr_smoke_ptes` | `.bss` | 1028 | BL synthetic PTEs | none | VGLA | after RING0 | BL | stack-localizable later | **KEEP** |
| `ntfs_bootsec_smoke` | `.bss` | 512 | AG | none | NTBS | early TMP | AG | stack-localizable later | **KEEP** |
| `ext_rat_smoke_*` | `.data` | 392 | BR inodes/outs | none | EXBR | early TMP | BR | stack-localizable later | **KEEP** |
| `cp_smoke_lfb/map/...` | `.data` | ~400 | CP synthetic LFB | none | blit | REG-003 | CP | KEEP (not needed for target) | **KEEP** |
| `ahci_smoke_pdata/port` | `.bss` | 224 | AV | none | AHCI | — | AV | KEEP | **KEEP** |
| remaining 8–96 B calendar/COFF/string fixtures | mix | <128 ea | smoke only | none | various | — | various | below threshold | **KEEP** |

Rejected as production/rollback/runtime (not compacted):

- phys-bitmap / `sys_pgmap` / allocator state (Cut CU)
- live `RESERVED_PORTS` / TSS I/O map
- live `SLOT_BASE` / `page_tabs`
- NTFS/EXT/XFS runtime `PARTITION` objects
- migration gates / blob contents

---

## 3. Cut CV `iglobal` (priority)

| Item | Value |
|------|--------|
| Symbols | `esfi_smoke_inode` 160, `esfi_smoke_bdfe` 32, `esfi_smoke_f70` 32, `esfi_smoke_disk` 4, `esfi_smoke_extfs` 16 |
| Before address | `.data` via `IncludeIGlobals` (not a fixed pack address) |
| Before size | **244 B** fixtures + 9 B path + 8 B result/seq; `calc_count` was 4 B in `.text` |
| Production ON | **no** — trampoline uses caller inode/BDFE; smoke-only |
| OFF rollback | smoke is `if ~ USE_RUST_EXT_SET_FILE_INFO / ret`; FASM body never touches these labels |
| After | 288 B stack (`ctx` 44 + fixtures 244); path/result/seq/calc_count stay iglobal (21 B) |
| Reclaimed | **240 B** `kernel.mnt` / `.data` (measured) |
| Coverage kept | Vector 0 success (inode times + seq 1+4+8+16), miss EAX=5 unlock-only, immutable EAX=5, write fail EAX=device; EBX/ESI/EBP canaries; `cld`; marker `ESFI`; hang `0xDEAD0C56` |
| Stack proof | 288 + 4×push + stdcall ≪ 4096 B TMP page; same class as CW 208 B frame; sequential (not nested with other huge smokes) |

---

## 4. Other compactons this pass

### Cut J — duplicate USA buffer

Vectors are sequential. Vector 2 refilled a second 1024 B buffer with
`0x5A` only to isolate it from vector 1's `0xA5`. Reusing `usa_smoke_buf`
and refilling preserves CF-mismatch + unchanged-buffer checks.

**Reclaim:** 1024 B `.bss`. Remaining 1024 B stays uglobal (early TMP
stack; 1024 is safe but not required once merge landed the target).

**OFF:** public `ntfs_restore_usa` still called; FASM body used the
merged buffer. QEMU USA-OFF desktop **PASS**, RESET=0.

### Cut AQ — oversized PTE table

Planted indices: `0x400`, `0x401`, `0x405`. Table was `0x500*4 = 5120`.
Shrunk to `0x406*4 = 4120`. Unused tail never read. Cannot stack 4120 on
the 4096 B TMP page.

**Reclaim:** 1000 B `.bss`.

---

## 5. Before / after

| Metric | Before (post-CW) | After | Δ |
|--------|------------------|-------|---|
| `TMP_STACK_TOP` | `0x008E000` | `0x008E000` | 0 |
| `sys_proc` | `OS_BASE+0x008E000` | same | 0 |
| `SLOT_BASE` | `OS_BASE+0x0090000` | same | 0 |
| end `.text` | (not recorded) | `OS_BASE+0x4EE92` | — |
| end `.data` | (not recorded) | `OS_BASE+0x5A5F8` | — |
| end `.bss` | `OS_BASE+0x8CFC3` | `OS_BASE+0x8C6C3` | **−0x900 (−2304 B)** |
| Assert | `0x8DFC3 < 0x8E000` | `0x8D6C3 < 0x8E000` | — |
| Effective slack | **61 B** | **2365 B (`0x93D`)** | **+2304 B** |
| `kernel.mnt` | 304872 | 304632 | **−240 B** |
| Gates / inventory | 106 / 105 | 106 / 105 | 0 |

Measured `.bss` drop (2304) = USA 1024 + PTE-table 1000 + CV `.data` 240
+ 40 B `align 16` landing. `kernel.mnt` drop tracks initialized CV
fixtures only (uglobals are not stored in the file).

---

## 6. Targets

| Target | Need | Achieved | Remaining slack |
|--------|------|----------|-----------------|
| A (+512) | slack ≥ 573 | **yes** (+2304) | 2365 |
| B (+1024) | slack ≥ 1085 | **yes** | 2365 |
| C (+1536) | slack ≥ 1597 | **yes** | 2365 |

No further storage was removed to chase a number. `r_f_port_area` 4096,
`pid_to_slot` 1024, and `v86` 1028 remain available later if a blob
larger than ~2 KiB needs more room.

---

## 7. Test preservation / rollback

| Layer | Result |
|-------|--------|
| Migration doctor | PASS |
| Production build (`python scripts/build.py --mode dev`) | PASS |
| Host suite | **856 / 856** PASS |
| ABI smoke (boot hang-on-fail) | PASS — desktop reached; ESFI/USAR/GPAD/NSFI would hang otherwise |
| QEMU ON | `kernel-20260814-141910.img`, non-black **779380**, **RESET=0**, desktop PASS |
| QEMU OFF (Cut J USA FASM + merged buf) | PASS, RESET=0, then gate restored `=1` |
| CV OFF | smoke is a no-op; production FASM does not reference compacted labels |
| Memory assert | PASS (`0x8D6C3 < 0x8E000`) |

No REG-020. No coverage removed.

---

## 8. Future blob feasibility (memory only)

Slack 2365 B is consumed 1:1 by new `.text` (blob + trampoline + smoke).
Leave ~64 B for the `align 16` cliff.

| Blob size | Physically viable? |
|-----------|-------------------|
| ~180 B (CW-class) | **yes** |
| ~250–500 B | **yes** |
| ~600–900 B | **yes** |
| >1 KiB up to ~2 KiB | **yes** (tight above ~2200 B) |
| >2.3 KiB | **no** without more compaction or a pack move |

Memory fit ≠ cut-ready. Example: `ipv4_output` still needs a packet-byte
oracle, QEMU netdev/user-net, pcap/filter-dump, guest stimulus, and a
host parser.

---

## 9. Decision

**HEADROOM COMPACTION COMPLETE**

Do **not** start Cut CX. Do **not** migrate `ipv4_output` / `map_page` /
`ntfs_create_partition` / `create_process`. Pack addresses unchanged.

- **REG-012 headroom compaction:** [`reg012-headroom-audit.md`](reg012-headroom-audit.md) — **COMPLETE** 2026-08-14. Effective assertion slack **61 B → 2365 B**. Next research after compaction (`ipv4_output` packet-byte oracle + QEMU netdev/pcap) is **COMPLETE**.
