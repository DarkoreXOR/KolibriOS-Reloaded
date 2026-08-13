# Cut CO Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-co-implementation.md`](cut-co-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CO** migrates KPCK/LZMA unpack —
> `unpack` in `kernel/unpacker.inc`.  
> Cuts CC–CN remain complete and must not be modified. Do not start Cut CP.

---

## Fresh post-CN repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **95 / 135** (`migration-todo.md`; 95 `[x]` + 40 `[ ]`) |
| Production gates | **95** `[[rust.migrations]]` with `enabled = true` |
| Cut CC–CM | intact (gates ON; blobs untouched) |
| Cut CN blob SHA | `e01986525b60bba7fc73747c7dddf3bf3f3e1832296e7f3139f3d407f9b5914b` (**37 B / 0 reloc**) |
| Cut CM blob SHA | `8dd04514d23f7448e300dfa833c33e6f2139683be8f7b80c515f741bd30a3b2a` (**101 B / 0 reloc**) |
| Cut CK/CL blob SHA | `766a371d747139c9f2520f4b6a55e18e6367fa9fdf6530637902d3a8be374572` (**25 B / 0 reloc** each) |
| `TMP_STACK_TOP` | **`0x008E000`** (`kernel/const.inc`) |
| `sys_proc` | **`OS_BASE+0x008E000`** |
| `SLOT_BASE` | **`OS_BASE+0x0090000`** (REG-012 pack; must end at `VGABasePtr`) |
| End `.bss` (CN) | **`OS_BASE+0x8CFC3`** |
| Early-stack assert | `0x8DFC3 < 0x8E000` → **0x3D (~61 B)** headroom |
| Docs vs tree | CN plan+impl, inventory, gates, blob SHA, memory pack agree |

**Note:** Do not restore stale `0x008F000` / `SLOT_BASE=0x91000`. Authoritative pack is CN/REG-012 above. Do not raise `TMP_STACK_TOP` speculatively.

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader orchestration stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD + `blit_32` | Geometry only; LFB / win_map / cursor stay FASM |
| AH+AI+CL + `exFAT_find_lfn` | Sector/hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| D+BB+BF+BH+CN + `strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| AL+BR+BS+CM EXT | Time pack + one address leaf ≠ EXT FS ownership |
| `unpack` | Single decoder island — **not** subsystem ownership |

Cut CO remains **Path B**.

---

## Complete candidate ranking (40 pending)

| Rank | Symbol | Oracle | Memory | Soak | Verdict |
|------|--------|--------|--------|------|---------|
| **1** | **`unpack`** | **5/5** independent KPCK/LZMA+E8/E9 bitstream | .text only; heap `unpack.p` already allocated; omit smoke | DLL `load_file` / `load_file_umode` | **SELECT** |
| 2 | `exFAT_find_lfn` | 3/5 | callbacks + 524 B stack LFN | `--disk exfat` | Defer — plugin/callback island; stack-arg ABI (REG-010 class) |
| 3 | `blit_32` | 3/5 (pixel buffer required) | KiB + LFB/cursor/win_map | syscall 73 | Defer — LFB blast; desktop non-black insufficient |
| 4 | `drawChar` | 2/5 | KiB + smoothing + `syscall_getpixel` | GUI | Defer — Stage 7 |
| 5 | `strnlen` | 5/5 | ~35 B | PE export; 0 public callers (`_strnlen` in taskman is a **private copy**) | Reject — thin export-only |
| 6 | `tcp_mss` | 5/5 | ~30 B | 1 TCP caller | Reject — thin clamp+store / TCP deepen |
| 7 | `mutex_init` | 5/5 | ~25 B | ~33 callers | Reject — thin + fan-out |
| 8 | `enable_irq` / `irq_eoi` | 1/5 | ~80 B | IRQ path | Reject — no deterministic mask/EOI oracle |
| 9 | `mem_test` | 3/5 | ~60 B | boot-only; skipped when E820 present | Defer — CR0/cache probe |
| 10 | `get_phys_addr` / `net_ptr_to_num` / `ntfs_restore_usa_frs` / `sysfn_*` / `pid_to_appdata` / `socket_check_owner` / `socket_ptr_to_num` | 5/5 | tiny | weak/dead | Reject — thin / wrapper / façade / dead |
| … | remaining Stage 4–7 orchestration | varies | High | deferred/ban/unsuitable | unchanged |

### Special scrutiny outcomes

| Target | Outcome |
|--------|---------|
| `unpack` | **SELECT.** `stdcall(packed, unpacked); ret 8` + `pushad`/`popad`. Heap `unpack.p` (~31.2 KiB) already `kernel_alloc`'d; static globals are 7 dwords + 1 byte already in `.bss`. Nested range-decoder labels are **not** independently callable — whole function is the cut. Custom range-init (`lodsd` LE, not 5-byte standard LZMA) + Kolibri match-literal (`CH=match_bit` planes 256/512) + FASM-not-bswap E8 demand a FASM-flow oracle. `.text` blob **does** raise linear `.bss` — move uninitialized pitch LUTs after `sys_pgmap` rather than touching `SLOT_BASE`. Omit in-kernel ABI smoke (CN/REG-012 pattern). |
| `exFAT_find_lfn` | Stack callbacks at `[esp+8]`/`[esp+4]`, unmigrated `exFAT_get_name`, EBP `exFAT*` mutation, CF/ESI/EDI outs. Real soak exists; ABI is a REG-010 minefield. Not Path A. |
| `blit_32` | LFB/cursor/bpp/`win_map`; Cut CD covers clip only. Pixel oracle required; non-black count is insufficient. |
| `drawChar` | ~540 lines + smoothing + `syscall_getpixel`. Stage 7. |
| IRQ | PIC `in`/`out` + APIC MMIO. Desktop reachability is not a mask/EOI oracle. |
| `strnlen` | Public `proc strnlen` is PE-export only. `taskman.inc` `_strnlen` is a distinct private leaf — migrating the export does not hit process-create. Thin `repne scasb`. |
| Thin/wrappers | `tcp_mss`, `mutex_init`, `get_phys_addr`, `net_ptr_to_num`, `ntfs_restore_usa_frs`, `sysfn_getfreemem`, `sysfn_mouse_acceleration`, `pid_to_appdata` (commented-only caller), `socket_check_owner` rejected. |

### Selection rationale

Post-CN, no pending in-kernel leaf combines **ABI clarity**, reloc-free size, and subsystem fit except `unpack`. Remaining high-value targets fail plugin/LFB/Stage-7/thin/IRQ bars. Prior “memory-blocked” deferrals conflated `.bss` smoke headroom with the **heap** probability buffer and `.text` blob. Cut CN established the omit-smoke pattern; CO follows it. Evidence quality (independent bitstream + exact dest bytes + malformed headers + live DLL KPCK path) outranks caller count.

---

## Selected target: `unpack`

| Field | Value |
|-------|-------|
| Source | `kernel/unpacker.inc` ~16–532 |
| Subsystem | core / KPCK+LZMA decoder (Stage-2 leaf) |
| Stage | Stage 2 / unpack helper |
| Path | **B** |
| Callers | **2** live (`dll.inc` `load_file`, `load_file_umode`) under `unpack_mutex` |
| Callees | none (nested local labels only) |
| Globals | `unpack.p` (heap ptr, injected); `code_`/`range`/`rep0–3`/`previousByte` become Rust locals |
| Callbacks | none |

### Legacy ABI

```text
unpack  stdcall(packed, unpacked)
  in:  stack — packed ptr, unpacked ptr
  out: none (void); dest filled on success
  preserves: all GPRs via pushad/popad
  clobbers: (internal only; restored)
  DF: unchanged (no cld; assumes DF=0 like FASM lods/stos)
  flags: not an observable return
  stack: ret 8
  fail: method bits != 1 or (flags & 0xC0)==0xC0 → dest untouched
