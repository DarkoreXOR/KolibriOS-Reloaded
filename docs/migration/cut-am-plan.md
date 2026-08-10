# Cut AM Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-am-implementation.md`](cut-am-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AM** migrates XFS DA interior-node first-match-by-hash —
> `xfs._.get_before_by_hashval` in `xfs.asm`.  
> Cuts A–AL remain complete and must not be redone. Do not start Cut AN.

---

## Post-AL migration audit (cluster readiness)

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AL) |
| EXT AL foothold → EXT Path A? | **No** — `ext_write_time` is CMOS/`fsGetTime` write; `ext_read_all_times` is inode fan-out |
| XFS bigtime + thin v4 sibling Path A? | **No** — `conv_time_to_kos_epoch` remains 3-insn wrapper |
| ISO compare pair Path A? | **No** — `cd_compare_name` already trampolines to AJ when ON |
| AH+AI rolling core = Path A? | **No** — helper reuse ≠ Rust-owned exFAT |
| Cut W leaf search + this node search = Path A? | **No** — complementary leaves; XFS state remains FASM-owned |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — `xfs._.get_before_by_hashval` |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| EXT read+write / all-times | AL + write + orchestrator | Write depends on `fsGetTime`; all-times is fan-out |
| XFS bigtime + thin v4 sibling | AK + `conv_time` | Thin anti-cluster wrapper |
| ISO compare pair | AJ + `cd_compare_name` | CD already AJ-routed when gate ON |
| XFS leaf + node hash search | W + AM | Two Path B leaves; no Rust-owned XFS subsystem |
| Socket membership | `socket_check` + `socket_num_to_ptr` | Unchanged blockers |
| Unicode map+string | `ansi2uni_*` | Table-reloc / anti-cluster post-AB |
| MCB encode+decode | `createMcbEntry` + I | FRS mutation / high blast |

### Ranked top candidates (post-AL)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `xfs._.get_before_by_hashval` | XFS | 2 | **DA node first-match** | Excellent | Low | **SELECT** |
| `ansi2uni_char` | Unicode | ~5–6 | CP866 decode | Excellent | Low–med | #2 (table-reloc) |
| `blit_clip` | Video | 1 | H composition | Good | Low | #3 (glue around H) |
| `fat_name_is_legal` | FAT | 1 | Legal-char CF | Excellent | Low | Thin after U |
| EXT write / all-times / thin v4 / CD / sockets / memmove | — | — | — | — | — | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: xfs._.get_before_by_hashval
Source: kernel/fs/xfs.asm
Subsystem: XFS / DA interior-node first-match-by-hash
Why selected:
    Strongest remaining Stage-2 leaf after AL: real BE linear search with
    unsigned jae first-match, dual v4/v5 btree offsets, EBX=node quirk
    (dead stdcall _base), EAX+ZF dual return — distinct from Cut W binary
    leaf search; excellent synthetic differential; low blast (2 callers).
Why this is a genuine migration boundary:
    Clear (EBX=node, count, hash, version)→(EAX=before/error, ZF) leaf with
    algorithmic search, not a thin wrapper around FASM-owned XFS state.
Why Path A / Path B:
    Path B — one leaf. Pairing with Cut W or thin v4 time would not create
    a Rust-owned XFS subsystem (lookups still FASM-orchestrated).
Rejected alternatives:
    Path A EXT/ISO/exFAT/XFS/socket claims; ext_write_time / ext_read_all_times;
    thin xfs conv_time; cd_compare_name (AJ-routed); fat_name_is_legal (thin);
    ansi2uni_char; blit_clip; createMcbEntry; memmove.
Expected legacy ABI:
    stdcall (_base, _count, _hash) / retn 12; EBX=node (live); EBP→XFS.version;
    EAX=before or ERROR_FILE_NOT_FOUND; ZF found/miss; preserves ebx/edx/esi/edi.
Expected Rust ABI:
    stdcall rust_xfs_get_before_by_hashval(node, count, hash, version) -> EDX:EAX;
    ret 16; trampoline reconstructs ZF via cmp edx,1.
Differential-testing strategy:
    Independent FASM-flow oracle; v4/v5 layouts; jae first-match; empty safe
    miss; BE edges; duplicates; 50k PRNG seed 0x4355544D ('CUTM').
ABI-risk assessment:
    Medium-low (Cut W ZF pack pattern + EBX-not-_base quirk + version inject);
    omit-FP trampoline mandatory.
QEMU validation:
    Gate OFF then ON; QMP running + screendump; e1000 N/A in current qemu.args.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
omit-FP register/stdcall trampoline; `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL`
rollback switch; ZF reconstruction via `cmp edx,1` (Cut W/P pattern).

---

## Out of scope

* Claiming Path A for XFS hash helpers or EXT/ISO/exFAT pairs
* Migrating `xfs_hashname` / `conv_time_to_kos_epoch` / `ext_write_time`
* Beginning Cut AN
* Stock-image XFS node-walk soak (unless available)
