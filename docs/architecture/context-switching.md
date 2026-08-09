# Context Switching

## Primary routine

**LOCAL FACT:** `do_change_task` in [`kernel/core/sched.inc`](../../kernel/core/sched.inc).

Triggered by:

- `irq0` after `find_next_task`
- `change_task` (voluntary)
- IRQ exit when higher-priority thread runnable

## Steps (FACT summary)

1. Save outgoing thread’s `APPDATA.saved_esp` (kernel stack pointer).
2. Load incoming `saved_esp`.
3. Remap I/O permission PTE pages for TSS bitmaps from incoming `APPDATA.io_map`.
4. If `APPDATA.process` differs → load `cr3` from new `PROC.pdt_0_phys`.
5. Update `tss._esp0` / saved_esp0 for privilege entries.
6. Update TLS GDT entry / `fs` from `APPDATA.tls_base`.
7. FPU/AVX: XSAVE outgoing if needed; set `CR0.TS` if FPU owner changed.
8. Restore debug registers if debugging.
9. Continue on incoming kernel stack (eventually `iretd` to user or resume kernel thread).

## Scheduler selection

**LOCAL FACT:** `find_next_task` walks three circular rings (`scheduler_current[0..2]`):

| Priority | Value | Typical threads |
|----------|-------|-----------------|
| MAX | 0 | OS kernel thread |
| USER | 1 | Applications |
| IDLE | 2 | IDLE |

Skips non-runnable; may wake `TSTATE_WAITING` via `wait_test` / timeout.

## Data touched

- `current_slot`, `current_slot_idx`, `current_process`, `thread_count`
- `APPDATA` fields listed above
- `PROC.pdt_*`
- TSS / GDT TLS

## Classification for Rust

| Piece | Class |
|-------|-------|
| Stack pointer swap + `iretd` path | Permanent asm / carefully unsafe arch |
| CR3 / TLB | Unsafe arch |
| Policy `find_next_task` | Can become safe Rust |
| Priority rings | Can become safe Rust |
| FPU ownership policy | Mostly Rust + asm save/restore |

See also [`process-model.md`](process-model.md), [`scheduler.md`](scheduler.md).
