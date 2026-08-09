# Structure ABI Audit

Exact layouts for externally visible or accidentally exposed structures.
Pointer size = 4 (32-bit). Alignment follows FASM sequential layout (natural packing as declared).

## Decision guide

| Structure | Reproduce exact layout? | Adapter OK? |
|-----------|-------------------------|-------------|
| `process_information` | **Yes** (copy-out) | Adapter internally |
| `IOCTL` | **Yes** | Adapter |
| `boot_data` / `e820entry` | **Yes** | Adapter |
| `APP_HEADER_01_` | **Yes** (file format) | Parser adapter |
| `SRV` | **Yes** for drivers/68.31 | Shim object |
| `DISKFUNC` / `DISKMEDIAINFO` | **Yes** | Shim |
| `USBFUNC` | **Yes** | Shim |
| `APPDATA` | Only if ring0 consumers | Prefer hide |
| `PROC` | Only if ring0 consumers | Prefer hide |
| `WDATA` | Only if ring0 consumers | Prefer hide |
| `SYSCALL_STACK` | Internal entry only | Internal |
| `EVENT` (kernel object) | Driver-facing fields | Per events docs |

---

## `process_information` — HARD (fn9)

Size **0x4C (76)**. No trailing padding in struct.

| Off | Size | Field |
|-----|------|-------|
| 0 | 4 | cpu_usage |
| 4 | 2 | window_stack_position |
| 6 | 2 | window_stack_value |
| 8 | 2 | reserved |
| 10 | 12 | process_name (11 used + 1 pad) |
| 22 | 4 | memory_start |
| 26 | 4 | used_memory |
| 30 | 4 | PID |
| 34 | 16 | box (BOX) |
| 50 | 2 | slot_state |
| 52 | 2 | reserved |
| 54 | 16 | client_box |
| 70 | 1 | wnd_state |
| 71 | 4 | event_mask |
| 75 | 1 | keyboard_mode |

---

## `BOX` / `RECT` / `POINT`

| Struct | Size | Fields |
|--------|------|--------|
| POINT | 8 | x,y |
| RECT | 16 | left,top,right,bottom |
| BOX | 16 | left,top,width,height |

---

## `IOCTL` — HARD

Size **24**.

| Off | Field |
|-----|-------|
| 0 | handle |
| 4 | io_code |
| 8 | input |
| 12 | inp_size |
| 16 | output |
| 20 | out_size |

---

## `SRV` — HARD (drivers + accidental app dump)

Size **48 (0x30)**. Magic dword **`' SRV'`**.

See [`driver-binary-contract.md`](driver-binary-contract.md).

---

## `APP_HEADER_01_` — HARD (executable)

Size **36** (0x24).

| Off | Field |
|-----|-------|
| 0 | banner qword (`MENUET01` / `MENUET02` ascii) |
| 8 | version |
| 12 | start |
| 16 | i_end |
| 20 | mem_size |
| 24 | stack_top |
| 28 | i_param |
| 32 | i_icon |

---

## `APPDATA` — size assert 256

**Not CPL3-visible.** Still **kernel stride HARD** (`SLOT_BASE + slot*256`). Field table: [`../architecture/data-structures.md`](../architecture/data-structures.md).

If Rust moves slots, update all kernel index math; drivers that hardcode base break (ACCIDENTAL).

---

## `WDATA` — size assert 128

Same as APPDATA: kernel-visible stride; not CPL3-direct.

---

## `PROC`

Large (includes 1024 PDE + handle table). INTERNAL unless drivers walk it.

---

## `DISKFUNC` — HARD

First field `strucsize`; then function pointers. Older drivers with smaller size must work.

`DISKMEDIAINFO`: Flags, SectorSize, Capacity (qword), LastSessionSector.

---

## `boot_data` — HARD

Full field list in `const.inc:762+`. Must match bootloader writers. Size depends on `MAX_MEMMAP_BLOCKS=32` e820 array.

---

## `SYSCALL_STACK` — INTERNAL entry

| Off | Field |
|-----|-------|
| 0 | eip (from call) |
| 4 | edi |
| 8 | esi |
| 12 | ebp |
| 16 | esp |
| 20 | ebx |
| 24 | edx |
| 28 | ecx |
| 32 | eax |

Matches `pushad` after syscall stub `call`.

---

## `EVENT` / `APPOBJ` — driver API

See `events_subsystem.txt` + `const.inc`. Treat exported event helpers as HARD; raw layout needed if drivers cast pointers.

---

## Padding / versioning assumptions

- Fn9 may grow beyond 0x4C later (docs); Rust should accept larger user buffers safely.
- `DISKFUNC.strucsize` / `USBFUNC.strucsize` are explicit versioning.
- `SRV.size` checked equal `sizeof.SRV` — changing size breaks IOCTL validation unless compat.
