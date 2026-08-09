# Process Model

## Two-level model

**LOCAL FACT:**

| Object | Struct | Role |
|--------|--------|------|
| Process (address space) | `PROC` | Page directory, heaps, handle table, DLL list, thread list |
| Thread (schedulable) | `APPDATA` (256 bytes) | Stack, state, priority, window, events, IPC buffer ptrs |

Historically both are called “process” in places. User-visible **PID** in syscall 9 is the thread id / slot identity (`APPDATA.tid` / slot) — **INFERENCE** confirm against `sys_cpuusage` fill of `process_information.PID`.

## Limits

**LOCAL FACT:** `max_processes` / slot array at `SLOT_BASE`; comments indicate 255 usable slots (slot map in `high_code`).

## States (`APPDATA.state`)

**LOCAL FACT** (`const.inc`):

| Value | Name |
|-------|------|
| 0 | `TSTATE_RUNNING` |
| 1 | `TSTATE_RUN_SUSPENDED` |
| 2 | `TSTATE_WAIT_SUSPENDED` |
| 3 | `TSTATE_ZOMBIE` |
| 4 | `TSTATE_TERMINATING` |
| 5 | `TSTATE_WAITING` |
| 9 | `TSTATE_FREE` |

## Creation paths

| Path | Symbols | File |
|------|---------|------|
| Execute file | syscall 70 op7 → `fs_execute` | `fs_lfn.inc`, `taskman.inc` |
| New thread | syscall 51 → `new_sys_threads` | `taskman.inc` (export `CreateThread`) |
| Boot | IDLE + OS slots in `high_code` | `kernel.asm` |

`create_process` allocates `PROC` + PDT; `set_app_params` wires EIP/ESP/cmdline and schedules.

## App binary header

**LOCAL FACT** — `APP_HEADER_01_` in `taskman.inc`: banner, version, `start`, `i_end`, `mem_size`, `stack_top`, `i_param`, `i_icon`.

## Windows relationship

Each GUI thread has `APPDATA.window` → `WDATA`; `WDATA.thread` back-pointer. Window number in `APPDATA.wnd_number`.

## Termination

Syscall **-1** / slot 255 → `sys_end`. Cleanup spans taskman, window close, handle teardown.

**UNKNOWN:** Full teardown ordering edge cases (debugger attached, shared `PROC` with surviving threads).
