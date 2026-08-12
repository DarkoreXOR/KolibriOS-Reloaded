# Cut CC Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-cc-implementation.md`](cut-cc-implementation.md)

---

## Fresh post-CB migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **83** (post-CB baseline) |
| `[[rust.migrations]]` entries | **83** (Cut A = 4 symbols) |
| `[ ]` pending | **52** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut CB (`ahci_port_wait`) | **closed** — untouched |
| Cut CA (`fsReadCMOS`) | **closed** — REG-005 preserved |
| All prior gates | **83/83 enabled** |

Baseline before this cut: **83 / 135**. Target after: **84 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| Calendar / CMOS | No | BV+CA complete |
| Unicode | No | BZ + Cut A closed |
| XFS / NTFS / EXT | No | leaf-only rejections unchanged |
| FAT / exFAT | No | calendar quartet complete |
| Networking / TCP | No | timer/flag leaves ≠ ownership |
| PE / COFF | No | Y+AT+BU leaves |
| AHCI / blkdev | No | AV/BM/CB leaves only |
| **Partition / disk scan** | **No** | Z+AD validate leaves; CC dispatch only |
| paging / V86 / PCI | No | AQ/BL/BA leaves |
| IRQ / scheduler / GUI | No | late-stage / boundaries non-cuts |

### Ranked candidates (52 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`process_partition_table_entry`** | MBR/EBR entry dispatch | 4 | desktop boot scan | **Good** | Med | **SELECT** |
| 2 | `unpack` | LZMA + relocs | 2 | desktop DLL | Excellent | **High** | Defer — 32KB global prob buf + ~500-line decoder |
| 3 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS island |
| 4 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med | Defer — interrupt path |
| 5 | `ntfs_restore_usa_frs` | J size preload | 4 | `--disk ntfs` | none | Low | Reject — 1-instruction wrapper |
| 6 | `tcp_mss` | TCP MSS clamp | 1 | partial net | Good | Low | Reject — 3-instruction store |
| 7 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |
| 8 | `fat_get_sector` | FAT LBA math | 5 | `--disk exfat` | Good | Med | Reject — AW ban-list |

### Why #1 wins

* **Distinct semantic class** after Cut Z (validate) / AD (protective MBR): partition **dispatch** (empty / extended / normal add).
* **4 live call sites** in `disk_scan_partitions` MBR loop — every boot with legacy MBR media.
* **Composes Cut Z** validation inline (no cross-blob call / reloc risk).
* **Strong independent oracle** with mock `disk_add_partition` + PRNG (`seed 0x43555443` / `'CUTC'`).
* **Desktop soak** exercises partition scan on every QEMU boot.

### Why alternatives lose

* `unpack`: excellent oracle but **32KB `unpack.p` global**, mutex coupling, and ~500-line LZMA — elevated defer beyond PE coupling alone.
* `exFAT_find_lfn`: explicit FS plugin island deferral.
* `irq_eoi`: interrupt I/O path + poor host oracle.
* `ntfs_restore_usa_frs` / `tcp_mss`: thin wrappers.
* Export-only string leaves: zero in-kernel callers.

### Legacy ABI

```text
process_partition_table_entry() → void; plain ret
in:  ECX → PARTITION_TABLE_ENTRY*
     EBP → current MBR/EBR absolute sector
     ESI → DISK*
     [esp+4] → caller extended-partition stack slot (dword)
out: extended type → writes FirstAbsSector to [esp+4]
     normal type → stdcall disk_add_partition(start, length, disk)
preserves: ECX, ESI, EBP (callers add ECX,16 between slots)
clobbers: EAX, EDX, AL (legacy); trampoline preserves all via pushad/popad
```

### Rust ABI

```text
stdcall rust_process_partition_table_entry(entry, mbr, cap_lo, cap_hi, ext_out, disk, add_fn); ret 28
Trampoline injects capacity from DISK + disk_add_partition callback.
Preserves ECX/ESI/EBP (REG-005).
```

### Production gate

`USE_RUST_PROCESS_PARTITION_TABLE_ENTRY = 1` in `kernel/blkdev/disk.inc`.
