# Cut AI Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ai-implementation.md`](cut-ai-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AI** migrates exFAT NameHash —
> `exFAT_hash_calculate` (extracted from the former inlined loop in
> `exFAT_find_lfn`).  
> Cuts A–AH remain complete and must not be redone. Do not start Cut AJ.

---

## Post-AH migration audit (cluster readiness)

### Verdict: **Path B — shared-core reuse, not a new Path A cluster**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AH) |
| AH rolling core available? | **Yes** — `exfat_rolling_checksum` |
| NameHash clean boundary? | **Yes** — `(ESI,ECX)→AX` over UTF-16 name bytes |
| Path A SetChecksum+NameHash? | **No** — SetChecksum already shipped; helper reuse ≠ Rust-owned subsystem |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| exFAT SetChecksum + NameHash as Path A | AH leaf + NameHash | AH complete; migrating NameHash alone reuses the core (Path B), does not create shared mutable Rust-owned exFAT state |
| Socket membership | `socket_check` + `socket_num_to_ptr` | Unchanged blockers: FASM `net_sockets`/`socket_mutex`, lock asymmetry, dead wrappers |
| NTFS / disk / FAT datetime / XFS / unicode / IPv4 / taskman | prior near-misses | Unchanged vs AH audit |

### NameHash boundary (live tree)

| Item | Finding |
|------|---------|
| Location | Inlined in `exFAT_find_lfn` (~L905); comment-labeled `exFAT_hash_calculate` |
| Call sites | **1** (after UTF-8→UTF-16 upper conversion; length = `edi−esi−2`) |
| Algorithm | Same rolling 16-bit hash as SetChecksum **without** skip of indices 2–3 |
| Side effects | AX out; call site stores `[exFAT.current_hash]`; ESI advance discarded by outer `pop edi esi` |
| Testability | Excellent — independent FASM-flow oracle; 50k PRNG; composes AH core |

### Socket near-miss (re-verified)

Blockers unchanged after live re-read of `socket.inc`: FASM list/mutex ownership,
asymmetric locking, dead `ptr_to_num`/`check_owner`, incomplete walk coverage.

### Ranked top candidates (post-AH)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `exFAT_hash_calculate` (NameHash) | exFAT | 1 | Shared-core NameHash leaf | Excellent | Low | **SELECT** |
| `socket_check` | Network | 4 | Socket-list ZF | Good | Low–med | Defer |
| FAT datetime quartet | FAT/exFAT | ~22 | DOS packed time | Excellent | Low | Reject (low novelty) |
| `xfs_hashname` / `is_string_userspace` | XFS / mem | 4 / 1 | Thin | Excellent | Low | Reject (thin) |
| `createMcbEntry` | NTFS | 5 | MCB encode | Hard | High | Defer |
| `memmove` | Memory | ~24 | Stage-4 | — | High | Defer Stage-4 |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: exFAT_hash_calculate (NameHash)
Subsystem: exFAT / NameHash
Why selected:
    Clean independently testable (buf,len)→u16 boundary after extracting the
    former inline; reuses verified AH exfat_rolling_checksum(skip=false);
    single call site; meaningful algorithm (not a wrapper); completes AH
    foreshadow without inventing a false Path A cluster.
Why Path A / Path B:
    Path B — one leaf extraction + shared-helper reuse. SetChecksum already
    shipped; no new Rust-owned exFAT subsystem state or reduced multi-crossing
    architecture beyond the existing core.
Rejected alternatives:
    Path A SetChecksum+NameHash claim; socket membership; FAT datetime;
    thin hashes; createMcbEntry; memmove.
Expected ABI:
    call / ret; in ESI→NameUTF16 bytes, ECX=byte len; out AX=hash;
    Rust trampoline preserves EBX/ECX/EDX/ESI/EDI; legacy FASM burns ECX
    and advances ESI (call site restores via push/pop).
Expected validation:
    Independent FASM-flow NameHash oracle; named UTF-16 / odd-len / wrap
    vectors; 50k PRNG seed 0x43555449 ('CUTI'); ABI smoke marker 'EXNH';
    QEMU OFF then ON.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
(ESI,ECX)→stdcall trampoline; `USE_RUST_EXFAT_HASH_CALCULATE` rollback switch;
extract labeled leaf from former inline in `exFAT_find_lfn`.

---

## Out of scope

* Claiming Path A for the exFAT checksum pair
* Migrating `socket_check` / `memmove` / FAT datetime / `createMcbEntry`
* Beginning Cut AJ
* Changing NameHash byte-oriented semantics or ECX==0 hang quirk on legacy path
* Stock-image exFAT lookup soak (unless available)
