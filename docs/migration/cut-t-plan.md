# Cut T Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-t-implementation.md`](cut-t-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut T** is the first migration of a **pointer-advancing calendar inverse** — `fsTime2bdfe`, which converts seconds since 2001-01-01 into an 8-byte BDFE block at `EDI` and returns with **`EDI += 8`**.  
> Cuts A–S remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `fsTime2bdfe` |
| **Source** | [`kernel/fs/fs_common.inc:116–163`](../../kernel/fs/fs_common.inc) |
| **Subsystem** | Filesystem / calendar |
| **Purpose** | Seconds since 2001-01-01 → BDFE `{sec,min,hour,pad,day,month,year}`; advance `EDI` by 8 |

---

## Post-S candidate audit (live tree)

### Coverage already proven (A–S)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string | A–D, Q |
| Net checksum / TCP RTT | E, F, M |
| Calendar BDFE→secs | G |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 / XFS BE unpack | I–K, R |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |

### Ranked remaining candidates

| Candidate | Callers | Novelty | Differential | Smoke | QEMU | Blast | Risk | Verdict |
|-----------|---------|---------|--------------|-------|------|-------|------|---------|
| `fsTime2bdfe` | 4 | **High** — EDI+=8 inverse calendar; completes G pair | Excellent | Easy | Weak stock (XFS/ext/NTFS) | Low | Med | **SELECT** |
| `set_window_clientbox` | 3 | Med–high GUI client/skin | Good | Med | **Strong** fn0 | Low | Med | Defer (#2) |
| `pci_make_config_cmd` | 2 | **High** first PCI | Excellent | Easy | **Strong** boot | Low | Low | Defer (#3; thin body) |
| `coff_get_align` | 2 | High first PE/COFF | Excellent | Easy | Med DLL | Low | Low | Defer |
| `blit_clip` | 1 | Low–med H composition | Excellent | Easy | Weak fn73 | Low | Low | Defer |
| `fat_time_to_bdfe` | 5 | Low bitfield | Excellent | Easy | Med FAT | Low | Low | Defer |
| `is_string_userspace` | 1 | Med P+scasb | Good | Easy | Narrow | Low | Med | Defer |
| `set_io_access_rights` | 2 | High TSS I/O bitmap | Good | Med | Sys46 | Low* | **High** | Defer |
| `mutex_init` | 34 | High fastcall | Easy | Easy | Everywhere | **Very high** | Med | Stage-4 |
| `memmove` | 24 | Low memcpy | Easy | Easy | Everywhere | **High** | Med–high | Stage-4 |
| `strtoint_dec` | 2 | Med parse | Excellent | Easy | Conf only | Low | Med | Defer |

\*Low call-site count, high system impact if wrong.

### Why this target beats the alternatives

* After Cut S, another window/`WDATA` leaf (`set_window_clientbox`) repeats GUI depth; `fsTime2bdfe` adds a **new ABI class** (callee advances `EDI` by 8) and finishes the calendar pair Cut G reserved `months`/`months2` for.
* Stronger algorithmic substance than `pci_make_config_cmd` (four-instruction config encoding).
* Contained blast (4 production sites: XFS×2, ext×1, NTFS `jmp`).
* Reloc-free strategy matches Cut G: stack-materialized month tables; trampoline owns `EDI+=8`.

---

## Implementation plan

```text
Selected target:
    fsTime2bdfe

Why selected:
    EDI+=8 pointer-advancing ABI + calendar inverse; max knowledge / risk.

Why alternatives were rejected:
    set_window_clientbox — same GUI class after S;
    pci_make_config_cmd — new bus, but algorithmically tiny;
    blit_clip / fat_time_to_bdfe — low novelty;
    memmove / mutex_init — Stage-4 fanout;
    set_io_access_rights — TSS privilege risk.

Legacy ABI:
    EAX = secs since 2001-01-01; EDI → 8-byte BDFE out;
    writes sec/min/hour(word)/day/month/year; EDI += 8; plain ret;
    clobbers EBX/ECX/EDX; ESI/EBP unused/preserved.

Critical invariants:
    Hour stored as word at +2 (pad +3 cleared);
    Leap adjust via signed jns after sub day, years/4;
    Month peel uses 16-bit DX (dec dh / jns) over months/months2;
    Year leap test is (year & 3) == 0; EDI advances exactly +8.

Rust strategy:
    Freestanding secs→BdfeTime; stack month tables (Cut G pattern);
    stdcall (secs, out*); trampoline adds EDI,8 after write.

Trampoline strategy:
    stdcall rust_fs_time2bdfe, eax, edi; add edi, 8; ret.
    USE_RUST_FS_TIME2BDFE gate (dev default 0 → production 1).

Differential strategy:
    Independent FASM-flow oracle vs Rust; named + grid + large PRNG;
    roundtrip vs Cut G on valid dates.

Smoke strategy:
    Public ABI vectors; EDI+=8; ESI/EBP preserve; BDFE byte layout.

QEMU strategy:
    cut-s-final.img lineage; OFF then ON desktop regression.
    Stock image does not exercise XFS/ext/NTFS list paths — state honestly.

Rollback:
    USE_RUST_FS_TIME2BDFE = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register ABI trampoline; `USE_RUST_FS_TIME2BDFE` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | Regcall leaf, plain `ret` |
| Register in | **EAX** = seconds since 2001-01-01; **EDI** → BDFE out |
| Out | 8 bytes at original EDI; **EDI = EDI + 8** |
| Layout | `+0 sec, +1 min, +2 hour (word), +4 day, +5 month, +6 year (u16 LE)` |
| Clobbers | EAX, EBX, ECX, EDX, flags (FASM) |
| Preserved | ESI, EBP (untouched by FASM body) |

---

## Out of scope

* Migrating `set_window_clientbox` / `pci_make_config_cmd` / `blit_clip` / `memmove`  
* Migrating `fat_time_to_bdfe` / other FAT date helpers  
* Cut U  

---

## Completion rule

Complete Cut T gates → document → **STOP**. Do not start Cut U.
