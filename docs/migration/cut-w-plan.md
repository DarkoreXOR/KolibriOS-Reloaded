# Cut W Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-w-implementation.md`](cut-w-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut W** migrates XFS directory leaf **binary search by hash** — `xfs._.get_addr_by_hash`, which returns a big-endian data address plus ZF found/miss for five lookup paths.  
> Cuts A–V remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `xfs._.get_addr_by_hash` |
| **Source** | [`kernel/fs/xfs.asm:1551–1581`](../../kernel/fs/xfs.asm) |
| **Subsystem** | FS / XFS directory lookup |
| **Purpose** | Binary search sorted `xfs_dir2_leaf_entry[]` by name hash; return BE address + ZF |

---

## Post-V candidate audit (live tree)

### Coverage already proven (A–V)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode / casefold / string / UTF stream | A–D, Q |
| Net checksum / TCP RTT + persist timer | E, F, M, V |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next+gen / XFS BE unpack | I–K, R, U |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace region gate | P |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| EDI-advancing calendar inverse | T |
| UTF-8→FAT 8.3 SM + pushad/popad | U |
| TCP persist-timer arming / clamp / sticky flag | V |

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer** | Preferred memory class, but Stage-4 fanout / blast |
| `blit_clip` | 1 | **reject** | Cut H composition; GUI geometry already proven |
| `fat_time_to_bdfe` | 5 | **reject** | Trivial pack / calendar-adjacent |
| `is_string_userspace` | 1 | **defer** | Syscall ZF+scasb; thinner novelty after P |
| `set_io_access_rights` | 2 | **defer (#3)** | CPU/TSS preferred class; privilege risk ≫ body |
| `pci_make_config_cmd` | 2 | **reject** | Device domain but trivial scalar |
| `mutex_init` | 34 | **defer** | Stage-4 sync / export surface |
| `coff_get_align` | 2 | **reject for W** | PE foothold but trivial Characteristics→mask (penalized) |
| `strtoint_dec` | 0 prod | **reject** | Dead — `conf_lib.inc` still commented out |
| `net_ptr_to_num4` | 12 | **defer** | Device index scan; thin loop + fanout |
| `xfs_hashname` | 4 | **defer** | Thin ROL7 hash leaf |
| `xfs._.get_addr_by_hash` | 5 | **promote** | Binary search multi-state + ZF/EAX dual return |

### Ranked top three

| Candidate | Callers | New class | Differential | ABI smoke | QEMU | Blast | Risk | Verdict |
|-----------|--------:|-----------|--------------|-----------|------|-------|------|---------|
| `xfs._.get_addr_by_hash` | 5 | **Binary search multi-state; EAX+ZF dual out; MOVBE table walk** | Excellent | Easy | Weak XFS | Low | Med | **SELECT** |
| `memmove` | 24 | Memory dword/byte move | Excellent | Easy | Everywhere | **Very high** | Med–high | Defer (#2) |
| `set_io_access_rights` | 2 | CPU/TSS I/O bitmap BTR/BTS | Good | Med | Sys46 | Low* | **High** | Defer (#3) |

\*Low call-site count, high system impact if wrong.

### Why this target beats the alternatives

* After V (TCP persist), another calendar / GUI / FAT / XFS **unpack** / trivial PE scalar adds little.
* `get_addr_by_hash` opens **algorithmic multi-state search** with an unusual **EAX result + ZF found/miss** contract (extends Cut P ZF reconstruction, but payload in EAX).
* Not Cut R’s bitfield unpack — sorted leaf-entry binary search with BE `movbe` loads and pointer advancement.
* Contained blast (5 co-located XFS lookup callers).
* Excellent differential (finite tables; empty/hit/miss/edges).
* Stock QEMU has no XFS volume — report soak honestly (same honesty as Cut R).

```text
Selected target:
    xfs._.get_addr_by_hash

Why selected:
    Algorithmic binary search with multi-state control flow and EAX+ZF dual
    return; first search-loop migration class; contained; reloc-free A+C.

Why #2 was rejected:
    memmove — strongest preferred memory-semantics class, but 24-caller
    Stage-4 blast radius is not contained for a single leaf cut.

Why #3 was rejected:
    set_io_access_rights — CPU/TSS class is valuable, but privilege/security
    failure mode outweighs ~14-insn body for Cut W.

Legacy ABI:
    EAX = hash (register in);
    stdcall (_base, _len) omit-frame-pointer; uses ebx,esi;
    on hit: EAX = BE address, ZF=1 (from equal cmp);
    on miss: EAX = ERROR_FILE_NOT_FOUND (5), ZF=0 (test esp,esp);
    retn 8.

Critical invariants:
    Entry stride = sizeof.xfs_dir2_leaf_entry = 8;
    mid = len >> 1; below keeps left half; above advances base by (mid+1)*8;
    BE loads for hashval/address; empty len → miss.

Rust strategy:
    Freestanding binary search over BE u32 pairs; return u64 (EDX:EAX) =
    (zf_flag << 32) | result; no tables / .rodata.

Trampoline strategy:
    Hand-written omit-FP trampoline: pass EAX hash + stack base/len to
    rust_xfs_get_addr_by_hash (ret 12); cmp EDX,1 for ZF; pop EAX restores
    payload without clobbering flags; retn 8.
    USE_RUST_XFS_GET_ADDR_BY_HASH gate (dev default 0 → production 1).

Differential strategy:
    Independent FASM-flow oracle vs Rust; empty/single/multi tables;
    hit first/mid/last; miss below/above/gap; unsorted-tolerant mid paths;
    large PRNG corpus on synthetic sorted tables.

Smoke strategy:
    Public ABI vectors on stack leaf tables; EAX/ZF contracts; register
    preserve; hang-on-fail marker.

QEMU strategy:
    cut-v-final.img lineage; OFF then ON desktop + network regression.
    XFS path not in stock image — state soak NOT AVAILABLE.

Rollback:
    USE_RUST_XFS_GET_ADDR_BY_HASH = 0
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin ABI trampoline; `USE_RUST_XFS_GET_ADDR_BY_HASH` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | Omit-FP stdcall, `retn 8` |
| Register in | **EAX** → hash |
| Stack in | `_base` → `xfs_dir2_leaf_entry*`, `_len` → entry count |
| Out | **EAX** = BE address or `5`; **ZF** = found (1) / miss (0) |
| Preserved | **EBX**, **ESI** (FASM `uses`); trampoline also saves ECX/EDX across body |
| Clobbers | ECX, EDX, flags (except reconstructed ZF) |
| Callees | none |

Locked layout:

| Field | Offset / size |
|-------|----------------|
| `hashval` | 0 (BE `dd`) |
| `address` | 4 (BE `dd`) |
| `sizeof.xfs_dir2_leaf_entry` | 8 |
| `ERROR_FILE_NOT_FOUND` | 5 |

---

## Out of scope

* Migrating `memmove` / `mutex_init` / `set_io_access_rights`  
* Migrating `xfs_hashname` / full XFS lookup orchestration  
* Cut X  

---

## Completion rule

Complete Cut W gates → document → **STOP**. Do not start Cut X.
