# Rust Migration TODO

**Purpose:** Durable inventory of KolibriOS kernel functions in the
Rust hybrid migration scope. One-time project map; update when a Cut
completes or the scoped candidate set changes.

**Last inventory baseline:** post-Cut CE (2026-08-12).

**Gates source of truth:** `project/build.toml` `[[rust.migrations]]`
(86 production entries, all `enabled = true` after CE).

---

## Counting rule (reproducible)

Count **one checklist item per kernel production symbol** in the union of:

1. **Completed:** every `[[rust.migrations]]` target in
   `project/build.toml`, mapped to its FASM kernel symbol
   (Cut A = four symbols: `crc_32`, `unicode.utf16.encode`,
   `unicode.cp866.encode`, `unicode.utf8.decode`).
2. **Remaining scoped:** every other named FASM callable (`proc` or
   global `label:`) under `kernel/` (excluding `kernel/rust/`) that is
   named in `docs/migration/cut-*-plan.md` audits **or** listed as a
   boundaries.md non-cut / late-stage named target, minus noise labels
   (section/end markers, diagnostics, data symbols) and minus symbols
   already represented by a completed cut (e.g. `cd_compare_name` → AJ).

**Do not count:** FASM trampolines, Rust-only helpers, host tests,
Phase C probe, duplicate wrappers already covered by a migrated leaf,
or every local `.label` / macro in the tree.

**`[x]` means:** production path migrated and accepted as a completed Cut
(gate ON in `build.toml`). Equivalent Rust logic alone is not enough.

**Path A vs Path B:** Path A requires Rust-owned subsystem state/ownership
(see raised bar in recent `cut-*-plan.md`). Most Stage-2 entries are Path B
leaves. Path A rejections are recorded in cut plans, not as fake completions.

---

## Legend

- `[x]` — migrated (completed production Cut)
- `[ ]` — not yet migrated (candidate, deferred, ban-listed, thin, late, or unsuitable-but-named)
- Notes on `[ ]` rows record deferral / ban / unsuitable reasons from project docs

---

## crc

- [x] `crc_32` — Cut A

## unicode

- [x] `unicode.cp866.encode` — Cut A
- [x] `unicode.utf16.encode` — Cut A
- [x] `unicode.utf8.decode` — Cut A

## fs/unicode

- [x] `ansi2uni_char` — Cut AN
- [x] `cp866toUpper` — Cut B
- [x] `UTF16to8` — Cut Q
- [x] `utf16toUpper` — Cut C
- [x] `utf8to16` — Cut AB
- [x] `cp866toUTF8_string` — Cut BQ
- [x] `uni2ansi_char` — Cut BZ
- [x] `UTF16to8_string` — Cut BP

## core/string

- [x] `strncmp` — Cut D
- [x] `strrchr` — Cut BB
- [x] `strncpy` — Cut BF
- [x] `strlen` — Cut BH
- [ ] `strchr` — candidate: string leaf / export-only
- [ ] `strnlen` — thin: string helper / export-only
- [ ] `strtoint_dec` — deferred: `conf_lib.inc` not linked (dead)

## fs/common

- [x] `fsCalculateTime` — Cut G
- [x] `fsTime2bdfe` — Cut T
- [x] `fsGetTime` — Cut BV
- [x] `fsReadCMOS` — Cut CA

## fs/FAT

- [x] `fat_gen_short_name` — Cut U
- [x] `fat_next_short_name` — Cut K
- [x] `fat_time_to_bdfe` — Cut AO
- [x] `fat_name_is_legal` — Cut BC
- [x] `fat_date_to_bdfe` — Cut BW
- [x] `bdfe_to_fat_date` — Cut BX
- [x] `bdfe_to_fat_time` — Cut BY
- [ ] `fat_get_sector` — deferred/ban: AW address-math sibling

## fs/exFAT

- [x] `calculate_SetChecksum_field` — Cut AH
- [x] `exFAT_hash_calculate` — Cut AI
- [ ] `exFAT_find_lfn` — deferred: AI caller; FS plugin island
- [ ] `exFAT_get_sector` — deferred/ban: AW address-math sibling

## fs/NTFS

