# Cut CL Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-cl-implementation.md`](cut-cl-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CL** migrates exFAT cluster/offset → absolute sector —
> `exFAT_get_sector` in `kernel/fs/exfat.inc`.  
> Cuts CC–CK remain complete and must not be modified. Do not start Cut CM.

---

## Fresh post-CK repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **92 / 135** (`migration-todo.md`; 92 `[x]` + 43 `[ ]`) |
| Production gates | **92** `[[rust.migrations]]` with `enabled = true` (1 non-cut probe entry `enabled = false`; total table rows 93) |
| Unique production cuts | **92** |
| Cut CC–CJ | intact (gates ON; not touched) |
| Cut CK | intact — `USE_RUST_FAT_GET_SECTOR = 1`; blob **25 B / 0 reloc**; SHA-256 `766a371d747139c9f2520f4b6a55e18e6367fa9fdf6530637902d3a8be374572` |
| `TMP_STACK_TOP` | **`0x008DF00`** (`kernel/const.inc`; `docs/compatibility/fixed-addresses.md` + `docs/architecture/memory-model.md` agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP`; CK end `.bss` @ `OS_BASE+0x8CE03` → needs `0x8DE03 < 0x8DF00` (**0xfd** ≈ 253 B headroom). **Do not lower.** Gap to `sys_proc` @ `0x8E000` = **0x100**. |
| Docs vs tree | CK plan+impl, inventory, gates, blob SHA agree |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **EBX+EDX+ESI+EDI+EBP**; restore **ECX**; **EAX is OUT** |
| REG-003 | Smoke mutates live globals | Synthetic stack `exFAT` + pair only — never touch live mounts |
| REG-009 | stdcall double cleanup | Rust `ret 16`; trampoline register-ABI outer — never `add esp` for Rust args |
| REG-010 | Trampoline arg offset | Account for every preserve-push before `stdcall rust_*` |
| REG-011 | Callers keep EBX/ESI/EDI/EBP | exFAT dir walk keeps those live across `exFAT_get_sector` |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD + `blit_32` | Geometry only; LFB / win_map / cursor stay FASM |
| AH+AI + `exFAT_find_lfn` | Hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| U+K+AO+BC+BW–BY+CK + `exFAT_get_sector` | FAT/exFAT calendar/name/LBA leaves ≠ plugin ownership |
| D+BB+BF+BH + `strchr`/`strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| `unpack` | Single decoder island, not subsystem ownership |

Cut CL remains **Path B**.

---

## Special investigations (mandatory)

### `unpack` — **DEFER** (memory architecture)

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + heap `unpack.p` ≈ 31.2 KiB |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Oracle | Excellent (bitstream + golden unpack) |
| Blob / mem | Multi-KiB code blob vs **~253 B** Stage-2 headroom (max raise toward `0x8E000` ≈ **256 B**). Structurally cannot embed as a Stage-2 reloc-free cut without redesigning the early map. |
| Sub-leaf? | Nested range-decoder labels are private to `unpack`; no coherent independently callable leaf with better evidence/risk |
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

### `drawChar` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/gui/font.inc` ~305–844 (~475 insn) |
| Verdict | **DEFER** — Stage-7 GUI mega-function; size + framebuffer blast |

### `enable_irq` / `irq_eoi` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/apic.inc` |
| Oracle | PIC `in`/`out` + APIC MMIO — production correctness not reducible to desktop reachability |
| Verdict | **REJECT** — I/O oracle class |

### `mutex_init` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/sync.inc` ~17–21 (3 stores) |
| Callers | ~35 + PE export |
| Verdict | **REJECT** — thin + extreme fan-out; near-zero architectural value |

### `exFAT_get_sector` — **SELECT** (AW ban aged out; CK twin with distinct soak)

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~2990–3003 (~10 insn) |
| Prior note | Listed `deferred/ban: AW address-math sibling` — **temporal anti-cluster** after Cut AW. CK closed the FAT twin; ban purpose for address-math *class* is served. Selecting the exFAT twin is justified by: independent production symbol, 3 live callers, `--disk exfat` soak, tiny blob fitting remaining headroom, and no stronger Stage-2 leaf remaining. |
| Semantics | `EAX → {cluster, sector_in_cluster}`; `EBP → exFAT*`; `EAX = (cluster-2)*SECTORS_PER_CLUSTER + CLUSTER_HEAP_START + sector_in_cluster` (32-bit wrap `imul`) |
| vs CK | Same math shape; field is **`CLUSTER_HEAP_START`** (not `DATA_START`); distinct struct (`exFAT*`) and callers |
| Callers | **3** live (`exFAT_hd_find_lfn.found`, `exFAT_notroot_next_sector`, `exFAT_notroot_first`) |
| Oracle | **Excellent** — independent integer LBA formula (i64 path vs FASM dec/dec/imul flow) |
| Soak | **`--disk exfat`** directory walk (desktop alone insufficient) |
| Blob | Tiny (~25 B class; expect fit within 0xfd headroom) |
| Verdict | **SELECT** — strongest remaining evidence/risk Path B leaf that fits Stage-2 memory |

### Overlooked leaves inspected

| Symbol | Verdict |
|--------|---------|
| `strchr` / `strnlen` | **REJECT** — PE export only; 0 in-kernel callers |
| `get_phys_addr` | **REJECT** — ~7-line AQ offset glue; PE-only |
| `tcp_mss` | **REJECT** — thin 1420 clamp+store |
| `ntfs_restore_usa_frs` | **REJECT** — 2-line fallthrough to Cut J |
| `socket_check_owner` | **REJECT** — dead (0 callers) |
| `getInodeLocation` | **DEFER** — AW sibling; **no `--disk ext`** soak |
| `pid_to_appdata` / `strtoint_dec` / `net_ptr_to_num` / `sysfn_*` | **REJECT** — dead / wrapper / façade |
| `alloc_page` / `map_page` / `disk_scan_*` / `*_SetFileInfo` / `tcp_output` / `drawChar` | **DEFER** — Stage 4–7 / orchestration / islands |

---

## Ranked candidates (43 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`exFAT_get_sector`** | exFAT cluster→LBA | 3 | **`--disk exfat`** | **Excellent** | Low | **SELECT** |
| 2 | `getInodeLocation` | EXT inode→LBA | 2 | none (no `--disk ext`) | Excellent | Low | Defer — soak |
| 3 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **Blocked** Stage-2 size | Defer — memory |
| 4 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **Very high** | Defer — LFB blast |
| 5 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — island |
| 6 | `drawChar` | GUI glyph render | 4 | desktop GUI | Hard | Very high | Defer — Stage 7 |
| 7 | `mutex_init` | sync init | ~35 | everywhere | Perfect | Med | Reject — thin+fanout |
| 8 | `enable_irq` / `irq_eoi` | PIC/APIC | 4–6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 9 | export-only / dead / Stage late | varies | — | — | — | — | Reject / defer |

### Why #1 wins

* Path A rejected; prior CL favorites fail size/LFB/island/oracle/thin bars.
* Fresh search: Stage-2 Path B leaf pool nearly exhausted; `exFAT_get_sector` is the only remaining address-math leaf with both an independent oracle and a real production soak (`--disk exfat`).
* Not selected merely because it resembles CK — selected because inventory has no stronger evidence/risk alternative that also fits ~253 B headroom.
* Blast is low (3 exFAT-internal callers); ABI mirrors successful CK (REG-001/009/011 lessons apply).

### Why alternatives lose

* `getInodeLocation`: no `--disk ext` soak.
* `unpack` / `blit_32` / `drawChar` / `exFAT_find_lfn`: Stage-2 size / LFB / plugin-island.
* `mutex_init` / IRQ / export-only / thin wrappers: fail substance or oracle bars.

---

## Selected target

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: exFAT_get_sector
Source: kernel/fs/exfat.inc
Subsystem: exFAT cluster/offset → absolute sector (Stage-2 / FS util)
Stage: Stage 2 / FS address-math leaf (post-CK inventory)
Why selected:
    Post-CK audit: Path A rejected; unpack/blit/exFAT_find_lfn/drawChar/
    IRQ/mutex fail memory/LFB/island/oracle/thin bars. AW address-math
    anti-cluster ban aged out (CK closed FAT twin). Strongest remaining
    evidence-quality Path B leaf with real --disk exfat soak is
    exFAT_get_sector.
Why this is a genuine migration boundary:
    Deterministic (cluster, ofs, SECTORS_PER_CLUSTER, CLUSTER_HEAP_START) → LBA.
    Complements AH/AI exFAT hash leaves + CK FAT LBA without claiming
    exFAT plugin ownership.
Why Path A / Path B:
    Path B — one LBA helper. exFAT lock/I/O/dir orchestration stays FASM.
Regression risks:
    REG-001/011: preserve EBX/EDX/ESI/EDI/EBP; restore ECX; EAX=sector out
    REG-009: no trampoline add esp for Rust stdcall args (ret 16)
    REG-003: synthetic exFAT + pair only
    imul 32-bit wrap / cluster<2 underflow must match FASM
    DF: legacy leaves DF unchanged
Memory impact:
    Measured end .bss @ OS_BASE+0x8CF43; need 0x8DF43 < TMP.
    Raised TMP_STACK_TOP 0x008DF00 → 0x008DF80 (+128 B).
    Gap to sys_proc @ 0x8E000 = 0x80. Do not lower. Do not raise further
    without measured assert failure.
```

---

## ABI audit (from FASM + callers)

```text
exFAT_get_sector  (register ABI; not stdcall)
  in:  EAX → dword pair {cluster @0, sector_in_cluster @4}
       EBP → exFAT*
  out: EAX = absolute sector
  body:
       push ecx
       ecx = [eax]; ecx -= 2
       ecx *= [ebp+exFAT.SECTORS_PER_CLUSTER]   ; 32-bit imul wrap
       ecx += [ebp+exFAT.CLUSTER_HEAP_START]
       ecx += [eax+4]
       eax = ecx
       pop ecx
       ret
  preserves: ECX (push/pop), EBX, EDX, ESI, EDI, EBP
  DF: unchanged
  CF/ZF/SF/OF: not a legacy contract (callers use EAX only)
  stack: ret 0 (no stdcall cleanup)
  interrupts: none
  callbacks: none
  globals: none (reads only through EBP→exFAT*)
  memory ownership: read-only pair + exFAT fields; no writes

Live callers (3):
  1. exFAT_hd_find_lfn.found — lea eax,[esp+8]; call; uses EAX sector
  2. exFAT_notroot_next_sector — push eax; call; store EAX; pop eax
  3. exFAT_notroot_first — same pattern as next_sector

Rust ABI:
  stdcall rust_exfat_get_sector(cluster, ofs, spc, cluster_heap_start); ret 16

Trampoline:
  omit-FP (EBP→exFAT must stay live)
  push ebx/ecx/edx/esi/edi
  inject pair fields + SECTORS_PER_CLUSTER + CLUSTER_HEAP_START
  stdcall rust_*; never add esp
  pop restore; ret
```

---

## Oracle / tests / validation plan

| Item | Plan |
|------|------|
| Oracle | Independent i64 `(cluster-2)*spc + heap_start + ofs` truncated to u32 |
| PRNG seed | `0x45534543` (`'ESEC'`) |
| PRNG cases | 50,000 |
| Focused host | `esec_*` edge + wrap + underflow + ptr + PRNG |
| Full suite | run full `kolibri_utils` host tests; record exact count |
| ABI smoke | marker `'ESEC'`; hang=`DEAD0C6C`; synthetic `sizeof.exFAT` + pair; public trampoline + direct `rust_*`; register canaries |
| QEMU | OFF / ON / A/B / ON×3 via `qmp_desktop_smoke.py` |
| Soak | `python scripts/run_qemu.py --disk exfat` (exFAT dir walk through callers) |
| Rollback | `USE_RUST_EXFAT_GET_SECTOR = 0` |

---

## Implementation checklist

1. Rust pure helper + focused tests (`exfat_get_sector.rs`)
2. FFI `rust_exfat_get_sector` stdcall `ret 16`
3. Extract blob via `project/build.toml` `[[rust.blobs]]` + `[[rust.migrations]]`
4. FASM trampoline + legacy body under gate in `exfat.inc`
5. Smoke include + `kernel32.inc` / early smoke call
6. Host tests → ABI smoke build → memory assert
7. QEMU OFF / ON / A/B / ON×3 / `--disk exfat` soak
8. Docs: plan (this), implementation, todo, migration-plan; memory docs only if TMP changes

**Measured:** end `.bss` @ `OS_BASE+0x8CF43` → need `0x8DF43 < TMP_STACK_TOP`.
`0x8DF00` failed by **0x43**. Raised `TMP_STACK_TOP` `0x008DF00` → `0x008DF80` (+128 B). Gap to `sys_proc` @ `0x8E000` remains (`0x80`). Do not lower.
