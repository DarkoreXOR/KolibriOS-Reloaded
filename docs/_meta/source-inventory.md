# Source Inventory

**LOCAL FACT** — repo root separates FASM (`kernel/`) from Rust (`rust_kernel/`). Full layout: [`project-structure.md`](project-structure.md).

## Top-level layout

| Path | Role |
|------|------|
| `rust_kernel/` | Cargo workspace for freestanding Rust kernel utils (`kolibri_utils`, Cuts A–AB) |
| `tools/build/` | Hybrid build orchestrator (`kolibri_build`): blobs, FASM, image, QEMU |
| `tools/kolibri_img/` | Host utility: FAT image inspect / CoW / extract / delete / replace |
| `fasm/` | Vendored FASM assembler |
| `kolibrios-*.img` | Read-only reference floppy image |
| `tmp_images/` | Disposable test images (gitignored) |
| `kernel/kernel.asm` | Sole main FASM unit; builds `kernel.mnt` |
| `kernel/bootbios.inc` | 16-bit entry + protected-mode switch |
| `kernel/kernel32.inc` | Include hub for 32-bit OS body |
| `kernel/const.inc` | Constants, fixed VAs, major structs |
| `kernel/memmap.inc` | Memory-map commentary (partially stale) |
| `kernel/data16.inc`, `data32.inc`, `data32*.inc` | Data / GDT / BSS |
| `kernel/macros.inc`, `struct.inc`, `proc32.inc`, `kglobals.inc` | Assembler infrastructure |
| `kernel/Makefile`, `Tupfile.lua`, `build.sh` | Build |
| `kernel/docs/` | In-tree English/Russian ABI notes |
| `kernel/boot/`, `bootloader/`, `sec_loader/` | Boot UI and loaders |
| `kernel/core/` | CPU, memory, sched, syscall, DLL/PE, IRQ |
| `kernel/gui/`, `video/`, `hid/` | GUI / display / input |
| `kernel/fs/`, `blkdev/` | Filesystems / block devices |
| `kernel/network/` | TCP/IP stack |
| `kernel/bus/pci/`, `bus/usb/` | Buses |
| `kernel/posix/`, `acpi/`, `sound/` | POSIX-ish, ACPI, speaker |

## Include graph (condensed)

```
kernel.asm
├── macros.inc, struct.inc, proc32.inc, kglobals.inc, lang.inc, encoding.inc, const.inc
├── bootbios.inc
│   └── boot/bootcode.inc, bootvesa, parsers, detect/biosmem, bus/pci/pci16, data16.inc
├── [pre-paging body: B32 calls test_cpu … init_page_map]
├── init.inc                    ; early CPU/mem init (restored 2026-08-09; see fasm-baseline-restoration)
├── fdo.inc
├── high_code body in kernel.asm
├── kernel32.inc                ; bulk OS
│   ├── core/{sync,sys32,sched,syscall,fpu,memory,mtrr,heap,malloc,taskman,
│   │         dll,peload,exports,string,v86,irq,apic,hpet,timers,clipboard,slab}
│   ├── acpi/acpi.inc, posix/posix.inc, boot/shutdown.inc
│   ├── video/*, gui/*, hid/*, sound/playnote.inc
│   ├── bus/pci/pci32.inc, bus/usb/init.inc → usb/*
│   ├── blkdev/*, fs/fs_lfn.inc → fat/exfat/ntfs/ext/iso9660/xfs
│   ├── network/stack.inc → ethernet/IP/ICMP/ARP/UDP/TCP/socket
│   └── crc.inc, unicode.inc
└── data32.inc (+ locale data)
```

## Existing documentation (secondary sources)

| File | Topic |
|------|-------|
| `kernel/docs/sysfuncs.txt` | Application syscall ABI (primary English) |
| `kernel/docs/sysfuncr.txt` | Russian syscalls |
| `kernel/docs/drivers_api.txt` | Disk/timer driver exports |
| `kernel/docs/usbapi.txt` | USB driver ABI |
| `kernel/docs/events_subsystem.txt` | Kernel event objects |
| `kernel/docs/stack.txt` | Network notes |
| `kernel/docs/loader_doc.txt` | Bootloader↔kernel handshake |
| `kernel/docs/apm.txt` | APM |
| `kernel/readme-ext-loader.txt` | Extended primary loader |
| `kernel/memmap.inc` | Address map comments |

## Generated / ephemeral

| Artifact | How |
|----------|-----|
| `lang.inc` | Makefile writes `lang fix <locale>` then deletes |
| `bin/kernel.mnt` | Default kernel image |
| `bin/kernel.bin` | Same sources with `-dextended_primary_loader=1` |
| `bin/boot_fat12.bin` | FAT12 boot sector |

## Critical local anomaly

**LOCAL FACT:** `kernel/kernel.asm` includes `'init.inc'` (line ~183) for early CPU/memory init.
**LOCAL FACT:** On-disk `kernel/init.inc` is USB subsystem init (duplicate of `bus/usb/init.inc`).
**LOCAL FACT:** `kernel32.inc` also includes `bus/usb/init.inc` for the real USB path.
**INFERENCE:** Accidental overwrite; tree does not link/assemble without restoring early init.
