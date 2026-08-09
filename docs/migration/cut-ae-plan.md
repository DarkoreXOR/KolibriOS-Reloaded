# Cut AE Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ae-implementation.md`](cut-ae-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AE** migrates NTFS FILETIME → BDFE conversion —
> `ntfs_datetime_to_bdfe` in `ntfs.inc`.  
> Cuts A–AD remain complete and must not be redone. Do not start Cut AF.

---

## Post-AD migration audit (cluster readiness)

### Verdict: **Path B — cluster migration remains premature**

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AD) |
| Enough build/diff/trampoline infra? | Yes |
| Subsystem with Rust-owned internals ready? | **No** — Z+AD leaves done; remaining disk path is orchestration |
| Would a cluster reduce repeated work? | **Not enough** — closest pairs (NTFS epoch twin, FAT DOS time×4) still better as sequential Path B |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| Disk/MBR/GPT (Z+AD) | protective + entry validate | Leaves done; left is `disk_scan_*` orchestration |
| FAT datetime quartet | `fat_{time,date}_to_bdfe` ± inverses | Coherent but low novelty vs G/T |
| NTFS epoch pair | `ntfs_datetime_to_bdfe` + `ntfsCalculateTime` | Twin later; AE is the read-path foothold first |
| XFS hash siblings | `xfs_hashname` / `get_before_by_hashval` | Weak boundary elimination after R+W |
| Unicode map+string | `uni2ansi_*` / `ansi2uni_*` | Anti-cluster post-AB |
| IPv4 helpers | `net_ptr_to_num4` | Anti-cluster post-AC |

### Proven classes (A–AD)

Scalar/CRC/Unicode/stream, net checksum/timers/routing, calendar pair, clip, NTFS MCB/USA / FAT 8.3 / XFS unpack+hash, HID, font AA, app header, userspace ZF, GUI screen-fit, TSS I/O, PE/COFF reloc, MBR CF validate, GPT protective ZF, process TID walk.

---

## Selected target (Path B)

| Field | Value |
|-------|-------|
| **Function** | `ntfs_datetime_to_bdfe` |
| **Source** | [`kernel/fs/ntfs.inc:1815–1827`](../../kernel/fs/ntfs.inc) |
| **Subsystem** | NTFS / FILETIME → BDFE |
| **Purpose** | Convert 1601×10⁷ FILETIME to BDFE datetime block; EDI+=8 |

### Ranked top candidates (post-AD)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `ntfs_datetime_to_bdfe` | NTFS time | 6 | **1601×10⁷ epoch + compose T** | Excellent | Low | **SELECT** |
| `ntfs_test_bootsec` | NTFS mount | 2 | FS bootsec+CF | Excellent | Low | Defer (#2) |
| `socket_check` | Network | 5 | Socket-list ZF | Good | Low–med | Defer (#3) |
| `calculate_SetChecksum_field` | exFAT | 1 | Rolling checksum skip-index | Good | Low | Defer |
| `xfs._.get_before_by_hashval` | XFS | 2 | Hash search sibling | Good | Med | Defer |
| `xfs_hashname` | XFS | 4 | Thin ROL7 hash | Excellent | Low | Reject (thin) |
| `is_string_userspace` | Memory | 1 | P-family | Excellent | Low | Reject (thin) |
| `memmove` | Memory | 24 | Stage-4 | — | High | Defer Stage-4 |

```text
Selected target:
    ntfs_datetime_to_bdfe

Source:
    kernel/fs/ntfs.inc

Subsystem:
    NTFS FILETIME → BDFE calendar

Why selected:
    New 1601×10⁷ epoch algorithm class (not another validate leaf after AD);
    composes Cut T (fsTime2bdfe); 6 live dirent callers; reloc-free;
    expands NTFS foothold beyond MCB/USA into timestamp ABI;
    excellent synthetic differential; low blast.

Why #2 was rejected:
    ntfs_test_bootsec — strong CF bootsec; deferred to avoid stacking
    another validate-shaped cut immediately after AD.

Why #3 was rejected:
    socket_check — useful Stage-5 membership foothold; less algorithm
    novelty than FILETIME epoch math composing the calendar pair.

Why cluster Path A was rejected:
    No multi-function cluster eliminates duplicated FASM↔Rust boundaries
    with controlled blast better than this single leaf. NTFS epoch twin
    (`ntfsCalculateTime`) is a natural later Path B follow-up, not a
    forced same-cut cluster.

Legacy ABI:
    call / ret  (tail-jumps to fsTime2bdfe when FASM body)
    in:  EDX:EAX = FILETIME (100ns since 1601-01-01)
         EDI → BDFE outbuf
    out: EDI = EDI+8; BDFE written
    clobbers: EAX, EBX, ECX, EDX (via fsTime2bdfe)
    preserves: ESI, EBP (untouched)
    quirks: bias sub/sbb 3365781504 / 29389701;
            if EDX>=10000000 after bias then EDX:=0 before div;
            wrapping pre-2001 FILETIME; Capacity of div remainder discarded

Critical invariants:
    Exact bias + clamp-before-div order
    Compose fsTime2bdfe semantics (hour as word; pad cleared; EDI+=8)
    Do not mutate caller FILETIME source; only write 8 bytes at EDI

Rust strategy:
    Freestanding ntfs_datetime_to_bdfe(+_ptr) → writes BDFE via inlined
    fs_time2bdfe (reloc-free; no cross-section call)

Trampoline strategy:
    stdcall rust_*(eax, edx, edi); add edi, 8; ret

Differential strategy:
    Independent FASM-flow oracle (bias/clamp/div + fasm_oracle_fs_time2bdfe)
    Named epoch / clamp / pre-2001 wrap / leap / end-of-day vectors
    50k PRNG seed 0x43555445 ('CUTE')

ABI smoke strategy:
    Synthetic FILETIME → public ntfs_datetime_to_bdfe
    EDI+=8 chaining; ESI/EBP canaries; BDFE layout checks

QEMU strategy:
    CoW OFF then ON from cut-ad-final.img lineage; desktop regression;
    NTFS dirent soak only if stock image evidences NTFS path

Rollback gate:
    USE_RUST_NTFS_DATETIME_TO_BDFE = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register→stdcall trampoline; `USE_RUST_NTFS_DATETIME_TO_BDFE` rollback switch.

---

## Out of scope

* Migrating `ntfsCalculateTime` / `ntfs_test_bootsec` / `socket_check` /
  `memmove` / FAT datetime quartet / XFS hashname
* Beginning Cut AF
* Changing FILETIME clamp / wrap quirks
* Rewriting NTFS dirent enumeration
