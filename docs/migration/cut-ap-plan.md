# Cut AP Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-ap-implementation.md`](cut-ap-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AP** migrates XFS directory name hash —
> `xfs_hashname` in `xfs.asm`.  
> Cuts A–AO remain complete and must not be redone. Do not start Cut AQ.

---

## Post-AO migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Trampoline **must** preserve **ECX+EDX** (legacy body left EDX untouched; `uses ecx esi`) |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still run `--disk xfs` attach A/B |
| Cut D | EDX across `strncmp` | Same family — preserve at trampoline |

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AO) |
| FAT datetime ×4 Path A? | **No** — AO shipped; siblings/inverses ≠ ownership |
| Unicode encode+decode Path A? | **No** — AN already Path B |
| XFS W+AM+hashname Path A? | **No** — complementary leaves; XFS state remains FASM |
| EXT AL + write/all-times? | **No** — orchestration stays FASM |
| AH+AI = Path A? | **No** — helper reuse ≠ Rust-owned exFAT |
| H+`blit_clip` / S+clientbox? | **No** — desktop composition / GUI policy |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — `xfs_hashname` (ROL7 name hash; real `--disk xfs`) |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| FAT `fat_date_to_bdfe` / `bdfe_to_fat_*` | AO siblings/inverses | Explicitly not auto-continued after AO |
| Unicode `uni2ansi_char` | AN inverse | Encode already Cut A; no new boundary |
| H + `blit_clip` | geometry pair | Composition glue; desktop-only soak |
| S + `set_window_clientbox` | GUI | Desktop-only; skin injection |
| Y + `coff_get_align` | PE | Tiny align mask; loader stays FASM |
| ISO `cd_compare_name` | AJ-routed | Already AJ when gate ON |
| EXT write / all-times | AL siblings | Fan-out / `fsGetTime` deps |
| W+AM+`xfs_hashname` Path A | XFS hash trio | Producer+search consumers ≠ Rust-owned XFS |
| Sockets / MCB / memmove | — | Unchanged blockers |

### Ranked top candidates (post-AO)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `xfs_hashname` | XFS | 4 | **ROL7 dir name hash** | Excellent | Low | `--disk xfs` | **SELECT** |
| `window._.set_window_clientbox` | GUI | 3 | clientbox inset | Good | Med | desktop | #2 |
| `blit_clip` | Video | 1 | H composition | Good | Low | desktop | #3 |
| `coff_get_align` | PE | 2 | align mask | Excellent | Low | none | #4 |
| FAT date siblings / uni2ansi / EXT write / sockets / memmove | — | — | — | — | — | — | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: xfs_hashname
Source: kernel/fs/xfs.asm
Subsystem: XFS directory name hash (ROL 7 ⊕ byte)
Why selected:
    Strongest remaining Stage-2 leaf after AO that is NOT an AO datetime
    sibling: clear stdcall ABI, trivial reloc-free body, excellent
    differential, real --disk xfs A/B on lookup paths that already use
    Cuts W/AM. Prefer soak over blit_clip/clientbox desktop-only peers.
Why this is a genuine migration boundary:
    Clear (_name,_len)→EAX hash leaf; not a thin wrapper around FASM-owned
    FS state; hash feeds leaf/node/btree lookup (W/AM consumers).
Why Path A / Path B:
    Path B — one leaf. Pairing with W+AM would not create a Rust-owned XFS
    directory subsystem (inode/partition/dir orchestration remains FASM).
Rejected alternatives:
    Path A Unicode/ISO/XFS/EXT/exFAT/GUI/PE claims; fat_date_to_bdfe;
    bdfe_to_fat_*; uni2ansi_char; blit_clip; set_window_clientbox;
    fat_name_is_legal; cd_compare_name; ext_write_time; sockets; memmove; MCB.
Expected legacy ABI:
    stdcall (_name,_len) / retn 8; EAX=hash; uses ecx esi; EDX untouched;
    len==0 do-while hang (documented).
Expected Rust ABI:
    stdcall rust_xfs_hashname(name,len) -> EAX; ret 8;
    trampoline push/pop ECX+EDX+ESI (REG-001); len==0 → 0 (no hang).
Differential-testing strategy:
    Independent FASM-flow oracle; single-byte exhaustive; two-byte exhaustive;
    named vectors; 50k PRNG seed 0x43555450 ('CUTP').
ABI-risk assessment:
    Medium (REG-001 class) — mitigated by ECX+EDX+ESI trampoline preserve +
    ABI smoke canaries; EBX live across callers (stdcall callee-saved).
Required A/B tests:
    Gate OFF vs ON desktop; --disk xfs boot+attach.
Required --disk soak:
    python scripts/run_qemu.py --disk xfs (A/B).
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **ECX+EDX+ESI** preserve; `USE_RUST_XFS_HASHNAME`
rollback switch.

---

## Out of scope

* Claiming Path A for XFS hash producer+search cluster
* Migrating FAT datetime siblings of Cut AO
* Migrating `blit_clip` / GUI clientbox / `uni2ansi_char`
* Beginning Cut AQ
