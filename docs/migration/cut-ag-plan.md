# Cut AG Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ag-implementation.md`](cut-ag-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AG** migrates NTFS bootsector multi-rule validation —
> `ntfs_test_bootsec` in `ntfs.inc`.  
> Cuts A–AF remain complete and must not be redone. Do not start Cut AH.

---

## Post-AF migration audit (cluster readiness)

### Verdict: **Path B — no real Rust-owned subsystem cluster yet**

Raised bar after AF: a cluster must establish a genuine Rust-owned internal
boundary (shared state/impl, fewer FASM↔Rust crossings, coherent test/rollback)
— not merely inverses, calendar siblings, or same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AF) |
| Enough build/diff/trampoline infra? | Yes |
| Subsystem with Rust-owned internals ready? | **No** — NTFS leaves + disk Z+AD; rest is orchestration / Stage-4–5 |
| Would a cluster reduce repeated work? | **Not enough** under the raised bar |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| NTFS FILETIME pair as Path A | AE+AF | Inverse conversions; explicitly not a Rust-owned NTFS core |
| NTFS mount+time | bootsec + CalculateTime | Unrelated ABIs; no shared internal calls |
| NTFS MCB encode+decode | `createMcbEntry` + I | Encode≠Stage-2 leaf; FRS mutation / high blast |
| Socket membership | `socket_check` / `num_to_ptr` / `ptr_to_num` | Dead `ptr_to_num`; mutex keeps list ownership in FASM |
| Disk after Z+AD | `disk_scan_*` / GPT | I/O + alloc orchestration |
| FAT DOS datetime ×4 | `fat_{time,date}_to_bdfe` ± inverses | Calendar siblings; low novelty vs G/T |
| XFS hash siblings | `xfs_hashname` / `get_before_by_hashval` | Thin / weak after R+W |
| Unicode map+string | `uni2ansi_*` / `ansi2uni_*` | Anti-cluster post-AB |
| IPv4 helpers | `net_ptr_to_num4` | Anti-cluster post-AC |
| Taskman after AA | getters / dead `pid_to_appdata` | Empty |
| TCP after V | cancel/outflags/mss | No small shared core |

### Proven classes (A–AF)

Scalar/CRC/Unicode/stream, net checksum/timers/routing, calendar pair + NTFS
FILETIME↔BDFE, clip, NTFS MCB/USA / FAT 8.3 / XFS unpack+hash, HID, font AA,
app header, userspace ZF, GUI screen-fit, TSS I/O, PE/COFF reloc, MBR CF
validate, GPT protective ZF, process TID walk.

---

## Selected target (Path B)

| Field | Value |
|-------|-------|
| **Function** | `ntfs_test_bootsec` |
| **Source** | [`kernel/fs/ntfs.inc:176+`](../../kernel/fs/ntfs.inc) |
| **Subsystem** | NTFS / mount bootsector validate |
| **Purpose** | Multi-rule NTFS bootsector check; CF=1 invalid |

### Ranked top candidates (post-AF)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `ntfs_test_bootsec` | NTFS mount | 2 | **FS bootsec multi-rule CF** | Excellent | Low | **SELECT** |
| `socket_check` | Network | 5→4 live | Socket-list ZF | Good | Low–med | Defer (#2) |
| `calculate_SetChecksum_field` | exFAT | 1 | Rolling checksum skip-index | Good | Low | Defer (#3) |
| FAT datetime quartet | FAT/exFAT | 22 | DOS packed time | Excellent | Low | Reject (low novelty) |
| `xfs_hashname` | XFS | 4 | Thin ROL7 | Excellent | Low | Reject (thin) |
| `is_string_userspace` | Memory | 1 | P-family | Excellent | Low | Reject (thin) |
| `memmove` | Memory | ~24 | Stage-4 | — | High | Defer Stage-4 |

```text
Selected target:
    ntfs_test_bootsec

Source:
    kernel/fs/ntfs.inc

Subsystem:
    NTFS mount bootsector validation

Why selected:
    Standing deferred #2 after AE/AF completed the FILETIME twin;
    genuinely new algorithm class (8-rule FS bootsec CF — distinct from
    Z partition-entry CF and AD protective ZF); validate-stacking after
    Z/AD cooled by AE+AF; reloc-free; excellent synthetic differential;
    low blast (only ntfs_create_partition ×2); extends NTFS foothold into
    the mount path without claiming a false cluster.

Why #2 was rejected:
    socket_check — useful Stage-5 membership foothold; less algorithm
    novelty than multi-rule bootsec; mutex siblings still FASM-owned.

Why #3 was rejected:
    calculate_SetChecksum_field — strong exFAT novelty; single caller;
    deferred so AG takes the long-standing NTFS mount foothold.

Why cluster Path A was rejected:
    No multi-function group meets the raised Rust-owned-subsystem bar.
    Closest near-miss (socket membership) collapses on a dead wrapper
    and FASM-owned net_sockets/mutex. AE+AF remain an inverse pair, not
    a Path A unit.

Legacy ABI:
    call / ret
    in:  EBX → bootsector buffer
         EDX = partition size (sectors)
    out: CF=1 invalid / CF=0 valid
    clobbers: EAX (EDX briefly pushed in rule 6)
    preserves: EBX, EDX, ECX, ESI, EDI, EBP
    quirks: TotalSectors high dword must be 0; MFT mul overflow reject;
            ClustersPerFRS/Index: -31..=-9 OR non-zero power of two
            (movsx/dec/js/test [byte],al)

Critical invariants:
    Exact eight-rule order and power-of-two tests
    EBX/EDX preserved for ntfs_create_partition
    CF polarity (set = invalid)

Rust strategy:
    Freestanding ntfs_test_bootsec(+_ptr) → bool/u32; reloc-free

Trampoline strategy:
    push ebx/edx; stdcall rust_*(ebx,edx); pop; test eax → clc/stc

Differential strategy:
    Independent FASM-flow oracle (duplicated control flow)
    Named OEM/bps/spc/FAT/total/MFT/FRS vectors
    50k PRNG seed 0x43555447 ('CUTG')

ABI smoke strategy:
    Synthetic bootsector → public ntfs_test_bootsec
    CF checks; EBX/EDX/ESI/EDI/EBP canaries

QEMU strategy:
    CoW OFF then ON from cut-af-final.img lineage; desktop regression;
    NTFS mount soak only if stock image evidences NTFS path

Rollback gate:
    USE_RUST_NTFS_TEST_BOOTSEC = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline; `USE_RUST_NTFS_TEST_BOOTSEC` rollback switch.

---

## Out of scope

* Migrating `socket_check` / `calculate_SetChecksum_field` / `memmove` /
  FAT datetime quartet / XFS hashname / `createMcbEntry`
* Beginning Cut AH
* Changing bootsec rule quirks
* Rewriting `ntfs_create_partition` orchestration
