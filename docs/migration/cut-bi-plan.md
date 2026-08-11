# Cut BI Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bi-implementation.md`](cut-bi-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BI** migrates ISO9660 volume-name encoding dispatch —
> `iso9660_copy_name` in `kernel/fs/iso9660.inc`.  
> Cuts A–BH remain complete and must not be redone. Do not start Cut BJ.

---

## Post-BH migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Derive preserve from **legacy FASM body**: leaf **preserves EBP only**; advances ESI/EDI; clobbers EAX/EBX/ECX/EDX/flags. Trampoline restores EAX/EBX (canary hygiene) optionally; **must** leave EBP intact; **EDI/ESI** come from Rust inout. |
| REG-002 | FS empty-path / `bdfe.name` NUL | **In scope** — leaf writes terminating NUL (byte if ASCII volume, word if UCS-2). Differential + smoke must assert terminator width. |
| REG-003 | ABI smoke mutates live globals | Smoke uses **iglobal synthetic VolumeName + dest buffers only** — never touches live ISO PARTITION / primary descriptor. |
| Cut BH | C-string length / `scasb` | Complete; this leaf is **volume-name encoding dispatch**, not string deepen |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**63 / 135** (`63` enabled `[[rust.migrations]]` = Cut A four symbols + B–BH).
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
| AJ+this leaf ISO Path A? | **No** — compare + copy leaves ≠ ISO mount/read ownership |
| L+BE+`set_mouse_data` HID Path A? | **No** — aggregator ≠ ownership |
| P+AZ+`is_string_userspace` Stage-3 Path A? | **No** — thin sibling |
| Strongest remaining **live** leaf with real soak? | **Yes** — `iso9660_copy_name` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `set_mouse_data` | HID deepen; PE-export aggregator; side-effect heavy (`BTN_DOWN`/`MOUSE_*`/`wakeup_osloop`) — REG-003 elevated |
| `is_string_userspace` | Thin P sibling; excellent oracle; weaker novelty vs encoding dispatch + real ISO soak |
| `ahci_is_sig_known` | Trivial 4-way CMP / ZF — AV deepen |
| `v86_get_lin_addr` | Stage-4 address-math after AQ; V86 soak weak |
| `coff_get_align` / `get_proc_ex` | PE thin / ban stretch |
| `strchr` / `strnlen` / `strncat` / `net_ptr_to_num` | Export-only — no kernel callers |
| `fsGetTime` / `ext_*` time | CMOS/calendar caution; no `--disk ext` |
| AO/AN-as-cut / address-math / socket ban-list | Unchanged — helpers **inlined** here, not cut as symbols |

### Ranked top candidates (post-BH)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `iso9660_copy_name` | ISO9660 | 1 (`GetFileInfo` `.rootdir`) | **Volume-name encoding dispatch** | Good | Med | `--disk iso9660` | **SELECT** |
| `is_string_userspace` | Stage-3 | 1 (`load_library`) | P+scasb string gate | Excellent | Low | Desktop lib load | #2 thin |
| `v86_get_lin_addr` | Stage-4 / V86 | 14 | PTE→linear | Excellent | Low | BIOS/V86 weak | #3 address-math |
| `coff_get_align` | PE | 2 | Characteristics→mask | Excellent | Low | Desktop `.sys` | #4 PE thin |
| `ahci_is_sig_known` | AHCI | 2 | Signature CMP | Excellent | Low | `--bus ahci` | #5 trivial |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: iso9660_copy_name
Source: kernel/fs/iso9660.inc
Subsystem: ISO9660 volume-name encoding copy (GetFileInfo root)
Stage: Stage-2 / Stage-5 FS plugin leaf (complements AJ compare)
Why selected:
    Post-BH audit: Path A rejected everywhere. HID side-effect aggregator /
    thin P sibling / Stage-4 address-math / PE thin / trivial AHCI sig stay
    weaker. Strongest remaining live leaf with an honest named soak is
    iso9660_copy_name — volume-name encoding dispatch (ASCII/UCS-2 ×
    cp866/utf16/utf8) + REG-002 NUL terminate; one live GetFileInfo caller;
    real --disk iso9660 soak. Ban-listed uni2ansi / deferred cp866toUTF8
    are inlined via existing Rust cp866_encode / cp866_decode + utf16_to_8
    (same compose style as Cut AJ) — not migrated as production symbols.
