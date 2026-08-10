# Cut Z Implementation — `is_partition_table_entry`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-z-plan.md`](cut-z-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `is_partition_table_entry` |
| Source | [`kernel/blkdev/disk.inc`](../../kernel/blkdev/disk.inc) |
| Callers | 5 (`disk_scan_partitions` ×4; `process_partition_table_entry` ×1) |
| Rust symbol | `rust_is_partition_table_entry` |
| Pure helper | `kolibri_utils::is_partition_table_entry` |
| Subsystem | Disk / MBR·EBR partition detection |

---

## Candidate comparison (post-Y audit)

| Candidate | Outcome |
|-----------|---------|
| `is_partition_table_entry` | **Selected** — first partition/device validate leaf |
| `is_protective_mbr` | Deferred #2 — GPT protective-MBR ZF gate; thinner |
| `pid_to_slot` | Deferred #3 — process TID→slot; globals |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |
| `get_coff_sym` | Rejected — anti-cluster after Cut Y |
| `irq_eoi` / `enable_irq` | Deferred — HW PIC/APIC |
| `is_string_userspace` / `net_ptr_to_num4` / `mutex_init` | Deferred — thin / Stage-4 |

---

## Why selected

Cut Z’s research question: does Strategy A + C remain viable for a **boot-reachable disk partition validator** that combines a Bootable-byte mask with unsigned 64-bit LBA arithmetic and returns success/fail via **CF** — distinct from PE reloc patching (Y), TSS privilege bits (X), XFS, TCP, calendar, GUI, and FAT naming?

| Preference | Result |
|------------|--------|
| Materially new vs A–Y | Yes — first partition / storage-discovery leaf |
| Anti-cluster after X/Y | Yes — not PE/COFF; not TSS; not XFS |
| Strategy A feasible | Pure read-only arithmetic; capacity trampoline-injected |
| Clear ABI | Register in; CF out; ECX/ESI/EBP preserved |
| Testability | Exhaustive bootable; capacity edges; 50k PRNG |
| Runtime coverage | Stock floppy MBR scan at every boot |

---

## Original implementation

FASM leaf in `disk.inc` (retained under `USE_RUST_IS_PARTITION_TABLE_ENTRY=0`):

* `ECX` → `PARTITION_TABLE_ENTRY`
* `EBP` = MBR/EBR absolute LBA base
* `ESI` → `DISK` (reads `.MediaInfo.Capacity` qword)
* Bootable: `and al, 7Fh` / `jnz` → invalid
* `edx:eax = ebp + FirstAbsSector + Length`, then `/2` via `shr`/`rcr`
* `sub`/`sbb` vs Capacity; `jnc` → invalid
* `clc` / `stc` return

Locked layout:

