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
; Phase C — Rust probe blob + FASM trampoline (see docs/migration/phase-c-integration.md)
include "rust/phase_c.inc"

; include "imports.inc"
; include "core/ext_lib.inc"
; include "core/conf_lib.inc"
