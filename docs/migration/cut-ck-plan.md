# Cut CK Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-ck-implementation.md`](cut-ck-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CK** migrates FAT cluster/offset → absolute sector —
> `fat_get_sector` in `kernel/fs/fat.inc`.  
> Cuts CC–CJ remain complete and must not be modified. Do not start Cut CL.

---

## Fresh post-CJ repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **91 / 135** (`migration-todo.md`; 91 `[x]` + 44 `[ ]`) |
| Production gates | **91** `[[rust.migrations]]`, **all `enabled = true`** |
| Unique cuts | **91** |
| Cut CC–CI | intact (gates ON; not touched) |
| Cut CJ | intact — `USE_RUST_MEMMOVE = 1`; blob **214 B / 0 reloc**; SHA-256 `87fe76d1f58e59581fe1c81e594b9c09f429bca5c82375cbf2a671c1f755ace3` |
| `TMP_STACK_TOP` | **`0x008DD00`** (`kernel/const.inc`; `docs/compatibility/fixed-addresses.md` + `docs/architecture/memory-model.md` agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP`; CJ end `.bss` @ `OS_BASE+0x8CC83` → needs `0x8DC83 < 0x8DD00` (**0x7d** ≈ 125 B headroom). **Do not lower.** Gap to `sys_proc` @ `0x8E000` = **0x300**. |
| Docs vs tree | CJ plan+impl, inventory, gates, blob SHA agree |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **EBX+EDX+ESI+EDI+EBP**; restore **ECX** (legacy push/pop); **EAX is OUT** |
| REG-003 | Smoke mutates live globals | Synthetic stack FAT + `{cluster,ofs}` pair only — never touch live `FAT.buffer` / mounts |
| REG-009 | stdcall double cleanup | Rust `ret 16`; trampoline register-ABI outer — never `add esp` for Rust args |
| REG-010 | Trampoline arg offset | Account for every preserve-push before `stdcall rust_*` |
| REG-011 | Callers keep EBX/ESI/EDI/EBP | FAT dir walk / write paths keep those live across `fat_get_sector` |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD + `blit_32` | Geometry only; LFB / win_map / cursor stay FASM |
| AH+AI + `exFAT_find_lfn` | Hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| U+K+AO+BC+BW–BY + `fat_get_sector` | FAT calendar/name leaves + one LBA helper ≠ FAT plugin ownership |
| D+BB+BF+BH + `strchr`/`strnlen` | Export/libc leaves ≠ string ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| `unpack` | Single decoder island, not subsystem ownership |
| `memmove` (CJ) | Core util leaf — not memory-subsystem ownership |

Cut CK remains **Path B**.

---

## Special investigations (mandatory)

### `unpack` — **DEFER** (memory architecture)

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + heap `unpack.p` ≈ 31.2 KiB |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Oracle | Excellent (bitstream + golden unpack) |
| Blob / mem | Multi-KiB code blob vs **~125 B** Stage-2 headroom (max raise toward `0x8E000` ≈ **~0.6–0.8 KiB**). Structurally cannot embed as a Stage-2 reloc-free cut without redesigning the early map. |
| Verdict | **DEFER** — excellent oracle, blocked by Stage-2 placement + LZMA state |

### `blit_32` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/video/blitter.inc` ~257–585 |
| vs CD | CD owns geometry; `blit_32` is LFB **pixel** hot path |
| Oracle | Buffer-level oracle buildable; desktop non-black is **insufficient** |
| Verdict | **DEFER** — LFB/cursor/bpp blast + large blob vs headroom |

### `exFAT_find_lfn` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Contract | Stack callbacks (`first`/`next`), calls unmigrated `exFAT_get_name`, EBP=`exFAT*` |
| Callers | 1 |
| Verdict | **DEFER** — FS plugin island; callback ABI blast |

### `enable_irq` / `irq_eoi` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/apic.inc` |
| Oracle | PIC `in`/`out` + APIC MMIO — host I/O model possible, but production correctness not reducible to desktop reachability; no stronger independent mask/EOI soak discovered |
| Verdict | **REJECT** — I/O oracle class |

### `mutex_init` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/sync.inc` ~17–21 (3 stores) |
| Callers | ~35 + PE export |
| Verdict | **REJECT** — thin + extreme fan-out; near-zero architectural value |

### `fat_get_sector` — **SELECT** (ban aged out)

| Item | Finding |
|------|---------|
| Source | `kernel/fs/fat.inc` ~2011–2021 (~11 insn) |
| Prior note | Listed `deferred/ban: AW address-math sibling` — **temporal anti-cluster** after Cut AW, not permanent unsuitability. AW closed many cuts ago; ban purpose served. |
| Semantics | `EAX → {cluster, sector_in_cluster}`; `EBP → FAT*`; `EAX = (cluster-2)*SECTORS_PER_CLUSTER + DATA_START + sector_in_cluster` (32-bit wrap `imul`) |
| Callers | **4** live (`hd_find_lfn.found`, `fat_notroot_next_sector`, `fat_notroot_end_write`, + related FAT dir path) |
| Oracle | **Excellent** — independent integer LBA formula (i64 path vs FASM dec/dec/imul flow) |
| Soak | Boot floppy is FAT — every desktop boot + FAT browse exercises it |
| Blob | Tiny (expected ≪ headroom; measure before any TMP change) |
| Verdict | **SELECT** — strongest remaining evidence/risk Path B leaf |

### Overlooked leaves inspected

| Symbol | Verdict |
|--------|---------|
| `strchr` / `strnlen` | **REJECT** — PE export only; 0 in-kernel callers; soak vacuous |
| `get_phys_addr` | **REJECT** — ~9-line AQ offset glue; PE-only |
| `tcp_mss` | **REJECT** — thin 1420 clamp+store |
| `ntfs_restore_usa_frs` | **REJECT** — 2-line fallthrough to Cut J |
| `socket_check_owner` | **REJECT** — dead (definition only; 0 callers) |
| `pid_to_appdata` / `strtoint_dec` / `net_ptr_to_num` / `sysfn_*` | **REJECT** — dead / wrapper / façade |
| `exFAT_get_sector` / `getInodeLocation` | **DEFER** — remaining AW-ban siblings; weaker soak (`exFAT` attach / no `--disk ext`) vs FAT boot path |
| `alloc_page` / `map_page` / `disk_scan_*` / `*_SetFileInfo` / `tcp_output` / `drawChar` | **DEFER** — Stage 4–7 / orchestration / islands |

---

## Ranked candidates (44 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`fat_get_sector`** | FAT cluster→LBA | 4 | **strong** FAT boot | **Excellent** | Low | **SELECT** (AW ban aged out) |
| 2 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **Blocked** Stage-2 size | Defer — memory |
| 3 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **Very high** | Defer — LFB blast |
| 4 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — island |
| 5 | `exFAT_get_sector` | exFAT LBA math | ~few | `--disk exfat` | Excellent | Low | Defer — weaker than FAT boot soak |
| 6 | `mutex_init` | sync init | ~35 | everywhere | Perfect | Med | Reject — thin+fanout |
| 7 | `enable_irq` / `irq_eoi` | PIC/APIC | 4–6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 8 | export-only / dead / Stage late | varies | — | — | — | — | Reject / defer |

### Why #1 wins

* Path A rejected; prior CK favorites fail size/LFB/island/oracle/thin bars.
* Fresh search overturns the old unpack/blit/exFAT/IRQ/mutex ranking: **`fat_get_sector`** was under-ranked solely due to a **stale AW anti-cluster ban**.
* Evidence quality: pure LBA math with independent oracle; production soak is the boot FAT volume itself.
* Blast is low (4 FAT-internal callers); semantic class matches successful Cut AW without reopening XFS.
* Tiny reloc-free blob expected to fit existing `TMP_STACK_TOP = 0x008DD00` headroom; raise only if measured assert fails.

### Why alternatives lose

* `unpack`: Stage-2 blob cannot fit early-map headroom.
* `blit_32` / `exFAT_find_lfn`: LFB / plugin-island risk.
* `mutex_init` / IRQ / export-only / thin wrappers: fail substance or oracle bars.

---

## Selected target

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: fat_get_sector
Source: kernel/fs/fat.inc
Subsystem: FAT cluster/offset → absolute sector (Stage-2 / FS util)
Stage: Stage 2 / FS address-math leaf (post-CJ inventory)
Why selected:
    Post-CJ audit: Path A rejected; unpack/blit/exFAT/mutex/IRQ fail
    memory/LFB/island/thin/oracle bars. AW address-math anti-cluster ban
    has aged out. Strongest remaining evidence-quality Path B leaf with
    real FAT boot soak is fat_get_sector.
Why this is a genuine migration boundary:
    Deterministic (cluster, ofs, SECTORS_PER_CLUSTER, DATA_START) → LBA.
    Complements FAT name/time leaves without claiming FAT plugin ownership.
Why Path A / Path B:
    Path B — one LBA helper. FAT lock/I/O/dir orchestration stays FASM.
Regression risks:
    REG-001/011: preserve EBX/EDX/ESI/EDI/EBP; restore ECX; EAX=sector out
    REG-009: no trampoline add esp for Rust stdcall args (ret 16)
    REG-003: synthetic FAT + pair only
    imul 32-bit wrap / cluster<2 underflow must match FASM
    DF: legacy leaves DF unchanged
Memory impact:
    Expect tiny blob within 0x7d headroom; measure end .bss before any
    TMP_STACK_TOP change. Do not lower 0x008DD00. Do not raise speculatively.
```

---

## ABI audit (from FASM + callers)

```text
fat_get_sector  (register ABI; not stdcall)
  in:  EAX → dword pair {cluster @0, sector_in_cluster @4}
       EBP → FAT*
  out: EAX = absolute sector
  body:
       push ecx
       ecx = [eax]; ecx -= 2
       ecx *= [ebp+FAT.SECTORS_PER_CLUSTER]   ; 32-bit imul wrap
       ecx += [ebp+FAT.DATA_START]
       ecx += [eax+4]
       eax = ecx
       pop ecx
       ret
  preserves: ECX (explicit), EBX, EDX, ESI, EDI, EBP (untouched)
  clobbers: EAX (out), flags
  DF: unchanged (no cld/std)
  stack: no args; ret 0
```

### Rust ABI

```text
stdcall rust_fat_get_sector(cluster, sector_ofs, sectors_per_cluster, data_start)
  → EAX = sector
  ret 16
```

Trampoline: omit-FP; extract fields from `EBP` + `[EAX]`/`[EAX+4]`; preserve
EBX/EDX/ESI/EDI/EBP; restore ECX; **never** `add esp,16`.

---

## Oracle / tests

| Item | Plan |
|------|------|
| Oracle | Independent i64 formula `(cluster as i64 - 2) * spc + data_start + ofs` truncated to u32 — **not** a copy of the Rust dec/dec/imul sequencing helper |
| PRNG seed | `0x46534543` (`'FSEC'`) |
| Cases | 50,000 randomized + fixed edges (cluster 0/1/2, ofs=spc-1, max u32 wrap) |
| Host tests | Focused `fsec_*` + full `kolibri_utils` suite |
| ABI smoke | Marker `'FSEC'`; hang `DEAD0C6B`; synthetic `sizeof.FAT` + pair; public trampoline + direct `rust_*`; register canaries |
| QEMU | OFF / ON / A/B / ON×3 via `scripts/qmp_desktop_smoke.py` |
| Soak | Desktop FAT boot path (floppy) + representative FAT browse; optional `--disk exfat` is **not** primary (leaf is FAT-only) |

---

## Rollback

```text
USE_RUST_FAT_GET_SECTOR = 0
```

in `kernel/fs/fat.inc` (or `enabled = false` for Cut CK in `project/build.toml`).

---

## Implementation checklist

1. Rust pure leaf + independent oracle + host tests  
2. `ffi.rs` `rust_fat_get_sector` + `lib.rs` wiring  
3. Extract blob (`project/build.toml` `[[rust.blobs]]`)  
4. `kernel/rust/fat_get_sector.inc` embed + smoke  
5. Gate + trampoline + legacy body in `fat.inc`  
6. `kernel32.inc` include + `kernel.asm` smoke call  
7. `[[rust.migrations]]` Cut CK enabled  
8. Host tests → ABI smoke → QEMU OFF/ON/A/B/×3 → FAT soak  
9. Memory assert; raise `TMP_STACK_TOP` only if measured  
10. Docs: plan (this) + implementation + todo + migration-plan  

**Stop after Cut CK. Do not start Cut CL.**
