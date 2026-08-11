# Cut BL Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bl-implementation.md`](cut-bl-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BL** migrates V86 address → linear translate —
> `v86_get_lin_addr` in `kernel/core/v86.inc`.  
> Cuts A–BK remain complete and must not be redone. Do not start Cut BM.

---

## Post-BK migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Legacy body **push/pop ECX+EDX**; **destroys nothing**. ESI=handle is documented but **unused** by the leaf — still preserve. Trampoline must restore **EBX/ECX/EDX/ESI/EDI/EBP**. |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — V86 address translate (no FS name buffer) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **synthetic PTE table only** — never write live `page_tabs` / V86 machine state. |
| Cut BK | PE Characteristics→align mask | Complete; PE thin Path B novelty exhausted (Y+AT+BK) |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**66 / 135** (`66` production symbols = Cut A four + B–BK; `66` `[[rust.migrations]]` with `enabled = true`).
No gate drift. `strtoint_dec` still dead (`conf_lib.inc` not linked).
`strncat` / `strchr` / `strnlen` / `net_ptr_to_num` / `set_mouse_data` remain
export-only (zero in-kernel callers). `get_phys_addr` is export + thin AQ
wrapper with **zero** in-kernel call sites found.

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** |
| Y+AT+BK+`get_proc_ex` PE Path A? | **No** — PE ban stretch; loader stays FASM |
| AV+BG AHCI Path A? | **No** |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+BF+BH string Path A? | **No** |
| AJ+BI ISO Path A? | **No** — Path B exhausted |
| L+BE+`set_mouse_data` HID Path A? | **No** |
| P+AZ+BJ Stage-3 Path A? | **No** — three gates ≠ façade ownership |
| AQ+this leaf paging Path A? | **No** — two translate leaves ≠ allocator/fault ownership |
| Strongest remaining **live** leaf? | **Yes** — `v86_get_lin_addr` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `ahci_is_sig_known` | Trivial 4-way CMP / ZF — AV deepen |
| `set_mouse_data` | HID deepen; side-effect heavy; **0** in-kernel callers (PE export) |
| `tcp_mss` | Trivial 1420 clamp — TCP deepen after BD |
| `xfs._.conv_time_to_kos_epoch` | Thin movbe+Cut T compose — XFS time deepen |
| `ntfs_restore_usa_frs` | Load size + fallthrough to J — too thin |
| `ext_*` / `fsGetTime` | No `--disk ext`; CMOS/calendar caution |
| `get_phys_addr` | Thin AQ page-offset glue; **0** in-kernel callers |
| `get_proc_ex` / `rebase_coff` / `fix_coff_symbols` | PE ban stretch after Y+AT+BK |
| `uni2ansi_char` / `cp866toUTF8_string` | Ban / deferred wrappers (Cut A encode path) |
| `strchr` / `strnlen` / `strncat` / `net_ptr_to_num` | Export-only / thin wrapper |
| `mutex_init` | Trivial 3-store (long rejected) |
| `blit_clip` | H composition glue; Stage-7 adjacency |
| AO/AN/address-math-FS/socket ban-list | Unchanged |

### New soak evidence (post-BK)

Prior audits labeled V86 soak “weak” (BIOS/exception only). Repository evidence
now strengthens the case without changing semantics:

* `scripts/reference_qemu.py` already enables BIOS disks via disposable CoW
  `config.ini` (`biosdisks=on`) so attached IDE images appear as guest `/bd*`.
* Hybrid kernel can use the same `config.ini` put on a test CoW; `bd_drv.inc`
  drives int 13h through `sys_v86_machine` → `v86_start` → **15 live**
  `v86_get_lin_addr` call sites in exception/simulate paths.
* This is distinct from AQ’s DMA/`--disk xfs` soak: V86 BIOS-disk path is the
  real subsystem environment for this leaf.

