# Cut AL Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-al-implementation.md`](cut-al-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AL** migrates EXT/ext4 Unix (+extra epoch bits) →
> Kolibri BDFE — `ext_read_time` in `ext.inc`.  
> Cuts A–AK remain complete and must not be redone.

---

## Post-AK migration audit (cluster readiness)

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AK) |
| AH+AI rolling core = Path A? | **No** — helper reuse ≠ Rust-owned exFAT subsystem |
| ISO compare pair Path A? | **No** — and `cd_compare_name` already trampolines to AJ Rust when ON |
| XFS bigtime + thin v4 sibling Path A? | **No** — `conv_time_to_kos_epoch` remains a thin `movbe`+`fsTime2bdfe` wrapper |
| EXT read+write times Path A? | **No** — write depends on `fsGetTime`; all-times is inode orchestration |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — first EXT foothold: `ext_read_time` |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| ISO `iso9660_compare_name` + `cd_compare_name` | two compare leaves | Pattern reuse; CD already shares AJ leaf when gate ON |
| exFAT SetChecksum + NameHash as Path A | AH + AI | Already shipped as two Path B cuts |
| XFS bigtime + thin v4 sibling | `conv_bigtime` + `conv_time` | Thin wrapper — anti-cluster (unchanged after AK) |
| EXT `ext_read_time` + `ext_write_time` / `ext_read_all_times` | read leaf + write/orchestrator | Write depends on `fsGetTime`; all-times is inode fan-out |
| Socket membership | `socket_check` + `socket_num_to_ptr` | Unchanged blockers |
| MCB encode+decode | `createMcbEntry` + I | FRS mutation / high blast |
| FAT datetime ×4 | `fat_*_to_bdfe` | Low novelty vs G/T/AE/AF |

### Ranked top candidates (post-AK)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `ext_read_time` | EXT | via `ext_read_all_times` (2 live) | **EXT/ext4 Unix+epoch** | Excellent | Low | **SELECT** |
| `fat_name_is_legal` | FAT | 1 | Legal-char CF | Excellent | Low | #2 (post-U novelty) |
| `xfs._.get_before_by_hashval` | XFS | 2 | Node walk | Good | Low–med | #3 (weaker after R+W) |
| `ansi2uni_char` | Unicode | ~5–6 | CP866 decode | Excellent | Low–med | #4 |
| `socket_check` / `cd_compare_name` / thin wrappers | — | — | — | — | — | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: ext_read_time
Source: kernel/fs/ext.inc
Subsystem: EXT / Unix (+ext4 extra epoch) → BDFE
Why selected:
    Strongest remaining Stage-2 leaf after AK: first EXT foothold with
    meaningful 34-bit epoch-bit + signed i_time sign-extension + pre/post
    KOS clamps; composes Cut T; excellent synthetic differential; low blast.
Why this is a genuine migration boundary:
    Clear (EAX=unix, EDX=extra, EDI→BDFE)→(EDI+=8, ECX preserved) leaf with
    algorithmic conversion, not a thin wrapper around FASM-owned EXT state.
Why Path A / Path B:
    Path B — one leaf. Pairing with ext_write_time / ext_read_all_times would
    be an anti-cluster (write depends on fsGetTime; all-times is orchestration).
Rejected alternatives:
    Path A ISO/exFAT/XFS-v4/EXT-pair/socket claims; cd_compare_name (AJ-routed);
    thin xfs conv_time; fat_name_is_legal (#2); get_before_by_hashval;
    ansi2uni_char; createMcbEntry; memmove; FAT datetime; thin hashes.
Expected legacy ABI:
    call / ret; in EAX=i_*time, EDX=extra (or 0), EDI→BDFE;
    out EDI+=8 via fsTime2bdfe; ECX preserved; composes calendar.
Expected Rust ABI:
    stdcall rust_ext_unix_to_secs(i_time, extra) -> EAX secs; ret 8;
    trampoline then call fsTime2bdfe (EDI+=8) with ECX push/pop.
Differential-testing strategy:
    Independent FASM-flow oracle; epoch/±1s/leap/EOD; pre-epoch clamp;
    signed-negative sign-extend; extra&3 mask; high-epoch clamp max;
    50k PRNG seed 0x4355544C ('CUTL').
ABI-risk assessment:
    Low (AK-style stdcall secs + Cut T compose); ECX preserve is critical
    for ext_read_all_times.
QEMU validation:
    Gate OFF then ON; QMP running + screendump; e1000 N/A in current qemu.args.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline; `USE_RUST_EXT_READ_TIME` rollback switch; compose
`fsTime2bdfe` (Cut T) for calendar (no large inlined calendar in the EXT path).

---

## Out of scope

* Claiming Path A for EXT time helpers or ISO/exFAT/XFS pairs
* Migrating `ext_write_time` / `ext_read_all_times` / `fat_name_is_legal`
* Beginning a follow-on cut during AL
* Stock-image EXT inode-time soak (unless available)
