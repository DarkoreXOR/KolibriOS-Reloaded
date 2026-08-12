# Cut CB Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-cb-implementation.md`](cut-cb-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut CB** migrates AHCI port TFD busy/DRQ poll leaf —
> `ahci_port_wait` in `kernel/blkdev/ahci.inc`.  
> Cuts A–CA remain complete. Cut CA is closed — do not modify. Do not start Cut CC.

---

## Fresh post-CA migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **82** (pre-CB baseline) |
| `[[rust.migrations]]` entries | **83** (Cut A = 4 symbols) |
| `[ ]` pending | **53** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut CA (`fsReadCMOS`) | **closed** — REG-005 trampoline preserved |
| All prior gates | **82/82 enabled** |

Baseline before this cut: **82 / 135**. Target after: **83 / 135**.

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
| **AHCI / blkdev** | **No** | AV/BM/CB leaves only; no controller ownership |
| paging / V86 / PCI | No | AQ/BL/BA leaves |
| IRQ / scheduler / GUI | No | late-stage / boundaries non-cuts |

### Ranked candidates (53 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`ahci_port_wait`** | AHCI TFD busy/DRQ poll | 2 | desktop + AHCI path | **Good** (mock TFD/ticks) | Low | **SELECT** |
| 2 | `unpack` | LZMA + relocs | 2 | desktop DLL | Excellent | Med | Defer — PE coupling |
| 3 | `process_partition_table_entry` | partition orchestration | 4 | desktop scan | Good | Med | Defer — Z sibling |
| 4 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS island |
| 5 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med | Defer — interrupt path |
| 6 | `ntfs_restore_usa_frs` | J size preload | 4 | `--disk ntfs` | none | Low | Reject — 1-instruction wrapper |
| 7 | `tcp_mss` | TCP MSS clamp | 1 | partial net | Good | Low | Reject — 3-instruction store |
| 8 | `fat_get_sector` | FAT LBA math | 5 | `--disk exfat` | Good | Med | Reject — AW ban-list |

### Why #1 wins

* **Distinct semantic class** after AV (cmdslot) and BM (PxSIG): timer-deadline MMIO poll before command issue.
* **2 live `stdcall` sites** on pre-command identify and sector I/O paths — meaningful, bounded blast radius.
* **Clean Path B boundary:** poll loop only; MMIO + `timer_ticks` via injected FASM callbacks.
* **Strong independent oracle:** mock TFD sequences + unsigned deadline wrap (`seed 0x43555457` / `'CUTW'`).
* **Desktop soak** exercises AHCI init smoke + live command prep when AHCI present.

### Why alternatives lose

* `ntfs_restore_usa_frs`: thin Cut J wrapper (`mov eax,[frs_size]` fallthrough).
* `tcp_mss`: three-instruction TCP deepen; single caller.
* `exFAT_find_lfn`: explicit FS plugin island deferral.
* `unpack`: strong oracle but high complexity / PE coupling.
* `process_partition_table_entry` / IRQ paths: orchestration ownership.
* Export-only / zero-caller symbols: no novelty.

### Legacy ABI

```text
stdcall ahci_port_wait(port, timeout) → EAX; ret 8
in:  port    = HBA_PORT pointer
     timeout = deadline offset in timer_ticks units
out: EAX = 0 success (TFD not BUSY|DRQ)
     EAX = 1 timeout (still busy at deadline)
poll: [port+task_file_data] & (0x80|0x08) until clear or timer_ticks >= start+timeout
preserves: EBX, ECX (explicit push/pop)
clobbers: EAX
side effects: reads timer_ticks, port MMIO; DEBUGF on timeout (FASM-only path)
```

### Rust ABI

```text
stdcall rust_ahci_port_wait(read_tfd, read_ticks, port, timeout) → EAX; ret 16
Trampoline injects ahci_port_wait_read_tfd + ahci_port_wait_read_ticks.
Preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001 canaries).
```

### Production gate

`USE_RUST_AHCI_PORT_WAIT = 1` in `kernel/blkdev/ahci.inc`.
