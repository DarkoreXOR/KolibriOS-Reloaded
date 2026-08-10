# Cut AK Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ak-implementation.md`](cut-ak-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AK** migrates XFS v5 bigtime → Kolibri BDFE —
> `xfs._.conv_bigtime_to_kos_epoch` in `xfs.asm`.  
> Cuts A–AJ remain complete and must not be redone. Do not start Cut AL.

---

## Post-AJ migration audit (cluster readiness)

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AJ) |
| AH+AI rolling core = Path A? | **No** — helper reuse ≠ Rust-owned exFAT subsystem |
| ISO compare pair Path A? | **No** — and `cd_compare_name` already trampolines to AJ Rust when ON |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — XFS v5 bigtime epoch conversion |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| ISO `iso9660_compare_name` + `cd_compare_name` | two compare leaves | Pattern reuse; CD already shares AJ leaf when gate ON |
| exFAT SetChecksum + NameHash as Path A | AH + AI | Already shipped as two Path B cuts |
| XFS bigtime + thin v4 sibling | `conv_bigtime` + `conv_time` | Thin `movbe`+`fsTime2bdfe` wrapper — anti-cluster |
| Socket membership | `socket_check` + `socket_num_to_ptr` | Unchanged blockers |
| MCB encode+decode | `createMcbEntry` + I | FRS mutation / high blast |
| FAT datetime ×4 | `fat_*_to_bdfe` | Low novelty vs G/T/AE/AF |

### Ranked top candidates (post-AJ)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `xfs._.conv_bigtime_to_kos_epoch` | XFS | 3 ind. | **XFS v5 bigtime epoch** | Excellent | Low | **SELECT** |
| `fat_name_is_legal` | FAT | 1 | Legal-char CF | Excellent | Low | #2 (post-U novelty) |
| `socket_check` | Network | 5 | Socket-list ZF | Good | Low–med | Defer |
| `cd_compare_name` | ISO legacy | 1 | Already on AJ Rust when ON | — | Low | **Not a cut** |
| `createMcbEntry` / `memmove` | NTFS / mem | 5 / ~24 | High blast | Hard | High | Defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: xfs._.conv_bigtime_to_kos_epoch
Source: kernel/fs/xfs.asm
Subsystem: XFS / v5 bigtime → BDFE
Why selected:
    Strongest remaining Stage-2 leaf: meaningful 64-bit ns epoch math with
    pre-epoch + high-dword clamps, composes Cut T calendar; 3 indirect
    callers on the inode info path; excellent synthetic differential;
    new XFS class beyond R/W.
Why this is a genuine migration boundary:
    Clear (ECX→BE64 DQ, EDI→BDFE)→(EDI+=8) leaf with algorithmic conversion,
    not a thin wrapper around FASM-owned mutable XFS state.
Why Path A / Path B:
    Path B — one leaf. Pairing with thin v4 `conv_time_to_kos_epoch` would be
    an anti-cluster (wrapper only).
Rejected alternatives:
    Path A ISO/exFAT/socket claims; cd_compare_name (already AJ-routed);
    fat_name_is_legal (#2); createMcbEntry; memmove; FAT datetime; thin hashes.
Expected legacy ABI:
    call / ret; in ECX→DQ bigtime (movbe hi_be/lo_be), EDI→BDFE;
    out EDI+=8; preserves ESI/EBP; clobbers EAX/EBX/ECX/EDX via fsTime2bdfe.
Expected Rust ABI:
    stdcall rust_xfs_conv_bigtime_to_kos_epoch(bt_lo, bt_hi, out)
    -> writes 8-byte BDFE; ret 12; trampoline movbe + add edi,8.
Differential-testing strategy:
    Independent FASM-flow oracle; epoch/±1s/leap/EOD; pre-epoch clamp;
    high-edx clamp; subsec discard; 50k PRNG seed 0x4355544B ('CUTK').
ABI-risk assessment:
    Low (AE-style stdcall + EDI+=8); mitigated by known Cut T compose path.
QEMU validation:
    Gate OFF then ON; QMP running + screendump; e1000 N/A in current qemu.args.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline; `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH` rollback
switch; compose inlined `fs_time2bdfe` (no FASM cross-calls from blob).

---

## Out of scope

* Claiming Path A for XFS time converters or ISO/exFAT pairs
* Migrating `xfs._.conv_time_to_kos_epoch` / `fat_name_is_legal` / `socket_check`
* Beginning Cut AL
* Stock-image XFS v5 inode-time soak (unless available)
