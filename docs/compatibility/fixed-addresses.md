# Fixed Addresses

For each important address: contents, producers, consumers, ABI class.

**Classes:**

- `HARD_ABI` — must keep same VA for external compatibility
- `BEHAVIORAL` — region must exist with same semantics; VA may move if all consumers updated (rare for Kolibri)
- `INTERNAL` — Rust may relocate
- `MMIO` — hardware / firmware
- `ACCIDENTAL` — implementation detail historically visible

---

## Boot / low physical

| Address | Symbol | Contents | Producer | Consumer | Class |
|---------|--------|----------|----------|----------|-------|
| `0x9000` | `BOOT_VARS` / `BOOT_LO` | `boot_data` (VESA, e820, disks, sys path, APM, …) | Boot path / loader | Kernel init, shutdown | **HARD_ABI** (boot protocol) |
| `0x10000` | `KERNEL_BASE` | Kernel image load | Bootloader | CPU fetch pre-paging | **HARD_ABI** (load address) |
| `0x100000` | `RAMDISK_BASE` | Ramdisk image (typical) | Loader / boot flags | `rd.inc` | **BEHAVIORAL** / boot-dependent |
| `0x008CC00` | `TMP_STACK_TOP` | Early stack | `B32` | Early init | INTERNAL |

## Kernel fixed VAs (`OS_BASE=0x80000000`)

| Address | Symbol | Contents | Class | Notes |
|---------|--------|----------|-------|-------|
| `0x80009000` | `BOOT` | High mapping of boot_data | HARD_ABI adjacent | Same struct as `0x9000` |
| `0x80001000` | `window_data` | `WDATA` array | INTERNAL used externally | Menuet-era slot poking; prefer syscalls |
| `0x80001080` | `background_window` | First window after slot0 | INTERNAL | |
| `0x8000B100` | `idts` | IDT | INTERNAL | |
| `0x8000C000` | `WIN_STACK` | Window z-order stack | INTERNAL used externally | |
| `0x8000C400` | `WIN_POS` | Window positions helper | INTERNAL used externally | |
| `0x8000D000` | `FDD_BUFF` | Floppy buffer | INTERNAL | |
| `0x8000F300` | `WIN_TEMP_XY` | Temp GUI | INTERNAL | |
| `0x8000F400` | `KEY_COUNT` / `KEY_BUFF` | Keyboard ring | INTERNAL used externally | Legacy apps may read |
| `0x8000F500` | `BTN_COUNT` / `BTN_BUFF` | Button ring | INTERNAL used externally | |
| `0x8000FE88` | `BTN_ADDR` | Button list ptr cell | INTERNAL | |
| `0x8000FE8C` | `MEM_AMOUNT` | Total RAM bytes | BEHAVIORAL | Also syscall-visible |
| `0x8000FF00` | `SYS_SHUTDOWN` | Shutdown request | BEHAVIORAL | |
| `0x80010000` | (memmap) | Kernel 32-bit code | INTERNAL | Equals `OS_BASE+KERNEL_BASE` |
| `0x8008E000` | `sys_proc` | Kernel `PROC` + PDT | INTERNAL | |
| `0x80090000` | `SLOT_BASE` | `APPDATA` × 256 | INTERNAL used externally | **memmap.inc wrongly documents `0x80080000`** |
| `0x800A0000` | `VGABasePtr` | VGA window | MMIO-ish | |
| `0x805FFF80` | `tss` | Shared TSS + I/O bitmaps | INTERNAL | |
| `0x80800000` | `HEAP_BASE` | Kernel heap start | INTERNAL | |
| `0xFDC00000` | `page_tabs` | Recursive PT window | INTERNAL | Must stay consistent with PDT self-map |
| `0xFDE00000` | `kernel_tabs` | Kernel PDE window | INTERNAL | |
| `0xFDFF7000` | `master_tab` | Self-map PDE slot | INTERNAL | Comment typo `0xFDFF70000` in const.inc |
| `0xFE000000` | `LFB_BASE` | Framebuffer VA | BEHAVIORAL / export `LFBAddress` | Phys LFB from boot; VA conventional |

## Page flags / sizes (not addresses)

See `PAGE_SIZE`, `PG_*`, `PDE_LARGE` in `const.inc` — ABI for drivers using `MapPage`/`MapIoMem` flag conventions (**HARD_ABI** for export semantics).

## Must-keep vs may-move (Rust guidance)

**Superseded in detail by** [`fixed-address-audit.md`](fixed-address-audit.md) **and** [`application-memory-contract.md`](application-memory-contract.md).

**Must preserve (HARD / strongly external):**

- Boot blob at `0x9000` layout (`boot_data`)
- Kernel load at `0x10000` **or** provide an equivalent loader contract with updated loaders (if loaders stay frozen → keep `0x10000`)
- User VA `< 0x80000000`, kernel `>= 0x80000000` split (apps assume 2 GiB user)
- **LFB/`GS` direct graphics** (`LFB_BASE` convention + user-accessible FB mapping)
- Syscall/event/buffer layouts (not all tied to fixed VA)

**Audit correction:** `SLOT_BASE` / `window_data` / key-button rings are **not CPL3-accessible** (kernel PDEs use `PG_SWR` without `PG_USER`). They are **INTERNAL for applications**. They remain **ACCIDENTAL** for ring-0 drivers that peek. Prefer moving them freely for app compat; shim only if driver corpus requires.

**Free to redesign:**

- Exact heap base, TSS VA, IDT VA, recursive PT VA (if all internal walkers updated)
- Doug Lea arena placement
- Slot/window array base addresses (keep 256/128 strides if any ring0 indexer remains)

## Stale documentation warning

**LOCAL FACT:** `memmap.inc` still describes “additional app info” at `0x80080000` with an old field layout. Live code uses `SLOT_BASE=0x80090000` and `struct APPDATA`. Prefer `const.inc`.
