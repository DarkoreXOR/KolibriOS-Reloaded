# Cut X Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-x-implementation.md`](cut-x-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut X** migrates TSS **I/O permission bitmap** updates — `set_io_access_rights`, which enables/disables port access bits for syscall 46 (`r_f_port_area`).  
> Cuts A–W remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `set_io_access_rights` |
| **Source** | [`kernel/kernel.asm:3289–3302`](../../kernel/kernel.asm) |
| **Subsystem** | CPU / TSS I/O permission bitmap |
| **Purpose** | `BTR`/`BTS` one port bit in `tss._io_map_0` (enable / disable I/O access) |

---

## Post-W candidate audit (live tree)

### Coverage already proven (A–W)

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

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer (#2)** | Preferred memory class; Stage-4 fanout still too large |
| `blit_clip` | 1 | **reject** | Cut H composition; GUI geometry already proven |
| `fat_time_to_bdfe` | 5 | **reject** | Trivial pack / calendar-adjacent |
| `is_string_userspace` | 1 | **defer** | Syscall ZF+scasb; thinner novelty after P |
| `set_io_access_rights` | 2 | **promote** | First CPU/TSS I/O-bitmap class; contained |
| `pci_make_config_cmd` | 2 | **reject** | Device domain but trivial scalar |
| `mutex_init` | 34 | **defer** | Stage-4 sync / export surface |
| `coff_get_align` | 2 | **reject** | PE foothold but trivial Characteristics→mask |
| `strtoint_dec` | 0 prod | **reject** | Dead — `conf_lib.inc` still commented out |
| `net_ptr_to_num4` | 12 | **defer** | Device index scan; thin loop + fanout |
| `xfs_hashname` | 4 | **reject** | Anti-cluster after R/W; thin ROL7 hash |
| `get_coff_sym` | 2 | **defer (#3)** | PE symbol scan; good foothold, thinner than TSS class |

### Ranked top three

| Candidate | Subsystem | Callers | New class | Differential | ABI smoke | QEMU | Blast | Risk | Verdict |
|-----------|-----------|--------:|-----------|--------------|-----------|------|-------|------|---------|
| `set_io_access_rights` | CPU/TSS | 2 | **I/O bitmap BTR/BTS privilege state** | Excellent | Med | Sys46 weak | Low* | Med–high | **SELECT** |
| `memmove` | Memory | 24 | Memory dword/byte move | Excellent | Easy | Everywhere | **Very high** | Med–high | Defer (#2) |
| `get_coff_sym` | PE/COFF | 2 | PE symbol-table name scan | Excellent | Easy | DLL load | Low | Low | Defer (#3) |

\*Low call-site count, high system impact if wrong (port `#GP`).

### Why this target beats the alternatives

* After R+W (both XFS), anti-clustering prefers a **different subsystem**.
* Opens first **CPU/TSS I/O-permission bitmap** migration class (privilege state leaf).
* Contained blast (2 co-located `r_f_port_area` callers).
* Exact BTR/BTS memory semantics; excellent differential on an 8 KiB private map.
* Not trivial scalar (`pci_*` / `coff_get_align`), not Stage-4 fanout (`memmove`).
* Privilege risk mitigated by: private-map differential, real-TSS smoke with restore, rollback gate.

```text
Selected target:
    set_io_access_rights

Why selected:
    First CPU/TSS I/O-bitmap privilege-state leaf; anti-cluster clean after
    XFS R/W; contained 2-caller blast; reloc-free A+C with trampoline-injected
    tss._io_map_0 pointer.

Why #2 was rejected:
    memmove — strongest preferred memory-semantics class, but 24-caller
    Stage-4 blast radius is not contained for a single leaf cut.

Why #3 was rejected:
    get_coff_sym — valuable PE foothold, but thinner strncmp-loop substance;
    TSS class adds a materially newer execution/privilege lesson.

Legacy ABI:
    EAX = port number (bit index into I/O map);
    EBP = 0 enable (BTR) / nonzero disable (BTS);
    mutates [tss._io_map_0] bit EAX;
    preserves EAX, EDI; plain ret (no stack args).

Critical invariants:
    Bit base = tss._io_map_0 (8192-byte map with _io_map_1);
    ebp==0 → clear bit (allow I/O); ebp!=0 → set bit (deny I/O);
    CF from BTR/BTS is not caller-observed;
    callers in r_f_port_area loop EAX through [ecx..=edx] with EDX/EBP live.

Rust strategy:
    Freestanding byte/bit update; stdcall(port, clear_access, io_map);
    trampoline injects tss._io_map_0 (no Rust reloc to TSS).

Trampoline strategy:
    Push/preserve EAX,ECX,EDX,EDI,EBP; push map/ebp/eax; call Rust (ret 12);
    restore; plain ret. USE_RUST_SET_IO_ACCESS_RIGHTS gate (dev 0 → prod 1).

Differential strategy:
    Independent BTR/BTS oracle vs Rust on 8 KiB maps;
    exhaustive all-ports enable/disable; range loops; nonzero-EBP;
    50k PRNG mixed ops.

Smoke strategy:
    Public ABI on real tss._io_map_0 with high unused ports + restore;
    register preserve; hang-on-fail marker.

QEMU strategy:
    cut-w-final.img lineage; OFF then ON desktop + network regression.
    Sys46 port-reserve path not forced in stock apps — report honestly.

Rollback:
    USE_RUST_SET_IO_ACCESS_RIGHTS = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin ABI trampoline; `USE_RUST_SET_IO_ACCESS_RIGHTS` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | Regcall leaf, plain `ret` |
| Register in | **EAX** → port; **EBP** → 0 enable / ≠0 disable |
| Stack in | none |
| Out | none (memory side effect only) |
| Memory | bit `EAX` of `tss._io_map_0` cleared (enable) or set (disable) |
| Preserved | **EAX**, **EDI** (legacy `push`/`pop`); trampoline also saves ECX/EDX/EBP |
| Clobbers | flags (CF from BTR/BTS; not observed by callers) |
| Callees | none |

Locked layout:

| Field | Value |
|-------|-------|
| `tss._io_map_0` | 4096 bytes |
| `tss._io_map_1` | 4096 bytes (contiguous after `_io_map_0`) |
| Full map | 8192 bytes / 65536 port bits |
| Enable | bit = 0 (I/O allowed) |
| Disable | bit = 1 (I/O `#GP`) |

---

## Out of scope

* Migrating `memmove` / `mutex_init` / `get_coff_sym` / `fix_coff_relocs`  
* Migrating full `r_f_port_area` / syscall 46 orchestration  
* Cut Y  

---

## Completion rule

Complete Cut X gates → document → **STOP**. Do not start Cut Y.