# Compatibility Boundary

Updated after adversarial audit ([`abi-audit.md`](abi-audit.md), [`KNOWN_COMPATIBILITY_SURFACES.md`](KNOWN_COMPATIBILITY_SURFACES.md)).

## HARD ABI (must remain binary-compatible)

- Bootloader protocol: load address, header, `KL`/`HARD` handshakes, `boot_data` @ `0x9000`
- Syscall numbers + **`int 0x40`** + **SYSENTER** + **AMD SYSCALL** entry paths
- Documented and implemented buffer layouts (`process_information` 0x4C, `IOCTL`, FS packets, …)
- App executable `MENUET01`/`MENUET02` headers + load/entry/stack semantics
- User VA `< OS_BASE`; kernel high half **not** CPL3-dereferenceable except intentional maps
- **GS direct framebuffer access** + fn61 mode parameters
- Driver PE load + `KERNEL` exports (`OS_BASE`-relative) + stdcall APIs
- `SRV` (magic `' SRV'`), `IOCTL`, `DISKFUNC`(+`strucsize`), USB/Net registration
- `AttachIntHandler` callback ABI (EAX≠0 handled)
- `DRV_ENTRY`/`DRV_EXIT`/`DRV_COMPAT`/`DRV_CURRENT`
- Event bit flags; dual event-object API for drivers
- `LFBAddress` export cell (+ conventional LFB mapping)

## BEHAVIORAL ABI

- Scheduling fairness / preemption timing
- GUI redraw/input ordering
- FS/network observable results
- Timer tick / delay visibility
- UP CLI locking model for drivers
- Non-LFB GS shadow behavior

## LEGACY / ACCIDENTAL (shim until proven unused)

- Fn68.31 dumping `SRV` pointers into user buffers
- Ring-0 drivers reading `SLOT_BASE` / `window_data` / other globals
- `APPDATA` 256-byte / `WDATA` 128-byte strides if indexed by drivers
- Doc/code oddities: fn58 dead, f68.15→0, f68.25 ecx-only, APM CF, MSR write filters

## INTERNAL (free to redesign for CPL3 apps)

- Exact VA of IDT, TSS, `sys_proc`, heap, recursive PT
- Exact VA of **`SLOT_BASE` / `window_data` / KEY/BTN rings`** — **audit finding:** CPL3 cannot access (PDEs lack `PG_USER`); do **not** treat as app HARD ABI
- FASM structure, macros, allocator algorithms behind stable edges
- Scheduler ring implementation details

## SHIM bucket (revised)

| Item | Prior status | Post-audit |
|------|--------------|------------|
| `SLOT_BASE` for apps | shim HARD | **Downgraded** — INTERNAL for apps; ACCIDENTAL for ring0 |
| `window_data` for apps | shim HARD | **Downgraded** — same |
| KEY/BTN fixed VA | shim | INTERNAL unless corpus finds use |
| GS/LFB | under-documented | **Upgraded to HARD** |
| SYSENTER | noted | **HARD undocumented** |
| f68.31 SRV dump | missing | **ACCIDENTAL shim** |

## Answer in one sentence

Preserve boot, syscall entry paths (including SYSENTER), syscall layouts, MENUET headers, driver PE/export/IOCTL/Disk contracts, and **GS graphics**; kernel-only fixed VAs such as slot/window arrays are **not** app ABI (paging denies them) but may still need ring0 shims; dump accidental pointer leaks carefully.
