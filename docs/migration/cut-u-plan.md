# Cut U Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-u-implementation.md`](cut-u-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut U** is the first migration of a **UTF-8→FAT 8.3 short-name generator** — `fat_gen_short_name`, a non-trivial string state machine that composes already-migrated `cp866toUpper` (Cut B) and `fat_next_short_name` (Cut K).  
> Cuts A–T remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `fat_gen_short_name` |
| **Source** | [`kernel/fs/fat.inc:517–599`](../../kernel/fs/fat.inc) |
| **Subsystem** | Filesystem / FAT naming |
| **Purpose** | UTF-8 LFN → 11-byte 8.3 buffer; lossy → `~N` via `fat_next_short_name` |

---

## Post-T candidate audit (live tree)

### Coverage already proven (A–T)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string / UTF stream | A–D, Q |
| Net checksum / TCP RTT | E, F, M |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next / XFS BE unpack | I–K, R |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| EDI-advancing calendar inverse | T |

### Ranked remaining candidates

| Candidate | Callers | Novelty | Differential | Smoke | QEMU | Blast | Risk | Verdict |
|-----------|---------|---------|--------------|-------|------|-------|------|---------|
| `fat_gen_short_name` | 1 | **High** — UTF-8→8.3 state machine; composes B+K | Excellent | Easy–med | Strong FAT create | Low | Med | **SELECT** |
| `tcp_set_persist` | 2 | High–med — TCP persist/RTO after M | Excellent | Easy | Strong net | Low | Low | Defer (#2) |
| `coff_get_align` | 2 | High — first PE/COFF align | Excellent | Easy | Med DLL | Low | Low | Defer (#3) |
| `set_window_clientbox` | 3 | Med — GUI after S | Good | Med | Strong fn0 | Low | Med | Defer |
| `pci_make_config_cmd` | 2 | High domain / trivial body | Excellent | Easy | Strong boot | Low | Low | Defer |
| `blit_clip` | 1 | Low–med H composition | Excellent | Easy | Weak fn73 | Low | Low | Defer |
| `fat_time_to_bdfe` | 5 | Low packed-field | Excellent | Easy | Med FAT | Low | Low | Reject |
| `is_string_userspace` | 1 | Med P+scasb | Good | Easy | Narrow | Low | Med | Defer |
| `set_io_access_rights` | 2 | High TSS I/O bitmap | Good | Med | Sys46 | Low* | **High** | Defer |
| `mutex_init` | 34 | High fastcall | Easy | Easy | Everywhere | **Very high** | Med | Stage-4 |
| `memmove` | 24 | Low memcpy | Easy | Easy | Everywhere | **High** | Med–high | Stage-4 |
| `strtoint_dec` | 2 | Med parse | Excellent | Easy | Conf only | Low | Med | Defer |

\*Low call-site count, high system impact if wrong.

### Why this target beats the alternatives

* After R/S/T, another calendar, GUI geometry, XFS bitfield, or trivial pack leaf adds little migration knowledge.
* `fat_gen_short_name` introduces a **new class**: multi-flag string state machine (BH lossy/dot), multi-dot rewind, table-gated legality, and **in-crate composition** of Cuts B+K.
* Stronger algorithmic substance than `coff_get_align` / `pci_make_config_cmd`.
* Contained blast (single create-path caller at `fat.inc:2017`).
* Beats `tcp_set_persist` slightly by opening FS naming policy rather than another TCP-timer sibling of M.

---

## Implementation plan

```text
Selected target:
    fat_gen_short_name

Why selected:
    Non-trivial UTF-8→8.3 state machine; composes B+K; max knowledge/risk.

Why alternatives were rejected:
    tcp_set_persist — strong #2, but TCP-timer family after M;
    coff_get_align — PE foothold but thin body;
    set_window_clientbox — GUI class after S;
    fat_time_to_bdfe — trivial pack (user: avoid);
    memmove / mutex_init — Stage-4 fanout;
    set_io_access_rights — TSS privilege risk.

Legacy ABI:
    ESI → UTF-8 NUL-terminated name; EDI → ≥12-byte out (caller `sub esp,12`);
    pushad/popad; fills 12 spaces then writes 8.3; may call fat_next_short_name;
    plain ret; all GPRs restored.

Critical invariants:
    fat_legal_chars bit2 gates short-name chars; high-bit UTF-8 → lossy skip;
    BH flags: bit0 lossy, bit2 had-dot; second+ dot sets BH=3 and rewinds;
    leading '.' is lossy; overflow of field is lossy;
    lossy → fat_next_short_name on basename; cp866toUpper per stored byte.

Rust strategy:
    Freestanding gen; stack-materialized 128-byte fat_legal_chars;
    inline cp866_to_upper + fat_next_short_name (reloc-free, no cross-blob calls);
    stdcall (src*, out*).

Trampoline strategy:
    pushad; stdcall rust_fat_gen_short_name, esi, edi; popad; ret.
    USE_RUST_FAT_GEN_SHORT_NAME gate (dev default 0 → production 1).

Differential strategy:
    Independent FASM-flow oracle vs Rust; named + PRNG corpus;
    covers dots, illegal, long, lossy→~N, leading-dot.

Smoke strategy:
    Public ABI vectors; 11-byte 8.3 layout; GPR preserve via pushad.

QEMU strategy:
    cut-t-final.img lineage; OFF then ON desktop regression.
    Stock image may not exercise FAT create — state honestly; use smoke + differential.

Rollback:
    USE_RUST_FAT_GEN_SHORT_NAME = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register ABI trampoline; `USE_RUST_FAT_GEN_SHORT_NAME` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | Regcall leaf wrapped in `pushad`/`popad`, plain `ret` |
| Register in | **ESI** → UTF-8 name; **EDI** → out buffer (≥12 bytes writable) |
| Out | 11-byte 8.3 at EDI; FASM also writes a 12th space (match) |
| Clobbers | None visible (pushad/popad); flags unspecified |
| Callees | `cp866toUpper`, conditional `fat_next_short_name` |

---

## Out of scope

* Migrating `fat_name_is_legal` / `fat_time_to_bdfe` / `tcp_set_persist` / `coff_get_align`  
* Migrating `set_window_clientbox` / `memmove`  
* Cut V  

---

## Completion rule

Complete Cut U gates → document → **STOP**. Do not start Cut V.
