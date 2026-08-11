# Cut AQ Implementation — `get_pg_addr`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-aq-plan.md`](cut-aq-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `get_pg_addr` |
| Source | [`kernel/core/memory.inc`](../../kernel/core/memory.inc) |
| Callers | ~15 live + PE export `GetPgAddr` (USB, AHCI, IDE DMA, taskman, kernel.asm, memory) |
| Rust symbol | `rust_get_pg_addr` |
| Pure helper | `kolibri_utils::get_pg_addr` |
| Subsystem | Memory — kernel linear address → physical page (Stage-4 foothold) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AP raised-bar audit: FS hash/datetime/unicode
siblings, XFS W+AM+hashname-as-subsystem, H+`blit_clip`, S+clientbox,
Y+`coff_get_align`, sockets, and `get_pg_addr`+Cut P / `v86_get_lin_addr` do
not meet the raised Path A bar. See [`cut-aq-plan.md`](cut-aq-plan.md).

REG-001 lesson applied: trampoline preserves **ECX+EDX** (and EBX/ESI/EDI/EBP).
HD DMA scatter-gather keeps ECX (page offset) and EDX (contiguous-phys compare)
live across `call get_pg_addr`; USB `get_phys_addr` keeps ECX live.

---

## Candidate comparison (post-AP audit)

| Candidate | Outcome |
|-----------|---------|
| `get_pg_addr` | **Selected** — new Stage-4 VA→PA class; DMA fanout; reloc-free inject |
| `blit_clip` | #2 — H composition; desktop-only soak |
| `coff_get_align` | #3 — trivial PE align mask after Y |
| `ahci_is_sig_known` / `pci_make_config_cmd` | Thin bus peers |
| FAT/XFS/Unicode ban-list / sockets / memmove | Reject / defer |

---

## Legacy ABI

FASM leaf in `memory.inc` (retained under `USE_RUST_GET_PG_ADDR=0`):

```text
call / ret
in:  EAX = linear address
out: EAX = physical page address (page-aligned)
preserves: ECX, EDX (untouched by body); EBX/ESI/EDI/EBP untouched
clobbers: flags
```

Algorithm:

```text
sub eax, OS_BASE
cmp eax, 0x400000 / jb .low
shr eax, 12
mov eax, [page_tabs + (eax+(OS_BASE shr 12))*4]
.low:
and eax, -PAGE_SIZE
```

First 4 MiB above `OS_BASE` are identity-mapped. Above that, PTE fetch.
`linear < OS_BASE` follows unsigned wrap (same as FASM) — callers normally pass
kernel/user buffers that resolve via `page_tabs`.

---

## Rust ABI

```text
stdcall rust_get_pg_addr(linear, page_tabs, os_base) -> EAX
  EAX = physical page (aligned)
  ret 12
```

Trampoline: `push` EBX/ECX/EDX/ESI/EDI/EBP / `stdcall rust_get_pg_addr, eax,
page_tabs, OS_BASE` / `pop` / `ret` (REG-001 / Cut AA inject pattern).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `get_pg_addr.rs` + `ffi.rs` section `.text.rust_get_pg_addr` |
| Extract | `extract_reloc_free_text.py` → `rust_get_pg_addr.bin` |
| Embed | `kernel/rust/get_pg_addr.inc` `file` directive |
| Trampoline | `memory.inc` under `USE_RUST_GET_PG_ADDR` |
| Gate | `USE_RUST_GET_PG_ADDR` (dev 0 → prod 1) |
| Smoke | `get_pg_addr_rust_smoke_test` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_get_pg_addr` |
| Blob/object size | 41 bytes |
| Relocations | 0 |
| SHA-256 | `D07988DC7BEFF926964E60D34D9574D0FAE9B80EE5E35781DA3DB55D3691AC94` |

Trailing instruction is `ret 12` (`C2 0C 00`). Reloc-free verified by extractor
(extraction fails if the section has relocations).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | identity window; high-path PTE; page-mask flags; wrap-under-OS_BASE |
| Exhaustive | all page starts in identity window + intra-page samples |
| PRNG | 50 000 cases, seed `0x43555451` (`'CUTQ'`) + production-OS_BASE campaign |
| Host tests | **419/419** cargo tests (411 AP baseline + 8 new get_pg_addr) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `get_pg_addr_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C51` hang) |
| Vectors | synthetic high-path PTE; low-path identity; page-mask; IDENTITY_WINDOW boundary; public trampoline OS_BASE / OS_BASE+0x1234 / last identity byte |
| Canaries | ECX=`0xC0C00002`, EDX=`0xD0D00003`, EBX/ESI/EDI/EBP across public call (REG-001) |
| Marker | `rust_get_pg_addr_smoke_result = 'GPAD'` on success |

---

## QEMU validation

Kernels built with Cuts A–AP production gates intact (`USE_RUST_XFS_HASHNAME=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_GET_PG_ADDR=0` | **OK** (QMP `running` + screendump `dev_build/cut-aq-off.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_GET_PG_ADDR=1` | **OK** (screendump `dev_build/cut-aq-on.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no fail hang; boot continued).

### A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop smoke | 779380 non-black | 779380 non-black | Match |
| `--disk xfs` boot+desktop | 779380 non-black | 779380 non-black | Match |

### Real subsystem soak

`--disk xfs` A/B: **PASS** (QMP `running`, identical non-black counts with
XFS HD attached). Exercises DMA/AHCI/IDE paths that call `get_pg_addr` for
scatter-gather phys translation (blast soak, not FS name parity).

Scripted Eolite XFS directory browse / path-lookup harness:
**NOT AVAILABLE** (attach + boot smoke only).

Production image: `dev_build/cut-aq-final.img`.

e1000: **N/A**

---

## Regressions discovered

**NONE** during Cut AQ validation.

---

## Production gate

```text
USE_RUST_GET_PG_ADDR = 1
```

Rollback: `USE_RUST_GET_PG_ADDR = 0` (or `enabled = false` in
`project/build.toml` Cut AQ migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/get_pg_addr.rs` — translate + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_get_pg_addr`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/core/memory.inc` — trampoline + gate + FASM rollback body
* `kernel/rust/get_pg_addr.inc` — blob embed + ABI smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-aq-plan.md` / `cut-aq-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No scripted Eolite browse soak (attach-only A/B DMA blast).
* Does not own paging / alloc / fault paths — Stage-4 foothold only.
* No Path A claim.
* Live high-path smoke deferred to production callers (early boot smoke uses
  synthetic PTE table + identity-window public trampoline).
