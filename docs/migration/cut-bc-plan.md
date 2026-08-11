# Cut BC Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bc-implementation.md`](cut-bc-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BC** migrates FAT LFN charset legality —
> `fat_name_is_legal` in `kernel/fs/fat.inc`.  
> Cuts A–BB remain complete and must not be redone. Do not start Cut BD.

---

## Post-BB migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Preserve **ECX+EDX** (FASM leaf never touched them); ESI preserved via stack arg |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; boot FAT desktop still soaked |
| REG-003 | ABI smoke mutates live globals | Smoke uses **iglobal synthetic C strings only** — never touch live FAT buffers / mounts |
| Cut BB | `strrchr` string reverse search | Complete; do **not** deepen `strchr`/`strlen`/`strncpy` as string Path A |
| Cut U/K/AO | FAT short-name / time | Charset validate is a **new** FAT semantic, not calendar/address ban |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**57 / 135** (`57` enabled `[[rust.migrations]]` = Cut A four symbols + B–BB).
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
| U+K+AO+this leaf as FAT Path A? | **No** — charset leaf ≠ FAT mount/LFN ownership |
| D+BB+string leaves as Path A? | **No** — still FASM-owned string/libc surface |
| Strongest remaining **live** leaf? | **Yes** — `fat_name_is_legal` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `strchr` | **Export-only** — no kernel callers (weak soak) |
| `strlen` / `strncpy` | String deepen after BB; EXT-only / shmem-only soak |
| `get_proc_ex` | PE ban stretch |
| `tcp_outflags` | Mild M/V TCP deepen; `.flaglist` reloc risk |
| `is_string_userspace` | Thin P sibling |
| `swap_bytes_in_words` / `ahci_is_sig_known` | AV trivial deepen |
| `hotkey_do_test` | Indirect call table — reloc-hostile |
| `strtoint_dec` | Dead / unlinked |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BB)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `fat_name_is_legal` | FAT | 1 | **LFN charset validate** | Excellent | Low | create path / boot FAT | **SELECT** |
| `tcp_outflags` | TCP | 1 | State→flags table | Excellent | Low | Partial net | #2 deepen |
| `swap_bytes_in_words` | AHCI util | 1 | Endian word-swap | Excellent | Low | `--bus ahci` | #3 thin |
| `get_proc_ex` | PE / DLL | 1 | Import name→VA | Excellent | Med | `.sys` load | #4 (PE ban) |
| `is_string_userspace` | Stage-3 | 1 | String region gate | Good | Low | path syscalls | #5 thin |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: fat_name_is_legal
Source: kernel/fs/fat.inc
Subsystem: FAT LFN charset legality (bit0 of fat_legal_chars)
Stage: Stage-2 / FS leaf (FAT create path)
Why selected:
    Post-BB audit: Path A rejected everywhere. String deepen after BB is weak
    (strchr export-only; strlen EXT-only). PE/TCP/AV leftovers stay ban/deferred.
    Strongest live leaf is fat_name_is_legal — new charset-validate semantic
    (bit0 LFN legality, distinct from U/K short-name gen and AO time), clean
    CF-out ABI, excellent differential (UTF-8 high-bit skip quirk), called from
    fat_CreateFile when generating short names for new LFNs.
Why this is a genuine migration boundary:
    Deterministic walk of UTF-8 name against fat_legal_chars bit0. Predicate
    form stays reloc-free (no .rodata table). Complements U/K without claiming
    FAT ownership.
Why Path A / Path B:
    Path B — one validate leaf. FAT mount, directory I/O, and LFN orchestration
    stay FASM.
Regression risks:
    REG-001: ECX/EDX preserve (FASM never touched them; Rust stdcall clobbers).
    CF polarity: 1→stc legal / 0→clc illegal (inverse of Cut K exhausted sense).
    Table fidelity: space=1 LFN-ok; '"'/'|' reject; '{'/`~` accept (iglobal).
    REG-003: smoke uses synthetic iglobal strings only.
CPU/interrupt-state risks:
    None in leaf — pure memory walk; no cli/sti.
Shared-state risks:
    None — read-only name walk; no globals in Rust path (predicate, not table).
Concurrency/locking risks:
    None.
Required differential tests:
    Independent FASM-flow oracle (table bit0 + high-bit skip);
    empty; space; LFN-only (+,;); illegal (*,"|,/); UTF-8 high bytes;
    50k PRNG seed 0x43554342 ('CUBC').
Required ABI tests:
    Marker FNIL; synthetic names; ECX/EDX (+EBX/ESI/EDI/EBP) canaries; CF both
    polarities; no live FAT mutation.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise.
Required real subsystem validation:
    Desktop boot on FAT floppy; note create-path leaf needs file create —
    report PARTIAL / NOT AVAILABLE if create harness absent.
Rejected alternatives:
    strchr (no kernel callers); strlen/strncpy deepen; get_proc_ex; tcp_outflags;
    swap_bytes; is_string_userspace; Path A clusters; ban-list.
Expected legacy ABI:
    call with ESI→NUL name; CF=1 legal / CF=0 illegal; preserves ESI/EBX/ECX/EDX/EDI/EBP;
    clobbers EAX/flags; plain ret.
Expected Rust ABI:
    stdcall rust_fat_name_is_legal(name) → EAX 1/0; ret 4;
    trampoline preserves ECX/EDX and maps EAX→stc/clc.
Differential-testing strategy:
    Independent oracle mirroring FASM lodsb/js/table-bit0; 50k 'CUBC'.
ABI-risk assessment:
    Low–med — FAT create path; REG-001; CF polarity; table edge cases.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **ECX+EDX** preserve and **CF** map;
`USE_RUST_FAT_NAME_IS_LEGAL` rollback.

---

## Out of scope

* Claiming Path A for FAT / LFN create
* Migrating `fat_gen_short_name` callers / `bdfe_to_fat_*` calendar ban
* Migrating `strchr` / `strlen` / `strncpy` / `tcp_outflags`
* Beginning Cut BD
