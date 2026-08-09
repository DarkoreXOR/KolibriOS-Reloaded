# Fixed-Address Audit

Adversarial pass: every hard-coded address evaluated for external observability.

Legend for **required in Rust?** — Yes / Shim / No / Unknown.

---

## Boot / physical

| Address | Range | Owner | Creator | Readers | Writers | User? | Driver? | HW? | Required? | Emulate? | Relocate? |
|---------|-------|-------|---------|---------|---------|-------|---------|-----|-----------|----------|-----------|
| `0x10000` | kernel image | boot | loader | CPU | loader | n/a | n/a | n | **Yes** (frozen loaders) | new loaders | only with loader change |
| `0x9000` | `boot_data` | boot | boot path | kernel | boot | no | no | n | **Yes** | no | no (HARD) |
| `0x100000` | ramdisk typical | boot | loader | rd.inc | loader | no | no | n | Behavioral | yes | possible if boot_data says |
| `0x008CC00` | TMP stack | boot | B32 | early | early | no | no | n | No | yes | **Yes** INTERNAL |

## Kernel VAs (`OS_BASE=0x80000000`)

| Address | Size/role | User-vis? | Driver-vis? | Class | Required in Rust? | Notes |
|---------|-----------|-----------|-------------|-------|-------------------|-------|
| `0x80009000` | BOOT mirror | no | yes (ring0) | HARD boot-related | Keep layout of boot_data; VA flexible if only kernel uses high alias | |
| `0x80001000` | window_data WDATA[] | **no CPL3** | yes | INTERNAL / ACCIDENTAL ring0 | **No** for apps; shim if driver audit finds use | Prior docs overstated app access |
| `0x8000B100` | IDT | no | yes | INTERNAL | No | |
| `0x8000C000` | WIN_STACK | no CPL3 | yes | INTERNAL | No | Used heavily in kernel windowing |
| `0x8000C400` | WIN_POS | no CPL3 | yes | INTERNAL | No | |
| `0x8000D000` | FDD_BUFF | no | yes | INTERNAL | No | |
| `0x8000F300` | WIN_TEMP_XY | no | yes | INTERNAL | No | |
| `0x8000F400` | KEY_COUNT/BUFF | no CPL3 | yes | INTERNAL; UNKNOWN wild apps | Prefer syscall-only; shim if needed | |
| `0x8000F500` | BTN_* | no CPL3 | yes | INTERNAL | same | |
| `0x8000FE88` | BTN_ADDR | no | yes | INTERNAL | No | |
| `0x8000FE8C` | MEM_AMOUNT | via syscalls also | yes | BEHAVIORAL value; VA INTERNAL | Value via API; VA free | |
| `0x8000FF00` | SYS_SHUTDOWN | no | yes | BEHAVIORAL | Semantic flag; VA free | |
| `0x80010000` | kernel code | no CPL3 | yes | INTERNAL | Linkage detail | |
| `0x8008E000` | sys_proc | no | yes | INTERNAL | No | |
| `0x80090000` | SLOT_BASE APPDATA×256 | **no CPL3** | yes | INTERNAL / ACCIDENTAL ring0 | **No** for apps; **sched assumes 256-byte stride** (`sched.inc` comment) | Size 256 is HARD for kernel/drivers poking |
| `0x800A0000` | VGA window | maybe modes | yes | MMIO-ish | Map as needed | |
| `0x805FFF80` | TSS | no | yes | INTERNAL | No | |
| `0x80800000` | HEAP_BASE | no | yes (exports return ptrs) | INTERNAL VA; pointers become ACCIDENTAL if apps store | Heap alg free; returned user ptrs HARD | |
| `0xFDC00000` | page_tabs | no | yes | INTERNAL | Can redesign with care | |
| `0xFE000000` | LFB_BASE | **YES** (GS + U pages) | yes + export | **HARD convention** | Keep or emulate GS→FB | Size up to ~32MB window comments |

## Selectors / not linear fixed VA but ABI

| Item | Notes | Class |
|------|-------|-------|
| GS = graphics segment | base patched to LFB | HARD |
| App CS/DS flat 0-base | GDT app_code/app_data | HARD |
| TLS FS | per-thread | BEHAVIORAL |

## Hardware / MMIO

| Item | Class | Relocate VA? |
|------|-------|--------------|
| PIT/PIC ports | MMIO/ports HARD behavior | N/A |
| APIC/HPET phys | map via MapIoMem | VA free |
| PCI config | via API | |

## Corrections vs prior `fixed-addresses.md`

1. Downgrade `SLOT_BASE` / `window_data` / key buffers from “shim for apps” to **INTERNAL for CPL3**, ACCIDENTAL for ring0.
2. Upgrade `LFB_BASE` + GS path to **must preserve or faithfully emulate**.
3. Emphasize `APPDATA` **size 256** and `WDATA` **size 128** as layout constraints even if base moves (kernel index arithmetic).
