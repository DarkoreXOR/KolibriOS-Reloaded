# Cut BF Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bf-implementation.md`](cut-bf-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BF** migrates bounded padded string copy —
> `strncpy` in `kernel/core/string.inc`.  
> Cuts A–BE remain complete and must not be redone. Do not start Cut BG.

---

## Post-BE migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Derive preserve from **legacy FASM body**: strncpy **clobbers EDX/ECX**; preserve **ESI/EDI** (push/pop) + EBX/EBP; do **not** invent EDX preserve (Cut BE smoke lesson) |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — string leaf (destination is always written for `n` bytes, including pad) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **iglobal synthetic dst/src buffers only** — never touches live `shmem_list` / SMEM nodes |
| Cut BE | HID hotkey | Complete; this leaf is **string padded copy**, not HID deepen |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**60 / 135** (`60` enabled `[[rust.migrations]]` = Cut A four symbols + B–BE).
No gate drift. `strtoint_dec` still dead (`conf_lib.inc` not linked).
`strncat` remains export-only (document in special cases; no count inflation).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV AHCI Path A? | **No** |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+this leaf as string Path A? | **No** — three string leaves ≠ libc ownership |
| L+BE+`set_mouse_data` HID Path A? | **No** — aggregator ≠ input ownership |
| P+AZ+`is_string_userspace` Stage-3 Path A? | **No** — thin sibling |
| Strongest remaining **live** leaf? | **Yes** — `strncpy` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `set_mouse_data` | Post-BE elevate (#2): PE-export live HID aggregator; strong mouse soak; **side-effect heavy** (`BTN_DOWN`/`MOUSE_*`/`wakeup_osloop`) + HID deepen after L+BE |
| `strlen` | Clean length leaf; **EXT-only callers**; no `--disk ext` |
| `iso9660_copy_name` | Real `--disk iso9660` soak; AJ glue + `uni2ansi` ban adjacency + REG-002 |
| `is_string_userspace` | Thin P sibling (explicitly ranked below) |
| `v86_get_lin_addr` | Stage-4 address math; BIOS/V86 soak weak |
| `swap_bytes_in_words` | AV trivial deepen |
| `coff_get_align` / `get_proc_ex` | PE thin / ban stretch |
| `strchr` / `strnlen` / `strncat` | Export-only — no kernel callers |
| `tcp_mss` / `tcp_output` | TCP deepen after BD |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BE)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `strncpy` | core/string | 1 + PE export | **Bounded padded copy** | Excellent | Low | Desktop shmem | **SELECT** |
| `set_mouse_data` | HID / mouse | PE export live | HID aggregator | Hard | Med–High | Desktop mouse | #2 deepen |
| `strlen` | core/string | 2 (EXT) | Length/`scasb` | Excellent | Low | No `--disk ext` | #3 soak |
| `iso9660_copy_name` | ISO9660 | 1 | Encoding dispatch | Good | Med | `--disk iso9660` | #4 glue |
| `swap_bytes_in_words` | AHCI util | 1 | Endian word-swap | Excellent | Low | `--bus ahci` | #5 AV deepen |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: strncpy
Source: kernel/core/string.inc
Subsystem: core string bounded padded copy (shmem name + PE export)
Stage: Stage-2 / string leaf (new semantic vs D compare / BB reverse search)
Why selected:
    Post-BE audit: Path A rejected everywhere. Thin P sibling / Stage-4
    address math / PE leftovers / AV deepen / HID deepen stay weaker.
    Strongest remaining live leaf is strncpy — first mutating padded-copy
    string class (always writes n bytes), live shmem_open caller + PE
    export, excellent independent oracle, desktop shmem soak path.
Why this is a genuine migration boundary:
    Deterministic (s1,s2,n) → EAX=s1 with exact C/FASM strncpy pad/trunc.
    Complements D+BB without claiming string/libc Path A.
Why Path A / Path B:
    Path B — one string leaf. shmem / PE export surface stay FASM.
Regression risks:
    REG-001: do NOT invent EDX preserve — FASM clobbers EDX/ECX;
    preserve ESI/EDI/EBX/EBP; leave DF cleared (cld).
    REG-003: synthetic dst/src only; never mutate live shmem_list.
    Edge: n=0; s2 longer than n (no NUL written); pad after early NUL.
CPU/interrupt-state risks:
    None in leaf — pure memory write of n bytes.
Shared-state risks:
    Writes only caller-provided s1[0..n); no globals.
Concurrency/locking risks:
    None in leaf (caller owns shmem list lock if any).
Required differential tests:
    Independent FASM-flow oracle; pad/trunc/n=0/no-NUL-in-n;
    50k PRNG seed 0x43554246 ('CUBF').
Required ABI tests:
    Marker SNCP; synthetic buffers; ESI/EDI/EBX/EBP canaries;
    ECX/EDX explicitly allowed clobber; cld.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise;
    prior cut-be-final.img.
Required real subsystem validation:
    Desktop boots; shmem_open name copy path exercised when apps use
    shared memory — report honestly if only boot/desktop available.
Rejected alternatives:
    set_mouse_data (HID deepen); strlen; iso9660_copy_name;
    is_string_userspace; v86; swap_bytes; Path A; ban-list.
Expected legacy ABI:
    stdcall strncpy(s1,s2,n) → EAX=s1; ret 12;
    preserves ESI/EDI/EBX/EBP; clobbers ECX/EDX/flags; cld; DF=0.
Expected Rust ABI:
    stdcall rust_strncpy(s1,s2,n) → EAX=s1; ret 12;
    trampoline may be thin + cld (EDX clobber matches legacy).
Differential-testing strategy:
    Independent oracle mirroring FASM scasb/movsb/stosb pad; 50k PRNG.
ABI-risk assessment:
    Low–Med — shmem name path; REG-001 EDX lesson; REG-003 synthetic only.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall trampoline with **cld**; `USE_RUST_STRNCPY` rollback.

---

## Out of scope

* Claiming Path A for core/string or shmem ownership
* Migrating `strchr` / `strnlen` / `strncat` / `strlen`
* Migrating `set_mouse_data`
* Beginning Cut BG
