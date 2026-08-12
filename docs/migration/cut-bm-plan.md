# Cut BM Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bm-implementation.md`](cut-bm-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BM** migrates AHCI port PxSIG known-device recognition —
> `ahci_is_sig_known` in `kernel/blkdev/ahci.inc`.  
> Cuts A–BL remain complete and must not be redone. Do not start Cut BN.

---

## Post-BL migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Legacy leaf **destroys flags only** — all GPRs preserved. Trampoline must restore **EBX/ECX/EDX/ESI/EDI/EBP**; map Rust `1/0` → legacy **ZF** via `cmp eax, 1`. |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — AHCI signature leaf (no FS name buffer) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **synthetic signatures only** — never writes live HBA port state. |
| Cut BL | V86 PTE→linear translate | Complete; Stage-4 V86 class exhausted |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**67 / 135** (`67` production symbols = Cut A four + B–BL; `67` `[[rust.migrations]]` with `enabled = true`).
No gate drift. `strtoint_dec` still dead (`conf_lib.inc` not linked).
`strncat` / `strchr` / `strnlen` / `net_ptr_to_num` / `set_mouse_data` remain
export-only (zero in-kernel callers).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** |
| Y+AT+BK+`get_proc_ex` PE Path A? | **No** — PE ban stretch; loader stays FASM |
| AV+BG+BM AHCI Path A? | **No** — three leaves ≠ controller/DMA/IRQ ownership |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+BF+BH string Path A? | **No** |
| AJ+BI ISO Path A? | **No** — Path B exhausted |
| L+BE+`set_mouse_data` HID Path A? | **No** |
| P+AZ+BJ Stage-3 Path A? | **No** |
| AQ+BL paging/V86 Path A? | **No** — translate leaves exhausted |
| Strongest remaining **live** leaf? | **Yes** — `ahci_is_sig_known` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `xfs._.conv_time_to_kos_epoch` | Thin movbe+Cut T compose — XFS time deepen (AK anti-cluster) |
| `blit_clip` | H composition glue; 1 caller; desktop-only soak |
| `set_mouse_data` | HID deepen; side-effect heavy; **0** in-kernel callers |
| `tcp_mss` | Trivial 1420 clamp — TCP deepen after BD |
| `fsGetTime` | CMOS/calendar caution; hard independent oracle |
| `ext_read_all_times` / `ext_write_time` | AL compose; no `--disk ext` |
| `get_phys_addr` | Thin AQ wrapper; **0** in-kernel callers |
| `get_proc_ex` / PE deepen | PE ban stretch after Y+AT+BK |
| `ntfs_restore_usa_frs` | Load size + fallthrough to J — too thin |
| `strchr` / export-only peers | Zero in-kernel callers |
| AO/AN/address-math/socket ban-list | Unchanged |

### Previously rejected — re-evaluated post-BL

| Candidate | New evidence? | Verdict |
|-----------|---------------|---------|
| `v86_get_lin_addr` | Completed Cut BL | **Done** |
| `ahci_is_sig_known` | Still strongest live leaf after BL; real `--bus ahci` soak | **SELECT** |
| `xfs._.conv_time_to_kos_epoch` | No material change; still thin AK sibling | Defer |
| `blit_clip` | No material change; still H glue / Stage 7 | Defer |
| `fsGetTime` | No material change; CMOS still hard | Defer |

### Ranked top candidates (post-BL)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `ahci_is_sig_known` | AHCI PxSIG CMP→ZF | 2 | `--bus ahci` | Low | AV/BG AHCI driver | **SELECT** |
| 2 | `xfs._.conv_time_to_kos_epoch` | BE32→`fsTime2bdfe` | 3 indirect | `--disk xfs` | Low | XFS time deepen | Defer |
| 3 | `blit_clip` | Dual `block_clip` compose | 1 | Desktop GUI | Med | H deepen / Stage 7 | Defer |
| 4 | `fsGetTime` | CMOS→BDFE | 6+ | Partial / CMOS hard | Med | Calendar caution | Defer |
| 5 | `tcp_mss` | MSS clamp | 1 | Partial net | Low | TCP deepen | Defer |
| 6 | `ext_read_all_times` | 3× AL compose | 2 | No `--disk ext` | Low | AL compose | Defer |
| 7 | `set_mouse_data` | HID aggregator | 0 in-kernel | Desktop mouse | Med–High | HID deepen | Defer |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: ahci_is_sig_known
Source: kernel/blkdev/ahci.inc
Subsystem: AHCI port bring-up / PxSIG validation
Stage: Stage-5 driver foothold (complements AV+BG)
Why selected:
    Post-BL audit: Path A rejected everywhere. V86 translate class
    complete (BL). Strongest remaining justified Path B with live
    AHCI port-init callers (2), real `--bus ahci` subsystem soak,
    clean ZF-only ABI, independent oracle, and reloc-free leaf.
    Preferred over xfs classic time (thin AK anti-cluster),
    blit_clip (H glue / desktop-only), CMOS fsGetTime, and
    export-only / TCP-trivial peers.
Why this is a genuine migration boundary:
    Deterministic PxSIG → known-device ZF gate on the link-reset /
    wait-ready path. Distinct from AV cmdslot scan and BG endian
    swap without claiming AHCI controller ownership.
Why Path A / Path B:
    Path B — one signature leaf. Port DMA / IRQ / wait loops stay FASM.
Regression risks:
    REG-001: preserve EBX/ECX/EDX/ESI/EDI/EBP; ZF must match legacy.
    REG-002: N/A.
    REG-003: synthetic signatures only in smoke.
    Flags: callers use `jz` after call — trampoline `cmp eax,1` maps Rust.
Required differential tests:
    Independent FASM-flow oracle; four known sigs; near-miss; 50k PRNG
    seed 0x4355544D ('CUTM').
Required ABI tests:
    Marker ASIG; direct rust_* + public trampoline ZF paths; GPR canaries.
Required A/B tests:
    Gate OFF vs ON via build.toml `enabled`; screendump parity.
Required real subsystem validation:
    `--bus ahci` QMP smoke (exercises ahci_port_init signature checks).
Rejected alternatives:
    xfs classic time; blit_clip; fsGetTime; tcp_mss; ext_*; PE deepen;
    Path A; ban-list.
Expected legacy ABI:
    call ahci_is_sig_known; EAX=PxSIG; ZF=1 known / ZF=0 unknown;
    destroys flags only; plain ret.
Expected Rust ABI:
    stdcall rust_ahci_is_sig_known(sig) -> EAX 1/0; ret 4;
    trampoline cmp eax,1 → legacy ZF.
Differential-testing strategy:
    Independent four-cmp oracle; 50k PRNG.
ABI-risk assessment:
    Low — pure compare; mitigated by ZF mapping + GPR preserve + smoke.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
ZF-mapping trampoline with full GPR preserve; `USE_RUST_AHCI_IS_SIG_KNOWN`
rollback.

---

## Out of scope

* Claiming Path A for AHCI controller ownership
* Migrating `ahci_port_wait` / DMA / IRQ paths
* Migrating `xfs._.conv_time_to_kos_epoch` / `blit_clip` / `set_mouse_data`
* Beginning Cut BN