| Field | Offset | Notes |
|-------|-------:|-------|
| `Bootable` | 0 | Must be 0 or 0x80 |
| `FirstAbsSector` | 8 | LBA dword |
| `Length` | 12 | Sectors dword |
| Entry size | 16 | |
| `DISK.MediaInfo.Capacity` | 56 | qword (trampoline / smoke stub) |

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/partition.rs`](../../rust_kernel/kolibri_utils/src/partition.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_is_partition_table_entry` |
| Build | [`rust_kernel/kolibri_utils/build-is-partition-table-entry.ps1`](../../rust_kernel/kolibri_utils/build-is-partition-table-entry.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_is_partition_table_entry.bin` |
| Embed | [`kernel/rust/is_partition_table_entry.inc`](../../kernel/rust/is_partition_table_entry.inc) |

`#![no_std]` freestanding; no tables / `.rodata`.

### Blob lock

| Field | Value |
|-------|-------|
| Size | **58** bytes |
| Relocations | **0** |
| SHA-256 | `92E7D11B03DD1A1F35AE0C5CE3D8892BC8E735C107B9B88D54AD501EFFE6E829` |
| Epilogue | `ret 16` (`c2 10 00`) |
| Return | EAX `0` valid / `1` invalid |

### Trampoline

Hand-written register→stdcall→CF forwarder in `disk.inc`:

```text
is_partition_table_entry:
        push    ecx / esi / ebp
        stdcall rust_is_partition_table_entry, ecx, ebp, \
                [esi+DISK.MediaInfo.Capacity], [esi+…+4]
        pop     ebp / esi / ecx
        test    eax, eax
        jnz     .rust_invalid
        clc
        ret
.rust_invalid:
        stc
        ret
```

Smoke builds a synthetic 16-byte entry + 64-byte DISK stub (Capacity @56) and calls the public ABI (valid/invalid bootable, half-capacity edges, ebp base, register preserve).

---

## Build / package sequence

```powershell
powershell -File rust_kernel/kolibri_utils/build-is-partition-table-entry.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\dev_build\cut-y-final.img ..\..\dev_build\cut-z-on.img
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-z-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **262/262** (includes Cut Z partition suite) |
| Exhaustive bootable byte (0…255) | **PASS** |
| Empty / zero-capacity / ebp base | **PASS** |
| Half-capacity + 2× media slack | **PASS** |
| 64-bit wrap / high capacity qword | **PASS** |
| Deterministic PRNG (50 000, seed `0x4355545A` / `'CUTZ'`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow add/adc + shr/rcr + sub/sbb oracle vs Rust | **PASS** |
| Boundary coverage | bootable mask; half==capacity; odd sums; ebp≠0; u64 wrap |

---

## ABI smoke

| Item | Result |
|------|--------|
| `is_partition_table_entry_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C5A` hang) |
| Vectors | valid 0/0x80; invalid 0x01; half==cap invalid; half&lt;cap valid; ebp base; high Capacity; ECX/ESI/EBP/EBX/EDI preserve |
| Marker | `rust_is_partition_table_entry_smoke_result = 'IPTE'` on success |

---

## QEMU validation

Kernels built with Cuts A–Y production gates intact (`USE_RUST_FIX_COFF_RELOCS=1`, etc.).

Images: CoW from `dev_build/cut-y-final.img`, replace `KERNEL.MNT`.

| Gate | Setting | Desktop | Network |
|------|---------|---------|---------|
| OFF | `USE_RUST_IS_PARTITION_TABLE_ENTRY=0` | **OK** (QMP `running` + screendump `dev_build/cut-z-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_IS_PARTITION_TABLE_ENTRY=1` | **OK** (screendump `dev_build/cut-z-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C5A`; boot continued to desktop).

**Real subsystem soak:** stock floppy image boots through `disk_scan_partitions`, which calls `is_partition_table_entry` four times for MBR-vs-bootsector discrimination and again from `process_partition_table_entry`. OFF/ON both reach desktop with identical non-black sample counts — partition scan succeeded on the live path.

Production default after completion: **`USE_RUST_IS_PARTITION_TABLE_ENTRY = 1`**.

Production image: `dev_build/cut-z-final.img`.

---

## Rollback

```text
USE_RUST_IS_PARTITION_TABLE_ENTRY = 0
```

Legacy FASM body remains in `disk.inc` under the `else` branch.

---

## Known limitations

* Other flags besides CF are unspecified after return (callers only use `jc`/`jnc`).
* Smoke DISK stub hardcodes Capacity at offset 56 (matches live `DISK.MediaInfo.Capacity`); trampoline uses the FASM symbolic offset.
* Does not migrate GPT protective-MBR checks (`is_protective_mbr`) or partition add orchestration.

---

## Files changed

* `rust_kernel/kolibri_utils/src/partition.rs` — algorithm + differential tests  
* `rust_kernel/kolibri_utils/src/lib.rs` — module export  
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_is_partition_table_entry`  
* `rust_kernel/kolibri_utils/build-is-partition-table-entry.ps1` — blob build  
* `rust_kernel/kolibri_utils/out/rust_is_partition_table_entry.bin` — reloc-free blob  
* `kernel/rust/is_partition_table_entry.inc` — embed + ABI smoke  
* `kernel/blkdev/disk.inc` — trampoline + `USE_RUST_IS_PARTITION_TABLE_ENTRY`  
* `kernel/kernel32.inc` — include  
* `kernel/kernel.asm` — smoke call after LTR  
* `docs/migration/cut-z-plan.md` / `cut-z-implementation.md` / `migration-plan.md`
