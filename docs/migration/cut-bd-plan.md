# Cut BD Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bd-implementation.md`](cut-bd-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BD** migrates TCP state→header-flags lookup —
> `tcp_outflags` in `kernel/network/tcp_subr.inc`.  
> Cuts A–BC remain complete and must not be redone. Do not start Cut BE.

---

## Post-BC migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | **EAX** is live socket across call (must preserve); **EDX is OUT** (flags); preserve **EBX/ECX** (window/data live in `tcp_output`) |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — network leaf |
| REG-003 | ABI smoke mutates live globals | Smoke uses **stack synthetic TCP_SOCKET** only — never touch live `net_*` / socket list (REG-003 class) |
| Cut M/V | TCP timers | Complete; this leaf is **state→flags table**, not timer deepen of SRTT/persist math |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**58 / 135** (`58` enabled `[[rust.migrations]]` = Cut A four symbols + B–BC).
No drift found; inventory remains authoritative.

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY+this network Path A? | **No** — leaves ≠ protocol ownership |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV AHCI Path A? | **No** |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+string leaves as Path A? | **No** |
| P+AZ+`is_string_userspace` Stage-3 Path A? | **No** — thin sibling ≠ façade ownership |
| Strongest remaining **live** leaf? | **Yes** — `tcp_outflags` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `strchr` / `strnlen` | Export-only — no kernel callers |
| `strlen` / `strncpy` | String deepen after BB; EXT-only / shmem-only soak |
| `is_string_userspace` | Thin P sibling (repeatedly #5) |
| `v86_get_lin_addr` | Stage-4 trivial address math; BIOS/V86 soak only |
| `swap_bytes_in_words` | AV trivial deepen |
| `get_proc_ex` / `coff_get_align` | PE ban / thin PE glue |
| `hotkey_do_test` | Indirect call table — reloc-hostile |
| `iso9660_copy_name` | AJ glue + `uni2ansi` ban path + REG-002 |
| `ext_*` / `fsGetTime` | No `--disk ext`; calendar/CMOS caution |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BC)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `tcp_outflags` | TCP | 1 | **State→flags table** | Excellent | Low | Partial net / desktop | **SELECT** |
| `is_string_userspace` | Stage-3 | 1 | String NUL-scan gate | Good | Low | load_library | #2 thin |
| `v86_get_lin_addr` | Stage-4 / V86 | 14 | PTE→linear | Excellent | Low | BIOS/V86 weak | #3 address-math |
| `coff_get_align` | PE / DLL | 2 | Align-mask decode | Excellent | Low | `.sys` load | #4 thin PE |
| `swap_bytes_in_words` | AHCI util | 1 | Endian word-swap | Excellent | Low | `--bus ahci` | #5 AV deepen |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: tcp_outflags
Source: kernel/network/tcp_subr.inc
Subsystem: TCP state → TCP header flags (tcp_output send path)
Stage: Stage-5 / net protocol leaf
Why selected:
    Post-BC audit: Path A rejected everywhere. String deepen / thin P sibling /
    Stage-4 address math / PE leftovers stay weaker. Strongest remaining live
    leaf is tcp_outflags — new TCP semantic (11-byte state→flags table), distinct
    from Cuts M/V timer arithmetic, clean EAX-in/EDX-out ABI, excellent
    differential (11 TCPS_* states), called from tcp_output hot path with
    EAX/EBX/ECX live across the call (REG-001 discipline).
Why this is a genuine migration boundary:
    Deterministic dword t_state → byte flags lookup. Reloc-free via inlined
    11-byte table (no PC-relative .flaglist). Complements M/V without claiming
    TCP protocol ownership.
Why Path A / Path B:
    Path B — one flags leaf. TCP input/output/timers/socket list stay FASM.
Regression risks:
    REG-001: preserve EAX (socket), EBX (SND delta), ECX (window); EDX = flags OUT.
    Reloc: must NOT leave .flaglist as PC-relative label in blob — inline table.
    Out-of-range t_state: FASM reads past .flaglist into following code — Rust
    defines only 0..=10; document limitation.
    REG-003: smoke uses stack synthetic socket only (never live net_device_list /
    socket list).
CPU/interrupt-state risks:
    None — pure memory read; no cli/sti.
Shared-state risks:
    Read-only t_state; no globals in Rust path.
Concurrency/locking risks:
    None in leaf (caller owns socket).
Required differential tests:
    Independent FASM-flow oracle (11-byte table); all TCPS_* 0..10;
    50k PRNG seed 0x43555446 ('CUDF' / Cut BD) over state domain.
Required ABI tests:
    Marker TCPF; synthetic socket; EAX/EBX/ECX/ESI/EDI/EBP canaries;
    EDX = expected flags; no live network mutation.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise; prior cut-bc-final.img.
Required real subsystem validation:
    Desktop boot with network stack init; note full TCP handshake soak may be
    PARTIAL if no remote peer — report honestly.
Rejected alternatives:
    is_string_userspace (thin); v86_get_lin_addr; coff_get_align; swap_bytes;
    string deepen; Path A clusters; ban-list.
Expected legacy ABI:
    call with EAX→TCP_SOCKET; EDX=flags (DL); preserves EAX/EBX/ECX/ESI/EDI/EBP;
    clobbers EDX/flags; plain ret.
Expected Rust ABI:
    stdcall rust_tcp_outflags(socket) → EAX=flags; ret 4;
    trampoline preserves EAX/EBX/ECX and moves result to EDX.
Differential-testing strategy:
    Independent oracle mirroring FASM movzx table; 50k PRNG.
ABI-risk assessment:
    Med — tcp_output hot path; REG-001 EAX/EBX/ECX; EDX polarity as OUT.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **EAX/EBX/ECX** preserve and **EDX=flags**;
`USE_RUST_TCP_OUTFLAGS` rollback.

---

## Out of scope

* Claiming Path A for TCP / `tcp_output`
* Migrating `tcp_mss` / `tcp_output` / M/V deepen
* Migrating `is_string_userspace` / string leaves
* Beginning Cut BE
