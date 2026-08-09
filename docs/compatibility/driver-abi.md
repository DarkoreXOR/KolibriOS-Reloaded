# Driver ABI

## Driver representation

**LOCAL FACT:** PE or stripped-PE (`.sys`) loaded into kernel space.

- Magic stripped PE: `STRIPPED_PE_SIGNATURE = 0x4503` (`const.inc`)
- Lifecycle constants: `DRV_ENTRY=1`, `DRV_EXIT=-1`, compat codes (`dll.inc`)
- Registration object `SRV` (`const.inc`): name, magic `' SRV'`, `base`, `entry` (START), `srv_proc` / `srv_proc_ex`

## Load / control from apps

Syscall **68** (`f68`):

| Subfn (notable) | Role |
|-----------------|------|
| 16 | `get_service` — open/load by name → `/sys/drivers/<name>.sys` |
| 17 | IOCTL via `IOCTL` structure |
| 21 | `load_pe_driver` |
| 30/31 | unload / enumerate |

## How drivers call the kernel

**LOCAL FACT:** Import from PE export directory of module `'KERNEL'` (`core/exports.inc` + `export.inc`). Addresses stored relative to `OS_BASE`.

Categories of exports:

- Memory: `AllocPage`, `KernelAlloc`, `MapIoMem`, `MapPage`, …
- Sync: `Mutex*`, `DownRead`/`UpWrite`, …
- Events: `CreateEvent`, `RaiseEvent`, `WaitEvent`, …
- Disk: `DiskAdd`, `DiskDel`, `DiskMediaChanged`, `FsRead*`/`FsWrite*`, …
- PCI/USB/Net/Timers/Threads/Display/MsgBoard/…

Full list: see prior inventory / [`abi-inventory.yaml`](abi-inventory.yaml).

## How the kernel calls drivers

1. `START` with `DRV_ENTRY` at load
2. `srv_proc` for IOCTL
3. IRQ callbacks registered via `AttachIntHandler`
4. Disk `DISKFUNC` table callbacks for block I/O
5. USB via `RegUSBDriver` / pipe APIs
6. Net via `NetRegDev` + input path `EthInput`

## Calling convention

**LOCAL FACT:** Exported kernel APIs are **stdcall**-style for documented disk/timer APIs (`docs/drivers_api.txt`). Match existing `proc stdcall` definitions when reimplementing.

## IRQ / DMA / memory

- ISRs must be non-blocking (USB docs emphasize deferred thread)
- `AllocDMA24` for ISA 24-bit DMA constraints
- `MapIoMem` for MMIO with cache attributes
- Ownership: pages allocated via exports freed by driver or on unload — verify per API (**UNKNOWN** edge cases)

## Synchronization

Drivers use exported mutex/rwsem/events. Kernel “spinlocks” are CLI-based — drivers inherit UP assumptions.

## Unload

Supported via syscall 68 unload path / `DRV_EXIT`. **UNKNOWN:** which in-tree drivers are safely unloadable.

## Dependencies on internals (non-API)

See [`driver-dependencies.md`](driver-dependencies.md).
