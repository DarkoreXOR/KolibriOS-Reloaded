# Cut Z Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-z-implementation.md`](cut-z-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut Z** migrates MBR/EBR partition-table entry validation —
> `is_partition_table_entry`, which decides whether a 16-byte slot is a valid
> partition-table entry during `disk_scan_partitions`.  
> Cuts A–Y remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `is_partition_table_entry` |
| **Source** | [`kernel/blkdev/disk.inc:1056–1084`](../../kernel/blkdev/disk.inc) |
| **Subsystem** | Disk / MBR·EBR partition detection |
| **Purpose** | Validate one partition-table entry (Bootable + LBA sum vs media capacity) |

---

## Post-Y candidate audit (live tree)

### Coverage already proven (A–Y)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string / UTF stream | A–D, Q |
| Net checksum / TCP RTT + persist timer | E, F, M, V |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next+gen / XFS BE unpack + hash search | I–K, R, U, W |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace region gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| EDI-advancing calendar inverse | T |
| UTF-8→FAT 8.3 SM + pushad/popad | U |
| TCP persist-timer arming / clamp / sticky flag | V |
| Binary search + EAX+ZF dual return + BE table walk | W |
| CPU/TSS I/O-bitmap BTR/BTS privilege state | X |
| PE/COFF section walk + DIR32/REL32 buffer patch | Y |

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer Stage-4** | Forward-only `rep movsd`/`movsb`; EAX/EBX/ECX preserve; cross-subsystem fanout |
| `is_protective_mbr` | 1 | **defer (#2)** | Strong GPT-class companion; thinner than #1 |
| `pid_to_slot` | ~8 | **defer (#3)** | Process TID→slot; globals (`SLOT_BASE`) raise risk |
| `get_coff_sym` | 3 | **reject** | Anti-cluster after Cut Y |
| `is_string_userspace` | 1 | **defer** | Thinner after Cut P |
| `irq_eoi` / `enable_irq` | 4 / 5 | **defer** | HW PIC/APIC; weak synthetic oracle |
| `mutex_init` | 34 | **defer Stage-4** | Sync export surface |
| `net_ptr_to_num4` | 12 | **defer** | Thin device index scan + fanout |
| `pci_make_config_cmd` | 2 | **reject** | Trivial scalar |

### `memmove` special evaluation

| Property | Finding |
|----------|---------|
| Implementation | Forward-only `rep movsd`/`movsb`; **not** bidirectional C `memmove` |
| Overlap | Correct for left-shift (`src = dest+N`); wrong for dest>src overlap |
| Callers | 24 across syscall/HID/GUI/FS; several rely on EAX/EBX/ECX preserve |
| Blast | Stage-4 memory class — not contained incremental util |
| Cut Z | **DEFER** — preferred class, wrong blast radius for Z |

### Ranked top three

| Candidate | Subsystem | Callers | New class | Differential | ABI smoke | QEMU | Blast | Risk | Verdict |
|-----------|-----------|--------:|-----------|--------------|-----------|------|-------|------|---------|
| `is_partition_table_entry` | Disk/MBR | 5 | **Partition validate + CF** | Excellent | Med CF+ESI/EBP | Strong boot scan | Med | Med–high | **SELECT** |
| `is_protective_mbr` | Disk/GPT | 1 | Protective-MBR ZF gate | Excellent | Easy ZF | Boot GPT | Low | Med | Defer (#2) |
| `pid_to_slot` | Process | ~8 | SLOT_BASE TID walk | Excellent | Easy EAX | Syscall | Med | Med | Defer (#3) |

```text
Selected target:
    is_partition_table_entry

Why selected:
    First disk/MBR partition-table validation leaf; preferred NEW class after
    Cuts X (TSS) and Y (PE reloc); 5 co-located callers; excellent synthetic
    differential; boot-reachable on every media scan; Strategy A+C with
    trampoline-injected capacity.

Why #2 was rejected:
    is_protective_mbr — excellent GPT companion, but 1 caller and thinner
    substance than the Bootable + 64-bit half-capacity compare of #1.

Why #3 was rejected:
    pid_to_slot — valuable process-state class, but SLOT_BASE / thread_count
    globals raise relocation and blast risk vs a pure partition leaf.

Legacy ABI:
    call / ret
    in:  ECX → PARTITION_TABLE_ENTRY*
         EBP = MBR/EBR LBA base
         ESI → DISK* (.MediaInfo.Capacity qword)
    out: CF=0 valid, CF=1 invalid
    preserves: ECX, ESI, EBP (callers keep them live)
    clobbers: EAX, EDX, flags

Critical invariants:
    Bootable & 0x7F == 0 (only 0 / 0x80)
    (ebp + FirstAbsSector + Length) / 2 < Capacity   (unsigned 64-bit)
    Read-only; no memory writes

Rust strategy:
    Freestanding is_partition_table_entry(+_ptr) → EAX 0/1
    Capacity passed as stack args (reloc-free)

Trampoline strategy:
    push ECX/ESI/EBP
    stdcall rust_*(ecx, ebp, cap_lo, cap_hi)
    pop; test eax → clc/stc

Differential strategy:
    Independent add/adc + shr/rcr + sub/sbb oracle
    Exhaustive bootable byte; capacity boundaries; 50k PRNG

ABI smoke strategy:
    Synthetic entry + DISK stub (Capacity @56)
    Valid / invalid bootable; half-capacity edges; ebp base; ECX/ESI/EBP preserve

QEMU strategy:
    CoW from cut-y-final.img; OFF then ON; desktop + e1000
    Target path exercised by stock floppy partition scan at boot

Rollback gate:
    USE_RUST_IS_PARTITION_TABLE_ENTRY = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register→stdcall→CF trampoline; `USE_RUST_IS_PARTITION_TABLE_ENTRY` rollback switch.

---

## Out of scope

* Migrating `memmove` / `is_protective_mbr` / `pid_to_slot` / `get_coff_sym`  
* Migrating `process_partition_table_entry` / GPT scan  
* Beginning Cut AA  
* Changing forward-only `memmove` overlap semantics  
