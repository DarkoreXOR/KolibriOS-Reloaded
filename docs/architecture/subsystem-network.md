# Subsystem: Networking

**Files:** `network/stack.inc` and protocol includes.  
**Init:** `stack_init` in `high_code`; polled from `osloop` via `stack_handler`.  
**Public:** syscalls 74–76; exports `NetRegDev`, `EthInput`, `NetAlloc`, …  
**Compat:** socket semantics HARD/BEHAVIORAL.
