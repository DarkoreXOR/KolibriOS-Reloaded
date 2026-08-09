# Known Compatibility Surfaces

**Checklist for Rust kernel authors.**

If you change an item marked HARD / LEGACY-ACCIDENTAL without a shim, you may break existing KolibriOS binaries (apps, drivers, or bootloaders).

Read also: [`abi-audit.md`](abi-audit.md), [`external-contract.md`](external-contract.md).

---

## HARD ABI

Must remain binary-compatible.

### Bootloader

- [ ] Kernel load address `KERNEL_BASE = 0x10000` (with current loaders)
- [ ] Header: near jump, `'KolibriOS '` signature, version string, `b32_offset`
- [ ] Optional UEFI jump using `b32_offset`
- [ ] `AX='KL'` + `DS:SI` save-settings structure v1
- [ ] `CX='HA'`, `DX='RD'`, `BX` `/sys` encoding
- [ ] `boot_data` layout at physical `0x9000`

### Application syscalls

- [ ] Entry `int 0x40`; number in EAX (low 8 bits); args in other GPRs
- [ ] Entry SYSENTER (`sysenter_entry` EBP/stack convention) if any stub uses it
- [ ] Entry AMD SYSCALL/SYSRET path if used
- [ ] `servetable2` function numbers for all implemented slots
- [ ] `undefined_syscall` / holes return EAX = −1
- [ ] Function −1 / 255 terminates process
- [ ] Nested fn18 / fn68 / fn70 / fn74–77 / fn80 semantics per **code** (resolve doc conflicts in favor of code unless docs are the published contract—prefer matching both when possible)
- [ ] Fn9 `process_information` layout size 0x4C
- [ ] Slot numbering from 1; IDLE/OS special slots as documented
- [ ] TID/PID assignment rules (unique; documented monotonic growth)
- [ ] Event bit constants (`EVENT_REDRAW`, …) and fn10/11/23/40
- [ ] IPC fn60 buffer protocol
- [ ] Clipboard, threads (51), resize, PCI, debug board as documented
- [ ] GS direct framebuffer access + fn61 parameters
- [ ] User addresses must be `< OS_BASE` where kernel enforces it

### Application binaries

- [ ] `MENUET01` / `MENUET02` header banner acceptance
- [ ] Header fields: version, start, i_end, mem_size, stack_top, i_param, i_icon
- [ ] Version dword = 2 DLL autoload behavior
- [ ] Default load base 0; per-process page tables

### Drivers

- [ ] PE / stripped-PE load and reloc
- [ ] Import resolution from `KERNEL` export directory (`symbol - OS_BASE` addresses)
- [ ] Export **names** (see `exports.inc` / `abi-inventory.yaml`)
- [ ] `LFBAddress` last export = writable address cell
- [ ] `RegService` / `GetService` / IOCTL `SRV`+`IOCTL` layouts; magic `' SRV'`
- [ ] `DRV_ENTRY=1`, `DRV_EXIT=-1`, `DRV_COMPAT/CURRENT` (5/6)
- [ ] `DiskAdd` / `DISKFUNC` / `DISKMEDIAINFO` / status codes / media lifecycle
- [ ] `AttachIntHandler`: stdcall; handler gets `user_data`; EAX≠0 means handled
- [ ] USB / Net / Timer / Event / memory export semantics
- [ ] stdcall for documented APIs

### Address space

- [ ] User VA `[0, 0x80000000)`
- [ ] Kernel mapped at `OS_BASE` and above for kernel/drivers
- [ ] LFB available at conventional `LFB_BASE` (or GS equivalent)

---

## BEHAVIORAL ABI

Observable behavior must stay compatible within testing tolerances; bit-identical internals not required.

- [ ] Scheduler fairness / preemption (timer + IRQ higher-prio)
- [ ] GUI redraw/input event ordering as apps perceive
- [ ] Delay/timer tick visibility (`Delay`, `GetTimerTicks`, f68.0 counter)
- [ ] FS path resolution results and error codes
- [ ] Network socket behavior
- [ ] Non-LFB GS shadow-buffer semantics
- [ ] Uniprocessor CLI critical sections (drivers expect no true SMP races)

---

## LEGACY / ACCIDENTAL ABI

Not designed as public API, but binaries may depend. Preserve with shims until corpus proves unused.

- [ ] Fn68.31 copy-out of `SRV` fields (kernel pointers into user buffer)
- [ ] Ring-0 drivers reading `SLOT_BASE` / `window_data` / other globals
- [ ] Exact `APPDATA` size 256 and `WDATA` size 128 if any driver indexes by stride
- [ ] Returned kernel heap pointers’ address range patterns (if cached by drivers)
- [ ] Fn58 documented-but-dead still returns −1
- [ ] APM CF flag return exception to “eflags preserved”
- [ ] f68.15 returns 0 (not −1)
- [ ] Silent refusal of SYSENTER MSR writes via f68.4

---

## INTERNAL (safe to change if edges preserved)

- [ ] Exact VA of IDT, TSS, `sys_proc`, kernel heap base, recursive `page_tabs`
- [ ] Exact VA of `SLOT_BASE` / `window_data` / key-button rings **for CPL3 apps** (paging denies access)
- [ ] FASM module structure, macros, include graph
- [ ] Allocator algorithms behind export/syscall semantics
- [ ] `do_change_task` implementation details behind same observables
- [ ] USB internal thread design behind USB API

---

## UNKNOWN (do not assume INTERNAL)

Requires binary corpus or runtime tracing.

- [ ] Whether any app uses SYSENTER stubs (likely libc)
- [ ] Whether any app maps/tricks around paging to read kernel
- [ ] Whether any `.sys` imports by ordinal only
- [ ] Whether any `.sys` depends on exact `HEAP_BASE` / `SLOT_BASE` values
- [ ] Safe driver unload races
- [ ] Full set of IOCTL codes per stock driver

---

## Prior documentation errors corrected by audit

1. **Apps cannot directly poke `SLOT_BASE`/`window_data` via flat addressing** — kernel PDEs lack `PG_USER`.
2. **GS/LFB direct access is first-class HARD ABI** — was under-emphasized.
3. **SYSENTER is part of the live contract** despite missing from `sysfuncs.txt`.
4. **`SRV.magic` is `' SRV'`** (leading space).
5. **f68 dispatch is split 0–4 / 5–10 / 11–31**, not a single table from 0.

---

## One-line rule

Preserve boot + syscall numbers/layouts + PE driver export/IOCTL/Disk contracts + GS graphics + user/kernel split; treat leaked kernel pointer dumps and ring0 global poking as shimmable accidents; do not freeze supervisor-only fixed VAs for applications.