- [x] `createMcbEntry` — Cut AX
- [x] `ntfs_datetime_to_bdfe` — Cut AE
- [x] `ntfs_decode_mcb_entry` — Cut I
- [x] `ntfs_restore_usa` — Cut J
- [x] `ntfs_test_bootsec` — Cut AG
- [x] `ntfsCalculateTime` — Cut AF
- [x] `ntfsGetTime` — Cut BT
- [ ] `ntfs_create_partition` — deferred: mount orchestration
- [ ] `ntfs_restore_usa_frs` — deferred: J sibling
- [ ] `ntfs_SetFileInfo` — deferred: FS write path

## fs/XFS

- [x] `xfs._.blkrel2sectabs` — Cut AW
- [x] `xfs._.conv_bigtime_to_kos_epoch` — Cut AK
- [x] `xfs._.conv_time_to_kos_epoch` — Cut BN
- [x] `xfs._.extent_unpack` — Cut R
- [x] `xfs._.get_addr_by_hash` — Cut W
- [x] `xfs._.get_before_by_hashval` — Cut AM
- [x] `xfs_hashname` — Cut AP
- [x] `xfs._.get_last_dirblock` — Cut BO

## fs/EXT

- [x] `ext_read_time` — Cut AL
- [x] `ext_read_all_times` — Cut BR
- [x] `ext_write_time` — Cut BS
- [ ] `ext_SetFileInfo` — deferred: FS write path
- [ ] `getInodeLocation` — deferred/ban: AW address-math; no --disk ext

## fs/ISO9660

- [x] `iso9660_compare_name` — Cut AJ
- [x] `iso9660_copy_name` — Cut BI

## fs

- [x] `file_system_is_operation_safe` — Cut AZ
- [ ] `fs_execute` — deferred: Stage 6 process create

## blkdev

- [x] `is_partition_table_entry` — Cut Z
- [x] `is_protective_mbr` — Cut AD
- [ ] `disk_scan_gpt` — deferred: Z/AD orchestration
- [ ] `disk_scan_partitions` — deferred: disk orchestration
- [x] `process_partition_table_entry` — Cut CC

## blkdev/AHCI

- [x] `ahci_find_cmdslot` — Cut AV
- [x] `ahci_is_sig_known` — Cut BM
- [x] `ahci_port_wait` — Cut CB

## network

- [x] `checksum_1` — Cut E
- [x] `checksum_2` — Cut F
- [x] `ipv4_find_fragment_slot` — Cut AU
- [x] `ipv4_route` — Cut AC
- [x] `net_ptr_to_num4` — Cut AY
- [x] `socket_check` — Cut AS
- [ ] `ipv4_output` — deferred: AC deepen
- [ ] `ipv4_output_raw` — deferred: AC deepen / protocol island
- [ ] `net_ptr_to_num` — thin: wrapper over net_ptr_to_num4
- [ ] `socket_check_owner` — deferred: socket deepen
- [ ] `socket_check_port` — deferred/ban: mutex + AS anti-cluster
- [ ] `socket_num_to_ptr` — deferred/ban: AS/AY anti-cluster
- [ ] `socket_ptr_to_num` — deferred: AS/AY anti-cluster

## network/TCP

- [x] `tcp_set_persist` — Cut V
- [x] `tcp_xmit_timer` — Cut M
- [x] `tcp_outflags` — Cut BD
- [ ] `tcp_mss` — deferred: M/V/BD TCP deepen
- [ ] `tcp_output` — deferred: Stage 5 protocol island

## core/PE

