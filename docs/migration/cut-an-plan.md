# Cut AN Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-an-implementation.md`](cut-an-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AN** migrates CP866 → Unicode decode —
> `ansi2uni_char` in `parse_fn.inc`.  
> Cuts A–AM remain complete and must not be redone. Do not start Cut AO.

---

## Post-AM migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across unicode | Trampoline **must** `push`/`pop` **ECX+EDX**; FAT/ISO `loop` needs ECX; do not leave EDX to callers |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still run `--disk iso9660` name-copy soak |
| Cut D | EDX across `strncmp` | Same family — preserve at trampoline |

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AM) |
| EXT AL foothold → EXT Path A? | **No** — write/all-times still FASM orchestration |
| XFS W+AM = Path A? | **No** — complementary leaves; state remains FASM |
| AH+AI = Path A? | **No** — helper reuse ≠ Rust-owned exFAT |
| ISO AJ + CD = Path A? | **No** — CD already AJ-routed when ON |
| Encode+decode Unicode Path A? | **No** — Cut A already owns encode; decode is Path B leaf |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — `ansi2uni_char` (CP866 decode; inverse of Cut A encode) |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| Unicode encode+decode Path A | Cut A `cp866_encode` + `ansi2uni_char` | Encode already shipped; pairing ≠ new Rust-owned subsystem |
| EXT read+write / all-times | AL + write + orchestrator | Write depends on `fsGetTime`; all-times is fan-out |
| XFS leaf + node hash | W + AM | Already two Path B leaves |
| XFS bigtime + thin v4 | AK + `conv_time` | Thin anti-cluster wrapper |
| ISO compare pair | AJ + `cd_compare_name` | CD already AJ-routed when gate ON |
| Socket membership | `socket_check` + `socket_num_to_ptr` | Unchanged blockers |
| MCB encode+decode | `createMcbEntry` + I | FRS mutation / high blast |
| Video H + `blit_clip` | geometry pair | Composition glue; no FS soak |
| FAT datetime ×4 | pack/unpack | Low novelty vs G/T |

### Ranked top candidates (post-AM)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `ansi2uni_char` | Unicode | 6 / 4 files | **CP866 decode** | Excellent | Low–med | `--disk iso9660` | **SELECT** |
| `blit_clip` | Video | 1 | H composition | Good | Low | desktop only | #2 |
| `fat_name_is_legal` | FAT | 1 | Legal-char CF | Excellent | Low | no FAT `--disk` | #3 |
| EXT write / thin v4 / CD / sockets / memmove / `xfs_hashname` | — | — | — | — | — | — | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: ansi2uni_char
Source: kernel/fs/parse_fn.inc
Subsystem: Unicode / CP866 → Unicode decode
Why selected:
    Strongest remaining Stage-2 leaf after AM: completes the CP866 map with
    Cut A encode without claiming Path A; real ISO name-copy callers; excellent
    exhaustive differential (256 inputs); REG-001-class trampoline discipline
    is the validation focus.
Why this is a genuine migration boundary:
    Clear AL→AX leaf with algorithmic range map + special F0–F7 table decode;
    not a thin wrapper around FASM-owned FS state.
Why Path A / Path B:
    Path B — one leaf. Pairing with already-migrated encode would not create
    a Rust-owned Unicode subsystem (string orchestrators remain FASM).
Rejected alternatives:
    Path A Unicode/EXT/ISO/exFAT/XFS/socket claims; blit_clip; fat_name_is_legal;
    ext_write_time / ext_read_all_times; thin xfs conv_time; cd_compare_name;
    xfs_hashname; createMcbEntry; memmove.
Expected legacy ABI:
    call/ret; AL in → AX out; movzx first; FASM body leaves ECX/EDX/ESI/EDI/EBX;
    special 0x14→U+00B6; 0x80–0xAF→0x410+; 0xE0–0xEF→0x440+; 0xF0–0xF7 table+0x400;
    else '_'.
Expected Rust ABI:
    stdcall rust_ansi2uni_char(ch) -> EAX (AX=Unicode); ret 4;
    trampoline push/pop ECX+EDX (REG-001).
Differential-testing strategy:
    Independent FASM-flow oracle; exhaustive 0..255; encode↔decode round-trip
    on mapped set; 50k PRNG seed 0x4355544E ('CUTN') over u8 domain.
ABI-risk assessment:
    Medium (REG-001 class) — mitigated by ECX+EDX trampoline preserve + ABI
    smoke canaries in ECX/EDX around public call.
Required A/B tests:
    Gate OFF vs ON desktop; --disk iso9660 browse name-copy path.
Required --disk soak:
    python scripts/run_qemu.py --disk iso9660 (A/B).
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **ECX+EDX** preserve; `USE_RUST_ANSI2UNI_CHAR`
rollback switch.

---

## Out of scope

* Claiming Path A for Unicode encode/decode pair
* Migrating `uni2ansi_char` body (already covered by Cut A encode trampoline path)
* Migrating `blit_clip` / `fat_name_is_legal` / `xfs_hashname`
* Beginning Cut AO
