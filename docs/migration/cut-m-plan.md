# Cut M Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-m-implementation.md`](cut-m-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut M** is the first migration of a **TCP protocol algorithm leaf** — RFC793-style fixed-point SRTT/RTTVAR update with unsigned `add`/`ja` clamp and in-place socket-field mutation.  
> Cuts A–L remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `tcp_xmit_timer` |
| **Source** | [`kernel/network/tcp_subr.inc:515–575`](../../kernel/network/tcp_subr.inc) |
| **Subsystem** | Network / TCP protocol (RTT estimator) |
| **Purpose** | Update smoothed RTT (`t_srtt`) and RTT variance (`t_rttvar`) from a measured sample. |

---

## Candidate comparison

### Candidate 1: `tcp_xmit_timer` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `network/tcp_subr.inc:515–575` |
| Purpose | RFC793 fixed-point SRTT/RTTVAR; init vs update paths; unsigned clamp-to-1 |
| Complexity | Dual-field mutate; signed abs via `cdq`; `add`/`ja` wrap+zero clamp |
| Callers | 5 (`tcp_input.inc`) |
| ABI | Regcall: `EAX`=rtt, `EBX`→socket; void; preserves `EBX`/`ECX`/`EDX`; plain `ret` |
| Deps | Socket field offsets; global `TCPS_rttupdated` (trampoline-owned) |
| Reloc risk | **None** if trampoline increments the stats counter |
| Compiler helper risk | Low (arithmetic + volatile dword stores) |
| Risk | Low–med (exact unsigned clamp + abs idiom) |

### Candidate 2: `memmove` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Source | `kernel.asm:3236–3268` |
| Why rejected | Strong memcpy-helper-risk probe (~23 callers), but algorithmically thin vs TCP estimator novelty; deferred again for a dedicated helper-risk cut. |

### Candidate 3: `coff_get_align` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Source | `core/dll.inc:820–839` |
| Why rejected | Novel PE/loader leaf and Stage 8 foothold, but tiny bitfield→mask with thin ABI pressure after A–L. |

### Candidate 4: `antiAliasing` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Source | `gui/font.inc:846–862` |
| Why rejected | GUI color blend (not blitter geometry) with quirky BP/EBP contract; deferred in favor of TCP protocol depth. |

### Candidate 5: `pci_make_config_cmd` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Source | `bus/pci/pci32.inc:135–140` |
| Why rejected | New PCI subsystem but too thin (few bit ops). |

### Candidate 6: `tcp_outflags` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Source | `network/tcp_subr.inc:251–271` |
| Why rejected | Inline `.flaglist` table → `.rodata`/reloc risk; weaker than `tcp_xmit_timer`. |

### Candidate 7: `UTF16to8` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Why rejected | FS-parse path after G/I/J/K FS weight; Cut M prefers non-FS. |

### Candidate 8: `hotkey_test*` / `hotkey_do_test` — rejected for Cut M

| Field | Detail |
|-------|--------|
| Why rejected | HID adjacency after Cut L; IRQ/table/reloc risks on `hotkey_do_test`. |

### Candidate 9: timers / clipboard / sound / sync leaves — rejected

| Field | Detail |
|-------|--------|
| Why rejected | Port I/O, allocators, locks, or `change_task` — not Strategy A+C leaves. |

---

## Why Cut M is a meaningful next step

Cuts A–L proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry
/ NTFS VLE MCB / NTFS USA / FAT DF+CF short-name / HID mouse accel
```

Cuts E/F touched network only as **Internet checksum** leaves.

Cut M must answer a **different** question:

> Does Strategy A + C remain viable for a **TCP protocol estimator leaf** (dual socket-field mutate; RFC793 fixed-point arithmetic; unsigned `add`/`ja` clamp; trampoline-injected stats counter) as a reloc-free blob with a byte-exact differential oracle?

`tcp_xmit_timer` is the right probe:

1. **Different property within network** — protocol state transform, not checksum  
2. **Outside FS / video / HID** — matches Cut M preference  
3. **New architectural property** — first dual-field in-place protocol estimator; trampoline owns `TCPS_rttupdated` (Cut L-style global materialization)  
4. **Strategy A+C fit** — no tables; no HW ports; pure arithmetic + dword stores  
5. **Limited blast radius** — 5 callers inside `tcp_input.inc`  
6. **Testability** — synthetic socket buffers; init + update + clamp + wrap; large PRNG  

---

## Strategy

**A + C** (unchanged architecture):

* Freestanding Rust → reloc-free extract → FASM `file` embed  
* FASM trampoline preserves public `tcp_xmit_timer` ABI  
* `USE_RUST_TCP_XMIT_TIMER` rollback switch  

---

## ABI (planned)

### Public FASM `tcp_xmit_timer`

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` = RTT sample; `EBX` → `TCP_SOCKET` |
| Output | void (mutates `t_srtt`, `t_rttvar`) |
| Side effect | `inc [TCPS_rttupdated]` (trampoline) |
| Preserved | `EBX`, `ECX`, `EDX` (callers keep `EDX` as TCP header ptr) |
| Reads | `[ebx+TCP_SOCKET.t_rtt]` (gate), `t_srtt`, `t_rttvar` |

### Locked field offsets (FASM struct audit)

| Field | Offset |
|-------|--------|
| `t_idle` | 198 |
| `t_rtt` | 202 |
| `t_rtseq` | 206 |
| `t_srtt` | 210 |
| `t_rttvar` | 214 |

### Rust `rust_tcp_xmit_timer`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(rtt: u32, socket: *mut u8)` |
| Epilogue | `ret 8` |
| Globals | none (stats counter in trampoline) |

### Trampoline sketch

```asm
tcp_xmit_timer:
        inc     [TCPS_rttupdated]
        push    ecx
        push    edx
        stdcall rust_tcp_xmit_timer, eax, ebx
        pop     edx
        pop     ecx
        ret
```

---

## Out of scope for Cut M

* Migrating other TCP timers / `tcp_mss` / `tcp_outflags`  
* Live TCP handshake / RTT measurement under QEMU network traffic  
* Changing `TCP_SOCKET` layout  
* `memmove` / PE loader / GUI color leaves  

---

## Completion rule

Complete Cut M gates → document → **STOP**. Do not start Cut N.
