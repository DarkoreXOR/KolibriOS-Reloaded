# Cut BB Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bb-implementation.md`](cut-bb-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BB** migrates reverse character search —
> `strrchr` in `kernel/core/string.inc`.  
> Cuts A–BA remain complete and must not be redone. Do not start Cut BC.

---

## Post-BA migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Preserve **EDX** (FASM never touched it); ECX is FASM-clobbered; EDI already saved by leaf/`uses` pattern |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| REG-003 | ABI smoke mutates live globals | Smoke uses **iglobal synthetic C strings only** — never touch live `path_string` / process slots |
| Cut D | `strncmp` string leaf | Same `string.inc` file; BB is reverse-search, not D deepen of compare |
| Cut BA | PCI config address | Complete; do **not** deepen PCI into `pci_read_reg` / mech-2 |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**56 / 135** (`56` enabled `[[rust.migrations]]` = Cut A four symbols + B–BA).
No drift found; inventory remains authoritative.

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** — leaves ≠ mount/I/O ownership |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY network Path A? | **No** |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV AHCI Path A? | **No** |
| BA + `pci_read_reg` PCI Path A? | **No** — one encode leaf ≠ bus ownership |
| P+AZ Stage-3 Path A? | **No** |
| D + this leaf as string Path A? | **No** — two string leaves ≠ libc/string ownership |
| Strongest remaining **live** leaf? | **Yes** — `strrchr` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `strtoint_dec` | **Dead** — `conf_lib.inc` not linked (`kernel32.inc` comment); BA #2 was stale |
| `get_proc_ex` | PE ban stretch |
| `tcp_outflags` | Mild M/V TCP deepen; `.flaglist` reloc risk |
| `fat_name_is_legal` | Charset table + mild FAT deepen after U/K/AO |
| `is_string_userspace` | Thin P sibling |
| `v86_get_lin_addr` | Stage-4 accessor; BIOS/V86 soak only (deferred since AB/AR/AQ) |
| `ahci_is_sig_known` / `swap_bytes_in_words` | AV trivial deepen |
| `strlen` / `strncpy` | Weaker soak (EXT-only / shmem-only) vs process-create |
| `hotkey_do_test` | Indirect call table — reloc-hostile |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BA)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `strrchr` | core/string | 1 kernel + PE export | **Reverse char search** | Excellent | Med (process name) | app launch / `fs_execute` | **SELECT** |
| `fat_name_is_legal` | FAT | 1 | Charset validate | Good | Low | LFN create | #2 deepen |
| `tcp_outflags` | TCP | 1 | State→flags table | Excellent | Low | Partial net | #3 deepen |
| `get_proc_ex` | PE / DLL | 1 | Import name→VA | Excellent | Med | `.sys` load | #4 (PE ban) |
| `is_string_userspace` | Stage-3 | 1 | String region gate | Good | Low | path syscalls | #5 thin |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: strrchr
Source: kernel/core/string.inc
Subsystem: core string reverse character search
Stage: Stage-2 leaf (string cluster; process-create path exercises it)
Why selected:
    Post-BA audit: Path A rejected everywhere. BA #2 strtoint_dec is not
    linked. PE/TCP/FAT/AV/V86 leftovers stay ban/deferred. Strongest live leaf
    is strrchr — new reverse-search algorithm (not strncmp deepen), clean
    stdcall, excellent differential domain, exercised on every fs_execute /
    app launch when extracting APPDATA.appname, plus PE export.
Why this is a genuine migration boundary:
    Deterministic forward NUL find + reverse scasb for last `c`. Distinct from
    Cut D compare without claiming string-library ownership.
Why Path A / Path B:
    Path B — one search leaf. Remaining string ops and process create stay FASM.
Regression risks:
    REG-001: EDX preserve (FASM never touched EDX; Rust stdcall clobbers it).
    DF: FASM always `cld` before return — trampoline must `cld`.
    REG-003: smoke uses synthetic iglobal strings only.
CPU/interrupt-state risks:
    None in leaf — pure memory walk; no cli/sti.
Shared-state risks:
    None — read-only string walk; no globals.
Concurrency/locking risks:
    None.
Required differential tests:
    Independent FASM-flow oracle (forward NUL length + reverse scan);
    empty; no match; match first/last/mid; c==0 → NUL ptr; multi-slash paths;
    50k PRNG seed 0x43554242 ('CUBB').
Required ABI tests:
    Marker STRR; synthetic strings; EDX (+EBX/ESI/EDI/EBP) canaries; cld;
    no live process/path mutation.
Required A/B tests:
    Gate OFF vs ON desktop; app launch path (fs_execute_from_sysdir).
Required real subsystem validation:
    Boot launches /sys apps via fs_execute → strrchr on path; desktop smoke
    is a real process-create soak for this leaf (not FS-disk soak).
Rejected alternatives:
    strtoint_dec (dead); get_proc_ex; tcp_outflags; fat_name_is_legal;
    is_string_userspace; v86/AHCI trivial; Path A clusters; ban-list.
Expected legacy ABI:
    stdcall(s, c) → EAX = ptr to last c or NULL; preserves EDX/EBX/ESI/EDI/EBP;
    clobbers ECX/flags; leaves DF=0; ret 8.
Expected Rust ABI:
    stdcall rust_strrchr(s, c) → EAX; ret 8;
    trampoline preserves EDX and executes cld.
Differential-testing strategy:
    Independent oracle mirroring FASM forward+reverse scasb; 50k 'CUBB'.
ABI-risk assessment:
    Medium — process-create hot path + PE export; REG-001 EDX; DF restore.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall→stdcall trampoline with **EDX** preserve + **`cld`**;
`USE_RUST_STRRCHR` rollback.

---

## Out of scope

* Claiming Path A for core/string or process create
* Migrating `strchr` / `strncpy` / `strlen` / `strnlen`
* Migrating `strtoint_dec` (unlinked) / `fat_name_is_legal` / `tcp_outflags`
* Beginning Cut BC
