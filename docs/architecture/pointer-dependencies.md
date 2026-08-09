# Pointer and Offset Dependencies

## Methodology

Search patterns in FASM sources:

- `[reg + constant]` / `APPDATA.field` / `PROC.field`
- Absolute `0x800xxxxx` / symbols from `const.inc`
- Export consumers in drivers (out of tree — **UNKNOWN** without driver corpus)

## Classification legend

| Class | Meaning | Rust fate |
|-------|---------|-----------|
| **A** | Internal offset | May disappear |
| **B** | ABI-visible structure layout | Must keep or shim |
| **C** | Fixed VA apps/drivers depend on | Keep or remap+shim |
| **D** | MMIO / hardware | Keep hardware addrs; VA mapping flexible if API hides it |

---

## B — ABI-visible layouts

| Dependency | Location | Notes |
|------------|----------|-------|
| `process_information` offsets | `const.inc`; filled by `sys_cpuusage` | Syscall 9 |
| `IOCTL` struct | `const.inc`; syscall 68.17 | Drivers/apps |
| `boot_data` fields | `const.inc` | Bootloader |
| `APP_HEADER_01_` | `taskman.inc` | App binary format |
| Syscall register/`SYSCALL_STACK` mapping | `syscall.inc` | Entry convention |
| KERNEL export names | `exports.inc` | Drivers import by name |
| `SRV` magic/`srv_proc` | `dll.inc` | Driver registration |
| Disk `DISKFUNC` callback table | `disk.inc` | Driver **HARD** |
| Event object fields used by exports | `event.inc` + docs | Driver API |

## C — Fixed virtual addresses

See [`../compatibility/fixed-addresses.md`](../compatibility/fixed-addresses.md). Highest risk legacy:

- `SLOT_BASE` + `n*256` thread slots
- `window_data` + `n*128`
- `KEY_BUFF` / `BTN_BUFF`
- `LFB_BASE` / `LFBAddress` export cell

## D — MMIO

| Address / mechanism | Notes |
|---------------------|-------|
| Phys LFB from `BOOT.lfb` mapped to `LFB_BASE` | Display |
| PCI config / PCIe window (`memmap` `0xF0000000`) | PCI |
| APIC/HPET/PIT ports & MMIO | Timers/IRQ |
| VGA `0xA0000` window at `VGABasePtr` | Legacy video |

## A — Internal (examples)

| Pattern | File | Notes |
|---------|------|-------|
| `REG_*` offsets on ring0 stack | `const.inc` | Context frame |
| `do_change_task` field touches | `sched.inc` | Can redesign with Rust TCB |
| Heap `MEM_BLOCK` linkage | `heap.inc` | Internal |
| PDT self-map arithmetic | `page_tabs` | Internal if isolated |
| FASM macros expanding to offsets | widespread | Replace with typed Rust |

## Producer / consumer examples

| Offset/addr | Producer | Consumers | Class |
|-------------|----------|-----------|-------|
| `APPDATA.saved_esp` | create/switch | `do_change_task` | A |
| `APPDATA.tid` | create | syscall 9, IPC send-by-PID | B (value) / A (field site) |
| `WDATA.fl_redraw` | windowing | `get_event_for_app` | A (or B if apps poke windows) |
| `servetable2[eax]` | static table | all syscall entries | B (numbers) |
| `exp_lfb` / `LFBAddress` | `high_code` sets | Drivers reading export | B+C |

## Migration guidance

1. Inventory third-party apps/drivers for direct `0x8009xxxx` reads (**UNKNOWN** in this tree).
2. Assume worst case: keep shims for slot/window buffers until audit clears them.
3. All **B** items belong in `compatibility/` layer with explicit `#[repr(C)]` clones.
