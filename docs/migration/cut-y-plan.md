# Cut Y Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-y-implementation.md`](cut-y-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut Y** migrates COFF relocation application — `fix_coff_relocs`, which walks section reloc tables and patches DIR32/REL32 dwords in a loaded image.  
> Cuts A–X remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `fix_coff_relocs` |
| **Source** | [`kernel/core/dll.inc:726–778`](../../kernel/core/dll.inc) |
| **Subsystem** | PE/COFF driver/DLL loader |
| **Purpose** | Apply COFF relocations (type 6 DIR32, type 20 REL32) in-place |

---

## Post-X candidate audit (live tree)

### Coverage already proven (A–X)

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

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer Stage-4** | Forward-only memcpy; EAX/EBX/ECX preserve; 24-site fanout |
| `get_coff_sym` | 2 | **defer (#3)** | PE name→Value; thinner than reloc patch |
| `is_string_userspace` | 1 | **defer** | Syscall ZF+scasb; thinner after P |
| `net_ptr_to_num4` | 12 | **defer** | Thin device index scan + fanout |
| `mutex_init` | 34 | **defer Stage-4** | Sync export surface |
| `pci_make_config_cmd` | 2 | **reject** | Trivial scalar |
| `coff_get_align` | 2 | **reject** | Trivial Characteristics→mask |
| `blit_clip` | 1 | **reject** | Cut H composition |
| `xfs_hashname` | 4 | **reject** | Anti-cluster after R/W |
| `strtoint_dec` | 0 prod | **reject** | Dead — `conf_lib` commented out |
| `is_partition_table_entry` | 5 | **defer (#2)** | Strong disk/device class; CF+ESI/EBP |
| `fix_coff_relocs` | 1 live | **promote** | PE buffer patch + structure walk |

### `memmove` special evaluation

| Property | Finding |
|----------|---------|
| Implementation | Forward-only `rep movsd`/`movsb`; **not** bidirectional C `memmove` |
| Overlap | Correct for left-shift (`src = dest+N`); wrong for dest>src overlap |
| Callers | 24 across syscall/HID/GUI/FS; several rely on EAX/EBX/ECX preserve |
| Blast | Stage-4 memory class — not contained incremental util |
| Cut Y | **DEFER** — preferred class, wrong blast radius for Y |

### Ranked top three

| Candidate | Subsystem | Callers | New class | Differential | ABI smoke | QEMU | Blast | Risk | Verdict |
|-----------|-----------|--------:|-----------|--------------|-----------|------|-------|------|---------|
| `fix_coff_relocs` | PE/COFF | 1 | **Reloc buffer patch + section walk** | Excellent | Easy | Med DLL | Low* | Med | **SELECT** |
| `is_partition_table_entry` | Disk/MBR | 5 | Device partition validate | Excellent | Med CF+ESI | Strong boot | Med | Med–high | Defer (#2) |
| `get_coff_sym` | PE/COFF | 2 | PE symbol name scan | Excellent | Easy | Med DLL | Low | Low | Defer (#3) |

\*Low call-site count; high system impact if wrong (COFF drivers).

```text
Selected target:
    fix_coff_relocs

Why selected:
    First PE/COFF relocation buffer-patch leaf; anti-cluster clean after
    Cut X (CPU/TSS); structure traversal + type dispatch + in-place dword
    mutation; 1 live caller; excellent synthetic differential; Stage-8 foothold
    without thinness of coff_get_align / get_coff_sym.

Why #2 was rejected:
    is_partition_table_entry — excellent first disk/device class and stronger
    QEMU partition-scan coverage, but heavier ESI/EBP/CF ABI and higher boot
    blast; PE reloc patch adds more buffer-transform novelty this cut prefers.

Why #3 was rejected:
    get_coff_sym — valuable PE foothold, but thinner strncmp-loop substance;
    fix_coff_relocs is the stronger PE structure+mutation lesson.

Legacy ABI:
    stdcall(coff, sym, delta); uses ebx esi;
    walks COFF_HEADER.nSections × COFF_SECTION reloc tables;
    type 6 DIR32 / type 20 REL32; other types skipped;
    patches dword at (reloc.VA + sec.VA + delta);
    void return; plain stdcall ret 12.

Critical invariants:
    sizeof.COFF_HEADER = 20; sections start at coff+20;
    sizeof.COFF_SECTION = 40; sizeof.COFF_RELOC = 10; sizeof.COFF_SYM = 18;
    SymIndex * 18 indexes symbol table;
    DIR32: [VA+secVA+delta] += sym.Value;
    REL32: [VA+secVA+delta] += sym.Value - (VA+secVA) - 4;
    wrapping u32 arithmetic throughout.

Rust strategy:
    Freestanding A+C; stdcall(coff, sym, delta) matching legacy;
    no globals / tables; reloc-free blob.

Trampoline strategy:
    Thin stdcall forwarder under USE_RUST_FIX_COFF_RELOCS
    (dev 0 → prod 1); preserve ebx/esi via proc uses.

Differential strategy:
    Independent FASM-flow oracle on synthetic images;
    empty/zero sections; DIR32/REL32/unknown type; multi-section;
    wraparound addends; large PRNG corpus.

Smoke strategy:
    Synthetic COFF in uglobal; absolute VA→patch dword;
    DIR32 + REL32; register preserve; hang-on-fail marker.

QEMU strategy:
    cut-x-final.img lineage; OFF then ON desktop + network regression.
    DLL/driver load path not forced in stock apps — report honestly.

Rollback:
    USE_RUST_FIX_COFF_RELOCS = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin ABI trampoline; `USE_RUST_FIX_COFF_RELOCS` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | `stdcall` (`proc … stdcall uses ebx esi`) |
| Stack in | `coff`, `sym`, `delta` (12 bytes; callee cleans) |
| Register in | none beyond stack |
| Out | void (EAX undefined) |
| Memory | in-place dword patches at computed absolute addresses |
| Preserved | EBX, ESI (`uses`); Rust stdcall also preserves EDI/EBP |
| Clobbers | EAX, ECX, EDX, EDI, flags |
| Callees | none |

Locked layouts (`kernel/const.inc`):

| Struct | Size | Key fields |
|--------|-----:|------------|
| `COFF_HEADER` | 20 | `nSections` @2 (u16) |
| `COFF_SECTION` | 40 | `VirtualAddress` @12; `PtrReloc` @24; `NumReloc` @32 (u16) |
| `COFF_RELOC` | 10 | `VirtualAddress` @0; `SymIndex` @4; `Type` @8 (u16) |
| `COFF_SYM` | 18 | `Value` @8 (u32) |

---

## Out of scope

* Migrating `get_coff_sym` / `rebase_coff` / `fix_coff_symbols` / `coff_get_align`  
* Migrating `memmove` / `is_partition_table_entry`  
* Cut Z  

---

## Completion rule

Complete Cut Y gates → document → **STOP**. Do not start Cut Z.
