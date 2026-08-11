# Cut AU Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-au-implementation.md`](cut-au-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AU** migrates IPv4 reassembly fragment-slot lookup —
> `ipv4_find_fragment_slot` in `kernel/network/IPv4.inc`.  
> Cuts A–AT remain complete and must not be redone. Do not start Cut AV.

---

## Post-AT migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EAX+EBX+ECX+EDX** (legacy push/pop); callers keep EDX→packet / EBX→device across the call; smoke also canaries EDI/EBP |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| Cut D | `strncmp` EDX | N/A — no string helper |
| Cut Y / AT | PE reloc + symbol lookup | Path A with Y/AT/`rebase_coff` rejected — loader stays FASM |
| Cut AS / AC | Socket list / IPv4 route | Stage-5 footholds only; do **not** claim reassembly ownership |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| Y + AT + `rebase_coff` Path A? | **No** — loader orchestration stays FASM; shared buffers ≠ ownership |
| I + `createMcbEntry` Path A? | **No** — encode ≠ FRS/bitmap ownership; high write blast |
| AS + socket lifecycle Path A? | **No** — mutex / insert / remove still FASM |
| Strongest remaining leaf? | **Yes** — `ipv4_find_fragment_slot` (Stage-5 reassembly slot scan) |

### Clusters considered and rejected

| Cluster | Why not now |
|---------|-------------|
| Y + AT + `rebase_coff` / `coff_get_align` | Loader remains FASM; PE anti-cluster after AT |
| I + `createMcbEntry` | Encode leaf ≠ NTFS write ownership; FRS blast |
| AS + `socket_check_port` / `socket_num_to_ptr` | Mutex + divergent ABIs; artificial AS extension |
| Ban-list FS/Unicode / H+blit / thin sysfn | Unchanged rejects |

### Ranked top candidates (post-AT)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `ipv4_find_fragment_slot` | IPv4 reassembly | 2 | **Fragment table keyed scan** | Excellent | Low | Weak (frag traffic) | **SELECT** |
| `createMcbEntry` | NTFS write | 5 | MCB encode | Hard | **High** | NTFS write | #2 |
| `ahci_find_cmdslot` | AHCI | 2 | Free-slot bit scan | Good | Med | AHCI disk | #3 |
| `rebase_coff` | PE / DLL | 1 | DIR32 rebase | Good | Med | Rare rebase | Defer (Y anti-cluster) |
| `memmove` / `blit_clip` / thin peers | Util / GUI | — | Forward copy / composition | — | High / Low | — | Defer / reject |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: ipv4_find_fragment_slot
Source: kernel/network/IPv4.inc
Subsystem: IPv4 fragment reassembly slot lookup
Stage: Stage-5 foothold (complements AC/M/V/AS; reassembly ownership stays FASM)
Why selected:
    Post-AT audit: PE Path A rejected; createMcbEntry remains high FRS write blast;
    rebase_coff is a Y mutate anti-cluster. Strongest remaining leaf is Stage-5
    IPv4 reassembly keyed table scan: new semantic class, read-only, excellent
    differential domain, 2 production callers, low blast.
Why this is a genuine migration boundary:
    Deterministic linear walk of IPv4_fragments[64] matching Identification +
    SrcIP + DstIP; returns slot pointer or -1. Distinct from route/timer/socket_check
    without claiming fragment chain / TTL / rebuild ownership.
Why Path A / Path B:
    Path B — one lookup leaf. First-fragment free-slot fill, chain link, rebuild,
    TTL sweep remain FASM.
Regression risks:
    REG-001 on EDX (packet) / EBX (device) across ipv4_input; first-match vs miss;
    empty-slot id=0/IP=0 false-hit quirk retained; stride/offsets of FRAGMENT_slot.
CPU/interrupt-state risks:
    None — pure read walk; no cli/sti; no locks.
Shared-state risks:
    Read-only walk of global IPv4_fragments; writers are FASM callers under
    network input path.
Concurrency/locking risks:
    None in leaf (same as legacy).
Required differential tests:
    Independent FASM-flow oracle; empty/miss/hit first/mid/last; duplicate keys
    first-match; zeroed empty-slot quirk; 50k PRNG seed 0x43555455 ('CUTU').
Required ABI tests:
    Marker FRAG; synthetic header + table; EAX/EBX/ECX/EDX/EDI/EBP canaries;
    ESI = slot|-1.
Required A/B tests:
    Gate OFF vs ON desktop.
Required real subsystem validation:
    Prefer real fragmented IPv4 traffic if available; else report NOT AVAILABLE.
Rejected alternatives:
    Y+AT+rebase_coff Path A; createMcbEntry; rebase_coff; ahci_find_cmdslot;
    memmove; blit_clip; socket Path A; ban-list FS/Unicode.
Expected legacy ABI:
    call ipv4_find_fragment_slot; EDX→IPv4_header;
    out ESI=slot|-1; preserves EAX/EBX/ECX/EDX via push/pop; plain ret.
Expected Rust ABI:
    stdcall rust_ipv4_find_fragment_slot(packet, fragments, count) → EAX=slot|-1;
    ret 12; trampoline injects IPv4_fragments + IPv4_MAX_FRAGMENTS; mov esi,eax;
    preserves EAX/EBX/ECX/EDX (+ EDI/EBP canaries).
Differential-testing strategy:
    Independent oracle mirroring FASM id/SrcIP/DstIP walk; synthetic 16-byte
    slots + minimal IPv4 headers; 50k PRNG 'CUTU'.
ABI-risk assessment:
    Medium-low — register ABI → stdcall with injected globals (AS/AC pattern);
    REG-001 focus is EDX packet pointer live across call.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **EAX+EBX+ECX+EDX** preserve and ESI return;
`USE_RUST_IPV4_FIND_FRAGMENT_SLOT` rollback.

---

## Out of scope

* Claiming Path A for IPv4 reassembly or PE loader
* Migrating `createMcbEntry` / `rebase_coff` / first-fragment free-slot fill
* Beginning Cut AV