```

### Rust ABI

```text
stdcall rust_unpack(packed, unpacked, p); ret 12
  p = trampoline-injected [unpack.p]  (7990 u32 probability slots)
```

Trampoline: snapshot args into registers **before** stdcall pushes (REG-010); `pushad`/`popad`; **no** `add esp` (REG-009). `cld` not added (FASM has none).

### Oracle

| Item | Value |
|------|-------|
| Independent | Spec-structured FASM-semantic decoder (not a line copy of the production helpers); custom LE `lodsd` init; Kolibri match-literal planes `256+symbol` / `512+symbol` (`CH=match_bit`); E8 rel32 = FASM `shr ax,8`/`ror 16`/`xchg al,ah` (not `bswap`) |
| PRNG seed | `0x5550434B` (`'UPCK'`) |
| PRNG cases | 50,000 (bounded dest; flags without E8/E9 to avoid unbounded scan) |
| Edge cases | `0xC0` fail, method≠1 fail, dest_len=0, dest_len=1, aligned dest, E8/E9 `.c1`/`.c2` crafted, end-marker `rep0` overflow |

### Validation plan

| Layer | Plan |
|-------|------|
| Host tests | `upck_*` focused + 50k PRNG + full suite |
| ABI smoke | **N/A in-kernel** (REG-012 ~61 B `.bss` headroom); host `upck_*` + trampoline contract |
| QEMU | OFF / ON / A/B / ON×3 |
| Subsystem soak | Desktop DLL/PE KPCK path (`load_file`); PE-export class apps that hit `unpack` |
| Rollback | `USE_RUST_UNPACK = 0` |

### Memory impact

The ~9 KiB `.text` blob **does** raise linear `.bss`. Pitch LUTs `BPSLine_calc_area` / `d_width_calc_area` moved after `sys_pgmap` (uninitialized `rd`, not in `kernel.mnt`; first-4MiB PSE; B32 wipe). **Do not** raise `TMP_STACK_TOP` or move `sys_proc`/`SLOT_BASE`. Heap `unpack.p` unchanged. Omit in-kernel smoke (REG-012). Measured ON end `.bss` `OS_BASE+0x8AA83`.

### Gate

`USE_RUST_UNPACK = 1` in `kernel/unpacker.inc` / `project/build.toml`.

---

**Stop after Cut CO. Do not start Cut CP.**
