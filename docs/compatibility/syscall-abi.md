# System Call ABI

## Entry mechanisms

**LOCAL FACT:**

| Mechanism | Vector / MSR | Handler | Documented for apps? |
|-----------|--------------|---------|----------------------|
| `int 0x40` | IDT trap gate | `i40` | **Yes** (`docs/sysfuncs.txt`) |
| SYSENTER | MSRs | `sysenter_entry` | No in sysfuncs.txt |
| SYSCALL (AMD) | MSRs | `syscall_entry` | No |

All three dispatch: `movzx eax, byte`; `call [servetable2+eax*4]`.

## Calling convention (public)

**LOCAL FACT** — [`kernel/docs/sysfuncs.txt`](../../kernel/docs/sysfuncs.txt):

- Function number in **EAX** (low 8 bits index table; `-1` = 255 = exit)
- Invoke **`int 0x40`**
- Other args in **EBX, ECX, EDX, ESI, EDI** (per function)
- Returns in EAX (sometimes more); unspecified regs + EFLAGS preserved

Kernel frame after `pushad`: `SYSCALL_STACK` in `const.inc`.

## Stability classes

| Class | Meaning |
|-------|---------|
| `stable` | Documented in sysfuncs.txt and implemented |
| `legacy` | Old Menuet-era; still wired |
| `undocumented_external` | Implemented; not in primary docs (e.g. sysenter) |
| `internal_unused` | `undefined_syscall` → returns `-1` |

## Dispatch table (`servetable2`)

**LOCAL FACT** — `kernel/core/syscall.inc`:

| EAX | Handler | Class | Summary |
|-----|---------|-------|---------|
| 0 | `syscall_draw_window` | stable | Create/draw window |
| 1 | `syscall_setpixel` | stable | Put pixel |
| 2 | `sys_getkey` | stable | Get key |
| 3 | `sys_clock` | stable | Time |
| 4 | `syscall_writetext` | stable | Text |
| 5 | `delay_hs_unprotected` | stable | Delay |
| 6 | `undefined_syscall` | internal_unused | |
| 7 | `syscall_putimage` | stable | Image |
| 8 | `syscall_button` | stable | Define button |
| 9 | `sys_cpuusage` | stable | Process info buffer |
| 10 | `sys_waitforevent` | stable | Wait events |
| 11 | `sys_getevent` | stable | Check events |
| 12 | `sys_redrawstat` | stable | Begin/end redraw |
| 13 | `syscall_drawrect` | stable | Rectangle |
| 14 | `syscall_getscreensize` | stable | Screen size |
| 15 | `sys_background` | stable | Background |
| 16 | `sys_cachetodiskette` | legacy | Floppy cache |
| 17 | `sys_getbutton` | stable | Get button ID |
| 18 | `sys_system` | stable | System control (EBX subfn) |
| 19–20 | undefined | internal_unused | |
| 21 | `sys_setup` | stable | MIDI/setup |
| 22 | `sys_settime` | stable | Set time |
| 23 | `sys_wait_event_timeout` | stable | Wait + timeout |
| 24 | `syscall_cdaudio` | legacy | CD audio |
| 25 | `syscall_putarea_backgr` | stable | Put area to background |
| 26 | `sys_getsetup` | stable | Get setup |
| 27–28 | undefined | internal_unused | |
| 29 | `sys_date` | stable | Date |
| 30 | `sys_current_directory` | stable | CWD |
| 31–33 | undefined | internal_unused | |
| 34 | `syscall_getpixel_WinMap` | stable | |
| 35 | `syscall_getpixel` | stable | |
| 36 | `syscall_getarea` | stable | |
| 37 | `readmousepos` | stable | Mouse |
| 38 | `syscall_drawline` | stable | Line |
| 39 | `sys_getbackground` | stable | |
| 40 | `set_app_param` | stable | Event mask (f40) |
| 41–45 | undefined | internal_unused | |
| 46 | `syscall_reserveportarea` | stable | I/O ports |
| 47 | `display_number` | stable | Draw number |
| 48 | `syscall_display_settings` | stable | |
| 49 | `sys_apm` | legacy | APM |
| 50 | `syscall_set_window_shape` | stable | |
| 51 | `syscall_threads` | stable | Threads |
| 52–53 | undefined | internal_unused | |
| 54 | `sys_clipboard` | stable | Clipboard |
| 55 | `sound_interface` | legacy | Speaker |
| 56 | undefined | internal_unused | |
| 57 | `sys_pcibios` | legacy | PCI BIOS |
| 58–59 | undefined | internal_unused | |
| 60 | `sys_IPC` | stable | IPC |
| 61 | `sys_gs` | stable | Graphics |
| 62 | `pci_api` | stable | PCI |
| 63 | `sys_msg_board` | stable | Debug board |
| 64 | `sys_resize_app_memory` | stable | Resize memory |
| 65 | `sys_putimage_palette` | stable | |
| 66 | `sys_process_def` | stable | Process control |
| 67 | `syscall_move_window` | stable | |
| 68 | `f68` | stable | Heap/drivers/misc (EBX subfn) |
| 69 | `sys_debug_services` | stable | Debugger |
| 70 | `sys_file_system_lfn` | stable | LFN FS |
| 71 | `syscall_window_settings` | stable | |
| 72 | `sys_sendwindowmsg` | stable | Window msg |
| 73 | `blit_32` | stable | Blit |
| 74 | `sys_network` | stable | Net devices |
| 75 | `sys_socket` | stable | Sockets |
| 76 | `sys_protocols` | stable | Protocols |
| 77 | `sys_posix` | stable | POSIX subset |
| 78–79 | undefined | internal_unused | |
| 80 | `sys_fileSystemUnicode` | stable | Unicode FS |
| 81–254 | undefined | internal_unused | |
| 255 (−1) | `sys_end` | stable | Terminate |

Machine-readable twin: [`syscalls.yaml`](syscalls.yaml). Nested details for 18/68/70/74–77: see `sysfuncs.txt` and handler sources; YAML lists subfunction entry points where known.

## Nested: syscall 18 (`sys_system`)

**LOCAL FACT:** `sys_system_table` in `kernel.asm` — subfunction in EBX (after `dec ebx` indexing). Controls window focus, shutdown styles, cache, CDI, etc.

## Nested: syscall 68 (`f68`)

**LOCAL FACT:** `f68call` in `core/memory.inc` — heap alloc/free, load driver (`68.16`/`68.21`), IOCTL (`68.17`), unload/enum, etc.

## Error behavior

**LOCAL FACT:** `undefined_syscall` sets return EAX = `-1`. Many syscalls use negative error codes; exact codes per call — see `sysfuncs.txt` (**verify in handler** when conflicting).

## Interrupt / sync constraints

Syscalls run in kernel with IF as trap gate allows (typically enabled). May block via `change_task`. Not called from IRQ context by apps.

## Compatibility requirement

**HARD ABI:** numbers, `int 0x40` convention, documented buffer layouts, and observable side effects for implemented calls. Fast-syscall paths are **undocumented_external** but should keep working if present in libc stubs.
