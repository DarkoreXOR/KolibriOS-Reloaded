# Cut BK Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bk-implementation.md`](cut-bk-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BK** migrates PE/COFF section alignment mask decode —
> `coff_get_align` in `kernel/core/dll.inc`.  
> Cuts A–BJ remain complete and must not be redone. Do not start Cut BL.

---

## Post-BJ migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Derive preserve from **legacy FASM body**: leaf **push/pop ECX**; **EDX** is section cursor (untouched). Callers keep **EBX** size / **ECX** preferred-base / **EDI** image cursor / **ESI** DLLDESCR across `call`. Trampoline must restore **ECX+EDX** (EBX/ESI/EDI/EBP via stdcall callee-save or explicit). |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — PE align leaf (no FS name buffer) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **synthetic `COFF_SECTION` only** — no live DLL list / `load_library` mutation. |
| Cut BJ | NUL-scan userspace ZF gate | Complete; Stage-3 Path B gates P+AZ+BJ exhausted for novelty |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**65 / 135** (`65` production symbols = Cut A four + B–BJ).
No gate drift. `strtoint_dec` still dead (`conf_lib.inc` not linked).
`strncat` / `strchr` / `strnlen` / `net_ptr_to_num` remain export-only (zero in-kernel callers).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch; loader stays FASM |
| AV+BG AHCI Path A? | **No** |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+BF+BH string Path A? | **No** |
| AJ+BI ISO Path A? | **No** — Path B exhausted |
| L+BE+`set_mouse_data` HID Path A? | **No** |
| P+AZ+BJ Stage-3 Path A? | **No** — three gates ≠ façade ownership |
| AQ+`v86_get_lin_addr` paging Path A? | **No** |
| Strongest remaining **live** leaf? | **Yes** — `coff_get_align` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `v86_get_lin_addr` | Stage-4 address-math after AQ/AW; V86 soak **weak** (BIOS/exception only) |
| `ahci_is_sig_known` | Trivial 4-way CMP / ZF — AV deepen |
| `set_mouse_data` | HID deepen; side-effect heavy; **0** in-kernel callers (PE export) |
| `tcp_mss` | Trivial 1420 clamp — TCP deepen after BD |
| `xfs._.conv_time_to_kos_epoch` | Thin movbe+Cut T compose — XFS time deepen |
| `ntfs_restore_usa_frs` | Load size + fallthrough to J — too thin |
| `ext_*` / `fsGetTime` | No `--disk ext`; CMOS/calendar caution |
| `strchr` / `strnlen` / `strncat` / `net_ptr_to_num` | Export-only / thin wrapper |
| `get_proc_ex` / `rebase_coff` | PE ban stretch / mutate anti-cluster |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BJ)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `coff_get_align` | COFF Characteristics→align mask | 2 (`load_library`) | Desktop `.sys` | Low | PE thin glue | **SELECT** |
| 2 | `v86_get_lin_addr` | PTE→linear | 15 | BIOS/V86 weak | Low–Med | Stage-4 / AW theme | Defer |
| 3 | `ahci_is_sig_known` | Signature CMP→ZF | 2 | `--bus ahci` | Low | AV deepen / trivial | Defer |
| 4 | `xfs._.conv_time_to_kos_epoch` | BE32→`fsTime2bdfe` | 3 indirect | `--disk xfs` | Low | XFS time deepen | Defer |
| 5 | `set_mouse_data` | HID aggregator | 0 in-kernel | Desktop mouse | Med–High | HID deepen | Defer |
| 6 | `tcp_mss` | MSS clamp | 1 | Partial net | Low | TCP deepen | Defer |
| 7 | `ext_read_all_times` | 3× AL compose | 2 | No `--disk ext` | Low | AL compose | Defer |
| 8 | `fsGetTime` | CMOS→BDFE | 6 | Partial / CMOS hard | Med | Calendar caution | Defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: coff_get_align
Source: kernel/core/dll.inc
Subsystem: Stage-8 PE/COFF section alignment mask (load_library)
Stage: Stage-8 foothold (complements Y+AT; loader stays FASM)
Why selected:
    Post-BJ audit: Path A rejected everywhere. Stage-3 Path B novelty
    exhausted (P+AZ+BJ). Strongest remaining justified Path B with clean
    register ABI, independent oracle, live load_library callers, and
    realistic desktop .sys soak is coff_get_align — Characteristics
    high-nibble → (1<<n)-1 mask with 4K default/clamp.
    Preferred over v86_get_lin_addr (weak soak / address-math theme),
    ahci_is_sig_known (trivial CMP), and HID/TCP/XFS deepen leaves.
Why this is a genuine migration boundary:
    Deterministic Characteristics byte → alignment mask. Distinct from
    fix_coff_relocs / get_coff_sym without claiming PE loader ownership.
Why Path A / Path B:
    Path B — one decode leaf. load_library / DLL list stay FASM.
Regression risks:
    REG-001: preserve ECX+EDX (+ EBX/ESI/EDI canaries); EAX = mask.
    REG-002: N/A.
    REG-003: synthetic COFF_SECTION only; no live DLL mutation.
    Edge: Characteristics align field 0 / >13 → default 4K (mask 0xFFF);
    field 1 → mask 0; field 13 → 0xFFF; field 14–15 → clamp 4K.
CPU/interrupt-state risks:
    None in leaf — pure byte decode + shift.
Shared-state risks:
    Reads only caller-supplied COFF_SECTION.Characteristics.
Concurrency/locking risks:
    None in leaf.
Required differential tests:
    Independent FASM-flow oracle; field 0..15; junk low Characteristics
    bits; 50k PRNG seed 0x4355424B ('CUBK').
Required ABI tests:
    Marker CGAL; synthetic section; ECX/EDX/EBX/ESI/EDI/EBP canaries;
    never mutate live DLL lists.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise;
    prior cut-bj-final.img baseline.
Required real subsystem validation:
    Desktop path that exercises load_library / .sys (same class as BJ/Y/AT).
    Document PARTIAL if not separately automated beyond boot.
Rejected alternatives:
    v86_get_lin_addr; ahci_is_sig_known; set_mouse_data; tcp_mss;
    xfs classic time; Path A; ban-list / deepen / export-only.
Expected legacy ABI:
    call coff_get_align; EDX→COFF_SECTION; EAX=(1<<n)-1 mask;
    preserves ECX; EDX untouched; plain ret (not stdcall).
Expected Rust ABI:
    stdcall rust_coff_get_align(section) -> EAX mask; ret 4.
Differential-testing strategy:
    Independent oracle mirroring FASM SHR/DEC/JS/CMP/SHL/DEC; 50k PRNG.
ABI-risk assessment:
    Low — pure decode; mitigated by Cut BA register→stdcall trampoline
    pattern + host oracle + synthetic smoke.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline preserving ECX/EDX; `USE_RUST_COFF_GET_ALIGN`
rollback.

Compose nothing — pure Characteristics nibble → mask (no cross-section calls).

---

## Out of scope

* Claiming Path A for PE loader / export directory ownership
* Migrating `get_proc_ex` / `rebase_coff` / `fix_coff_symbols`
* Migrating `v86_get_lin_addr` / `ahci_is_sig_known` / `set_mouse_data`
* Beginning Cut BL
