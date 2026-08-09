# Cut AF Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-af-implementation.md`](cut-af-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AF** migrates NTFS BDFE → FILETIME conversion —
> `ntfsCalculateTime` in `ntfs.inc`.  
> Cuts A–AE remain complete and must not be redone. Do not start Cut AG.

---

## Post-AE migration audit (cluster readiness)

### Verdict: **Path B — cluster migration remains premature**

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AE) |
| Enough build/diff/trampoline infra? | Yes |
| Subsystem with Rust-owned internals ready? | **No** — NTFS leaves (MCB/USA/datetime) + disk Z+AD; rest is orchestration |
| Would a cluster reduce repeated work? | **Not enough** |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| NTFS epoch twin as same-cut cluster | AE + `ntfsCalculateTime` | AE already shipped; twin is sequential Path B |
| NTFS GetTime+CalculateTime | shared ×10⁷+bias tail | One logical algorithm / two labels → still Path B (GetTime CMOS stays FASM) |
| NTFS mount+time | `ntfs_test_bootsec` + CalculateTime | Unrelated ABIs; no shared boundary |
| Disk after Z+AD | `disk_scan_*` / GPT orchestration | I/O + alloc; not Stage-2 leaves |
| FAT DOS datetime ×4 | `fat_{time,date}_to_bdfe` ± inverses | Coherent but **low novelty** vs G/T |
| XFS hash siblings | `xfs_hashname` / `get_before_by_hashval` | Thin / weak after R+W |
| Unicode map+string | `uni2ansi_*` / `ansi2uni_*` | Anti-cluster post-AB |
| IPv4 helpers | `net_ptr_to_num4` | Anti-cluster post-AC |
| Taskman after AA | getters / dead `pid_to_appdata` | Empty |

### Proven classes (A–AE)

Scalar/CRC/Unicode/stream, net checksum/timers/routing, calendar pair + NTFS FILETIME→BDFE, clip, NTFS MCB/USA / FAT 8.3 / XFS unpack+hash, HID, font AA, app header, userspace ZF, GUI screen-fit, TSS I/O, PE/COFF reloc, MBR CF validate, GPT protective ZF, process TID walk.

---

## Selected target (Path B)

| Field | Value |
|-------|-------|
| **Function** | `ntfsCalculateTime` |
| **Source** | [`kernel/fs/ntfs.inc:4158+`](../../kernel/fs/ntfs.inc) |
| **Subsystem** | NTFS / BDFE → FILETIME |
| **Purpose** | Convert BDFE datetime to 1601×10⁷ FILETIME; inverse of Cut AE |

### Ranked top candidates (post-AE)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `ntfsCalculateTime` | NTFS time | 3 (+GetTime shares scale) | **AE inverse; compose G** | Excellent | Low | **SELECT** |
| `ntfs_test_bootsec` | NTFS mount | 2 | FS bootsec+CF | Excellent | Low | Defer (#2) |
| `socket_check` | Network | 5 | Socket-list ZF | Good | Low–med | Defer (#3) |
| `calculate_SetChecksum_field` | exFAT | 1 | Rolling checksum skip-index | Good | Low | Defer |
| FAT datetime quartet | FAT/exFAT | many | DOS packed time | Excellent | Low | Reject (low novelty) |
| `xfs_hashname` | XFS | 4 | Thin ROL7 | Excellent | Low | Reject (thin) |
| `is_string_userspace` | Memory | 1 | P-family | Excellent | Low | Reject (thin) |
| `memmove` | Memory | 24 | Stage-4 | — | High | Defer Stage-4 |

```text
Selected target:
    ntfsCalculateTime

Source:
    kernel/fs/ntfs.inc

Subsystem:
    NTFS BDFE → FILETIME calendar

Why selected:
    Completes the NTFS FILETIME pair after AE; composes Cut G
    (fsCalculateTime) + AE bias constants; 3 live SetFileInfo callers;
    reloc-free; excellent synthetic differential; low blast.
    GetTime keeps FASM ×10⁷+bias after fsGetTime (CMOS path).

Why #2 was rejected:
    ntfs_test_bootsec — strong CF bootsec; deferred so AF finishes the
    epoch twin AE named as the natural follow-up.

Why #3 was rejected:
    socket_check — useful Stage-5 membership foothold; less narrative
    leverage than completing the FILETIME pair.

Why cluster Path A was rejected:
    No multi-function cluster eliminates duplicated FASM↔Rust boundaries
    with controlled blast better than this single leaf. AE+AF as a forced
    same-cut cluster is moot (AE already production).

Legacy ABI:
    call / ret
    in:  ESI → BDFE datetime block
    out: EDX:EAX = FILETIME (100ns since 1601-01-01)
    clobbers: EAX, EBX, ECX, EDX (via fsCalculateTime + mul)
    preserves: ESI, EDI, EBP (stdcall / SetFileInfo ESI advances)
    quirks: mov edx,10000000; mul edx; add/adc bias 3365781504/29389701;
            year clamp via fsCalculateTime; wrapping mul/add

Critical invariants:
    Exact mul then bias add/adc order
    Compose fsCalculateTime seconds semantics
    ESI not advanced by the leaf
    EDX:EAX dual-word FILETIME return

Rust strategy:
    Freestanding ntfs_calculate_time(+_ptr) → inlines fs_calculate_time
    + filetime_from_secs_2001 (reloc-free; no cross-section call)
    FFI returns u64 in EDX:EAX

Trampoline strategy:
    stdcall rust_*(esi); ret   ; EDX:EAX from Rust u64

Differential strategy:
    Independent FASM-flow oracle (G oracle + mul/add/adc)
    Named epoch / leap / end-of-day / pre-2001 clamp vectors
    AF→AE round-trip; 50k PRNG seed 0x43555446 ('CUTF')

ABI smoke strategy:
    Synthetic BDFE → public ntfsCalculateTime
    ESI preserve + chaining; EDX:EAX vs FASM mul path

QEMU strategy:
    CoW OFF then ON from cut-ae-final.img lineage; desktop regression;
    NTFS SetFileInfo soak only if stock image evidences NTFS path

Rollback gate:
    USE_RUST_NTFS_CALCULATE_TIME = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register→stdcall trampoline; `USE_RUST_NTFS_CALCULATE_TIME` rollback switch.

---

## Out of scope

* Migrating `ntfsGetTime` CMOS path / `ntfs_test_bootsec` / `socket_check` /
  `memmove` / FAT datetime quartet / XFS hashname
* Beginning Cut AG
* Changing FILETIME bias / mul quirks
* Rewriting NTFS SetFileInfo orchestration
