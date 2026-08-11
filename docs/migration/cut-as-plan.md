# Cut AS Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-as-implementation.md`](cut-as-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AS** migrates socket-list membership —
> `socket_check` in `kernel/network/socket.inc`.  
> Cuts A–AR remain complete and must not be redone. Do not start Cut AT.

---

## Post-AR migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EBX+ECX+EDX+ESI+EDI+EBP**; set **ZF** from EAX after Rust return (`test`; `pop` leaves flags) |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| Cut D / AC | Network EDX / table inject | Inject `net_sockets` sentinel; do not claim socket subsystem ownership |
| Cut AR | I/O reservation + cli | Do **not** Path-A with Cut X; TSS/IRQ seed still FASM |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| X + AR as Path A? | **No** — TSS I/O seed / IRQ / PIC still FASM; shared bitmap ≠ ownership |
| AQ + paging peers? | **No** — Stage-4 Path B exhausted; accessors/alloc/fault remain FASM |
| Socket membership Path A? | **No** — list/mutex/UDP-TCP walks stay FASM; one ZF leaf ≠ core |
| Y + `get_coff_sym` / `coff_get_align`? | **No** — loader stays FASM; align trivial |
| Strongest remaining leaf? | **Yes** — `socket_check` (Stage-5 lock-free list ZF membership) |

### Clusters considered and rejected

| Cluster | Why not now |
|---------|-------------|
| X + AR I/O Path A | Incomplete ownership (seed / IRQ / sync) |
| AQ + `v86_get_lin_addr` | Thin accessor; shared `page_tabs` |
| `socket_check` + `socket_num_to_ptr` / `socket_check_port` | Mutex + divergent ABIs; not one coherent Rust API yet |
| Easy `sysfn_get*` | Independent loads; no shared algorithm |
| Ban-list FS/Unicode / H+blit / S+clientbox | Unchanged rejects |
| `createMcbEntry` as same-cut Path A with I | Encode ≠ ownership; high FRS blast |

### Ranked top candidates (post-AR)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `socket_check` | Network / sockets (Stage-5) | 4 live | **Lock-free list ZF membership** | Excellent | Low–med | desktop net | **SELECT** |
| `get_coff_sym` | PE / DLL | 3 | Name→Value scan | Excellent | Low | `.sys` load | #2 |
| `createMcbEntry` | NTFS write | 5 | MCB encode | Hard | **High** | NTFS write | #3 |
| `ipv4_find_fragment_slot` | IPv4 reassembly | 2 | Fragment table | Excellent | Low | Weak | Defer |
| `memmove` | Util | ~24 | Forward copy | Good | **Very high** | Everywhere | Defer |
| `blit_clip` / `is_string_userspace` / `sysfn_get*` | GUI / thin | 1 | Composition / thin | — | Low | — | Reject |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: socket_check
Source: kernel/network/socket.inc
Subsystem: Network / socket list membership
Stage: Stage-5 foothold (after AC/M/V; Stage-2/3/4 Path B exhausted)
Why selected:
    Post-AR audit: Stage-4 leaves exhausted; X+AR Path A rejected; ban-list
    FS/Unicode/GUI peers disfavored. Strongest remaining leaf is Stage-5
    lock-free socket-list membership (ZF-out) with clear ABI, excellent
    differential domain, and production callers on accept/notify/free/local.
Why this is a genuine migration boundary:
    Deterministic singly-linked walk over net_sockets; null reject; EAX+ZF
    dual return; trampoline injects sentinel so blob stays reloc-free.
Why Path A / Path B:
    Path B — one membership leaf. Mutex, insert/remove, port alloc, and
    protocol walks remain FASM; no coherent Rust-owned socket core.
Regression risks:
    REG-001 on ECX/EDX across notify/accept; ZF loss after stdcall;
    empty-list / null / first-hit / miss parity.
CPU/interrupt-state risks:
    None — lock-free read; no cli/sti (unlike socket_check_port).
Shared-state risks:
    Reads net_sockets only; list ownership stays FASM.
Required differential tests:
    Independent FASM-flow oracle; null; empty; hit first/middle/last;
    miss; duplicate first-match; 50k PRNG seed 0x43555453 ('CUTS').
Required ABI tests:
    Marker SCHK; synthetic list + live empty net_sockets; ZF canaries;
    EBX/ECX/EDX/ESI/EDI/EBP preserve.
Required A/B tests:
    Gate OFF vs ON desktop; optional network apps if present.
Required real subsystem validation:
    Prefer socket accept/notify path if stock image exercises it;
    else report NOT AVAILABLE for forced socket soak.
Rejected alternatives:
    X+AR Path A; AQ paging Path A; get_coff_sym; createMcbEntry;
    ipv4_find_fragment_slot; memmove; blit_clip; is_string_userspace;
    sysfn_get*; ban-list FS/Unicode; socket_check_port (mutex).
Expected legacy ABI:
    call/ret; EAX=ptr in; EAX=ptr/0 out; ZF set on miss/null;
    preserves EBX; no cli.
Expected Rust ABI:
    stdcall rust_socket_check(candidate, net_sockets) -> EAX; ret 8;
    trampoline injects net_sockets; preserves regs; test eax,eax for ZF.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **EBX+ECX+EDX+ESI+EDI+EBP** preserve and
`test eax,eax` ZF restore; `USE_RUST_SOCKET_CHECK` rollback.

---

## Out of scope

* Claiming Path A for sockets or I/O-rights
* Migrating `socket_check_port` / `socket_num_to_ptr` / `get_coff_sym`
* Migrating `createMcbEntry` / `memmove` / `blit_clip`
* Beginning Cut AT
