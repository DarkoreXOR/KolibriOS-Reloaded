# Cut AB Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ab-implementation.md`](cut-ab-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AB** migrates the ESI-advancing UTF-8→UTF-16 streaming
> decoder — `utf8to16` in `parse_fn.inc`.  
> Cuts A–AA remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `utf8to16` |
| **Source** | [`kernel/fs/parse_fn.inc:386–419`](../../kernel/fs/parse_fn.inc) |
| **Subsystem** | FS/GUI path & string decode |
| **Purpose** | Decode one UTF-8 character; advance `ESI`; return UTF-16 code unit in `AX` |

---

## Post-AA candidate audit (live tree)

### Coverage already proven (A–AA)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode encode+decode(length-bounded) / casefold / string | A–D |
| Net checksum / TCP RTT + persist timer | E, F, M, V |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next+gen / XFS BE unpack + hash search | I–K, R, U, W |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace region gate | P |
| EDI-advancing UTF-16→UTF-8 + SF | Q |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| UTF-8→FAT 8.3 SM + pushad/popad | U |
| CPU/TSS I/O-bitmap BTR/BTS | X |
| PE/COFF section walk + DIR32/REL32 buffer patch | Y |
| MBR/EBR partition validate + CF + 64-bit capacity | Z |
| Process-table TID walk + signed `jle` | AA |

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer Stage-4** | Forward-only `rep movsd`/`movsb`; EAX/EBX/ECX preserve; not C `memmove`; cross-subsystem fanout |
| `get_pg_addr` | 15 | **defer** | Stage-4 VA→PA; `page_tabs` / `OS_BASE` coupling |
| `net_ptr_to_num4` | 12 | **defer** | Thin device-list scan; packet-hot |
| `v86_get_lin_addr` | 15 | **defer** | Stage-4 PTE walk; same blast class as `get_pg_addr` |
| `ntfs_test_bootsec` | 2 | **defer (#2)** | Strong FS bootsec validate+CF; selected AB prefers streaming ABI novelty |
| `ipv4_route` | 4 | **defer (#3)** | New net routing class; Stage-5 foothold after AB |
| `uni2ansi_char` | 11 | **defer** | Same `parse_fn` family; scalar map; pick streaming first |
| `irq_eoi` / `enable_irq` | 4 / 5 | **defer** | HW PIC/APIC; weak synthetic oracle |
| `mutex_init` | ~30 | **reject** | Trivial 3-store |
| `strtoint_dec` | 0 live | **reject** | `conf_lib.inc` not linked |

### `memmove` special evaluation

| Property | Finding |
|----------|---------|
| Implementation | Forward-only `rep movsd`/`movsb` — **not** bidirectional C `memmove` |
| Overlap | Correct for left-shift (`src = dest+N`); wrong for dest>src |
| ECX | Byte count; signed `test`/`jle` early-out |
| Callers | 24 across kernel/FS/GUI/HID |
| Cut AB | **DEFER** — preferred memory class, wrong blast radius; do not “fix” overlap |

### Ranked top three

| Candidate | Subsystem | Callers | New class | Diff | ABI smoke | QEMU | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-----------|------|-------|---------|
| `utf8to16` | FS/GUI strings | 16 | **ESI-advancing UTF-8→UTF-16 stream** | Excellent | Easy AX/ESI | Path/font | Med | **SELECT** |
| `ntfs_test_bootsec` | NTFS mount | 2 | FS bootsec validate+CF | Excellent | Easy CF | Mount | Low | Defer (#2) |
| `ipv4_route` | IPv4 | 4 | On-link/gateway route | Excellent | Device table | Net | Low–med | Defer (#3) |

```text
Selected target:
    utf8to16

Source:
    kernel/fs/parse_fn.inc

Subsystem:
    FS/GUI path & string decode

Why selected:
    First ESI-advancing UTF-8→UTF-16 streaming decoder class; explicit Cut-A
    leftover; complements Cut Q (EDI-advancing UTF16to8); 16 live callers across
    taskman/FAT/NTFS/exFAT/ISO/LFN/font; pure transform; excellent differential;
    reloc-free via ESI inout pointer.

Candidate #2 and rejection reason:
    ntfs_test_bootsec — strong contained validate+CF class, but AB prefers the
    new streaming pointer-advance ABI over another structural validate after Z.

Candidate #3 and rejection reason:
    ipv4_route — valuable Stage-5 net routing foothold; deferred so AB expands
    the streaming encode/decode envelope first.

Legacy ABI:
    call / ret
    in:  ESI → UTF-8 byte stream (advances)
    out: AX = UTF-16 code unit (EAX high bits algorithm-dependent)
    preserves: EBX, ECX, EDX, EDI, EBP (untouched)
    clobbers: EAX, ESI, flags
    quirks: invalid-lead restart; mid-stream ASCII → .got (xor ah,ah);
            continuation gather via shl/jc; 2-byte vs 3-byte via shl ax,3 CF;
            incoming EAX high bits can affect 3-byte path (shl eax,3)

Critical invariants:
    Exact FASM bit shifts / CF branches (not unicode.utf8.decode)
    ESI advances by consumed bytes (incl. restart skips)
    .got clears AH only
    Flags unspecified to callers (clobbered)

Rust strategy:
    Freestanding utf8to16(+_ptr) with esi_inout + initial_eax
    → EAX result; *esi advanced

Trampoline strategy:
    push preserve regs; stack slot for ESI; stdcall rust_*( &esi, eax );
    pop ESI; EAX = result

Differential strategy:
    Independent FASM-flow bit oracle
    ASCII / 2-byte / 3-byte / invalid lead restart / mid-ASCII abort /
    overlong / chained initial_eax / empty-adjacent
    50k PRNG seed 0x43555442 ('CUTB')

ABI smoke strategy:
    Public utf8to16 + direct rust_*; ESI advance; AX values;
    EBX/ECX/EDX/EDI/EBP preserve; adversarial multi-byte + restart

QEMU strategy:
    CoW from cut-aa-final.img; OFF then ON; desktop + e1000
    Target path: font UTF-8 draw + LFN/path decode smoke

Production gate:
    USE_RUST_UTF8TO16

Rollback gate:
    USE_RUST_UTF8TO16 = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline; `USE_RUST_UTF8TO16` rollback switch.

---

## Out of scope

* Migrating `memmove` / `get_pg_addr` / `net_ptr_to_num4` / `ntfs_test_bootsec` /
  `ipv4_route` / `uni2ansi_char`
* Beginning Cut AC  
* Changing forward-only `memmove` overlap semantics
* Replacing quirky restart/invalid behavior with strict UTF-8
