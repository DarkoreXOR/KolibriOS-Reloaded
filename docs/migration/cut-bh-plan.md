# Cut BH Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bh-implementation.md`](cut-bh-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BH** migrates C-string length —
> `strlen` in `kernel/fs/parse_fn.inc`.  
> Cuts A–BG remain complete and must not be redone. Do not start Cut BI.

---

## Post-BG migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Derive preserve from **legacy FASM body**: leaf **restores EAX/EDI** via push/pop; **ESI/EBX/EDX/EBP** untouched; **ECX = length out**. Trampoline must restore EAX/EBX/EDX/ESI/EDI/EBP and place length in ECX (Rust returns length in EAX). |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — length leaf (no name buffer write) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **iglobal synthetic C strings only** — never touches live EXT inode/name buffers |
| Cut BG | endian word-swap | Complete; this leaf is **scasb length**, not AHCI deepen |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**62 / 135** (`62` enabled `[[rust.migrations]]` = Cut A four symbols + B–BG).
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
| D+BB+BF+this leaf string Path A? | **No** — four string leaves ≠ libc ownership |
| L+BE+`set_mouse_data` HID Path A? | **No** — aggregator ≠ ownership |
| P+AZ+`is_string_userspace` Stage-3 Path A? | **No** — thin sibling |
| Strongest remaining **clean** leaf? | **Yes** — `strlen` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `set_mouse_data` | Post-BG #2: PE-export HID aggregator; strong mouse soak; **side-effect heavy** (`BTN_DOWN`/`MOUSE_*`/`wakeup_osloop` + display dims) — REG-003 elevated; HID deepen after L+BE |
| `iso9660_copy_name` | Real `--disk iso9660` soak; AJ orchestration glue + ban-listed `uni2ansi_char` + deferred `cp866toUTF8_string` + REG-002 adjacency |
| `ahci_is_sig_known` | Trivial 4-way CMP / ZF — AV deepen right after BG AHCI util |
| `v86_get_lin_addr` | Stage-4 address math after AQ; V86 soak conditional on BIOS-disk option |
| `is_string_userspace` | Thin P sibling (explicitly ranked below) |
| `coff_get_align` / `get_proc_ex` | PE thin / ban stretch |
| `strchr` / `strnlen` / `strncat` | Export-only — no kernel callers |
| `tcp_mss` / `tcp_output` | TCP deepen after BD (`tcp_mss` is a 1420 clamp) |
| `fsGetTime` | CMOS I/O + calendar cluster caution; time-dependent oracle |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BG)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `strlen` | fs/parse + EXT | 2 (EXT) | **Length/`scasb`** | Excellent | Low | No `--disk ext` | **SELECT** |
| `set_mouse_data` | HID / mouse | PE export live | HID aggregator | Hard | Med–High | Desktop mouse | #2 side-effects |
| `iso9660_copy_name` | ISO9660 | 1 | Encoding dispatch | Good | Med | `--disk iso9660` | #3 glue+ban |
| `ahci_is_sig_known` | AHCI | 2 | Signature CMP | Excellent | Low | `--bus ahci` | #4 trivial+stack |
| `v86_get_lin_addr` | Stage-4 / V86 | 14 | PTE→linear | Excellent | Low | BIOS/V86 weak | #5 address-math |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: strlen
Source: kernel/fs/parse_fn.inc
Subsystem: C-string length (EXT dirent name paths)
Stage: Stage-2 / string leaf (new semantic vs D compare / BB reverse / BF pad-copy)
Why selected:
    Post-BG audit: Path A rejected everywhere. HID side-effect aggregator /
    ISO glue+ban / trivial AHCI sig (post-BG AHCI stack) / Stage-4 V86
    stay weaker. Strongest remaining clean leaf is strlen — first
    NUL-terminated length / scasb class; two live EXT callers; excellent
    independent oracle; reloc-free trivial. Soak honestly NOT AVAILABLE
    (no --disk ext). Prefer clean novel class over side-effect-heavy #2.
Why this is a genuine migration boundary:
    Deterministic ESI→ECX length matching FASM or ecx,-1 / repnz scasb /
    inc / not. Complements D+BB+BF without claiming string Path A.
Why Path A / Path B:
    Path B — one length leaf. EXT FS orchestration stays FASM.
Regression risks:
    REG-001: preserve EAX/EBX/EDX/ESI/EDI/EBP; ECX = length out
    (legacy restores EAX/EDI; leaves ESI/EBX/EDX/EBP alone).
    REG-003: synthetic C strings only; never mutate live EXT buffers.
    DF: legacy does NOT cld — leave DF unchanged.
    Edge: empty string → 0; long strings; embedded high bytes.
CPU/interrupt-state risks:
    None in leaf — pure memory read until NUL.
Shared-state risks:
    Reads only caller-provided string; no globals.
Concurrency/locking risks:
    None in leaf (caller owns name buffer).
Required differential tests:
    Independent FASM-flow oracle; empty / single / long / binary bytes;
    50k PRNG seed 0x43554248 ('CUBH').
Required ABI tests:
    Marker STRL; synthetic strings; EAX/EBX/EDX/ESI/EDI/EBP canaries;
    ECX = length; DF unchanged.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise;
    prior cut-bg-final.img.
Required real subsystem validation:
    NOT AVAILABLE — no --disk ext; EXT callers only. Report desktop
    boot only; do not claim EXT soak.
Rejected alternatives:
    set_mouse_data (HID side-effects); iso9660_copy_name; ahci_is_sig_known;
    v86_get_lin_addr; is_string_userspace; Path A; ban-list.
Expected legacy ABI:
    register strlen; ESI→string in; ECX=length out; plain ret;
    preserves EAX/EDI (push/pop) + ESI/EBX/EDX/EBP (untouched);
    clobbers flags; DF unchanged (no cld/std).
Expected Rust ABI:
    stdcall rust_strlen(s); ret 4 → EAX=length;
    trampoline: mov ecx,eax then restore EAX/EBX/EDX/ESI/EDI/EBP.
Differential-testing strategy:
    Independent oracle mirroring FASM ecx=-1/scasb/inc/not; 50k PRNG.
ABI-risk assessment:
    Low — length util; REG-001 full preserve except ECX out;
    REG-003 synthetic only; DF leave-alone.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with full preserve except ECX=length;
`USE_RUST_STRLEN` rollback.

---

## Out of scope

* Claiming Path A for string/libc ownership
* Migrating `strchr` / `strnlen` / `strncat` (export-only)
* Migrating `set_mouse_data` / `iso9660_copy_name` / `ahci_is_sig_known`
* Beginning Cut BI
