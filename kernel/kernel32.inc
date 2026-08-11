;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2004-2024. All rights reserved. ;;
;;  Distributed under terms of the GNU General Public License.  ;;
;;                                                              ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


include "core/sync.inc"         ; macros for synhronization objects
include "core/sys32.inc"        ; process management
include "core/sched.inc"        ; process scheduling
include "core/syscall.inc"
include "core/fpu.inc"          ; all fpu/sse support
include "core/memory.inc"
include "core/mtrr.inc"
include "core/heap.inc"
include "core/malloc.inc"       ; small kernel heap
include "core/taskman.inc"
include "core/dll.inc"
include "core/peload.inc"
include "core/exports.inc"
include "core/string.inc"
include "core/v86.inc"          ; 16-bit mode machine
include "core/irq.inc"          ; interrupt handling functions
include "core/apic.inc"
include "core/hpet.inc"
include "core/timers.inc"
include "core/clipboard.inc"
include "core/slab.inc"

include "acpi/acpi.inc"

include "posix/posix.inc"

include "boot/shutdown.inc"     ; kernel shutdown

include "video/vesa20.inc"
include "video/blitter.inc"
include "video/vga.inc"         ; VGA 16 color functions
include "video/cursors.inc"
include "video/framebuffer.inc"

include "gui/window.inc"
include "gui/event.inc"
include "gui/font.inc"
include "gui/button.inc"
include "gui/mouse.inc"         ; cursor
include "gui/skincode.inc"      ; windows' skin
include "gui/background.inc"

include "hid/keyboard.inc"
include "hid/mousedrv.inc"
include "hid/set_dtc.inc"       ; setting date,time,clock and alarm-clock

include "sound/playnote.inc"    ; player Note for PC Speaker

include "bus/pci/pci32.inc"
include "bus/usb/init.inc"

include "blkdev/flp_drv.inc"    ; floppy driver
include "blkdev/fdc.inc"
include "blkdev/cd_drv.inc"     ; CD driver
include "blkdev/ide_cache.inc"  ; CD cache
include "blkdev/hd_drv.inc"     ; HDD driver
include "blkdev/bd_drv.inc"     ; BIOS disks driver
include "blkdev/rd.inc"         ; ramdisk driver
include "blkdev/disk.inc"       ; support for plug-n-play disks
include "blkdev/disk_cache.inc" ; caching for plug-n-play disks

include "fs/fs_lfn.inc"         ; sysfunction 70

include "network/stack.inc"

include "crc.inc"               ; checksums (optional Rust via USE_RUST_CRC)
include "unicode.inc"           ; utf8/utf16/cp866 optional Rust via USE_RUST_*

