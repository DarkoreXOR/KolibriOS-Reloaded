# External Contract

> **Q1 answer:** the complete externally observable contract of the current kernel for applications, drivers, and the bootloader.

This document is the normative overview. Details live in linked docs.

---

## A. Bootloader ↔ kernel

| Item | Kind | Must preserve? |
|------|------|----------------|
| Load `kernel.mnt` at phys `KERNEL_BASE` (`0x10000`) | fixed address / boot ABI | **Yes** (with frozen loaders) |
| Header: jmp, `'KolibriOS '`, version ASCII, `b32_offset` | public boot ABI | **Yes** |
| Optional UEFI jump to `B32` via `b32_offset` | public boot ABI | **Yes** |
| `AX='KL'` + `DS:SI` save-settings struct v1 | public boot ABI | **Yes** |
| `CX='HA'`, `DX='RD'`, `BX` `/sys` encoding | public boot ABI | **Yes** |
| `boot_data` at `0x9000` (`BOOT_LO`) filled before/during boot path | shared structure layout | **Yes** |
| Ramdisk presence / `rd_load_from` / devices.dat fields | boot ABI | **Yes** where used |

Evidence: `bootbios.inc`, `const.inc:boot_data`, `docs/loader_doc.txt`.

---

## B. Applications ↔ kernel

| Item | Kind | Must preserve? |
|------|------|----------------|
| `int 0x40`, EAX=number, GPR args | public ABI | **Yes** |
| Syscall numbers 0–80 used slots + (−1) exit | public ABI | **Yes** |
| Documented buffer layouts (`process_information`, FS packets, IOCTL, …) | shared structure layout | **Yes** |
| App header `APP_HEADER_01_` load + entry/stack | public ABI / binary format | **Yes** |
| User VA `< 2GiB`, kernel high half | address space contract | **Yes** |
| Event bit meanings + wait/check syscalls | public ABI | **Yes** |
| GUI drawing/input/window semantics | behavioral ABI | **Yes** (observable) |
| FS paths `/sys`, disks, 70/80 ops | public ABI | **Yes** |
| IPC/clipboard/network/POSIX subset | public ABI | **Yes** |
| SYSENTER/SYSCALL entry | undocumented but used | **Keep** if stubs exist |
| Direct poke of `SLOT_BASE` / `window_data` / key buffers | **Not CPL3-accessible** (no `PG_USER` on kernel PDEs) | **INTERNAL for apps**; ACCIDENTAL if ring0 drivers peek |
| GS direct LFB access + fn61 | public ABI (sysfuncs.txt) | **Yes — HARD** |
| Exact scheduler quanta | behavioral / accidental | Match within tests |

---

## C. Drivers ↔ kernel

| Item | Kind | Must preserve? |
|------|------|----------------|
| PE/stripped-PE load + `START(DRV_ENTRY)` | public ABI | **Yes** |
| Export module name `KERNEL` + export names | exported symbols | **Yes** |
| `LFBAddress` special export cell | exported symbol + VA | **Yes** |
| `RegService`/`GetService`/`IOCTL` | public API | **Yes** |
| `DiskAdd`/`DISKFUNC` | public API + struct | **Yes** |
| USB/Net registration exports | public API | **Yes** |
| Event/timer/mutex/memory exports | public API | **Yes** |
| stdcall for documented APIs | calling convention | **Yes** |
| Non-exported global walks | accidental | Avoid; shim if found in wild |

---

## D. Hardware / MMIO (observable via drivers/apps)

| Item | Kind |
|------|------|
| PIT/APIC timer rates as seen by `GetTimerTicks` / delays | behavioral |
| PCI config access via exports/syscalls | public API |
| Framebuffer pixels via LFB mapping | behavioral + MapIoMem |

---

## E. Explicitly **not** part of external contract

- FASM module file layout / macros
- Exact VA of IDT, TSS, kernel heap, recursive PT (unless leaked)
- Internal `PROC` field offsets (unless drivers poke — assume risk)
- Implementation of allocator algorithms
- Presence of Tup vs Makefile

---

## Classification key (used in inventory)

1. **public ABI/API** — documented or syscall/export stable
2. **exported symbol** — PE export name
3. **shared structure layout** — buffers crossing trust boundary
4. **internal structure layout used externally** — leaked kernel structs
5. **fixed virtual address** — stable VA
6. **MMIO/hardware address** — device phys/ports
7. **accidental implementation detail** — must not freeze unless proven needed

See [`abi-inventory.yaml`](abi-inventory.yaml).
