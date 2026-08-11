# Cut AR Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-ar-implementation.md`](cut-ar-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AR** migrates I/O port-area reserve/free —
> `r_f_port_area` in `kernel.asm` (syscall 46 / `ReservePortArea`).  
> Cuts A–AQ remain complete and must not be redone. Do not start Cut AS.

---

## Post-AQ migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **ECX+EDX** (+ EBX/ESI/EDI/EBP) at trampoline; syscall 46 only consumes EAX out |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut; desktop A/B only |
| Cut D / Cut X | EDX/EAX live across IO-bitmap helpers | Compose Cut X helper inside Rust body; do not regress X preserve contract |
| Cut AQ | Stage-4 VA→PA foothold | Do **not** pick another paging leaf / accessor / wrapper |

### Verdict: **Path B — Stage-4 Path A still rejected**

| Question | Finding |
|----------|---------|
| Stage-4 Path A (`get_pg_addr` + peers)? | **No** — remaining memory APIs are TLB/`cli`/alloc/fault or thin accessors; shared `page_tabs` ≠ ownership |
| `v86_get_lin_addr` / `get_phys_addr`? | **Reject** — page-table accessor / AQ wrapper |
| `map_page` / `alloc_page` / fault? | **Reject** — CPU-state / allocator ownership |
| W+AM+hashname / FAT datetime / Unicode / H+blit / Y+align / sockets? | **Unchanged rejects** |
| Strongest remaining leaf? | **Yes** — `r_f_port_area` (Cut X parent; Stage-3 I/O reservation policy) |

### Clusters considered and rejected

| Cluster | Why not now |
|---------|-------------|
| Stage-4 translate Path A | AQ foothold only; no coherent Rust-owned paging |
| `v86_get_lin_addr` + AQ | Shared `page_tabs` read ≠ ownership |
| Easy `sysfn_get*` Path A | Independent global loads; no shared algorithm / fewer crossings |
| X + `r_f_port_area` as Path A *this cut* | X already shipped Path B; this cut is one function |
| `blit_clip` / `coff_get_align` / `is_string_userspace` | Composition / trivial / thin-after-P vs X-parent substance |
| Ban-list FS/Unicode/sockets/memmove | Unchanged |

### Ranked top candidates (post-AQ)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `r_f_port_area` | CPU / I/O ports (Stage-3) | syscall 46 + export (+ optional COM) | **Port reservation policy** | Excellent | Med | syscall 46 N/A forced | **SELECT** |
| `blit_clip` | Video | 1 | H composition | Good | Low | desktop | #2 |
| `is_string_userspace` | Syscall gate | 1 | P + scasb | Excellent | Low | load_lib | #3 (thin) |
| `sysfn_getfreemem` | Stage-3 query | 1 | trivial load | Excellent | Low | desktop | Reject thin |
| `v86_get_lin_addr` | Stage-4 | 15 (v86) | PTE\|offset | Excellent | Low | BIOS only | Reject accessor |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: r_f_port_area
Source: kernel/kernel.asm
Subsystem: CPU / I/O port reservation (syscall 46)
Stage: Stage-3 (Cut X follow-on; Stage-4 Path B exhausted)
Why selected:
    After AQ, Stage-4 leaves are exhausted (TLB/alloc/fault/accessors only).
    Strongest remaining legitimate leaf: Cut X parent — reserve/free port
    ranges, overlap checks, RESERVED_PORTS table, IO-map enable/disable.
    Prefer architectural completion of I/O rights over blit_clip composition
    and is_string_userspace thin-after-P.
Why this is a genuine migration boundary:
    Clear register ABI; deterministic table+bitmap mutation; Cut X bit ops
    reused as pure helper; cli/sti stay in FASM trampoline (reserve only).
Why Path A / Path B:
    Path B — one orchestration leaf. Pairing with already-shipped Cut X does
    not create a new multi-function Path A cut; crossings for set_io_access
    are internalized in the Rust body (inline helper), not a subsystem claim.
Regression risks:
    REG-001 on ECX/EDX; interrupt-window mismatch if cli/sti misplaced;
    overlap / max-255 / free-compact parity; IO-map bit polarity.
CPU-state risks:
    cli/sti on reserve path only (match FASM); no CR0–CR4; IO map via Cut X.
Required differential tests:
    Independent FASM-flow oracle; empty/full table; overlap; max 255;
    free miss/hit/compact; range enable/disable; 50k PRNG seed 0x43555452.
Required ABI tests:
    Marker; canaries; public trampoline reserve+free; restore table/map.
Required A/B tests:
    Gate OFF vs ON desktop; optional --disk xfs attach.
Required real subsystem validation:
    Forced syscall-46 soak NOT AVAILABLE (same class as Cut X).
Rejected alternatives:
    Stage-4 Path A; v86_get_lin_addr; get_phys_addr; map/alloc/fault;
    blit_clip; is_string_userspace; sysfn_get*; coff_get_align;
    FAT/XFS/Unicode ban-list; sockets; memmove.
Expected legacy ABI:
    call/ret; EBX=0 reserve / ≠0 free; ECX=start; EDX=end inclusive;
    EAX=0 ok / 1 err; destroys EAX/EBX/EBP; flags clobbered; reserve uses cli/sti.
Expected Rust ABI:
    stdcall rust_r_f_port_area(op, start, end, reserved_ports, tid, io_map) -> EAX;
    ret 24; trampoline injects globals + tid; cli around reserve only.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **ECX+EDX** (+ EBX/ESI/EDI/EBP) preserve,
`RESERVED_PORTS` / `current_slot.tid` / `tss._io_map_0` injection, and
**cli/sti wrapping the reserve path only**; `USE_RUST_R_F_PORT_AREA` rollback.

---

## Out of scope

* Claiming Path A for paging or I/O-rights subsystem
* Migrating `blit_clip` / `is_string_userspace` / `v86_get_lin_addr`
* Migrating allocator / `map_page` / page-fault paths
* Beginning Cut AS
