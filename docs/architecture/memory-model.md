# Memory Model

## Address space split

**LOCAL FACT** — [`kernel/const.inc`](../../kernel/const.inc), [`kernel/memmap.inc`](../../kernel/memmap.inc):

| Region | Range | Owner |
|--------|-------|-------|
| User / applications | `0x00000000` – `0x7FFFFFFF` | Per-`PROC` page directory (user half) |
| Kernel | `OS_BASE=0x80000000` – … | Shared kernel mappings copied into every `PROC` |
| Recursive page tables | `page_tabs=0xFDC00000` (4 MiB window) | Current address space PT walk |
| Linear framebuffer | `LFB_BASE=0xFE000000` (window size commented as 32 MiB) | Display |

**LOCAL FACT:** `std_application_base_address = 0` — apps typically load at VA 0.

## Page size and paging mode

| Item | Value | Label |
|------|-------|-------|
| Page size | `PAGE_SIZE = 4096` | LOCAL FACT |
| Page directory | Classic 32-bit PDE/PTE (non-PAE) | LOCAL FACT (`PROC.pdt_0`) |
| Large pages | Optional `PDE_LARGE` + `CR4_PSE` | UPSTREAM FACT in `init_mem`; constants LOCAL FACT |
| PAE | Capability detected; not used as kernel paging mode | INFERENCE |
| WP | `CR0_WP` set with paging | LOCAL FACT `kernel.asm` |
| Global pages | Optional `CR4_PGE` / `PG_GLOBAL` | LOCAL FACT `high_code` |

## Physical memory

**UPSTREAM FACT** (`docs/_upstream/init.inc`):

1. Prefer BIOS e820 in `BOOT_LO.memmap_*`.
2. Else `mem_test` probes by writing sentinel every 1 MiB.
3. `init_mem` computes `MEM_AMOUNT`, `pg_data.pages_count`, builds initial mappings.
4. `init_page_map` builds bitmap of free physical pages.

**LOCAL FACT:** Runtime allocator `alloc_page` / `free_page` in `core/memory.inc`; metadata `struct PG_DATA` / `sys_pgmap`.

## Kernel heaps

| Heap | Base / API | Notes |
|------|------------|-------|
| Kernel virtual heap | `HEAP_BASE = 0x80800000`; `init_kernel_heap`, `kernel_alloc` (`heap.inc`) | Large kernel VA allocator |
| Doug Lea malloc | `init_malloc`, `malloc`/`free` (`malloc.inc`) | Small objects; limited arena |
| User heap | `PROC.heap_base` / `heap_top` | Grown via resize syscall paths |

**LOCAL FACT** (`memmap.inc` comments): kernel heap extends toward `0xFDBFFFFF` below page-table window.

## Process memory

**LOCAL FACT:**

- Each process has `PROC` with `pdt_0` (1024 PDEs) and physical `pdt_0_phys`.
- Creating a process copies the **kernel half** of `sys_proc`'s page directory and allocates a fresh user half (`create_process` in `taskman.inc`).
- Threads (`APPDATA`) share the parent `PROC` address space; context switch reloads `cr3` only when `APPDATA.process` changes (`do_change_task`).

## Stacks

| Stack | Location | Role |
|-------|----------|------|
| Early TMP | `TMP_STACK_TOP = 0x008D800` | Pre-high_code (raised Cut CF from `0x008D000`; Cut CE from `0x008CC00`) |
| Boot high | ~`0x8007CC00` (memmap comment) | Early after map |
| Per-thread ring0 | `APPDATA.pl0_stack`, size `RING0_STACK_SIZE=0x2000` | Privilege transitions / IRQ |
| User stack | From app header `stack_top` | Userspace |

**LOCAL FACT:** Saved user context lives at fixed offsets from ring0 stack top (`REG_EIP`, `REG_EAX`, … in `const.inc`).

## Shared / special regions

| VA | Role | ABI? |
|----|------|------|
| `SLOT_BASE=0x80090000` | `APPDATA` slots × 256 bytes | Internal layout; historically pokeable — see fixed-addresses |
| `window_data=0x80001000` | `WDATA` array | Same |
| `sys_proc=0x8008E000` | Kernel `PROC` | Internal |
| `BOOT` / `BOOT_LO` | Boot parameter block | Boot ABI |
| `KEY_BUFF`, `BTN_BUFF` | Input rings | Legacy observable |
| IPC temp maps | Allocated in `high_code` (`ipc_tmp`, …) | Internal helper for IPC mapping |

## Executable loading

**LOCAL FACT:** `fs_execute` (`taskman.inc`) loads Menuet/Kolibri app header `APP_HEADER_01_`, allocates process+thread, maps image, sets EIP/ESP from header.

Drivers load as PE/stripped-PE via `load_PE` / `load_pe_driver` (`peload.inc`, `memory.inc`).

## Protection

- User pages: `PG_USER` + R/W flags.
- Kernel pages generally not user-accessible (shared kernel half without U bit — **INFERENCE** verify per mapping site).
- I/O permission bitmaps in TSS remapped per thread (`APPDATA.io_map`).
- Port ranges reserved via syscall 46 / `ReservePortArea`.

## Ownership summary

| Resource | Owner |
|----------|-------|
| Physical page | Page bitmap / allocator until freed |
| `PROC` | Process lifetime; destroyed when last thread ends (**INFERENCE** — confirm in `taskman.inc` teardown) |
| `APPDATA` slot | Thread lifetime; `TSTATE_FREE` when unused |
| Kernel heap blocks | Caller until `kernel_free` |
| Driver PE image | Service object `SRV.base` until unload |

## Hard-coded addresses

Full catalog: [`../compatibility/fixed-addresses.md`](../compatibility/fixed-addresses.md).