### Ranked top candidates (post-BK)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `v86_get_lin_addr` | V86 PTE→linear | 15 | BIOS `/bd*` via `biosdisks=on` | Low–Med | Stage-4 translate | **SELECT** |
| 2 | `ahci_is_sig_known` | Signature CMP→ZF | 2 | `--bus ahci` | Low | AV deepen / trivial | Defer |
| 3 | `xfs._.conv_time_to_kos_epoch` | BE32→`fsTime2bdfe` | 3 indirect | `--disk xfs` | Low | XFS time deepen | Defer |
| 4 | `blit_clip` | Dual `block_clip` compose | 1 | Desktop GUI | Med | H deepen / Stage 7 | Defer |
| 5 | `set_mouse_data` | HID aggregator | 0 in-kernel | Desktop mouse | Med–High | HID deepen | Defer |
| 6 | `tcp_mss` | MSS clamp | 1 | Partial net | Low | TCP deepen | Defer |
| 7 | `fsGetTime` | CMOS→BDFE | 6 | Partial / CMOS hard | Med | Calendar caution | Defer |
| 8 | `ext_read_all_times` | 3× AL compose | 2 | No `--disk ext` | Low | AL compose | Defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: v86_get_lin_addr
Source: kernel/core/v86.inc
Subsystem: Stage-4 V86 linear address translate
Stage: Stage-4 foothold (complements AQ; paging/alloc stay FASM)
Why selected:
    Post-BK audit: Path A rejected everywhere. PE thin Path B novelty
    exhausted (Y+AT+BK). Strongest remaining justified Path B with new
    semantic class (V86 addr→linear via page_tabs PTE), 15 live callers,
    clean “destroys nothing” ABI, independent oracle, reloc-free via
    trampoline-injected page_tabs, and real BIOS-disk soak via
    biosdisks=on + /bd* (not merely desktop convenience).
    Preferred over ahci_is_sig_known (trivial CMP), XFS/HID/TCP deepen,
    blit_clip (H glue), and CMOS/EXT leaves without harness.
Why this is a genuine migration boundary:
    Deterministic V86 address → linear: page = addr>>12; PTE =
    page_tabs[page]; linear = (PTE & ~0xFFF) | (addr & 0xFFF).
    Distinct from get_pg_addr (kernel VA−OS_BASE → phys page) without
    claiming paging ownership.
Why Path A / Path B:
    Path B — one translate leaf. V86 machine / page_tabs / alloc stay FASM.
Regression risks:
    REG-001: preserve ECX+EDX+ESI (+ EBX/EDI/EBP canaries); EAX = linear.
    REG-002: N/A.
    REG-003: synthetic PTE table only; never mutate live page_tabs.
    ESI handle unused by leaf — still preserve (callers pass V86 handle).
    Flags: legacy clobbers via arithmetic; callers ignore — no flag restore.
CPU/interrupt-state risks:
    None in leaf — pure PTE read + mask/or.
Shared-state risks:
    Reads only caller-supplied V86 address + trampoline-injected page_tabs.
Concurrency/locking risks:
    None in leaf.
Required differential tests:
    Independent FASM-flow oracle; page 0 / mid / last; offset 0/0xFFF;
    PTE flags stripped; unmapped PTE=0; 50k PRNG seed 0x4355424C ('CUBL').
Required ABI tests:
    Marker VGLA; synthetic PTE table; ECX/EDX/ESI/EBX/EDI/EBP canaries;
    never write live page_tabs / V86 structures.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise;
    prior cut-bk-final.img baseline.
Required real subsystem validation:
    Hybrid CoW with config.ini biosdisks=on + attached disk; confirm
    /bd* BIOS-disk path exercises V86 (bd_drv → v86_start). Document
    honestly if browse not fully scripted beyond boot/attach.
Rejected alternatives:
    ahci_is_sig_known; xfs classic time; blit_clip; set_mouse_data;
    tcp_mss; fsGetTime; ext_*; PE deepen; Path A; ban-list.
Expected legacy ABI:
    call v86_get_lin_addr; EAX=V86 addr; ESI=handle (unused);
    EAX=linear; destroys nothing; plain ret.
Expected Rust ABI:
    stdcall rust_v86_get_lin_addr(v86_addr, page_tabs) -> EAX linear; ret 8.
Differential-testing strategy:
    Independent oracle mirroring FASM SHR/PTE/AND/OR; 50k PRNG.
ABI-risk assessment:
    Low–Med — pure translate; mitigated by Cut AQ inject+preserve pattern
    + host oracle + synthetic smoke.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with `page_tabs` injection and full GPR preserve;
`USE_RUST_V86_GET_LIN_ADDR` rollback.

Compose nothing — pure PTE translate (no cross-section calls).

---

## Out of scope

* Claiming Path A for paging / V86 machine ownership
* Migrating `get_phys_addr` / `map_page` / `alloc_page`
* Migrating `ahci_is_sig_known` / `set_mouse_data` / XFS classic time
* Beginning Cut BM
