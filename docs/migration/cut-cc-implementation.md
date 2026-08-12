# Cut CC Implementation — `process_partition_table_entry`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-cc-plan.md`](cut-cc-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CC** |
| FASM symbol | `process_partition_table_entry` |
| Source | [`kernel/blkdev/disk.inc`](../../kernel/blkdev/disk.inc) |
| Callers | 4× in `disk_scan_partitions` MBR/EBR loop |
| Rust symbol | `rust_process_partition_table_entry` |
| Pure helper | `kolibri_utils::process_partition_table_entry` |
| Subsystem | blkdev partition scan dispatch |
| Stage | Stage 5 disk/partition foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — partition cluster has no Rust-owned disk scan; Z+AD+CC are Path B leaves.

Selected `process_partition_table_entry` over `unpack`, `exFAT_find_lfn`, and `irq_eoi` for boot-path dispatch semantics + Cut Z composition + mockable oracle.

---

## Legacy ABI

```text
process_partition_table_entry() → void; plain ret
in:  ECX → PARTITION_TABLE_ENTRY*
     EBP → current MBR/EBR sector
     ESI → DISK*
     [esp+4] → caller extended-partition stack slot
out: extended → writes FirstAbsSector to [esp+4]
     normal → stdcall disk_add_partition(start, length, disk)
preserves: ECX, ESI, EBP
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_process_partition_table_entry` |
| Blob | **137** bytes, **0 relocations** |
| SHA-256 | `1c6c2c370cad1d18de114d0698450efd211d1388c32ec18f5954b9ec1b0ce123` |
| Callback | naked `cc_disk_add_partition_for_rust` → tail-`jmp disk_add_partition` (REG-008/009) |
| Trampoline | `pushad`; save **`EBX`→`cc_trampoline_mbr_buf`**; FASM **`stdcall rust_*`** (no `add esp`); `popad` |
| Gate | `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY` (prod 1) |
| Rust ABI | `stdcall rust_process_partition_table_entry(entry, mbr, cap_lo, cap_hi, ext_out, disk, add_fn); ret 28` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_process_partition_table_entry`) |
| PRNG seed | `0x43555443` (`'CUTC'`) |
| PRNG cases | 50,000 |
| Cut CC host tests | **6/6 PASS** |
| Full host suite | **652/652 PASS** |
| ABI smoke | **PASS** — marker `'PART'` |

---

## QEMU validation

| Config | Gate | non-black | Result |
|--------|------|-----------|--------|
| OFF | `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY=0` | 779380 | PASS |
| ON | `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY=1` | 779380 | PASS |
| A/B | match | 779380 = 779380 | PASS |
| ON ×3 consecutive | 1 | 779380 each | PASS |
| AHCI + 4 FS disks | 1 | 779380 | PASS, **resets=0** (REG-009 verify) |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90` (now fails on QMP `RESET`)  
Desktop: `python scripts/qmp_desktop_smoke.py --wait 90 --bus ahci --disk exfat --disk ntfs --disk iso9660 --disk xfs`  
Image: `dev_build/test/kernel-20260812-152359.img`

---

## Subsystem soak

AHCI multi-disk attach required for REG-008 (`--bus ahci --disk exfat --disk ntfs --disk iso9660 --disk xfs`). Partition scan runs on every boot via `disk_scan_partitions`.

---

## Regressions

**REG-007** (fixed): smoke mock `ret` vs `ret 20`. **REG-008** (fixed): live `disk_add_partition` required **`ESI`+`EBX`** thunk. **REG-009** (fixed): trampoline used `call`+`add esp,28` against stdcall **`ret 28`** (bootloader reset loop; QMP non-black false PASS). See [`regression-log.md`](regression-log.md).

---

## Production gate

`USE_RUST_PROCESS_PARTITION_TABLE_ENTRY = 1` in `kernel/blkdev/disk.inc` (via `project/build.toml` migration registry).

---

## Rollback

Set `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY=0` in `kernel/blkdev/disk.inc` or `enabled = false` for Cut CC in `project/build.toml`; rebuild kernel.

---

## Files changed

| Path | Change |
|------|--------|
| `rust_kernel/kolibri_utils/src/partition.rs` | Cut CC logic + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_process_partition_table_entry` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | re-exports |
| `kernel/blkdev/disk.inc` | gate + trampoline |
| `kernel/rust/process_partition_table_entry.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | smoke call |
| `project/build.toml` | blob + migration entry |
| `docs/migration/cut-cc-plan.md` | plan |
| `docs/migration/cut-cc-implementation.md` | this report |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | Cut CC entry |

---

## Known limitations

* `disk_scan_partitions` / `disk_scan_gpt` orchestration remain FASM.
* `unpack` deferred (32KB global prob buffer + large LZMA decoder).

---

## Updated inventory

**84 / 135** (`84` enabled migrations = Cut A four symbols + B–CC).
