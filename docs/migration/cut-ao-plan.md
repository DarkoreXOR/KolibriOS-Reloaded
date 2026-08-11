# Cut AO Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-ao-implementation.md`](cut-ao-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AO** migrates DOS FAT packed-time unpack —
> `fat_time_to_bdfe` in `fat.inc`.  
> Cuts A–AN remain complete and must not be redone. Do not start Cut AP.

---

## Post-AN migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Trampoline **must** `push`/`pop` **ECX+EDX** (legacy body already did; FAT/exFAT entry→BDFE keeps ESI/EDI live) |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still run `--disk exfat` entry-time soak |
| Cut D | EDX across `strncmp` | Same family — preserve at trampoline |

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AN) |
| Unicode encode+decode Path A? | **No** — AN already Path B; orchestrators remain FASM |
| ISO AJ + AN = Path A? | **No** — name match ≠ CP866 decode ownership |
| XFS W+AM+AK = Path A? | **No** — complementary leaves; state remains FASM |
| EXT AL + write/all-times? | **No** — write/all-times still FASM orchestration |
| AH+AI = Path A? | **No** — helper reuse ≠ Rust-owned exFAT |
| FAT datetime ×4 Path A? | **No** — sequential Path B at best; start with time unpack |
| Socket membership ready? | **No** — FASM `net_sockets`/`socket_mutex` blockers unchanged |
| Strong Stage-2 leaf available? | **Yes** — `fat_time_to_bdfe` (DOS pack layout; real `--disk exfat`) |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| Unicode `uni2ansi_char` | AN inverse | Auto-rejected; Cut A already covers encode; no new boundary |
| H + `blit_clip` | geometry pair | Composition glue around Cut H; desktop-only soak |
| S + `set_window_clientbox` | GUI | Desktop-only; global skin injection |
| FAT datetime ×4 | time/date pack/unpack | Inverse siblings ≠ Path A; migrate one leaf |
| Y + `coff_get_align` | PE | Tiny align mask; loader stays FASM |
| ISO `cd_compare_name` | AJ-routed | Already AJ when gate ON |
| EXT write / all-times | AL siblings | Fan-out / `fsGetTime` deps |
| Sockets / MCB / memmove | — | Unchanged blockers |

### Ranked top candidates (post-AN)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `fat_time_to_bdfe` | FAT/exFAT | ~22 | **DOS packed time** | Excellent | Low–med | `--disk exfat` | **SELECT** |
| `window._.set_window_clientbox` | GUI | 3 | clientbox inset | Good | Med | desktop | #2 |
| `blit_clip` | Video | 1 | H composition | Good | Low | desktop | #3 |
| `coff_get_align` | PE | 2 | align mask | Excellent | Low | none | #4 |
| EXT write / CD / sockets / memmove / unicode inverse | — | — | — | — | — | — | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: fat_time_to_bdfe
Source: kernel/fs/fat.inc
Subsystem: FAT/exFAT DOS packed-time → BDFE unpack
Why selected:
    Strongest remaining Stage-2 leaf after AN: new DOS pack layout class,
    ~22 FAT+exFAT callers, excellent exhaustive u16 differential, real
    --disk exfat A/B validation (boot FAT + attached exFAT). Prefer soak
    over blit_clip/clientbox desktop-only peers.
Why this is a genuine migration boundary:
    Clear EAX→EAX bitfield unpack leaf; not a thin wrapper around FASM-owned
    FS state; entry→BDFE conversion is shared by FAT and exFAT.
Why Path A / Path B:
    Path B — one leaf. Pairing with fat_date / bdfe_to_fat_* would not create
    a Rust-owned FAT datetime subsystem (entry orchestration remains FASM).
Rejected alternatives:
    Path A Unicode/ISO/XFS/EXT/exFAT/GUI/PE claims; uni2ansi_char; blit_clip;
    set_window_clientbox; fat_name_is_legal; cd_compare_name; ext_write_time;
    xfs_hashname; sockets; memmove; MCB.
Expected legacy ABI:
    call/ret; EAX FAT time in → EAX BDFE time out; body push/pop ECX+EDX;
    hours<<16 | mins<<8 | (secs_field*2); no calendar clamp.
Expected Rust ABI:
    stdcall rust_fat_time_to_bdfe(fat_time) -> EAX; ret 4;
    trampoline push/pop ECX+EDX (REG-001).
Differential-testing strategy:
    Independent FASM-flow oracle; exhaustive 0..0xFFFF; named vectors;
    high-bit u32; 50k PRNG seed 0x4355544F ('CUTO'); pack round-trip oracle.
ABI-risk assessment:
    Medium (REG-001 class) — mitigated by ECX+EDX trampoline preserve + ABI
    smoke canaries around public call; ESI/EDI preserved by stdcall callee-save.
Required A/B tests:
    Gate OFF vs ON desktop; --disk exfat boot+attach.
Required --disk soak:
    python scripts/run_qemu.py --disk exfat (A/B).
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **ECX+EDX** preserve; `USE_RUST_FAT_TIME_TO_BDFE`
rollback switch.

---

## Out of scope

* Claiming Path A for FAT datetime pack/unpack cluster
* Migrating `fat_date_to_bdfe` / `bdfe_to_fat_time` / `bdfe_to_fat_date`
* Migrating `blit_clip` / GUI clientbox / `uni2ansi_char`
* Beginning Cut AP
