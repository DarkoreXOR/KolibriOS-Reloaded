# Permanent Assembly Layer

Verified present; keep as asm (or arch `global_asm!`) in final Rust kernel.

| Routine / area | Why not pure Rust |
|----------------|-------------------|
| 16-bit boot / PM switch (`bootbios`) | Real mode; or replace entire path with UEFI Rust entry |
| Interrupt/trap stubs | Exact stack frames, error codes, IRET |
| `i40` / `sysenter_entry` / `syscall_entry` | Entry conventions, MSR paths |
| `do_change_task` low-level swap | ESP switch, IRET continuity — policy can be Rust |
| `lgdt`/`lidt`/`ltr`/`str` | Privileged instructions |
| CR0/CR2/CR3/CR4, MSR read/write | Privileged |
| `in`/`out` port I/O | Privileged |
| FPU/XSAVE asm helpers | ABI to FXSAVE/XSAVE area |
| Early AP trampoline `ap_init16` | 16-bit + paging bring-up |
| TLB shootdown helpers (if added) | Privileged |

## Can move to Rust intrinsics / `core::arch`

- CPUID wrappers
- Some SSE save/restore via intrinsics
- Bit ops, atomics (when SMP)

## Can become safe Rust

- `find_next_task` policy
- Syscall handler bodies (after register decode)
- FS/network protocols
- Most GUI logic

## Can remove

- Dead `if 0` paths
- Duplicate macros once typed APIs exist
- Stale memmap comments as “code”
