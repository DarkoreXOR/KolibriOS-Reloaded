# Upstream `init.inc` Reconstruction Diff

## Problem

**LOCAL FACT:** [`kernel/kernel.asm`](../../kernel/kernel.asm) includes `'init.inc'` after the pre-paging `B32` body (~line 183).

**LOCAL FACT:** On-disk [`kernel/init.inc`](../../kernel/init.inc) is a copy of USB subsystem initialization (header: “Initialization of the USB subsystem”), identical in role to [`kernel/bus/usb/init.inc`](../../kernel/bus/usb/init.inc).

**LOCAL FACT:** [`kernel/kernel32.inc`](../../kernel/kernel32.inc) already includes `"bus/usb/init.inc"`.

**INFERENCE:** Root `init.inc` was accidentally overwritten; early CPU/memory init symbols are missing from this tree.

## Upstream mirror

| Item | Value |
|------|-------|
| Mirror path | [`docs/_upstream/init.inc`](../_upstream/init.inc) |
| Source URL | `https://raw.githubusercontent.com/KolibriOS/kolibrios/main/kernel/trunk/init.inc` |
| Fetched | 2026-08-09 |
| Size | 14219 bytes |
| Copyright header | KolibriOS team 2004–2024 |

**Do not copy this file into `kernel/`** as part of documentation work. It is analysis-only.

## Symbols required by local `kernel.asm` (pre-paging)

**LOCAL FACT** — call order in `kernel.asm` (~135–143):

```
call test_cpu
bts  [cpu_caps - OS_BASE], CAPS_TSC
call acpi_locate
call init_BIOS32
call mem_test
call init_mem
call init_page_map
; then enable paging via sys_proc.pdt_0
```

**LOCAL FACT** — locals defined in `kernel.asm` near these calls:

- `bios32_entry dd ?`
- `tmp_page_tabs dd ?`

## Upstream symbol coverage

**UPSTREAM FACT** — [`docs/_upstream/init.inc`](../_upstream/init.inc) defines:

| Symbol | Kind | Approx line | Role |
|--------|------|-------------|------|
| `mem_test` | `proc` | 10 | If no E820 map, probe RAM by writing `'TEST'` every 1 MiB with cache disabled; synthesize one e820 entry |
| `init_mem` | `proc` | 45 | Sum usable e820; set `MEM_AMOUNT`, `pg_data.*`; allocate/build page tables; may use PSE 4 MiB |
| `init_page_map` | `proc` | 184 | Build physical page bitmap `sys_pgmap`; mark free vs used low memory |
| `init_BIOS32` | label | 276 | Scan `0xE0000`–`0xFFFF0` for `_32_` BIOS32 service; fill PCI BIOS GDT entries; store `bios32_entry` |
| `test_cpu` | `proc` | 339 | Detect 386/486/CPUID; fill `cpu_caps`, `cpu_vendor`, `cpu_sign`, phys addr width, SSE/AVX-related caps |
| `acpi_locate_tables` | `proc` | 414 | Locate ACPI tables |
| `acpi_locate` | label | 471 | Entry used by `kernel.asm` |

## Shared globals (local references vs upstream writers)

These are **LOCAL FACT** as readers/writers in `kernel.asm` / `const.inc` / `data32.inc`, and **UPSTREAM FACT** as written by upstream `init.inc`:

| Global / field | Local consumer | Upstream writer |
|----------------|----------------|-----------------|
| `BOOT_LO.memmap_*` | Boot path / mem_test skip | `mem_test`, `init_mem` |
| `MEM_AMOUNT` | `high_code` logging, syscalls | `init_mem` |
| `pg_data.*` | memory allocator, mutex | `init_mem`, `init_page_map` |
| `cpu_caps` | SYSENTER, PAT, PGE, mwait | `test_cpu` (+ local `bts CAPS_TSC`) |
| `cpu_vendor` | AMD SYSCALL path | `test_cpu` |
| `bios32_entry` | defined in `kernel.asm` | `init_BIOS32` |
| `sys_proc` PDT | paging enable | `init_mem` maps kernel into PDT (**UPSTREAM FACT**) |
| `tmp_page_tabs` | defined in `kernel.asm` | used during page-map setup (**UPSTREAM FACT**) |

## Recorded differences / caveats

1. **LOCAL vs UPSTREAM version identity:** This tree reports kernel version string `'v0.7.7.0'` in `bootbios.inc`. Upstream `main` may have drifted. Treat reconstructed init as **best available**, not bit-identical to the missing local file.

2. **Addressing mode:** Pre-paging code uses `symbol - OS_BASE` for absolute phys/low addresses. Post-paging `high_code` uses `OS_BASE`-mapped VAs. Upstream `init.inc` follows the same pattern (**UPSTREAM FACT**).

3. **Local `bts CAPS_TSC` after `test_cpu`:** Local `kernel.asm` forces TSC capability after `test_cpu`. Upstream `test_cpu` also fills caps from CPUID; the forced bit is a **LOCAL FACT** post-step.

4. **Duplicate USB include:** If someone restored a real early `init.inc` without removing USB content from the root file, USB would be included twice. Current root file is **only** USB content.

5. **UNKNOWN:** Exact git revision of this local kernel export relative to upstream `main`. Diff all of `kernel/` before treating upstream as authoritative for non-init files.

## Reconstruction checklist for future agents

To build this tree:

1. Replace `kernel/init.inc` with a verified early-init source (upstream trunk or matching tag).
2. Keep `bus/usb/init.inc` as the USB include (via `kernel32.inc`).
3. Assemble with `make lang=en_US` and confirm symbols resolve.
4. Do **not** treat documentation mirrors under `docs/_upstream/` as build inputs unless explicitly copied by a human/build step.

## Restoration status (2026-08-09)

**LOCAL FACT:** Steps 1–3 completed. `kernel/init.inc` now matches `docs/_upstream/init.inc` and upstream commit `944d74f01` (SHA-256 `F7391BA4…`). Assemble + QEMU desktop smoke documented in [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md).
