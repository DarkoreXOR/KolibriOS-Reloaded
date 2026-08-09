# Subsystem: Timers

**Files:** `core/timers.inc`, `apic.inc` PIT/APIC, `hpet.inc`.  
**Public:** `TimerHS`, `CancelTimerHS`, `Delay`, `Sleep`, `GetTimerTicks`, `GetClockNs`.  
**Sched interaction:** `irq0` tick.
