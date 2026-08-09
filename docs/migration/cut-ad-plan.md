# Cut AD Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ad-implementation.md`](cut-ad-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AD** migrates GPT protective-MBR recognition —
> `is_protective_mbr` in `disk.inc`.  
> Cuts A–AC remain complete and must not be redone. Do not start Cut AE.

---

## Post-AC migration audit (cluster readiness)

### Verdict: **Path B — cluster migration is premature**

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AC cover ZF/CF/EAX/streaming/tables/globals inject) |
| Enough build/diff/trampoline infra? | Yes (`tools/build`, Strategy A+C, smoke harness) |
| Subsystem with Rust-owned internals ready? | **No** — footholds exist (Z, AA, AC, G/T, Q/AB) but remaining siblings are thin or Stage-4/5 |
| Would a cluster reduce repeated work? | **Not enough** — strongest multi-fn groups either anti-cluster after a recent cut or add blast without eliminating FASM↔Rust edges |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| IPv4 helpers | `net_ptr_to_num4`, fragment/socket siblings | AC already inlined `net_ptr_to_num4`; anti-cluster; packet-hot fanout |
| Unicode map+string | `uni2ansi_char` / `ansi2uni_char` / `*_string` | Anti-cluster post-AB; pattern repeat; Cut A CP866 overlap |
| FAT DOS datetime quartet | `fat_{time,date}_to_bdfe` + inverses | Coherent but low novelty vs G/T; no Rust-boundary elimination |
| taskman around AA | — | No remaining leaf cluster |
| Z + protective MBR as one cut | already split | Z done three cuts ago; protective MBR is a **single** follow-up, not a rewrite cluster |

### Proven classes (A–AC) — still single-cut Stage-2

Scalar/CRC/Unicode/stream, net checksum/timers/routing, calendar pair, clip, NTFS/FAT/XFS leaves, HID, font AA, app header, userspace ZF, GUI screen-fit, TSS I/O bitmap, PE/COFF reloc, MBR entry CF validate, process TID walk.

---

## Selected target (Path B)

| Field | Value |
|-------|-------|
| **Function** | `is_protective_mbr` |
| **Source** | [`kernel/blkdev/disk.inc:1021–1048`](../../kernel/blkdev/disk.inc) |
| **Subsystem** | Disk / GPT protective MBR |
| **Purpose** | Recognize protective GPT MBR (ZF) before `disk_scan_gpt` |

### Ranked top three (post-AC)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `is_protective_mbr` | Disk/GPT | 1 | **GPT protective-MBR ZF** | Excellent | Low | **SELECT** |
| `ntfs_test_bootsec` | NTFS mount | 2 | FS bootsec+CF | Excellent | Low | Defer (#2) |
| `socket_check` | Network | 5 | Socket-list ZF | Good | Low–med | Defer (#3) |

```text
Selected target:
    is_protective_mbr

Source:
    kernel/blkdev/disk.inc

Subsystem:
    Disk / GPT protective-MBR recognition

Why selected:
    Standing #2 since AB/AC; Cut Z is three cuts behind (anti-cluster cooled);
    new GPT protective-MBR ZF class (not a CF entry-clone of Z); 1 caller;
    excellent synthetic differential; capacity inject like Cut Z; low blast;
    AC just took network — avoid IPv4 siblings.

Why #2 was rejected:
    ntfs_test_bootsec — strong CF bootsec leaf; deferred to prefer GPT ZF
    companion now that Z→AA→AB→AC cooling is done.

Why #3 was rejected:
    socket_check — useful Stage-5 membership; less foothold than completing
    the GPT protective path next to Cut Z.

Why cluster Path A was rejected:
    No coherent multi-function cluster eliminates duplicated FASM↔Rust
    boundaries with controlled blast and independent rollback better than
    this single leaf.

Legacy ABI:
    call / ret
    in:  ECX → partition-table array (MBR+0x1BE)
         ESI → DISK (Capacity low dword only)
    out: ZF set = protective MBR; ZF clear = not
    preserves: ECX, EDI (explicit push/pop); ESI untouched
    clobbers: EAX (left 0), flags (ZF is ABI; others unspecified)
    quirks: [ecx-2] word must be 0; entry0 bootable=0, type=0xEE,
            FirstAbs=1, Length=0xFFFFFFFF OR (Capacity_lo-1);
            entries 1–3 must be 48 zero bytes (repz scasw);
            Capacity HIGH dword ignored

Critical invariants:
    Exact early-out order (sig word → bootable → type → LBA → length → zeros)
    Length compare uses wrapping add of -1 + Capacity_lo
    Do not mutate MBR buffer or DISK
    Caller already verified 0xAA55 at [ecx+0x40]

Rust strategy:
    Freestanding is_protective_mbr(+_ptr) → EAX 0=protective / 1=not
    capacity_lo passed as stack arg (reloc-free)

Trampoline strategy:
    push ecx/esi/edi
    stdcall rust_*(ecx, [esi+DISK.MediaInfo.Capacity])
    pop; test eax,eax → ZF

Differential strategy:
    Independent FASM-flow oracle on synthetic MBR buffers
    Named protective / non-protective / capacity edge / empty slots
    50k PRNG seed 0x43555444 ('CUTD')

ABI smoke strategy:
    Synthetic MBR + DISK stub via public is_protective_mbr
    ECX/ESI/EDI preserve; ZF polarity canaries

QEMU strategy:
    CoW OFF then ON from cut-ac-final.img lineage; desktop regression;
    GPT soak only if stock image evidences protective-MBR path

Rollback gate:
    USE_RUST_IS_PROTECTIVE_MBR = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register→stdcall trampoline; `USE_RUST_IS_PROTECTIVE_MBR` rollback switch.

---

## Out of scope

* Migrating `memmove` / `get_pg_addr` / `net_ptr_to_num4` / `ntfs_test_bootsec` /
  `socket_check` / unicode map cluster / FAT datetime cluster
* Beginning Cut AE
* Changing protective-MBR quirks (Capacity high dword ignored, etc.)
* Rewriting `disk_scan_gpt`
