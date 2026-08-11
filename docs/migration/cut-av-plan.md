# Cut AV Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-av-implementation.md`](cut-av-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AV** migrates AHCI free command-list slot lookup —
> `ahci_find_cmdslot` in `kernel/blkdev/ahci.inc`.  
> Cuts A–AU remain complete and must not be redone. Do not start Cut AW.

---

## Post-AU migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EBX+ECX+EDX+ESI** (legacy push/pop); callers keep ESI→`PORT_DATA` across the call inside `pushad`; smoke also canaries EDI/EBP |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| Cut D | `strncmp` EDX | N/A — no string helper |
| Cut AU | IPv4 fragment-slot scan | Stage-5 foothold only; free-slot fill / chain / rebuild remain FASM; do **not** claim reassembly Path A |
| Cut Y / AT | PE reloc + symbol lookup | Path A with Y/AT/`rebase_coff` still rejected |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| AC/M/V/AS/AU network Path A? | **No** — fragment writers, TTL free TODO, sockets mutex/alloc, ARP still FASM |
| AU + free-slot extract Path A? | **No** — complementary thin leaf ≠ reassembly ownership |
| I + `createMcbEntry` Path A? | **No** — encode ≠ FRS/bitmap ownership; high write blast |
| Y + AT + `rebase_coff` Path A? | **No** — loader orchestration stays FASM |
| Strongest remaining leaf? | **Yes** — `ahci_find_cmdslot` (first AHCI/driver free-slot bit scan) |

### Clusters considered and rejected

| Cluster | Why not now |
|---------|-------------|
| AU + `.find_free_slot` / chain / rebuild / TTL | Incomplete lifecycle; rebuild dropped; TTL free TODO; anti-cluster |
| AS + `socket_check_port` / `socket_num_to_ptr` | Mutex + divergent ABIs |
| I + `createMcbEntry` | Encode leaf ≠ NTFS write ownership; FRS blast |
| Y + AT + `rebase_coff` / `coff_get_align` | PE anti-cluster after Y+AT |
| Ban-list FS/Unicode / H+blit / thin sysfn | Unchanged rejects |

### Ranked top candidates (post-AU)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `ahci_find_cmdslot` | AHCI / blkdev | 2 | **HW free-slot bit scan** | Excellent | Med | `--bus ahci` | **SELECT** |
| `createMcbEntry` | NTFS write | 5 | MCB encode | Hard | **High** | NTFS write | #2 |
| `getInodeLocation` | EXT | 2 | Inode→LBA math | Good | Low | Weak EXT harness | #3 |
| `xfs._.blkrel2sectabs` | XFS | — | AG→sector | Good | Low | `--disk xfs` | Defer |
| `rebase_coff` | PE / DLL | 1 | DIR32 rebase | Good | Med | Rare rebase | Defer (Y anti-cluster) |
| free-slot extract / `net_ptr_to_num4` | IPv4 / stack | — | AU sibling / thin walk | — | Low | Weak | Reject / defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: ahci_find_cmdslot
Source: kernel/blkdev/ahci.inc
Subsystem: AHCI free command-list slot lookup
Stage: Stage-5/driver foothold (first AHCI leaf; controller ownership stays FASM)
Why selected:
    Post-AU audit: network Path A rejected; createMcbEntry remains high FRS write
    blast; rebase_coff is a Y mutate anti-cluster. Strongest remaining leaf is the
    AHCI free-slot bit scan: new driver semantic class, clean stdcall ABI, excellent
    differential domain (synthetic SACT|CI + CAP.NCS), 2 production callers, real
    --bus ahci soak available.
Why this is a genuine migration boundary:
    Deterministic scan of (SACT|CI) bits bounded by CAP.NCS&0xf quirk; returns slot
    index or -1. Distinct from AU keyed fragment walk without claiming AHCI command
    issue / DMA / IRQ ownership.
Why Path A / Path B:
    Path B — one read-only lookup leaf. Command table fill, CI issue, wait, and
    recovery remain FASM.
Regression risks:
    REG-001 on registers across identify/rw; NCS &0xf / exclusive-bound quirk;
    MMIO race same as legacy (SACT|CI can change underfoot).
CPU/interrupt-state risks:
    None in leaf — no cli/sti; no locks; read-only MMIO.
Shared-state risks:
    Read-only MMIO of port SACT/CI and HBA CAP; writers are FASM command issue.
Concurrency/locking risks:
    None in leaf (same as legacy); port mutex is outside this leaf.
Required differential tests:
    Independent FASM-flow oracle; empty/first/last free; all occupied; ncs=0;
    &0xf quirk; 50k PRNG seed 0x43555456 ('CUTV').
Required ABI tests:
    Marker SLOT; synthetic PORT_DATA→HBA_PORT→AHCI_CTR→HBA_MEM; EBX/ECX/EDX/ESI/EDI/EBP
    canaries; EAX = slot|-1.
Required A/B tests:
    Gate OFF vs ON desktop; AHCI disk I/O when available.
Required real subsystem validation:
    python scripts/run_qemu.py --disk <fs> --bus ahci (identify + rw path).
Rejected alternatives:
    createMcbEntry; rebase_coff; AU free-slot extract; getInodeLocation (rank #3);
    network Path A; PE Path A; MCB Path A.
Expected legacy ABI:
    stdcall ahci_find_cmdslot(pdata) → EAX=slot|-1;
    preserves EBX/ECX/EDX/ESI via push/pop; ret 4.
Expected Rust ABI:
    stdcall rust_ahci_find_cmdslot(slots, ncs) → EAX=slot|-1; ret 8;
    trampoline reads SACT|CI and (CAP>>8)&0xf from pdata chain.
Differential-testing strategy:
    Independent oracle mirroring FASM bt/shr loop + jae bound; 50k PRNG 'CUTV'.
ABI-risk assessment:
    Low–medium — stdcall→stdcall with MMIO extract in trampoline; REG-001 focus is
    preserved callee-saved set matching legacy.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall trampoline that materializes `(SACT|CI, NCS&0xf)` then calls Rust;
`USE_RUST_AHCI_FIND_CMDSLOT` rollback.

---

## Out of scope

* Claiming Path A for AHCI / blkdev / DMA
* Migrating `createMcbEntry` / `rebase_coff` / fragment free-slot fill
* Beginning Cut AW
