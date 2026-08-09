# Legacy Problems (“Spaghetti”)

For each: what / why / depends / external? / preserve? / eliminate how.

## 1. Corrupted local `init.inc`

1. Root `init.inc` is USB init duplicate.  
2. Accidental overwrite.  
3. Build/boot.  
4. Not external.  
5. Must fix before any binary testing (restore file; don’t redesign yet).  
6. N/A to Rust — ensure Rust boot owns this stage cleanly.

## 2. Fixed global address soup

1. Dozens of `OS_BASE+offset` globals for windows, slots, buffers.  
2. Menuet heritage / no dynamic alloc early.  
3. GUI, input, legacy apps.  
4. Partially external (SHIM).  
5. Preserve via shims initially.  
6. Capability APIs + stop exporting raw VAs.

## 3. CLI-as-spinlock UP model

1. `SchedulerLock` not real.  
2. Simplicity on single CPU.  
3. All critical sections.  
4. Behavioral for drivers.  
5. Preserve UP semantics initially.  
6. Real locks when SMP becomes a goal.

## 4. Thread vs process naming confusion

1. PID often means thread slot.  
2. Historical.  
3. Syscalls/IPC.  
4. Yes — behavioral/naming ABI.  
5. Preserve observable IDs.  
6. Clear Rust types + compat ID mapping.

## 5. Dual event systems

1. GUI bitmasks vs kernel `EVENT` objects.  
2. Features layered over time.  
3. Apps vs drivers.  
4. Both external.  
5. Preserve both.  
6. Unify internally; two façades.

## 6. `memmap.inc` stale vs `const.inc`

1. Wrong slot base documented.  
2. Doc drift.  
3. Humans/agents.  
4. Docs only.  
5. No.  
6. Trust `const.inc`; fix docs (this suite).

## 7. Sysenter undocumented

1. Fast paths exist; docs say only int 0x40.  
2. Perf.  
3. Possible libc.  
4. Undocumented external.  
5. Keep.  
6. Document in Rust compat.

## 8. Cross-subsystem globals

1. Direct peeks across GUI/FS/net.  
2. Monolith asm.  
3. Many.  
4. Mostly internal.  
5. No.  
6. Module APIs + ownership.

## 9. Functions depending on register/stack layout

1. Syscall handlers read `SYSCALL_STACK` offsets; IRQ stubs.  
2. Asm efficiency.  
3. All syscalls.  
4. Convention is ABI; layout internal.  
5. Preserve register ABI.  
6. Rust wrappers decode once at boundary.

## 10. Driver PE linked to kernel export VAs

1. Imports resolved to `OS_BASE`-relative.  
2. Custom PE loader.  
3. All `.sys`.  
4. HARD.  
5. Yes.  
6. Compat export table forever or versioned.

## 11. Init order implicit

1. `high_code` mega-sequence.  
2. Organic growth.  
3. Everything.  
4. Boot behavioral.  
5. Observable end-state yes; order flexible if deps respected.  
6. Explicit Rust init graph.

## 12. Disabled code paths (`if 0`)

1. `dll.Load` / conf load disabled.  
2. Incomplete features.  
3. None currently.  
4. No.  
5. No.  
6. Omit or revive intentionally.
