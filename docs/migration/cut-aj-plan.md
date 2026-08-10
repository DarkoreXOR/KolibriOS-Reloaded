# Cut AJ Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-aj-implementation.md`](cut-aj-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AJ** migrates ISO9660 path-component name match —
> `iso9660_compare_name` in `iso9660.inc`.  
> Cuts A–AI remain complete and must not be redone. Do not start Cut AK.

---

## Post-AI migration audit (cluster readiness)

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AI) |
| AH+AI rolling core = Path A? | **No** — helper reuse ≠ Rust-owned exFAT subsystem |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — ISO9660 compare (new class) |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| exFAT SetChecksum + NameHash as Path A | AH + AI | Already shipped as two Path B cuts; no shared mutable Rust exFAT state |
| ISO `iso9660_compare_name` + `cd_compare_name` | two compare leaves | Shared pattern ≠ Rust-owned ISO core; migrate the modern leaf first |
| Socket membership | `socket_check` + `socket_num_to_ptr` | Unchanged blockers |
| XFS bigtime + thin v4 sibling | `conv_bigtime` + `conv_time` | Calendar stack already deep; thin sibling anti-cluster |
| MCB encode+decode | `createMcbEntry` + I | FRS mutation / high blast |
| FAT datetime ×4 | `fat_*_to_bdfe` | Low novelty vs G/T/AE/AF |

### Ranked top candidates (post-AI)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `iso9660_compare_name` | ISO9660 | 1 | **ISO path-component match** | Excellent | Low | **SELECT** |
| `xfs._.conv_bigtime_to_kos_epoch` | XFS | 3 ind. | XFS v5 epoch | Excellent | Low | #2 |
| `fat_name_is_legal` | FAT | 1 | Legal-char CF | Excellent | Low | #3 (post-U novelty) |
| `cd_compare_name` | ISO legacy | 1 | Near-dupe of #1 | Good | Low | Follow-up |
| `socket_check` | Network | 4 | Socket-list ZF | Good | Low–med | Defer |
| `createMcbEntry` / `memmove` | NTFS / mem | 5 / ~24 | High blast | Hard | High | Defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: iso9660_compare_name
Source: kernel/fs/iso9660.inc
Subsystem: ISO9660 / path-component name match
Why selected:
    Overlooked ISO leaf with meaningful UTF-8↔ASCII/UCS-2BE upper compare,
    version-separator (';') and name_len end rules; single caller; composes
    already-migrated utf8to16 + utf16toUpper; excellent synthetic differential;
    first ISO foothold without inventing a Path A cluster.
Why this is a genuine migration boundary:
    Clear (ESI,EDI,type_encoding)→(CF,ESI) leaf with algorithmic match logic,
    not a thin wrapper around FASM-owned mutable ISO state.
Why Path A / Path B:
    Path B — one leaf. Pairing with cd_compare_name would be pattern reuse,
    not a Rust-owned ISO subsystem.
Rejected alternatives:
    Path A exFAT/ISO/socket claims; fat_name_is_legal (weaker novelty);
    xfs bigtime (#2); createMcbEntry; memmove; FAT datetime; thin hashes.
Expected legacy ABI:
    call / ret; in ESI→UTF-8, EDI→ISO9660_DIRECTORY_RECORD, EBP→ISO9660;
    out CF=0 match (ESI advanced to '/' or NUL), CF=1 miss (ESI restored);
    preserves EAX/ECX/EDI; clobbers EDX.
Expected Rust ABI:
    stdcall rust_iso9660_compare_name(esi_inout, dir_record, type_encoding)
    -> EAX=0 match / 1 miss; ret 12; updates *esi_inout on match.
Differential-testing strategy:
    Independent FASM-flow oracle; ASCII/UCS-2; ';1' version; short/long;
    mismatch; 50k PRNG seed 0x4355544A ('CUTJ').
ABI-risk assessment:
    Low–med (ESI inout + CF); mitigated by Cut I-style esi_inout trampoline.
QEMU validation:
    Gate OFF then ON; QMP running + screendump; e1000 N/A in current qemu.args.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline; `USE_RUST_ISO9660_COMPARE_NAME` rollback switch;
compose inlined `utf8to16` + `utf16_to_upper` (no FASM cross-calls from blob).

---

## Out of scope

* Claiming Path A for ISO compare pair or exFAT checksum pair
* Migrating `cd_compare_name` / `socket_check` / `createMcbEntry` / `memmove`
* Beginning Cut AK
* Stock-image ISO path-lookup soak (unless available)
