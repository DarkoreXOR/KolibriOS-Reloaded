# Interrupts

## Controllers

**LOCAL FACT:**

- Legacy PIC initialized by `PIC_init` (`core/apic.inc`); IRQs redirected to CPU vectors (commonly `0x20+`).
- Optional APIC via `APIC_init`.
- PIT via `PIT_init`; HPET optional (`core/hpet.inc`) after ACPI.
- SoftIRQ registration: `attach_int_handler` / `init_irqs` (`core/irq.inc`), `IRQ_RESERVED=56`.

## Exception vectors

**LOCAL FACT** — `core/sys32.inc` `sys_int` table / `build_interrupt_table`:

- Faults `e0`–`e19` style handlers
- `#PF` → `page_fault_exc` → `page_fault_handler` (`memory.inc`)
- `#NM` (7) → FPU path (`fpu.inc`)
- `#MF` / IRQ13 style: `irqD` clears FPU error (`irq.inc`)

## IRQ entry points

| Entry | File | Role |
|-------|------|------|
| `irq0` | `sched.inc` | Timer tick, scheduling |
| `irq_serv.irq_N` | `irq.inc` | Generic chained handlers |
| `irqD` | `irq.inc` | FPU IRQ13 |
| `i40` | `syscall.inc` | Syscall trap gate |
| Driver handlers | via `AttachIntHandler` | Soft registered |

## Entry / exit mechanics

**LOCAL FACT (pattern):**

1. Hardware pushes EIP/CS/EFLAGS (and SS/ESP on CPL change).
2. Stub saves GPRs (`pushad` common on syscall path).
3. Handler runs with IF cleared for interrupt gates; **syscall 0x40 is a trap gate** (IF preserved) — `sys32.inc` gate type `11101111b`.
4. EOI to PIC/APIC as appropriate.
5. May call `find_next_task` + `do_change_task` before return.
6. Restore + `iretd`.

## Stack switching

Privilege transitions use TSS `esp0` pointing at current thread’s ring0 stack (`APPDATA.saved_esp0` / `pl0_stack` maintenance in `do_change_task`).

## Register saving

Syscall path: `pushad` layout matches `SYSCALL_STACK` (`const.inc`). IRQ path: similar full save before C-like asm procs.

## Interrupt-context restrictions

**LOCAL FACT / documented in USB notes:** IRQ handlers must not take locks that sleep; USB defers work to a dedicated thread. General rule for drivers: keep ISRs short; use events/timers.

## Preemption from IRQ

**LOCAL FACT** (`sched.inc` / `irq.inc`):

- Timer: `SCHEDULE_ANY_PRIORITY`
- Other IRQs: `SCHEDULE_HIGHER_PRIORITY` only