- [x] `fix_coff_relocs` — Cut Y
- [x] `get_coff_sym` — Cut AT
- [x] `coff_get_align` — Cut BK
- [x] `fix_coff_symbols` — Cut BU
- [ ] `get_proc_ex` — deferred: PE ban stretch after Y+AT (AY/AZ #2)
- [ ] `rebase_coff` — deferred: Y mutate anti-cluster

## core/memory

- [x] `get_pg_addr` — Cut AQ
- [ ] `alloc_page` — deferred: Stage 4 / boundaries allocator Cut B
- [ ] `get_phys_addr` — deferred: Stage 4 after AQ
- [ ] `map_page` — deferred: Stage 4 paging ownership
- [ ] `mem_test` — deferred: boot/memory test
- [x] `v86_get_lin_addr` — Cut BL

## core/IO

- [x] `r_f_port_area` — Cut AR
- [x] `set_io_access_rights` — Cut X

## core/taskman

- [x] `pid_to_slot` — Cut AA
- [x] `test_app_header` — Cut O
- [ ] `create_process` — deferred: Stage 6 / boundaries Cut G
- [ ] `pid_to_appdata` — deferred: AA sibling
- [ ] `set_app_params` — deferred: Stage 6

## core/sync

- [ ] `mutex_init` — deferred: locking primitive

## core/irq

- [ ] `enable_irq` — deferred: interrupt path
- [ ] `irq_eoi` — deferred: interrupt path

## core

- [ ] `memmove` — deferred: high blast / low novelty
- [x] `swap_bytes_in_words` — Cut BG
- [ ] `unpack` — deferred: unpack helper

## syscall

- [x] `is_region_userspace` — Cut P
- [x] `is_string_userspace` — Cut BJ
- [ ] `i40` — unsuitable: Cut C0 — preserve syscall entry asm
- [ ] `syscall_entry` — unsuitable: Cut C0 — preserve entry asm
- [ ] `sysenter_entry` — unsuitable: Cut C0 — preserve entry asm
- [ ] `sysfn_getfreemem` — deferred: Stage 3 query façade
- [ ] `sysfn_mouse_acceleration` — thin: L façade

## sched

- [ ] `change_task` — unsuitable/late: related to do_change_task
- [ ] `do_change_task` — unsuitable: boundaries non-cut — keep asm owner
- [ ] `find_next_task` — deferred: Stage 6 / boundaries Cut F — late

## gui

- [x] `antiAliasing` — Cut N
- [x] `window._.check_window_position` — Cut S
- [ ] `drawChar` — deferred: GUI Stage 7
- [x] `window._.set_window_clientbox` — Cut CE

## video

- [x] `block_clip` — Cut H
- [x] `blit_clip` — Cut CD
- [ ] `blit_32` — deferred: blit hot path

## hid

- [x] `mouse_acceleration` — Cut L
- [x] `hotkey_do_test` — Cut BE
- [ ] `set_mouse_data` — deferred: HID deepen

## bus/USB

- [ ] `usb_td_to_virt` — deferred: AQ compose + weak USB soak

## bus/PCI

- [x] `pci_make_config_cmd` — Cut BA

---

## Special cases (not separate checklist counts)

- **cd_compare_name:** Already satisfied by Cut AJ (`iso9660_compare_name`); not a separate checklist item.
- **Phase C probe (`rust_phase_c_probe`):** Diagnostic blob only — not a production migration / not counted.
- **FASM trampolines / `kernel/rust/*.inc`:** Wiring only — not counted as functions.
- **Rust-only helpers (e.g. `exfat_rolling_checksum`):** Implementation detail — not counted unless they are the migrated kernel symbol.
- **`strncat`:** Live PE export (`exports.inc`), **zero** in-kernel callers — same export-only class as `strchr`/`strnlen`. Documented here without count inflation (still absent from the checklist total).

### Boundaries non-cuts (named above as unsuitable / late)

From [`boundaries.md`](boundaries.md): do not split mid-`do_change_task`;
preserve syscall entry asm (`i40` / `sysenter_entry` / `syscall_entry`);
USB ISR, GS/LFB, and half-`SRV` without export table are non-cuts.
Scheduler policy (`find_next_task`) and process create are **late** Stage 6–7,
not early Path B leaves.

### Active ban-list themes (post-AZ)

Documented across Cuts AO–AZ plans:

- AO calendar siblings — **BY closed** (`bdfe_to_fat_time`); date/time pack-unpack quartet complete
- AN inverse (`uni2ansi_char`) — **BZ closed** (public encode leaf; Cut A export path separate)
- AW address-math siblings (`exFAT_get_sector`, `fat_get_sector`, `getInodeLocation`)
- AS/AY socket anti-cluster (`socket_check_port`, `socket_num_to_ptr`, …)
- PE ban stretch (`get_proc_ex`) / Y anti-cluster (`rebase_coff`)
- USB weak soak (`usb_td_to_virt`)

---

## Progress

**Functions completed / functions total**

`86 / 135`

(Mechanically: `86` `[x]` + `49` `[ ]` = `135`.)

When a new Cut completes: mark its `[ ]` → `[x]`, move the note to the Cut id,
and update this counter so it still matches the checklist.

