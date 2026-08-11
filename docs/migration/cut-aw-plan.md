# Cut AW Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-aw-implementation.md`](cut-aw-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AW** migrates XFS AG-relative block → absolute sector
> translation — `xfs._.blkrel2sectabs` in `kernel/fs/xfs.asm`.  
> Cuts A–AV remain complete and must not be redone. Do not start Cut AX.

---

## Post-AV migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EBX+ESI** (legacy `uses esi`; EBX live buffer across `read_blocks`); also ECX/EDI/EBP canaries; EDX:EAX are outputs |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still run `--disk xfs` attach A/B |
| Cut D | `strncmp` EDX | N/A — no string helper |
| Cut AV | AHCI free-slot scan | Driver foothold only; do **not** claim AHCI Path A |
| Cut R/W/AM/AP/AK | XFS leaves | Complementary Path B footholds; XFS state remains FASM |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| AV + AHCI wait/IRQ/issue Path A? | **No** — controller/DMA/IRQ/recovery stay FASM; peers are trivial (`sig_known`, endian) or impure wait |
| AC/M/V/AS/AU network Path A? | **No** — fragment writers, TTL free TODO, sockets mutex/alloc still FASM |
| Y + AT + `rebase_coff` Path A? | **No** — loader orchestration stays FASM |
| I + `createMcbEntry` Path A? | **No** — encode ≠ FRS/bitmap ownership; high write blast |
| AQ + paging / X + AR? | **No** — unchanged rejects |
| XFS R+W+AM+AP+AK+blkrel Path A? | **No** — complementary leaves; partition/inode/dir orchestration stays FASM |
| Strongest remaining leaf? | **Yes** — `xfs._.blkrel2sectabs` (AG→absolute-sector address math) |

### Clusters considered and rejected

| Cluster | Why not now |
|---------|-------------|
| AV + `ahci_port_wait` / cmd_wait / IRQ / recovery | HW poll + lifecycle; ownership stays FASM |
| AV + `swap_bytes_in_words` / `ahci_is_sig_known` | Thin / trivial; not ownership |
| AU + free-slot extract / chain / TTL | Incomplete lifecycle; anti-cluster |
| AS + `socket_check_port` / `socket_num_to_ptr` | Mutex + divergent ABIs |
| I + `createMcbEntry` | Encode leaf ≠ NTFS write ownership; FRS blast + DF flip |
| Y + AT + `rebase_coff` | PE anti-cluster after Y+AT |
| `fat_date_to_bdfe` | AO datetime sibling ban |
| Ban-list FS/Unicode / H+blit / thin sysfn | Unchanged rejects |

### Ranked top candidates (post-AV)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `xfs._.blkrel2sectabs` | XFS | 1 (+read fan-out) | **AG→sector address math** | Excellent | Low | `--disk xfs` | **SELECT** |
| `createMcbEntry` | NTFS write | 5 | MCB VLE encode | Hard | **High** | NTFS write | #2 |
| `getInodeLocation` | EXT | 2 | Inode→LBA math | Good | Low | **No EXT harness** | #3 |
| `rebase_coff` | PE / DLL | 1 | DIR32 rebase | Good | Med | Rare rebase | Defer (Y anti-cluster) |
| `fat_date_to_bdfe` | FAT/exFAT | 6 | Date unpack | Excellent | Low | `--disk exfat` | Reject (AO sibling) |
| AHCI wait / endian / sig | AHCI | 1–2 | Thin / impure | Mixed | Med–high | `--bus ahci` | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: xfs._.blkrel2sectabs
Source: kernel/fs/xfs.asm
Subsystem: XFS AG-relative block → absolute sector translation
Stage: Stage-5/FS foothold (address math; XFS ownership stays FASM)
Why selected:
    Post-AV audit: AHCI/network/PE/MCB/memory Path A rejected.
    createMcbEntry remains richest semantic prize but high FRS write blast + DF.
    getInodeLocation diversifies EXT but lacks --disk ext soak harness.
    Strongest remaining leaf is XFS AG→sector address translation: new semantic
    class vs R/W/AM/AP/AK, clean differential domain, real --disk xfs soak via
    read_blocks (every XFS block read).
Why this is a genuine migration boundary:
    Deterministic (edx:eax block, XFS.ag* / sectpblog) → edx:eax absolute sector.
    Distinct from hash/search/unpack/time leaves without claiming FS ownership.
Why Path A / Path B:
    Path B — one pure address-math leaf. Disk I/O, inode, dir orchestration remain FASM.
Regression risks:
    REG-001 on EBX across read_blocks; x86 shift-count &31 quirk; mul uses only
    EAX of AG# (hi discarded); ESI preserve via uses.
CPU/interrupt-state risks:
    None in leaf — no cli/sti; no locks; pure arithmetic.
Shared-state risks:
    Read-only fields from caller-owned XFS object; no globals written.
Concurrency/locking risks:
    None in leaf (FS Lock held by callers outside this leaf).
Required differential tests:
    Independent FASM-flow oracle; agblklog/sectpblog &31; zero block; AG0;
    mask boundaries; mul product hi; 50k PRNG seed 0x43555457 ('CUTW').
Required ABI tests:
    Marker BL2S; fake XFS with ag*/mask/sectpblog; EBX/ECX/ESI/EDI/EBP canaries;
    EDX:EAX sector out.
Required A/B tests:
    Gate OFF vs ON desktop; --disk xfs boot+attach.
Required real subsystem validation:
    python scripts/run_qemu.py --disk xfs (read path reaches blkrel2sectabs).
Rejected alternatives:
    createMcbEntry; getInodeLocation; rebase_coff; AHCI Path A / wait / thin;
    network Path A; PE Path A; fat_date_to_bdfe; AO ban-list.
Expected legacy ABI:
    register call; in EDX:EAX=AG-rel block, EBP→XFS;
    out EDX:EAX=absolute sector; uses esi; clobbers ecx; ret 0.
Expected Rust ABI:
    stdcall rust_xfs_blkrel2sectabs(block_lo, block_hi, agblklog, agblocks,
      mask_lo, mask_hi, sectpblog, out_hi) → EAX=sector_lo; ret 32;
    trampoline extracts XFS fields; writes hi via out_hi.
Differential-testing strategy:
    Independent oracle mirroring FASM shrd/mul/mask/shld; 50k PRNG 'CUTW'.
ABI-risk assessment:
    Medium — omit-FP trampoline with EBP→XFS; REG-001 focus EBX/ESI live across
    read_blocks; shift &31 and mul-EAX-only quirks.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
omit-FP trampoline that materializes XFS fields then calls Rust;
`USE_RUST_XFS_BLKREL2SECTABS` rollback.

---

## Out of scope

* Claiming Path A for XFS / AHCI / PE / network / MCB
* Migrating `createMcbEntry` / `getInodeLocation` / `rebase_coff`
* Beginning Cut AX
