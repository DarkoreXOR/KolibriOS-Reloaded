# Data Structures

Primary definitions: [`kernel/const.inc`](../../kernel/const.inc). Additional structs in subsystem files (see inventory).

Field offsets for FASM `struct` are sequential; sizes from `sizeof.*` / asserts.

---

## `APPDATA` (thread slot) — HARD layout internally; externally pokeable

**LOCAL FACT:** `assert sizeof.APPDATA = 256`. Base array `SLOT_BASE=0x80090000`.

| Off | Field | Size | Meaning |
|-----|-------|------|---------|
| 0 | `app_name` | 11+pad | Name |
| 16 | `list` | 8 | General list link |
| 24 | `process` | 4 | → `PROC` |
| 28 | `fpu_state` | 4 | FPU save area ptr |
| 32 | `exc_handler` | 4 | User exception handler |
| 36 | `except_mask` | 4 | |
| 40 | `pl0_stack` | 4 | Ring0 stack base |
| 44 | `exc_reserve_stack` | 4 | |
| 48 | `fd_ev` / `bk_ev` | 8 | Event event list heads |
| 56 | `fd_obj` / `bk_obj` | 8 | Object list heads |
| 64 | `saved_esp` | 4 | Kernel SP for switch |
| 68 | `io_map` | 8 | I/O bitmap page addrs |
| 76 | `dbg_state` | 4 | |
| 80 | `cur_dir` | 4 | CWD string |
| 84 | `wait_timeout` | 4 | |
| 88 | `saved_esp0` | 4 | TSS esp0 |
| 92–100 | wait_* | 12 | Wait predicate triple |
| 104 | `tls_base` | 4 | |
| 108 | `event_mask` | 4 | Allowed GUI/IPC events |
| 112 | `tid` | 4 | Thread id |
| 116 | `def_priority` | 1 | |
| 117 | `cur_priority` | 1 | |
| 124 | `state` | 1 | `TSTATE_*` |
| 125 | `wnd_number` | 1 | |
| 128 | `window` | 4 | → `WDATA` |
| 140 | `counter_sum` | 4 | |
| 160 | `ipc_start` | 4 | User IPC buffer |
| 164 | `ipc_size` | 4 | |
| 168 | `occurred_events` | 4 | Sticky event bits |
| 172 | `debugger_slot` | 4 | |
| 176 | `terminate_protection` | 4 | |
| 180 | `keyboard_mode` | 1 | |
| 184 | `exec_params` | 4 | |
| 188 | `dbg_event_mem` | 4 | |
| 192 | `dbg_regs` | 20 | `DBG_REGS` |
| 232 | `priority` | 4 | Ring priority |
| 236 | `in_schedule` | 8 | Scheduler ring link |
| 244 | `counter_add` | 4 | |
| 248 | `cpu_usage` | 4 | |

**Allocated by:** slot allocator in taskman / boot `setup_os_slot`.  
**Lifetime:** kernel scheduler owns; apps observe via syscall 9 buffer (not raw struct) officially.  
**Layout compatibility:** treat as **internal structure used externally** until proven unused.

---

## `PROC` (address space)

| Field | Meaning |
|-------|---------|
| `list`, `thr_list` | Links |
| `heap_lock`, `heap_base`, `heap_top`, `mem_used` | User heap |
| `dlls_list_ptr` | Loaded DLLs |
| `pdt_0_phys`, `pdt_1_phys` | Page dir physical |
| `io_map_0/1` | Default I/O maps |
| `ht_*`, `htab` | Handle table (stdin/out/err + objs) |
| `pdt_0` | Embedded page directory (1024 PDEs) |

Kernel instance fixed at `sys_proc`.

---

## `process_information` — **HARD ABI** (syscall 9)

User buffer layout (**LOCAL FACT** `const.inc`):

| Off | Field |
|-----|-------|
| 0 | `cpu_usage` |
| 4 | `window_stack_position` |
| 6 | `window_stack_value` |
| 10 | `process_name` (12) |
| 22 | `memory_start` |
| 26 | `used_memory` |
| 30 | `PID` |
| 34 | `box` |
| 50 | `slot_state` |
| 54 | `client_box` |
| 70 | `wnd_state` |
| 71 | `event_mask` |
| 75 | `keyboard_mode` |

---

## `WDATA` (window) — size 128

| Off | Field |
|-----|-------|
| 0 | `box` |
| 16–24 | colors |
| 28–31 | z/state/redraw flags |
| 32 | `clientbox` |
| 48 | `shape` / scale |
| 56 | `caption` |
| 64 | `saved_box` |
| 80 | `cursor` |
| 96 | `draw_data` |
| 112 | `thread` → `APPDATA` |
| 116 | `buttons` |

---

## Sync / IPC structs

| Struct | Role | External? |
|--------|------|-----------|
| `MUTEX`, `RWSEM` | Kernel locks | Driver API uses via exports |
| `FUTEX` | POSIX futex | Syscall 77 |
| `FILED`, `PIPE` | POSIX fd/pipe | Syscall 77 |
| `EVENT` / `APPOBJ` | Kernel event objects | Driver API |
| `SMEM` / `SMAP` | Shared memory | Syscall paths |
| `IOCTL` | Driver IOCTL block | Syscall 68.17 **HARD** |

---

## Driver structs

| Struct | Role |
|--------|------|
| `SRV` | Registered service: name, magic `' SRV'`, `entry`, `srv_proc` |
| `USBSRV`, `USBFUNC` | USB driver registration |
| `IRQH` | IRQ handler node |
| `STRIPPED_PE_HEADER`, COFF_* | PE load |

---

## Boot / display / devices

| Struct | Role | ABI |
|--------|------|-----|
| `boot_data` | Boot protocol blob | **HARD** |
| `e820entry` | Memory map entry | Boot |
| `display_t` | Display state | Driver `GetDisplay` |
| `DISK` / `DISKFUNC` / `PARTITION` | Block layer (`disk.inc`) | Driver **HARD** |
| `NET_DEVICE`, `SOCKET`, … | Network | Mixed |
| `FileSystem` | FS plugin (`fs_lfn.inc`) | Internal/driver |
| `PG_DATA`, `MEM_BLOCK` | Memory | Internal |
| `TIMER` | `timers.inc` | Driver timers |
| `SYSCALL_STACK` | Syscall frame | Internal |

---

## Ownership / lifetime (summary)

```text
PROC ──owns──> PDT, heaps, handles, thr_list of APPDATA
APPDATA ──refs──> PROC, WDATA, fpu_state pages, cur_dir string
WDATA ──refs──> APPDATA, CURSOR, buttons
SRV ──owns──> PE image base, entry
DISK ──owns──> partitions, caches
EVENT ──linked──> thread object lists
```

Cyclic: `APPDATA.window` ↔ `WDATA.thread`.

Full graph: [`object-relationships.md`](object-relationships.md).