; Cut A Rust embeds / smokes (see docs/migration/cut-a-final-architecture.md)
include "rust/crc.inc"
; UTF-16 encode — Rust blob + smoke (see docs/migration/utf16-migration.md)
include "rust/utf16.inc"
; CP866 encode — Rust blob + smoke (see docs/migration/cp866-migration.md)
include "rust/cp866.inc"
; UTF-8 decode — Rust blob + smoke (see docs/migration/utf8-migration.md)
include "rust/utf8.inc"
; Cut B: CP866 uppercase — Rust blob + smoke (see docs/migration/cut-b-plan.md)
include "rust/cp866_upper.inc"
; Cut C: UTF-16 uppercase — Rust blob + smoke (see docs/migration/cut-c-plan.md)
include "rust/utf16_upper.inc"
; Cut D: strncmp — Rust blob + smoke (see docs/migration/cut-d-plan.md)
include "rust/strncmp.inc"
; Cut E: checksum_1 — Rust blob + smoke (see docs/migration/cut-e-plan.md)
include "rust/checksum1.inc"
; Cut F: checksum_2 — Rust blob + smoke (see docs/migration/cut-f-plan.md)
include "rust/checksum2.inc"
; Cut G: fsCalculateTime — Rust blob + smoke (see docs/migration/cut-g-plan.md)
include "rust/fs_calculate_time.inc"
include "rust/block_clip.inc"
; Cut I: ntfs_decode_mcb_entry — Rust blob + smoke (see docs/migration/cut-i-plan.md)
include "rust/ntfs_decode_mcb_entry.inc"
; Cut AX: createMcbEntry — Rust blob + smoke (USE_RUST_NTFS_CREATE_MCB_ENTRY)
include "rust/ntfs_create_mcb_entry.inc"
; Cut AY: net_ptr_to_num4 — Rust blob + smoke (USE_RUST_NET_PTR_TO_NUM4)
include "rust/net_ptr_to_num4.inc"
; Cut AZ: file_system_is_operation_safe — Rust blob + smoke (USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE)
include "rust/file_system_is_operation_safe.inc"
include "rust/pci_make_config_cmd.inc"
; Cut BB: strrchr — Rust blob + smoke (USE_RUST_STRRCHR)
include "rust/strrchr.inc"
; Cut BC: fat_name_is_legal — Rust blob + smoke (USE_RUST_FAT_NAME_IS_LEGAL)
include "rust/fat_name_is_legal.inc"
; Cut J: ntfs_restore_usa — Rust blob + smoke (see docs/migration/cut-j-plan.md)
include "rust/ntfs_restore_usa.inc"
; Cut K: fat_next_short_name — Rust blob + smoke (see docs/migration/cut-k-plan.md)
include "rust/fat_next_short_name.inc"
; Cut L: mouse_acceleration — Rust blob + smoke (see docs/migration/cut-l-plan.md)
include "rust/mouse_acceleration.inc"
; Cut M: tcp_xmit_timer — Rust blob + smoke (see docs/migration/cut-m-plan.md)
include "rust/tcp_xmit_timer.inc"
; Cut N: antiAliasing — Rust blob + smoke (see docs/migration/cut-n-plan.md)
include "rust/anti_aliasing.inc"
; Cut O: test_app_header — Rust blob + smoke (see docs/migration/cut-o-plan.md)
include "rust/test_app_header.inc"
; Cut P: is_region_userspace — Rust blob + prepared smoke (switch OFF by default)
include "rust/is_region_userspace.inc"
; Cut Q: UTF16to8 — Rust blob + smoke (USE_RUST_UTF16_TO_8=1; production ON)
include "rust/utf16_to_8.inc"
; Cut R: xfs._.extent_unpack — Rust blob + smoke (USE_RUST_XFS_EXTENT_UNPACK)
include "rust/xfs_extent_unpack.inc"
; Cut S: window._.check_window_position — Rust blob + smoke (USE_RUST_CHECK_WINDOW_POSITION)
include "rust/check_window_position.inc"
; Cut T: fsTime2bdfe — Rust blob + smoke (USE_RUST_FS_TIME2BDFE)
include "rust/fs_time2bdfe.inc"
; Cut AE: ntfs_datetime_to_bdfe — Rust blob + smoke (USE_RUST_NTFS_DATETIME_TO_BDFE)
include "rust/ntfs_datetime_to_bdfe.inc"
; Cut AF: ntfsCalculateTime — Rust blob + smoke (USE_RUST_NTFS_CALCULATE_TIME)
include "rust/ntfs_calculate_time.inc"
; Cut AG: ntfs_test_bootsec — Rust blob + smoke (USE_RUST_NTFS_TEST_BOOTSEC)
include "rust/ntfs_test_bootsec.inc"
; Cut AH: calculate_SetChecksum_field — Rust blob (USE_RUST_CALCULATE_SET_CHECKSUM_FIELD)
; ABI smoke lives in fs/exfat.inc (needs exFAT struct offsets).
include "rust/calculate_set_checksum_field.inc"
; Cut AI: exFAT_hash_calculate (NameHash) — Rust blob (USE_RUST_EXFAT_HASH_CALCULATE)
; ABI smoke lives in fs/exfat.inc.
include "rust/exfat_hash_calculate.inc"
; Cut AJ: iso9660_compare_name — Rust blob (USE_RUST_ISO9660_COMPARE_NAME)
; ABI smoke lives in fs/iso9660.inc.
include "rust/iso9660_compare_name.inc"
; Cut AK: xfs._.conv_bigtime_to_kos_epoch — Rust blob + smoke (USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH)
include "rust/xfs_conv_bigtime_to_kos_epoch.inc"
; Cut AL: ext_read_time — Rust blob + smoke (USE_RUST_EXT_READ_TIME)
include "rust/ext_read_time.inc"
; Cut AM: xfs._.get_before_by_hashval — Rust blob + smoke (USE_RUST_XFS_GET_BEFORE_BY_HASHVAL)
include "rust/xfs_get_before_by_hashval.inc"
; Cut AN: ansi2uni_char — Rust blob + smoke (USE_RUST_ANSI2UNI_CHAR)
include "rust/ansi2uni_char.inc"
; Cut AO: fat_time_to_bdfe — Rust blob + smoke (USE_RUST_FAT_TIME_TO_BDFE)
include "rust/fat_time_to_bdfe.inc"
; Cut AP: xfs_hashname — Rust blob + smoke (USE_RUST_XFS_HASHNAME)
include "rust/xfs_hashname.inc"
; Cut AQ: get_pg_addr — Rust blob + smoke (USE_RUST_GET_PG_ADDR)
include "rust/get_pg_addr.inc"
; Cut AR: r_f_port_area — Rust blob + smoke (USE_RUST_R_F_PORT_AREA)
include "rust/r_f_port_area.inc"
; Cut AS: socket_check — Rust blob + smoke (USE_RUST_SOCKET_CHECK)
include "rust/socket_check.inc"
; Cut AU: ipv4_find_fragment_slot — Rust blob + smoke (USE_RUST_IPV4_FIND_FRAGMENT_SLOT)
include "rust/ipv4_find_fragment_slot.inc"
; Cut AV: ahci_find_cmdslot — Rust blob + smoke (USE_RUST_AHCI_FIND_CMDSLOT)
include "rust/ahci_find_cmdslot.inc"
; Cut AW: xfs._.blkrel2sectabs — Rust blob + smoke (USE_RUST_XFS_BLKREL2SECTABS)
include "rust/xfs_blkrel2sectabs.inc"
; Cut AT: get_coff_sym — Rust blob + smoke (USE_RUST_GET_COFF_SYM)
include "rust/get_coff_sym.inc"
; Cut U: fat_gen_short_name — Rust blob + smoke (USE_RUST_FAT_GEN_SHORT_NAME)
include "rust/fat_gen_short_name.inc"
; Cut V: tcp_set_persist — Rust blob + smoke (USE_RUST_TCP_SET_PERSIST)
include "rust/tcp_set_persist.inc"
; Cut W: xfs._.get_addr_by_hash — Rust blob + smoke (USE_RUST_XFS_GET_ADDR_BY_HASH)
include "rust/xfs_get_addr_by_hash.inc"
; Cut X: set_io_access_rights — Rust blob + smoke (USE_RUST_SET_IO_ACCESS_RIGHTS)
include "rust/set_io_access_rights.inc"
; Cut Y: fix_coff_relocs — Rust blob + smoke (USE_RUST_FIX_COFF_RELOCS)
include "rust/fix_coff_relocs.inc"
; Cut Z: is_partition_table_entry — Rust blob + smoke (USE_RUST_IS_PARTITION_TABLE_ENTRY)
include "rust/is_partition_table_entry.inc"
; Cut AA: pid_to_slot — Rust blob + smoke (USE_RUST_PID_TO_SLOT)
include "rust/pid_to_slot.inc"
; Cut AB: utf8to16 — Rust blob + smoke (USE_RUST_UTF8TO16)
include "rust/utf8to16.inc"
; Cut AC: ipv4_route — Rust blob + smoke (USE_RUST_IPV4_ROUTE)
include "rust/ipv4_route.inc"
; Cut AD: is_protective_mbr — Rust blob + smoke (USE_RUST_IS_PROTECTIVE_MBR)
include "rust/is_protective_mbr.inc"
; Phase C — Rust probe blob + FASM trampoline (see docs/migration/phase-c-integration.md)
include "rust/phase_c.inc"

; include "imports.inc"
; include "core/ext_lib.inc"
; include "core/conf_lib.inc"
