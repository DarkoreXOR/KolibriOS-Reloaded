# Cut CM Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-cm-implementation.md`](cut-cm-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CM** migrates EXT inode → sector/offset —
> `getInodeLocation` in `kernel/fs/ext.inc`.  
> Cuts CC–CL remain complete and must not be modified. Do not start Cut CN.

---

## Fresh post-CL repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **93 / 135** (`migration-todo.md`; 93 `[x]` + 42 `[ ]`) |
| Production gates | **93** `[[rust.migrations]]` with `enabled = true` (probe excluded from production count) |
| Unique production cuts | **93** |
| Cut CC–CK | intact (gates ON; not touched) |
| Cut CL | intact — `USE_RUST_EXFAT_GET_SECTOR = 1`; blob **25 B / 0 reloc**; SHA-256 `766a371d747139c9f2520f4b6a55e18e6367fa9fdf6530637902d3a8be374572` |
| Cut CK | intact — same blob SHA (distinct symbol/section/gate; **not** mergeable) |
| `TMP_STACK_TOP` | **`0x008DF80`** (`kernel/const.inc`; fixed-addresses + memory-model agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP`; CL end `.bss` @ `OS_BASE+0x8CF43` → needs `0x8DF43 < 0x8DF80` (**0x3d** ≈ 61 B bss-growth headroom). Gap to `sys_proc` @ `0x8E000` = **0x80**. **Do not lower.** |
| Docs vs tree | CL plan+impl, inventory, gates, blob SHA agree |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **ESI+EDI+EBP**; **EAX/EDX/ECX are OUT**; **EBX clobbered** (legacy) |
| REG-003 | Smoke mutates live globals | Truncated synthetic EXTFS + stack BGDESCR only — never touch live mounts |
| REG-009 | stdcall double cleanup | Rust `ret 32`; trampoline register-ABI outer — never `add esp` for Rust args |
| REG-010 | Trampoline arg offset | Account for every preserve-push + `sub esp` out-slots before `stdcall rust_*` |
| REG-011 | Callers keep ESI/EDI/EBP | `readInode`/`writeInode` keep those live across `getInodeLocation` |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD + `blit_32` | Geometry only; LFB / win_map / cursor stay FASM |
| AH+AI+CL + `exFAT_find_lfn` | Sector/hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| U+K+AO+BC+BW–BY+CK+CL + EXT calendar | FAT/exFAT/EXT leaves ≠ FS plugin ownership |
| D+BB+BF+BH + `strchr`/`strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| `unpack` | Single decoder island, not subsystem ownership |
| AL+BR+BS + `getInodeLocation` | Time pack/unpack + one address leaf ≠ EXT FS ownership |

Cut CM remains **Path B**.

---

## Special investigations (mandatory)

### `getInodeLocation` — **SELECT**

| Item | Finding |
|------|---------|
| Source | `kernel/fs/ext.inc` ~1149–1172 (~24 insn + `load_bgd_64`) |
| Semantics | `EAX=inode`; `EBP→EXTFS*`; out `EDX:EAX=sector`, `ECX=ofs∈[0,511]`; pure math + BGDESCR memory read |
| Callers | **2** live (`readInode`, `writeInode`) — every EXT inode I/O |
| Oracle | Independent u64/i32-faithful math with synthetic BGDESCR buffers |
| Soak gap | No `--disk ext` historically — **not absolute**: whole-disk probe at SB@1024 matches `mkfs.ext*`; Docker+e2fsprogs (XFS pattern) can create Kolibri-compatible EXT2 |
| Memory | Expected reloc-free blob ~80–150 B; may need smallest TMP raise within remaining **0x80** gap |
| Verdict | **SELECT** — strongest remaining Stage-2 Path B leaf; AW-class address math with buildable soak |

### `unpack` — **DEFER** (memory architecture)

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + heap `unpack.p` ≈ 31.2 KiB |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Oracle | Excellent (bitstream + golden unpack) |
| Blob / mem | Multi-KiB code blob vs **~61 B** Stage-2 headroom (max raise toward `0x8E000` ≈ **128 B**). Structurally cannot embed as a Stage-2 reloc-free cut. |
| Sub-leaf? | Nested range-decoder labels are private to `unpack`; no coherent independently callable leaf |
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
| Post-CL | `exFAT_get_sector` migration does **not** own the plugin island |
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

### `mutex_init` / thin / export-only — **REJECT**

| Symbol | Verdict |
|--------|---------|
| `mutex_init` | **REJECT** — thin + extreme fan-out |
| `ntfs_restore_usa_frs` | **REJECT** — 2-line fallthrough to Cut J |
| `tcp_mss` | **REJECT** — thin 1420 clamp+store |
| `strchr` / `strnlen` | **REJECT** — PE export only; 0 in-kernel callers |
| `get_phys_addr` / `net_ptr_to_num` / `sysfn_*` / `pid_to_appdata` | **REJECT** — thin / wrapper / façade / dead |

---

## Complete candidate ranking (post-CL)

| Rank | Symbol | Class | Callers | Soak | Oracle | Mem cost | Decision |
|------|--------|-------|---------|------|--------|----------|----------|
| **1** | **`getInodeLocation`** | EXT inode→LBA | 2 | **buildable** `--disk ext` (Docker/e2fs) | **Excellent** | Low–Med | **SELECT** |
| 2 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **Blocked** Stage-2 size | Defer — memory |
| 3 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **Very high** | Defer — LFB blast |
| 4 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — island |
| 5 | `drawChar` | GUI glyph render | 4 | desktop GUI | Hard | Very high | Defer — Stage 7 |
| 6 | `mutex_init` | sync init | ~35 | everywhere | Perfect | Med | Reject — thin+fanout |
| 7 | `enable_irq` / `irq_eoi` | PIC/APIC | 4–6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 8 | Stage 4–7 / orchestration | alloc/map/scan/SetFileInfo/tcp_output/… | varies | varies | varies | High | Defer — stage |

### Selection rationale

* Fresh search confirms prior top remaining leaf: **`getInodeLocation`**.
* Lack of `--disk ext` was a **tooling gap**, not an absolute blocker: `ext2_create_partition` probes whole-disk SB @ LBA 2 (byte 1024); Docker+e2fsprogs can mint a Kolibri-compatible EXT2 image (feature set ⊆ `INCOMPATIBLE_SUPPORT`, `blocksTotal_hi=0`).
* Evidence quality (independent LBA oracle + real EXT inode I/O soak) outranks raw FASM reduction and caller count.
* Memory fit: small leaf; TMP raise only if measured high-water requires it, within remaining **0x80**.

### Rejected / deferred (summary)

* `unpack` / `blit_32` / `drawChar` / `exFAT_find_lfn`: Stage-2 size / LFB / plugin-island.
* `mutex_init` / IRQ / export-only / thin wrappers: fail substance or oracle bars.
* Stage 4–7 orchestration: out of Path B leaf scope.

---

## Selected target

| Field | Value |
|-------|-------|
| Cut | **CM** |
| Symbol | `getInodeLocation` |
| Source | [`kernel/fs/ext.inc`](../../kernel/fs/ext.inc) |
| Subsystem | EXT inode number → absolute sector + in-sector offset |
| Stage | Stage 2 / FS address-math leaf |
| Path | **B** |
| Gate | `USE_RUST_GET_INODE_LOCATION` |
| Rollback | `USE_RUST_GET_INODE_LOCATION = 0` |

### Live callers

1. `writeInode` (`ext.inc`) — uses `EDX:EAX` sector + `ECX` offset for read-modify-write
2. `readInode` (`ext.inc`) — uses `EDX:EAX` + `ECX` for inode sector fetch

### Callees / globals / callbacks

* **Callees:** none (leaf; `load_bgd_64` is a macro)
* **Globals:** none
* **Callbacks:** none
* **Memory ownership:** reads `EXTFS` fields + one `BGDESCR` at `descriptorTable + (group<<descShift)`; no writes

### Legacy ABI (audited)

```text
getInodeLocation  (register ABI; not stdcall; omit-FP)
  in:  EAX = inode number (1-based); EBP → EXTFS*
  out: EDX:EAX = partition-relative inode sector
       ECX = byte offset within that 512-byte sector (0..511)
  clobbers: EAX, EBX, ECX, EDX, EFLAGS
  preserves: ESI, EDI, EBP
  DF: unchanged
  stack: balanced push/pop; ret 0
```

### Rust ABI

```text
stdcall rust_get_inode_location(
  inode, inodes_per_group, inode_table_lo, inode_table_hi,
  sectors_per_block, inode_size, out_hi*, out_ofs*
) → EAX = sector_lo
; writes *out_hi, *out_ofs
; ret 32
```

### Trampoline

* Omit-FP (`EBP` stays → `EXTFS*`)
* Preserve ESI/EDI; leave EBX clobbered (legacy)
* Perform BGD locate + `inodeTable_lo/hi` load in FASM (keeps Rust blob tiny)
* Inject scalars; stack out-slots for hi/ofs; **never** `add esp` for Rust args (REG-009)
* Restore ESI/EDI; load EDX/ECX from out-slots; `ret 0`

### Oracle design

* Independent implementation of the exact x86 sequence (wrapping `dec`, `div`, `shl` with `cl&31`, `imul` truncate, `mul` full 64, `adc`)
* Synthetic BGDESCR buffers for `descShift` 5 and ≥6 paths
* Fixed PRNG seed **`0x47494C4F`** (`'GILO'`)
* 50,000 randomized cases + focused edge vectors (inode 0/1, IPG boundaries, inodeSize 128/256, sector-crossing byte ofs, hi-path)

### Host tests / ABI smoke / QEMU

* Focused `gilo_*` tests before QEMU
* ABI smoke: truncated synthetic EXTFS (≥ through `superblock.inodeSize`) + stack BGDESCR; marker `'GILO'`; hang `DEAD0C6D`
* QEMU: gate OFF → ON → A/B → ON×3
* Subsystem soak: `--disk ext` (new) — attach Kolibri-compatible EXT2 image; desktop running, resets=0 (exercises mount → `readInode` → `getInodeLocation`)

### Expected blob / memory

* Measured reloc-free blob **101 B**, 0 relocations
* SHA-256 `8dd04514d23f7448e300dfa833c33e6f2139683be8f7b80c515f741bd30a3b2a`
* **REG-012:** do not move `SLOT_BASE` (must end at `VGABasePtr`). Final pack:
  `TMP_STACK_TOP`/`sys_proc` = `0x008E000`, `SLOT_BASE` = `0x0090000`. Compact smoke
  so end `.bss` = `0x8CFC3` clears the assert (`align 16` cliff in `data32.inc`).

### EXT soak tooling (required for this cut)

* `tools/mkfs_utils/create_ext_image.py` + Docker `e2fsprogs` helper (mirror XFS)
* `scripts/mkfs.py ext`
* `python scripts/run_qemu.py --disk ext` / `qmp_desktop_smoke.py --disk ext`
* Image: `images/ext-image.img` — plain EXT2, SB@1024, features Kolibri accepts

---

## Implementation checklist

1. Rust pure helper + oracle tests + stdcall FFI export
2. Extract reloc-free blob; wire `build.toml` blob + migration
3. FASM trampoline + retained FASM body behind gate
4. ABI smoke + kernel.asm / kernel32.inc hooks
5. EXT image tooling + `--disk ext`
6. Host suite → ABI smoke → QEMU OFF/ON/A/B/ON×3 → `--disk ext` soak
7. Memory assert; TMP adjust only if measured
8. Docs + inventory 94/135

**Stop after Cut CM. Do not start Cut CN.**
