# Driver Binary Contract

Complete binary contract between a KolibriOS `.sys` PE driver and the kernel.
Goes beyond export **names**.

## 1. Image format

| Item | Fact | Class |
|------|------|-------|
| PE or stripped-PE (`STRIPPED_PE_SIGNATURE=0x4503`) | `const.inc`, `peload.inc` | HARD |
| Load into kernel VA (`OS_BASE+`) | `load_PE` | HARD |
| Relocations applied | `peload.inc` | HARD |
| Imports resolved against `__exports` | `peload.inc` + `export.inc` | HARD |
| Export addresses stored as `symbol - OS_BASE` | `export.inc:23` | HARD |
| Path `/sys/drivers/<Name>.sys` for 68.16 | `sysfuncs.txt`, `dll.inc` | HARD |

## 2. Entry / lifecycle

| Item | Fact | Class |
|------|------|-------|
| `START` called with arg `DRV_ENTRY` (=1) | `load_pe_driver` | HARD |
| Optional cmdline arg | `load_pe_driver` | HARD |
| Return value: pointer to `SRV` (or 0 fail) | `load_pe_driver` sets `SRV.base`/`entry` from return | HARD |
| Unload calls entry with `DRV_EXIT` (=−1) | docs + unload path | HARD |
| Version model `DRV_COMPAT=5`, `DRV_CURRENT=6` | `dll.inc:9–12` | HARD |

**Calling convention:** stack args; PE entry uses cdecl-like `call eax` with pushes (`load_pe_driver`). Treat as matching existing drivers (effectively stdcall/cdecl as used by Kolibri toolchains).

## 3. `SRV` object (shared structure)

**LOCAL FACT** — `const.inc:956–966`, size **0x30** (48 bytes) including `srv_proc_ex`:

| Off | Field | Size | Notes |
|-----|-------|------|-------|
| 0 | `srv_name` | 16 | ASCIIZ, max 16 with NUL |
| 0x10 | `magic` | 4 | **`' SRV'`** (leading space) — code, not comment |
| 0x14 | `size` | 4 | must equal `sizeof.SRV` in handlers |
| 0x18 | `fd` | 4 | list next |
| 0x1C | `bk` | 4 | list prev |
| 0x20 | `base` | 4 | image base |
| 0x24 | `entry` | 4 | START |
| 0x28 | `srv_proc` | 4 | IOCTL handler (user/IOCTL path) |
| 0x2C | `srv_proc_ex` | 4 | present; not always copied out |

**Registration:**

- `RegService(name, handler)` → stdcall, `ret 8` (`dll.inc:160–169`)
- Allocates `SRV`, links into `srv` list, sets `srv_proc=handler`
- Returns `SRV*`

**Lookup:** `GetService(name)` loads if needed; returns `SRV*`.

**IOCTL:** `srv_handler` / `srv_handlerEx` validate magic+size, then `stdcall srv_proc, ioctl*`.

**App-visible:** handle is kernel `SRV*`; 68.31 may copy fields to user (**ACCIDENTAL**).

## 4. `IOCTL` structure (app↔driver)

| Off | Field |
|-----|-------|
| 0 | handle (`SRV*`) |
| 4 | io_code |
| 8 | input |
| 12 | inp_size |
| 16 | output |
| 20 | out_size |

**HARD ABI** — `sysfuncs.txt` 68.17; `const.inc:IOCTL`.

On failure, handler may set `output=−1`, `out_size=4` (`dll.inc:64–68`).

## 5. KERNEL export directory

- Module name string: `'KERNEL'`
- PE export dir layout via `export` macro
- Import by **name** (ordinals sequential from 0 in builder — **UNKNOWN** if drivers use ordinals only)
- **`LFBAddress` must be last** — special address cell, not function (`exports.inc`)
- `high_code` writes `[LFBAddress]=LFB_BASE`

Full name list: [`abi-inventory.yaml`](abi-inventory.yaml) / `exports.inc`.

**Convention:** documented disk/timer APIs are **stdcall** (`drivers_api.txt`).

## 6. DiskAdd contract

**LOCAL FACT** — `disk.inc` + `drivers_api.txt`:

```
void* DiskAdd(DISKFUNC* functions, const char* name, void* userdata, int flags);
```

`DISKFUNC` callbacks (all pointers; optional NULL where documented):

| Field | Signature (docs/comments) |
|-------|---------------------------|
| strucsize | size of this table |
| close | `void close(void* userdata)` |
| closemedia | `void closemedia(void* userdata)` |
| querymedia | `int querymedia(void* userdata, DISKMEDIAINFO*)` |
| read | `int read(void* userdata, void* buf, int64 start, int* nsec)` |
| write | optional write |
| flush | optional |
| adjust_cache_size | optional |
| LoadTray | optional |

Status codes: `DISK_STATUS_*` (`disk.inc:13–18`).

Order: `DiskAdd` → zero or more `DiskMediaChanged` → `DiskDel`.

**Ownership:** kernel allocates `DISK`; driver owns userdata; refcounts delay free.

## 7. Interrupt registration

```
IRQH* AttachIntHandler(dword irq, void* handler, void* user_data);  // stdcall
```

**LOCAL FACT** — invocation (`irq.inc`):

1. `push [IRQH.data]`
2. `call [IRQH.handler]`
3. `test eax, eax` — **nonzero ⇒ handled** (stop chain)

Handler runs in IRQ context: no sleeping locks; CLI-style kernel locks only.

`detach_int_handler` stub currently empty (`irq.inc`) — **UNKNOWN**/weak teardown.

## 8. USB / Net / Timers / Events / Memory

See existing `usbapi.txt`, `drivers_api.txt`, `events_subsystem.txt`, and export list.

Key HARD pieces:

- `RegUSBDriver` + `USBFUNC.strucsize` pattern (like Disk)
- `NetRegDev` / `EthInput` / `NetAlloc`
- `TimerHS` / `CancelTimerHS`
- Event exports
- `MapIoMem`, `AllocPage`, `KernelAlloc`, `AllocDMA24`

## 9. DMA / memory ownership

| API | Assumption |
|-----|------------|
| `AllocDMA24` | Buffer in 24-bit DMA space |
| `MapIoMem` | Returns kernel VA; flags cache attrs |
| Pages from `AllocPage` | Driver must free appropriately |
| Failure of `START` | `kernel_free` image (`load_pe_driver.fail_init`) |

Exact free rules for every export: **UNKNOWN** without per-driver audit — treat docs + existing drivers as oracle.

## 10. Initialization / teardown order

Drivers loaded after PCI/disk subsystem in `high_code` for built-in PE loads; dynamic via 68.16 anytime after services list init.

Unload: `DRV_EXIT`, unlink `SRV`, free image — races with IOCTL **UNKNOWN**.

## 11. Direct structure access (ring0)

Drivers run in kernel mode → can read `SLOT_BASE`, `current_slot`, etc.

**Classification:** **LEGACY/ACCIDENTAL** — not an API; any driver that does this couples to layouts.

**Migration:** export stable accessors; shim layouts or break those drivers.

## 12. Machine-readable sketch

See [`driver-contract.yaml`](driver-contract.yaml).
