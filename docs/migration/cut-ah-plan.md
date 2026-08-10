# Cut AH Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ah-implementation.md`](cut-ah-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AH** migrates exFAT directory-entry SetChecksum —
> `calculate_SetChecksum_field` in `exfat.inc`.  
> Cuts A–AG remain complete and must not be redone. Do not start Cut AI.

---

## Post-AG migration audit (cluster readiness)

### Verdict: **Path B — no real Rust-owned subsystem cluster yet**

Raised bar (post-AF/AG): a cluster must establish a genuine Rust-owned internal
boundary (shared state/impl, fewer FASM↔Rust crossings, coherent test/rollback)
— not merely inverses, calendar siblings, or same-file proximity.

Live-tree re-audit after AG confirmed the prior near-miss table still holds;
no new multi-function group crossed the bar.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AG) |
| Enough build/diff/trampoline infra? | Yes |
| Subsystem with Rust-owned internals ready? | **No** — leaves + orchestration / Stage-4–5 |
| Would a cluster reduce repeated work? | **Not enough** under the raised bar |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| Socket membership | `socket_check` + `socket_num_to_ptr` (+ dead `ptr_to_num`) | Shared *walk helper* possible, but `net_sockets`/`socket_mutex` stay FASM-owned; lock-free vs mutex ABIs incompatible; inline walks elsewhere; result = two leaves + helper, not Rust-owned core |
| NTFS FILETIME / mount | AE+AF+AG | No shared internal calls between validate and FILETIME |
| MCB encode+decode | `createMcbEntry` + I | Encode mutates FRS; high blast |
| Disk after Z+AD | `disk_scan_*` / GPT | I/O + alloc orchestration |
| FAT DOS datetime ×4 | `fat_{time,date}_to_bdfe` ± inverses | Calendar siblings; low novelty vs G/T/AE/AF |
| XFS hash siblings | `xfs_hashname` / linear scan | Thin after R+W |
| Unicode map+string | `uni2ansi_*` / `ansi2uni_*` | Anti-cluster post-AB |
| IPv4 helpers | `net_ptr_to_num4` | Already inlined in Rust `ipv4_route`; anti-cluster |
| Taskman after AA | getters / dead `pid_to_appdata` | Empty |
| TCP after V | cancel/outflags/mss | No small shared core |
| exFAT SetChecksum + NameHash | checksum + inlined `exFAT_hash_calculate` | NameHash is **not** a labeled callable yet — future Path A *after* this leaf lands a reusable rolling core |

### Socket near-miss (re-verified live)

| Symbol | Live callers | Notes |
|--------|-------------:|-------|
| `socket_check` | 4 | Lock-free list membership; ZF-out |
| `socket_num_to_ptr` | 10 | Mutex-wrapped walk by `SOCKET.Number` |
| `socket_ptr_to_num` | **0** | Dead; only caller of `socket_check` among the pair |
| `socket_check_owner` | **0** | Dead |

Blockers unchanged: FASM list/mutex ownership, asymmetric locking, dead third
member, incomplete walk coverage (UDP/ICMP/TCP inline their own walks).

### Proven classes (A–AG)

Scalar/CRC/Unicode/stream, net checksum/timers/routing, calendar + NTFS
FILETIME↔BDFE, bootsec CF validate, clip, NTFS MCB/USA / FAT 8.3 / XFS
unpack+hash, HID, font AA, app header, userspace ZF, GUI screen-fit, TSS I/O,
PE/COFF reloc, MBR CF validate, GPT protective ZF, process TID walk.

---

## Selected target (Path B)

| Field | Value |
|-------|-------|
| **Function** | `calculate_SetChecksum_field` |
| **Source** | [`kernel/fs/exfat.inc:2333+`](../../kernel/fs/exfat.inc) |
| **Subsystem** | exFAT / directory-entry SetChecksum |
| **Purpose** | Rolling 16-bit checksum over File+Stream+Name entries; skip indices 2–3; store AX at entry+2 |

