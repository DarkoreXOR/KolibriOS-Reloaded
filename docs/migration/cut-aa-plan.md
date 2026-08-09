# Cut AA Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-aa-implementation.md`](cut-aa-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AA** migrates process TID→slot lookup —
> `pid_to_slot`, which walks `SLOT_BASE` by `APPDATA.tid` during terminate /
> IPC / debug / events / syscalls.  
> Cuts A–Z remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `pid_to_slot` |
| **Source** | [`kernel/core/taskman.inc:521–556`](../../kernel/core/taskman.inc) |
| **Subsystem** | Process / taskman (TID→slot) |
| **Purpose** | Linear scan of `SLOT_BASE` for matching `APPDATA.tid`; return slot index or 0 |

---

## Post-Z candidate audit (live tree)

### Coverage already proven (A–Z)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string / UTF stream | A–D, Q |
| Net checksum / TCP RTT + persist timer | E, F, M, V |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next+gen / XFS BE unpack + hash search | I–K, R, U, W |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace region gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| EDI-advancing calendar inverse | T |
| UTF-8→FAT 8.3 SM + pushad/popad | U |
| TCP persist-timer arming / clamp / sticky flag | V |
| Binary search + EAX+ZF dual return + BE table walk | W |
| CPU/TSS I/O-bitmap BTR/BTS privilege state | X |
| PE/COFF section walk + DIR32/REL32 buffer patch | Y |
| MBR/EBR partition validate + CF + 64-bit capacity | Z |

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer Stage-4** | Forward-only `rep movsd`/`movsb`; EAX/EBX/ECX preserve; cross-subsystem fanout; not overlap-safe C `memmove` |
| `get_pg_addr` | 15 | **defer (#2)** | Strong Stage-4 VA→PA foothold; higher blast + `page_tabs` coupling |
| `net_ptr_to_num4` | 12 | **defer (#3)** | Thin device-list scan; packet-hot fanout |
| `is_protective_mbr` | 1 | **reject** | Anti-cluster after Cut Z |
| `is_string_userspace` | 1 | **defer** | Completes P family; thinner novelty |
| `get_coff_sym` | 3 | **reject** | Anti-cluster after Cut Y |
| `blit_clip` / `set_window_clientbox` | 1 / 3 | **reject** | GUI geometry cluster |
| `irq_eoi` / `enable_irq` | 4 / 5 | **defer** | HW PIC/APIC; weak synthetic oracle |
| `mutex_init` | ~30 | **reject** | Trivial 3-store init |
| `pci_make_config_cmd` | 2 | **reject** | Trivial scalar |
| `r_f_port_area` | 2 | **defer** | Builds on Cut X; heavier side effects |
| `strtoint_dec` | internal | **defer** | Strong parser class; smaller production surface |

### `memmove` special evaluation

| Property | Finding |
|----------|---------|
| Implementation | Forward-only `rep movsd`/`movsb`; **not** bidirectional C `memmove` |
| Overlap | Correct for left-shift (`src = dest+N`); wrong for dest>src overlap |
| ECX | Byte count; `test ecx,ecx` / `jle .ret` (signed ≤0 early-out) |
| ESI/EDI | Used internally; restored from stack |
| EAX/EBX | Preserved (inputs remain) |
| Callers | 24 across kernel/FS/GUI/HID |
| Blast | Stage-4 memory class — not contained incremental util |
| Cut AA | **DEFER** — preferred class, wrong blast radius; do not “fix” overlap |

### Ranked top three

| Candidate | Subsystem | Callers | New class | Differential | ABI smoke | QEMU | Blast | Risk | Verdict |
|-----------|-----------|--------:|-----------|--------------|-----------|------|-------|------|---------|
| `pid_to_slot` | Process/taskman | 8 | **SLOT_BASE TID walk** | Excellent | Easy EAX | Syscall/IPC | Med | Med | **SELECT** |
| `get_pg_addr` | Memory | 15 | VA→PA Stage-4 | Excellent | Easy | Everywhere | Med–high | Med | Defer (#2) |
| `net_ptr_to_num4` | Network device | 12 | Device-list scan | Excellent | Easy EDI | Packet path | Med | Med | Defer (#3) |

```text
Selected target:
    pid_to_slot

Source:
    kernel/core/taskman.inc

Subsystem:
    Process / taskman (TID → slot index)

Why selected:
    First process-table walk class (Stage 6 foothold); 8 live callers across
    terminate/IPC/debug/events/syscalls; exact linear scan with signed jle bound;
    reloc-free via trampoline-injected SLOT_BASE + thread_count; strong
    differential; contained blast vs memmove.

Why #2 was rejected:
    get_pg_addr — valuable Stage-4 memory class, but 15 callers and page_tabs /
    OS_BASE coupling raise blast vs a self-contained TID walk.

Why #3 was rejected:
    net_ptr_to_num4 — thin pointer scan with 12 packet-hot callers; less
    algorithmic substance than process-table semantics.

Legacy ABI:
    call / ret
    in:  EAX = TID (pid)
    out: EAX = slot index (1..thread_count) or 0
    preserves: EBX, ECX (explicit); EDX/ESI/EDI/EBP de facto (untouched)
    clobbers: flags
    skips slot 0; scans offsets sizeof.APPDATA .. thread_count*sizeof.APPDATA
    inclusive via signed jle; skips TSTATE_FREE; dword TID match

Critical invariants:
    sizeof.APPDATA = 256 (BSF = 8)
    APPDATA.tid @ +112; APPDATA.state @ +124; TSTATE_FREE = 9
    Signed bound compare (jle), not unsigned
    Read-only; no memory writes

Rust strategy:
    Freestanding pid_to_slot(+_ptr) → EAX slot/0
    slot_base + thread_count passed as stack args (reloc-free)

Trampoline strategy:
    push ebx/ecx/edx/esi/edi/ebp
    stdcall rust_*(eax, SLOT_BASE, [thread_count])
    pop; EAX = result

Differential strategy:
    Independent FASM-flow oracle on synthetic APPDATA tables
    Empty / single / multi / free-skip / first-match / bound edges / signed jle
    50k PRNG seed 0x43555441 ('CUTA')

ABI smoke strategy:
    Synthetic table via rust_pid_to_slot
    Live SLOT_BASE after OS/IDLE setup via public pid_to_slot
    EBX/ECX/EDX/ESI/EDI/EBP preserve; missing TID → 0

QEMU strategy:
    CoW from cut-z-final.img; OFF then ON; desktop + e1000
    Target path: sysfn_pid_to_slot + boot SLOT walk smoke

Rollback gate:
    USE_RUST_PID_TO_SLOT = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register→stdcall trampoline; `USE_RUST_PID_TO_SLOT` rollback switch.

---

## Out of scope

* Migrating `memmove` / `get_pg_addr` / `net_ptr_to_num4` / `pid_to_appdata`  
* Migrating `is_protective_mbr` / PE-COFF / GUI geometry siblings  
* Beginning Cut AB  
* Changing forward-only `memmove` overlap semantics  
