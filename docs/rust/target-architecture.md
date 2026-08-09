# Target Rust Architecture

Design after ABI analysis — **not** a FASM mirror.

**Repo placement (actual):** freestanding Rust code lives under [`../../rust_kernel/`](../../rust_kernel/), separate from FASM [`../../kernel/`](../../kernel/). See [`../_meta/project-structure.md`](../_meta/project-structure.md). The module tree below is the **intended** crate shape inside that workspace as migration proceeds — not a claim that all modules exist yet.

## Crate / module layout

```text
rust_kernel/         # Cargo workspace (actual root for Rust)
  # future crates / modules, conceptually:
  arch/x86/          # GDT IDT TSS entry asm, ports, MSR, paging ops
  boot/              # handoff from loader, boot_data parse
  memory/            # phys alloc, VAS, heap, maps
  process/           # Proc, Thread, handles
  scheduler/         # policy
  sync/              # Mutex, WaitQueue, …
  ipc/               # IPC, events façades
  syscall/           # dispatch only
  fs/                # VFS-like clean core + Kolibri path adapters
  drivers/           # modern driver model
  graphics/          # framebuffer
  gui/               # window server
  network/           # stack
  compatibility/     # HARD ABI shims (syscalls layouts, exports, fixed maps)
  init/              # ordered init
```

## Principles

- **Compatibility at edges, clean core inside.**
- Ownership: `Proc` owns threads; threads own windows optionally.
- Sync: start with UP `InterruptLock`; design traits for future SMP.
- Lifetimes: no raw `'static` mut globals without `SpinLock`/`InterruptLock` wrappers.
- Unsafe: confined to `arch`, `compatibility`, paging, MMI O — see [`unsafe-boundary.md`](unsafe-boundary.md).

## Per-subsystem sketch

| Module | Public API | Depends on |
|--------|------------|------------|
| `arch` | descriptors, IRET, Port | none |
| `memory` | FrameAllocator, AddressSpace, KHeap | arch |
| `process` | spawn, exit, handles | memory, scheduler |
| `scheduler` | schedule, yield | process, arch |
| `syscall` | `dispatch(num, regs)` | all via traits |
| `compatibility` | layouts, export table, fixed VA maps | thin adapters over core |
| `fs` | Path, FileOps | block layer |
| `gui` | WindowServer | graphics, input, process |

## Coexistence

During migration, FASM kernel may call Rust via thin ABI (stdcall/C) for isolated components, or Rust may host FASM objects — see [`../migration/boundaries.md`](../migration/boundaries.md).