### Ranked top candidates (post-AG)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-------|---------|
| `calculate_SetChecksum_field` | exFAT | 1 | **exFAT skip-index rolling checksum** | Excellent | Low | **SELECT** |
| `socket_check` | Network | 4 | Socket-list ZF | Good | Low–med | Defer (#2) |
| FAT datetime quartet | FAT/exFAT | ~22 | DOS packed time | Excellent | Low | Reject (low novelty) |
| `xfs_hashname` | XFS | 4 | Thin ROL7 | Excellent | Low | Reject (thin) |
| `is_string_userspace` | Memory | 1 | P-family | Excellent | Low | Reject (thin) |
| `createMcbEntry` | NTFS | 5 | MCB encode | Hard | High | Defer |
| `memmove` | Memory | ~24 | Stage-4 | — | High | Defer Stage-4 |

```text
Selected target:
    calculate_SetChecksum_field

Source:
    kernel/fs/exfat.inc

Subsystem:
    exFAT directory SetChecksum

Why selected:
    Standing deferred #3 after AG took NTFS bootsec; genuinely new algorithm
    class (exFAT rolling checksum with SetChecksum field skip — distinct from
    CRC32 and IP checksum_1/2); first exFAT Stage-2 foothold; unlocks a future
    NameHash Path A once the inlined exFAT_hash_calculate shares the same
    Rust rolling core; reloc-free via (buf,len) trampoline; excellent
    synthetic differential; low blast (single write-path caller).

Why #2 was rejected:
    socket_check — useful Stage-5 membership foothold; less algorithm novelty;
    cluster blockers (mutex/list ownership) unchanged after live re-audit.

Why cluster Path A was rejected:
    No multi-function group meets the raised Rust-owned-subsystem bar.
    Closest near-miss (socket membership) still collapses on FASM-owned
    net_sockets/mutex + lock asymmetry + dead wrappers.
    exFAT SetChecksum+NameHash is the strongest *future* Path A, but NameHash
    is not yet a separate callable — migrating SetChecksum first plants the
    shared core without forcing a false cluster.

Legacy ABI:
    call / ret
    in:  EBP → exFAT
         (internal) ESI = &file_dir_entry
         (internal) ECX = fname_extdir_offset − ESI (byte length)
    out: AX = checksum; [file_dir_entry+2] = AX
    preserves: EBX, ECX, EDX, ESI, EDI (push/pop); EBP
    quirks: skip absolute indices 2 and 3 (SetChecksum field);
            16-bit wrap; FASM do-while hangs if ECX==0 (callers use len≥32)

Critical invariants:
    Exact ((c&1)?0x8000:0)+(c>>1)+byte per non-skipped index
    Store to entry+2 before return
    Caller also copies AX to [EDI+2] (unchanged)

Rust strategy:
    Freestanding calculate_set_checksum_field(+_ptr) → u16;
    internal exfat_rolling_checksum(skip_2_3) for future NameHash;
    reloc-free

Trampoline strategy:
    lea buf; len = fname_extdir_offset−buf;
    stdcall rust_*(buf,len); AX returned; Rust writes [buf+2]

Differential strategy:
    Independent FASM-flow oracle (duplicated control flow)
    Named skip-index / empty-name / full-name / wrap vectors
    50k PRNG seed 0x43555448 ('CUTH')

ABI smoke strategy:
    Synthetic exFAT-shaped buffer + length; public calculate_SetChecksum_field
    AX + [buf+2] checks; EBX/ECX/EDX/ESI/EDI/EBP canaries

QEMU strategy:
    CoW OFF then ON from cut-ag-final.img lineage; desktop regression;
    exFAT write soak only if stock image evidences exFAT SetChecksum path

Rollback gate:
    USE_RUST_CALCULATE_SET_CHECKSUM_FIELD = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
EBP→(buf,len) trampoline; `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD` rollback switch.

---

## Out of scope

* Migrating `socket_check` / `memmove` / FAT datetime / `xfs_hashname` /
  `createMcbEntry` / extracting NameHash as a public symbol
* Beginning Cut AI
* Changing SetChecksum skip-index quirks
* Rewriting exFAT write orchestration