Why this is a genuine migration boundary:
    Deterministic (ESI,EDI,ECX,EDX,type_encoding) → dest write + EDI@NUL
    matching FASM dispatch. Complements AJ without claiming ISO Path A.
Why Path A / Path B:
    Path B — one copy/dispatch leaf. ISO mount/read/dir stay FASM.
Regression risks:
    REG-001: preserve EBP; EDI/ESI from Rust inout; EAX/EBX/ECX/EDX
    legacy-clobbered (smoke canaries on EBP + dest/EDI).
    REG-002: byte vs word NUL by type_encoding.
    REG-003: synthetic VolumeName/dest only; never mutate live ISO state.
    DF: legacy lods/stos/movs assume DF=0; Rust DF-agnostic; smoke uses cld.
    Edge: nameenc∉{1,2,3} → NUL only; ecx=32 volume window; UCS-2 shr ecx,1;
    ASCII→UTF8 may scas one byte past src when no embedded NUL (FASM quirk).
CPU/interrupt-state risks:
    None in leaf — pure memory transform of caller buffers.
Shared-state risks:
    Reads type_encoding via trampoline-injected value (not raw ebp in Rust);
    writes only caller dest.
Concurrency/locking risks:
    None in leaf (caller owns buffers).
Required differential tests:
    Independent FASM-flow oracle; all 6 encoding paths + invalid nameenc;
    ASCII/UCS-2 NUL width; 50k PRNG seed 0x43554249 ('CUBI').
Required ABI tests:
    Marker ICPN; synthetic VolumeName/dest; EBP canary; EDI@NUL;
    dest contents; never touch live ISO PARTITION.
Required A/B tests:
    Gate OFF vs ON with --disk iso9660; same non-black ± clock noise;
    prior cut-bh-final.img; volume label path when nameenc≠0.
Required real subsystem validation:
    python scripts/run_qemu.py --disk iso9660 — browse CD volume /
    GetFileInfo name encoding path that calls iso9660_copy_name.
Rejected alternatives:
    set_mouse_data (HID side-effects); is_string_userspace (thin);
    v86_get_lin_addr; coff_get_align; ahci_is_sig_known; Path A; ban-list
    symbol cuts (uni2ansi_char / cp866toUTF8_string remain FASM symbols).
Expected legacy ABI:
    register iso9660_copy_name;
    ESI→src, EDI→dest, ECX=max bytes, EDX=nameenc, EBP→ISO9660;
    EDI advanced to terminator position; byte[edi]=0, and word[edi]=0 if
    type_encoding≠0; plain ret; preserves EBP; clobbers EAX/EBX/ECX/EDX/ESI/flags.
Expected Rust ABI:
    stdcall rust_iso9660_copy_name(esi_inout, edi_inout, max_len, nameenc,
    type_encoding); ret 20; updates *esi_inout / *edi_inout.
Differential-testing strategy:
    Independent oracle mirroring FASM dispatch + inlined encode helpers;
    50k PRNG.
ABI-risk assessment:
    Med — REG-002 NUL width + encoding compose; mitigated by oracle + ISO soak.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with EBP preserved and ESI/EDI inout;
`USE_RUST_ISO9660_COPY_NAME` rollback.

Compose inlined `cp866_decode` / `cp866_encode` / `utf16_to_8` (no FASM
cross-calls from blob; do not add production gates for ban/deferred helpers).

---

## Out of scope

* Claiming Path A for ISO compare+copy ownership
* Migrating `uni2ansi_char` / `cp866toUTF8_string` / `UTF16to8_string` as cuts
* Migrating `set_mouse_data` / `is_string_userspace` / `ahci_is_sig_known`
* Beginning Cut BJ
