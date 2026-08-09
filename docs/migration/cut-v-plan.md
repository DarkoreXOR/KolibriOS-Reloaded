# Cut V Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-v-implementation.md`](cut-v-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut V** is the first migration of TCP **persist-timer arming** — `tcp_set_persist`, which consumes Cut M’s SRTT/RTTVAR to arm the zero-window persist timer under mutual exclusion with retransmission.  
> Cuts A–U remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `tcp_set_persist` |
| **Source** | [`kernel/network/tcp_subr.inc:469–500`](../../kernel/network/tcp_subr.inc) |
| **Subsystem** | Network / TCP timers |
| **Purpose** | Arm/restart persist timer from SRTT/RTTVAR; OR persist flag; bump `t_rxtshift` |

---

## Post-U candidate audit (live tree)

### Coverage already proven (A–U)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string / UTF stream | A–D, Q |
| Net checksum / TCP RTT estimator | E, F, M |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next+gen / XFS BE unpack | I–K, R, U |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| EDI-advancing calendar inverse | T |
| UTF-8→FAT 8.3 SM + pushad/popad | U |

### Ranked remaining candidates

| Candidate | Callers | Novelty | Differential | Smoke | QEMU | Blast | Risk | Verdict |
|-----------|---------|---------|--------------|-------|------|-------|------|---------|
| `tcp_set_persist` | 2 | **High–med** — persist RTO arming; consumes M’s SRTT/RTTVAR | Excellent | Easy | Strong net | Low | Low | **SELECT** |
| `coff_get_align` | 2 | High domain / trivial body | Excellent | Easy | Med DLL | Low | Low | Defer (#2) |
| `set_io_access_rights` | 2 | High — TSS I/O bitmap | Good | Med | Sys46 | Low* | **High** | Defer (#3) |
| `net_ptr_to_num4` | 12 | Med–high NEW — device index scan | Excellent | Easy | Strong net | Med–high | Low | Defer |
| `xfs_hashname` | 4 | Med NEW — ROL7+XOR | Excellent | Easy | Weak XFS | Low | Low | Defer (thin) |
| `is_string_userspace` | 1 | Med P+scasb | Good | Easy | Narrow | Low | Med | Defer |
| `set_window_clientbox` | 3 | Med GUI after S | Good | Med | Strong fn0 | Low | Med | Reject |
| `pci_make_config_cmd` | 2 | Trivial scalar | Excellent | Easy | Strong boot | Low | Low | Reject |
| `blit_clip` / `fat_time_to_bdfe` | 1 / 5 | Low H / pack | Excellent | Easy | Weak/Med | Low | Low | Reject |
| `mutex_init` / `memmove` | 35 / 24 | High/Low | Easy | Easy | Everywhere | **Very high** | Med | Stage-4 |
| `strtoint_dec` | **0** | — | — | — | — | — | — | **Dead** (`conf_lib` still commented out) |

\*Low call-site count, high system impact if wrong.

### Why this target beats the alternatives

* After U (FAT naming SM), another calendar / GUI / FAT / packed-field leaf adds little.
* `tcp_set_persist` opens **persist-timer arming** (not another RTT twin of M): retransmit mutual exclusion, `(srtt>>2 + rttvar)>>1 << rxtshift`, `tcpt_rangeset` clamp, sticky persist flag, bounded `t_rxtshift++`.
* Stronger algorithmic substance than `coff_get_align` / `pci_make_config_cmd`.
* Contained blast (2 callers: `tcp_timer.inc`, `tcp_output.inc`).
* Reuses locked Cut M socket offsets; reloc-free A+C pattern proven.

---

## Implementation plan

```text
Selected target:
    tcp_set_persist

Why selected:
    Persist-timer arming leaf; consumes M’s SRTT/RTTVAR; new TCP timer policy class;
    excellent differential; low blast; real network callers.

Why alternatives were rejected:
    coff_get_align — Stage-8 foothold but trivial scalar body;
    set_io_access_rights — TSS privilege risk outweighs ~10-insn body;
    net_ptr_to_num4 — new but thin linear scan + 12-caller fanout;
    set_window_clientbox / fat_time_to_bdfe / pci_* — banned classes;
    memmove / mutex_init — Stage-4 fanout;
    strtoint_dec — unlinked (conf_lib commented out).

Legacy ABI:
    EAX → TCP_SOCKET*;
    if timer_flags & retransmission → no-op ret;
    else compute RTO, clamp timer_persist to [8,94], OR persist flag,
    inc t_rxtshift if < 12;
    EBX preserved; EAX retained; ECX may be clobbered; plain ret.

Critical invariants:
    Retransmit and persist mutually exclusive (early exit on retransmit bit);
    unsigned jb/ja rangeset clamp;
    x86 SHL with CL (count & 31);
    byte t_rxtshift compare/inc;
    callers (tcp_timer) require EAX preserved as socket.

Rust strategy:
    Freestanding mutate via locked offsets; stdcall (socket*);
    constants as immediates (no .rodata).

Trampoline strategy:
    push ebx/ecx/edx/eax; stdcall rust_tcp_set_persist, eax; pop restore; ret.
    USE_RUST_TCP_SET_PERSIST gate (dev default 0 → production 1).

Differential strategy:
    Independent FASM-flow oracle vs Rust; named + PRNG corpus;
    covers retransmit gate, min/max clamp, shift edges, rxtshift saturate.

Smoke strategy:
    Public ABI vectors on stack fake socket; EAX/EBX/ECX/EDX preserve;
    hang-on-fail marker.

QEMU strategy:
    cut-u-final.img lineage; OFF then ON desktop + network regression.
    Persist path may need zero-window traffic — state honestly if not forced.

Rollback:
    USE_RUST_TCP_SET_PERSIST = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin register ABI trampoline; `USE_RUST_TCP_SET_PERSIST` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | Regcall leaf, plain `ret` |
| Register in | **EAX** → `TCP_SOCKET*` |
| Out | Mutates `timer_persist`, `timer_flags`, maybe `t_rxtshift` |
| Preserved | **EAX**, **EBX** (FASM push/pop); trampoline also saves ECX/EDX |
| Clobbers | ECX (CL shift) in legacy body; flags unspecified |
| Callees | none (`tcpt_rangeset` is inline macro) |

Locked offsets (FASM struct audit / Cut M):

| Field | Offset |
|-------|--------|
| `t_rxtshift` | 118 |
| `t_srtt` | 210 |
| `t_rttvar` | 214 |
| `timer_flags` | 254 |
| `timer_persist` | 262 |

Constants: `TCP_time_pers_min=8`, `TCP_time_pers_max=94`, `TCP_max_rxtshift=12`, `timer_flag_retransmission=1`, `timer_flag_persist=8`.

---

## Out of scope

* Migrating `coff_get_align` / `set_io_access_rights` / `net_ptr_to_num4`  
* Migrating `tcp_output` / full timer wheel  
* Cut W  

---

## Completion rule

Complete Cut V gates → document → **STOP**. Do not start Cut W.
