# Subsystem: Memory Management

**Responsibilities:** physical page alloc, kernel heap, user address spaces, mapping helpers, PF handler.  
**Files:** `core/memory.inc`, `heap.inc`, `malloc.inc`, `slab.inc`, upstream `init.inc`.  
**Public:** exports `AllocPage`, `KernelAlloc`, `MapIoMem`, …; syscalls 64/68 heap.  
**Compat:** export semantics HARD; internals INTERNAL.  
**IRQ:** allocators generally not for heavy use in ISR — **INFERENCE**.  
**Asm:** page table walks, CR3, invlpg.
