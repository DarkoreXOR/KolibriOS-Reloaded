# Cut AT Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-at-implementation.md`](cut-at-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AT** migrates PE/COFF symbol name→Value lookup —
> `get_coff_sym` in `kernel/core/dll.inc`.  
> Cuts A–AS remain complete and must not be redone. Do not start Cut AU.

---

## Post-AS migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EBX+ESI+EDI+EBP** (and ECX/EDX canaries in smoke); callers keep `ESI→DLLDESCR` / `EBX→coff` across `get_coff_sym` |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| Cut D | `strncmp` EDX | Inline 8-byte name compare in reloc-free blob (do not call `rust_strncmp` — would relocate) |
| Cut Y | PE reloc applicator | Same `dll.inc` neighborhood; Path A with Y rejected — loader stays FASM |
| Cut AS | Socket list ZF | Stage-5 foothold only; do **not** extend socket cluster artificially |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| Socket lifecycle Path A after AS? | **No** — mutex / insert / remove / port alloc still FASM |
| Y + `get_coff_sym` / `coff_get_align` / `rebase_coff` Path A? | **No** — loader orchestration stays FASM; align trivial; shared buffers ≠ ownership |
| I + `createMcbEntry` Path A? | **No** — encode ≠ FRS/bitmap ownership; high write blast |
| AQ + paging / X + AR? | **No** — unchanged rejects |
| Strongest remaining leaf? | **Yes** — `get_coff_sym` (Stage-8 PE name→Value scan) |

### Clusters considered and rejected

| Cluster | Why not now |
|---------|-------------|
| `socket_check` + `socket_check_port` / `socket_num_to_ptr` | Mutex + divergent ABIs; artificial AS extension |
| Y + `get_coff_sym` + `rebase_coff` | Loader remains FASM; no Rust-owned PE subsystem |
| I + `createMcbEntry` | Encode leaf ≠ NTFS write ownership; FRS blast |
| Ban-list FS/Unicode / H+blit / thin sysfn | Unchanged rejects |

### Ranked top candidates (post-AS)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `get_coff_sym` | PE / DLL (Stage-8) | 3 | **Name→Value symbol scan** | Excellent | Low | `.sys`/DLL load | **SELECT** |
| `createMcbEntry` | NTFS write | 5 | MCB encode | Hard | **High** | NTFS write | #2 |
| `rebase_coff` | PE / DLL | 1 | DIR32 rebase walk | Good | Med | DLL load | Defer (Y anti-cluster) |
| `ipv4_find_fragment_slot` | IPv4 reassembly | 2 | Fragment table | Excellent | Low | Weak | Defer |
| `memmove` | Util | ~24 | Forward copy | Good | **Very high** | Everywhere | Defer |
| `blit_clip` / `is_string_userspace` / FS ban-list | GUI / thin / FS | — | Composition / thin | — | — | — | Reject |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: get_coff_sym
Source: kernel/core/dll.inc
Subsystem: PE/COFF symbol lookup
Stage: Stage-8 foothold (after Y; Stages 2–5 Path B exhausted or footholded)
Why selected:
    Post-AS audit: socket Path A rejected; extending socket cluster is artificial;
    Stage-4 leaves exhausted; ban-list FS/Unicode/GUI peers disfavored.
    Strongest remaining leaf is Stage-8 PE name→Value scan with clear stdcall
    ABI, excellent differential domain, 3 production load-path callers, low blast.
    Preferred over createMcbEntry (high FRS write blast) and rebase_coff (Y mutate
    anti-cluster).
Why this is a genuine migration boundary:
    Deterministic linear walk of COFF_SYM table; 8-byte strncmp-equivalent name
    match; returns Value dword or 0. Distinct semantic class from Cut Y reloc
    patch without claiming PE loader ownership.
Why Path A / Path B:
    Path B — one symbol-lookup leaf. load_library / ext_lib / rebase / exports
    wiring remain FASM; Y+get_coff_sym do not form a Rust-owned loader.
Regression risks:
    REG-001 on ESI/EBX across load_library; first-match vs miss; 8-byte name
    edge (NUL stop vs full 8); Value extract offset; count=0 FASM wrap quirk.
CPU/interrupt-state risks:
    None — pure read walk; no cli/sti.
Shared-state risks:
    Read-only walk of caller-owned symbol table; no globals.
Concurrency/locking risks:
    None in leaf (loader single-threaded context).
Required differential tests:
    Independent FASM-flow oracle; empty/miss/hit first/mid/last; EXPORTS/_EXPORTS
    style names; 8-byte exact; NUL early stop; count=1 edge; 50k PRNG seed
    0x43555454 ('CUTT'). Exclude or specially bound count=0 infinite-walk quirk.
Required ABI tests:
    Marker GCSY; synthetic COFF_SYM table; stdcall ret 12; EBX/ECX/EDX/ESI/EDI/EBP
    canaries on public trampoline.
Required A/B tests:
    Gate OFF vs ON desktop; driver/DLL load if stock image exercises load_library.
Required real subsystem validation:
    Prefer `.sys` / DLL load path if stock image loads drivers;
    else report NOT AVAILABLE for forced COFF-symbol soak.
Rejected alternatives:
    Socket Path A / socket_check_port; Y+get_coff_sym Path A; createMcbEntry;
    rebase_coff; ipv4_find_fragment_slot; memmove; blit_clip; is_string_userspace;
    ban-list FS/Unicode; AQ/X+AR Path A.
Expected legacy ABI:
    stdcall get_coff_sym(pSym, count, sz_sym) → EAX=Value|0; ret 12;
    no uses (does not touch EBX/ESI/EDI/EBP); may clobber ECX/EDX via strncmp.
Expected Rust ABI:
    stdcall rust_get_coff_sym(pSym, count, sz_sym) → EAX; ret 12;
    trampoline preserves EBX/ESI/EDI/EBP (and smoke-checks ECX/EDX).
Differential-testing strategy:
    Independent oracle mirroring FASM do-while + strncmp(n=8) + Value@+8;
    synthetic 18-byte symbol records; 50k PRNG 'CUTT'.
ABI-risk assessment:
    Medium-low — stdcall already matches; REG-001 focus is ESI/EBX live across
    load_library; inline name compare avoids Cut D reloc dependency.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall trampoline with **EBX+ESI+EDI+EBP** preserve; `USE_RUST_GET_COFF_SYM`
rollback.

---

## Out of scope

* Claiming Path A for PE/COFF loader or sockets
* Migrating `rebase_coff` / `coff_get_align` / `createMcbEntry`
* Beginning Cut AU
