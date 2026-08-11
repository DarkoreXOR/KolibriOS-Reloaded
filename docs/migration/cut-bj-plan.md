# Cut BJ Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bj-implementation.md`](cut-bj-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BJ** migrates Stage-3 NUL-terminated string userspace gate —
> `is_string_userspace` in `kernel/kernel.asm`.  
> Cuts A–BI remain complete and must not be redone. Do not start Cut BK.

---

## Post-BI migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Derive preserve from **legacy FASM body**: leaf **push/pop EAX, ECX, EDI**; EDX/EBX/ESI/EBP untouched. Trampoline must restore EAX/ECX/EDX/EDI (and leave other GPRs intact); result is **ZF only**. |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — gate leaf (no name buffer write) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **no live process/FS/net tables**; reject vectors only (base≥`OS_BASE`) so early-init (userspace PDT cleared) never dereferences. Accept/scan covered by host oracle. |
| Cut BI | ISO9660 volume-name copy | Complete; ISO Path B leaves exhausted (AJ+BI) |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**64 / 135** (`64` production symbols = Cut A four + B–BI).
No gate drift. `strtoint_dec` still dead (`conf_lib.inc` not linked).
`strncat` / `strchr` / `strnlen` remain export-only (zero in-kernel callers).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV+BG AHCI Path A? | **No** — util leaves ≠ controller ownership |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+BF+BH string Path A? | **No** — five string leaves ≠ libc ownership |
| AJ+BI ISO Path A? | **No** — compare+copy ≠ ISO mount/read ownership |
| L+BE+`set_mouse_data` HID Path A? | **No** — aggregator ≠ ownership |
| P+AZ+this leaf Stage-3 Path A? | **No** — three gates ≠ façade ownership |
| Strongest remaining **live** leaf? | **Yes** — `is_string_userspace` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `set_mouse_data` | HID deepen; side-effect heavy (`BTN_DOWN`/`MOUSE_*`/`wakeup_osloop`) — REG-003 elevated |
| `v86_get_lin_addr` | Stage-4 address-math after AQ; V86 soak weak; AW address-math ban theme |
| `coff_get_align` | PE thin Characteristics→mask |
| `ahci_is_sig_known` | Trivial 4-way CMP / ZF — AV deepen |
| `tcp_mss` | Trivial 1420 clamp — TCP deepen after BD |
| `xfs._.conv_time_to_kos_epoch` | Thin movbe+Cut T compose — XFS time deepen |
| `ntfs_restore_usa_frs` | Load size + fallthrough to J — too thin |
| `ext_*` / `fsGetTime` | No `--disk ext`; CMOS/calendar caution |
| `strchr` / `strnlen` / `strncat` / `net_ptr_to_num` | Export-only / thin wrapper — no count inflation |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BI)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `is_string_userspace` | Stage-3 | 1 (`load_library`) | **NUL-scan userspace ZF gate** | Excellent | Low | Desktop lib load | **SELECT** |
| `v86_get_lin_addr` | Stage-4 / V86 | 14 | PTE→linear | Excellent | Low | BIOS/V86 weak | #2 address-math |
| `coff_get_align` | PE | 2 | Characteristics→mask | Excellent | Low | Desktop `.sys` | #3 PE thin |
| `ahci_is_sig_known` | AHCI | 2 | Signature CMP | Excellent | Low | `--bus ahci` | #4 trivial |
| `set_mouse_data` | HID | PE export live | HID aggregator | Hard | Med–High | Desktop mouse | #5 side-effects |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: is_string_userspace
Source: kernel/kernel.asm
Subsystem: Stage-3 syscall userspace string gate (load_library)
Stage: Stage-3 (complements P region gate + AZ size→gate)
Why selected:
    Post-BI audit: Path A rejected everywhere. ISO Path B exhausted (AJ+BI).
    Stronger-novelty leaves already migrated. Remaining justified Path B with
    clean ZF ABI, independent oracle, live caller, and low blast is
    is_string_userspace — NUL-terminated string userspace membership
    (bound + 64K-capped scasb), distinct from fixed-size is_region_userspace.
Why this is a genuine migration boundary:
    Deterministic (base) → ZF matching FASM SUB/JB + capped repnz scasb.
    Complements P/AZ without claiming Stage-3 Path A.
Why Path A / Path B:
    Path B — one ZF gate leaf. Syscall façade / load_library stay FASM.
Regression risks:
    REG-001: preserve EAX/ECX/EDI (+ EDX canary); ZF via cmp eax,1 + pops.
    REG-002: N/A.
    REG-003: reject-only smoke (base≥OS_BASE); no live table writes.
    DF: legacy repnz scasb assumes DF=0; Rust DF-agnostic forward scan.
    Edge: base>OS_BASE-1 → ZF from SUB (always 0); cap ecx at 0x10000;
    scan window = min(OS_BASE-base, 64K); no NUL → ZF=0.
CPU/interrupt-state risks:
    None in leaf — pure bound + memory scan of caller buffer.
Shared-state risks:
    Reads only caller-supplied string bytes within computed window.
Concurrency/locking risks:
    None in leaf.
Required differential tests:
    Independent FASM-flow oracle; reject bases; empty/short/long strings;
    no-NUL within window; 64K cap; last-byte-at-OS_BASE-1; 50k PRNG
    seed 0x4355424A ('CUBJ').
Required ABI tests:
    Marker ISUS; reject vectors base≥OS_BASE; EAX/ECX/EDX/EDI canaries;
    never map/mutate live process memory.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise;
    prior cut-bi-final.img baseline.
Required real subsystem validation:
    Desktop path that exercises load_library / sysfn string gate
    (library load). Document if not separately automated beyond boot.
Rejected alternatives:
    set_mouse_data; v86_get_lin_addr; coff_get_align; ahci_is_sig_known;
    Path A; ban-list / deepen / export-only.
Expected legacy ABI:
    stdcall is_string_userspace(base);
    ZF=1 accept / ZF=0 reject; preserves EAX,ECX,EDI; ret 4.
Expected Rust ABI:
    stdcall rust_is_string_userspace(base) -> EAX∈{0,1}; ret 4.
Differential-testing strategy:
    Independent oracle mirroring FASM SUB/JB/cap/scasb on host buffers;
    50k PRNG.
ABI-risk assessment:
    Low–Med — ZF reconstruct + pointer scan; mitigated by Cut P trampoline
    pattern + host oracle. Early-init smoke cannot safely accept-scan
    (userspace PDT cleared before smokes).
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall→stdcall trampoline with ZF reconstruction (`cmp eax,1` + flag-neutral
pops); `USE_RUST_IS_STRING_USERSPACE` rollback.

Compose nothing new — pure integer bound + byte scan (no cross-section calls).

---

## Out of scope

* Claiming Path A for Stage-3 / syscall façade ownership
* Migrating `set_mouse_data` / `v86_get_lin_addr` / `coff_get_align` /
  `ahci_is_sig_known`
* Mapping userspace pages solely to strengthen early-init accept smoke
* Beginning Cut BK
