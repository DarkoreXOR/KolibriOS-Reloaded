# Cut AQ Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-aq-implementation.md`](cut-aq-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AQ** migrates kernel VA→PA translation —
> `get_pg_addr` in `memory.inc`.  
> Cuts A–AP remain complete and must not be redone. Do not start Cut AR.

---

## Post-AP migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | HD DMA / USB `get_phys_addr` keep **ECX** (and HD keeps **EDX**) live across `call get_pg_addr` — trampoline **must** preserve **ECX+EDX** |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still run `--disk` attach A/B as DMA blast soak |
| Cut D | EDX across `strncmp` | Same family — preserve at trampoline |
| Cut AA | Deferred `get_pg_addr` #2 | Blast + `page_tabs` coupling accepted now that Stage-2 FS leaf novelty is exhausted |

### Verdict: **Path B — no genuine Path A subsystem**

Raised bar unchanged: a Path A cluster must establish a genuine Rust-owned
internal subsystem boundary — not merely shared helpers, inverses, or
same-file proximity.

| Question | Finding |
|----------|---------|
| Enough proven ABI classes? | Yes (A–AP) |
| W+AM+hashname Path A? | **No** — complementary leaves; XFS state remains FASM |
| FAT datetime ×4 Path A? | **No** — AO shipped; siblings ≠ ownership |
| H+`blit_clip` / S+clientbox? | **No** — desktop composition / GUI policy |
| Y+`coff_get_align`? | **No** — tiny align mask; loader stays FASM |
| `get_pg_addr`+Cut P / `v86_get_lin_addr`? | **No** — related memory leaves ≠ Rust-owned paging |
| Socket membership ready? | **No** — blockers unchanged |
| Strong Stage-2/4 leaf available? | **Yes** — `get_pg_addr` (new Stage-4 VA→PA class) |

### Clusters considered and rejected

| Cluster | Members | Why not now |
|---------|---------|-------------|
| FAT `fat_date_to_bdfe` / `bdfe_to_fat_*` | AO siblings | Explicit ban after AO |
| Unicode `uni2ansi_char` | AN inverse | Encode already Cut A |
| XFS hash siblings/callers | after AP | Explicit ban |
| H + `blit_clip` | geometry pair | Composition glue; desktop-only |
| S + `set_window_clientbox` | GUI | Ban list; skin globals |
| Y + `coff_get_align` | PE | Trivial mask after Y |
| `pci_make_config_cmd` / `ahci_is_sig_known` | bus | Algorithmically trivial |
| Sockets / MCB / memmove | — | Unchanged blockers |
| `fat_name_is_legal` / `cd_compare_name` | — | Ban / AJ-routed |
| EXT write / all-times | AL siblings | Fan-out / `fsGetTime` |

### Ranked top candidates (post-AP)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `get_pg_addr` | Memory / Stage-4 | ~15 + export | **VA→PA** | Excellent | Med–high | DMA/`--disk` | **SELECT** |
| `blit_clip` | Video | 1 | H composition | Good | Low | desktop | #2 |
| `coff_get_align` | PE | 2 | align mask | Excellent | Low | none | #3 (thin) |
| `ahci_is_sig_known` / `pci_make_config_cmd` | bus | 2 | thin ZF/PCI | Excellent | Low | boot | #4 (trivial) |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: get_pg_addr
Source: kernel/core/memory.inc
Subsystem: Memory — kernel linear address → physical page
Why selected:
    Strongest remaining legitimate leaf after AP: first Stage-4 VA→PA
    foothold, new semantic class (not hash/datetime/unicode/XFS), ~15
    live callers across USB/AHCI/IDE DMA + taskman + PE export GetPgAddr.
    Prefer architectural value over blit_clip desktop-only composition
    and coff_get_align trivial PE bitfield.
Why this is a genuine migration boundary:
    Clear register ABI (EAX in → EAX out); pure translate leaf; no locks;
    page_tabs/OS_BASE injected by trampoline (reloc-free, Cut AA pattern).
Why Path A / Path B:
    Path B — one translation leaf. Pairing with Cut P / v86_get_lin_addr
    would not create Rust-owned paging (page_tabs / alloc / faults stay FASM).
Rejected alternatives:
    Path A claims; FAT datetime siblings; uni2ansi_char; XFS hash peers;
    blit_clip; set_window_clientbox; coff_get_align; fat_name_is_legal;
    cd_compare_name; ext_write_time; sockets; memmove; MCB; pci/ahci thin.
Expected legacy ABI:
    call/ret; EAX=linear → EAX=phys page-aligned; ECX/EDX untouched by body;
    EBX/ESI/EDI/EBP untouched; flags clobbered.
Expected Rust ABI:
    stdcall rust_get_pg_addr(linear, page_tabs, os_base) -> EAX; ret 12;
    trampoline injects page_tabs+OS_BASE; push/pop ECX+EDX (+ callee-saved).
Differential-testing strategy:
    Independent FASM-flow oracle; low-path exhaustive page offsets;
    high-path PTE fixtures; wrap-under-OS_BASE; 50k PRNG seed 0x43555451.
ABI-risk assessment:
    High (REG-001 class on DMA) — mitigated by ECX+EDX trampoline preserve +
    ABI smoke canaries matching HD/USB live register patterns.
Required A/B tests:
    Gate OFF vs ON desktop; --disk xfs (or other) boot+attach DMA soak.
Required --disk soak:
    python scripts/run_qemu.py --disk xfs (A/B attach); scripted browse N/A.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **ECX+EDX** (+ EBX/ESI/EDI/EBP) preserve and
`page_tabs`/`OS_BASE` injection; `USE_RUST_GET_PG_ADDR` rollback switch.

---

## Out of scope

* Claiming Path A for memory/paging subsystem
* Migrating `blit_clip` / GUI clientbox / `coff_get_align`
* Migrating `memmove` / `v86_get_lin_addr`
* Beginning Cut AR
