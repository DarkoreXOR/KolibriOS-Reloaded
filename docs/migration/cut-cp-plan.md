# Cut CP Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-cp-implementation.md`](cut-cp-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CP** migrates syscall-73 LFB blit —
> `blit_32` in `kernel/video/blitter.inc`.  
> Cuts CC–CO remain complete and must not be modified. Do not start Cut CQ.

---

## Fresh post-CO repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **96 / 135** (`migration-todo.md`; 96 `[x]` + 39 `[ ]`) |
| Production gates | **96** `[[rust.migrations]]` with `enabled = true` |
| Cut CN | intact — `strchr` SHA `e01986525b60bba7fc73747c7dddf3bf3f3e1832296e7f3139f3d407f9b5914b` (**37 B / 0 reloc**) |
| Cut CO | **complete** — `unpack` SHA `f42072296df5ed5b8a5e826bd7fe6e0d0a1159ab3a3830fdf5f5ec6c3493a744` (**8877 B / 0 reloc**) |
| REG-016 | **Fixed** — `fasm_load_rel32` is FASM `shr ax,8` / `ror 16` / `xchg al,ah`, not `bswap`; AL-only flag tests |
| Stale CO runs | FAT/Unicorn killed walk, `kernel-20260813-112145.img` (~8020), `kernel-20260813-115040.img` (~8021) are **pre-REG-016 diagnostics**, not final CO |
| Final CO image | `dev_build/test/kernel-20260813-121344.img` (exists; documented ON **779380** / `resets=0`) |
| CO OFF image | `kernel-20260813-111308.img` (documented **779380**) |
| `USE_RUST_UNPACK` | **1** (`kernel/unpacker.inc`) |
| `TMP_STACK_TOP` | **`0x008E000`** (`kernel/const.inc`) |
| `sys_proc` | **`OS_BASE+0x008E000`** |
| `SLOT_BASE` | **`OS_BASE+0x0090000`** (REG-012 pack) |
| End `.bss` (CO ON) | **`OS_BASE+0x8AA83`** (pitch LUTs after `sys_pgmap`) |
| Early-stack assert | `0x8BA83 < 0x8E000` → **~9.5 KiB** headroom |
| Docs vs tree | CO plan+impl, REG-016, gates, blob SHA, memory pack agree |

**Do not restore stale `0x008F000` / `SLOT_BASE=0x91000`.** Do not raise `TMP_STACK_TOP` speculatively. Do not reopen REG-016 without a new reproducible failure.

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader orchestration stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD + `blit_32` | Geometry is Rust; LFB / win_map / cursor policy still FASM-owned until this cut’s **leaf**, not subsystem ownership |
| AH+AI+CL + `exFAT_find_lfn` | Sector/hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| D+BB+BF+BH+CN + `strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| AL+BR+BS+CM EXT | Time pack + one address leaf ≠ EXT FS ownership |
| CO `unpack` | Decoder island — **done**; still not subsystem ownership |

Cut CP remains **Path B**.

---

## Complete candidate ranking (39 pending)

| Rank | Symbol | Oracle | Memory | Soak | Verdict |
|------|--------|--------|--------|------|---------|
| **1** | **`blit_32`** | **5/5** exact LFB/win_map pixels (not desktop non-black) | KiB blob; ~9.5 KiB `.bss` headroom after CO LUT move | syscall 73 / desktop blit | **SELECT** |
| 2 | `exFAT_find_lfn` | 3/5 (callbacks + `exFAT_get_name`) | 524 B stack LFN | `--disk exfat` | Defer — plugin/callback island; stack-arg ABI (REG-010 class) |
| 3 | `drawChar` | 2/5 (pixel + `syscall_getpixel`) | KiB + smoothing | GUI | Defer — Stage 7 |
| 4 | `strnlen` | 5/5 | ~35 B | PE export; `_strnlen` in taskman is a **private copy** | Reject — thin export-only after CN/BB |
| 5 | `tcp_mss` | 5/5 | ~30 B | 1 TCP caller | Reject — thin clamp+store / TCP deepen |
| 6 | `mutex_init` | 5/5 | ~25 B | ~33 callers | Reject — thin + fan-out |
| 7 | `enable_irq` / `irq_eoi` | 1/5 | ~80 B | IRQ path | Reject — no deterministic mask/EOI oracle |
| 8 | `mem_test` | 3/5 | ~60 B | boot-only; skipped when E820 present | Defer — CR0/cache probe |
| 9 | `get_phys_addr` / `net_ptr_to_num` / `ntfs_restore_usa_frs` / `sysfn_*` / `pid_to_appdata` / `socket_check_owner` / `socket_ptr_to_num` | 5/5 | tiny | weak/dead | Reject — thin / wrapper / façade / dead |
| … | remaining Stage 4–7 orchestration | varies | High | deferred/ban/unsuitable | unchanged |

### Special scrutiny outcomes

| Target | Outcome |
|--------|---------|
| `blit_32` | **SELECT.** Syscall 73 register ABI (`EBX=flags`, `ECX→params`). Calls Cut CD `blit_clip`; writes `LFB_BASE` through win_map; 32/24/16 bpp; software vs hardware cursor (`cmp [_display.select_cursor], select_cursor`). Exact pixel/buffer comparison is the oracle — desktop non-black is supplementary A/B only. Reloc-free via trampoline-injected `Blit32Ctx`. In-kernel smoke calls `rust_blit_32` with **synthetic** LFB (never live framebuffer). |
| `exFAT_find_lfn` | Stack callbacks at `[esp+8]`/`[esp+4]`, unmigrated `exFAT_get_name`, EBP `exFAT*` mutation, CF/ESI/EDI outs. Real soak exists; ABI is a REG-010 minefield. Not Path A. |
| `drawChar` | ~540 lines + smoothing + `syscall_getpixel`. Stage 7. |
| IRQ | PIC `in`/`out` + APIC MMIO. Desktop reachability is not a mask/EOI oracle. |
| `strnlen` | Public `proc strnlen` is PE-export only. `taskman.inc` `_strnlen` is a distinct private leaf. Thin `repne scasb`. Redundant after `strchr`/`strrchr`. |
| Thin/wrappers | `tcp_mss`, `mutex_init`, `get_phys_addr`, `net_ptr_to_num`, `ntfs_restore_usa_frs`, `sysfn_getfreemem`, `sysfn_mouse_acceleration`, `pid_to_appdata`, `socket_check_owner` rejected. |

### Selection rationale

Post-CO, the KPCK decoder is gone and LUT relocation opened **~9.5 KiB** `.bss` headroom — the previous memory veto on a KiB-class video blob no longer holds. Remaining named leaves are thin, IRQ-without-oracle, Stage 4–7 orchestration, or the exFAT plugin island. **`blit_32`** is the Cut CD follow-on with a **mandatory exact-pixel oracle**, live syscall-73 production path, and reloc-free ctx injection. Evidence quality (pixel buffers) outranks `exFAT_find_lfn` caller/soak convenience.

---

## Selected target: `blit_32`

| Field | Value |
|-------|-------|
| Source | `kernel/video/blitter.inc` ~257–586 |
| Subsystem | video / syscall-73 LFB blit (Stage-2 leaf; Cut CD compose) |
| Stage | Stage 2 / video hot path |
| Path | **B** |
| Callers | **1** live (`servetable2[73]` via `i40`/`sysenter`/`syscall` `pushad` frame) |
| Callees | `blit_clip` (Cut CD, inlined); `[_display.check_mouse]` (software-cursor only, injected) |
| Globals | `current_slot`, `current_slot_idx`, `_display.*`, `LFB_BASE`, pitch LUTs, `select_cursor` — **injected** |
| Callbacks | `check_mouse` register ABI (`EAX=color`, `ECX=x<<16\|y` → `EAX`) |

### Legacy ABI

```text
blit_32  (syscall 73 handler; plain `call` / `ret`, not stdcall)
  in:  EBX = flags (BLIT_CLIENT_RELATIVE = 0x20000000)
       ECX → userspace blit params:
         +0 dst_x, +4 dst_y, +8 w, +12 h,
         +16 src_x, +20 src_y, +24 src_w, +28 src_h,
         +32 bitmap, +36 stride
  out: none (void); LFB pixels owned by current_slot_idx in win_map
  preserves: EBX, ESI, EDI, EBP (push/pop)
  clobbers: EAX, ECX, EDX, flags
  DF: unchanged (no cld; syscall entry already cld)
  flags: not an observable return (blit_clip CF consumed internally)
  stack: local BLITTER + extras; ret 0
  fail/skip: clip reject, w=0, h=0 → dest untouched
  bpp: 32; else 24; else 16-path (any non-32/non-24)
```

### Rust ABI

```text
stdcall rust_blit_32(params, flags, ctx); ret 12
  ctx = trampoline-built Blit32Ctx (68 bytes)
```

Trampoline: snapshot `ECX`/`EBX` into registers **before** stdcall pushes (REG-010); preserve EBX/ESI/EDI/EBP; **no** `add esp` for Rust args (REG-009). `add esp, 68` is **local ctx only**. No `cld`.

### Oracle

| Item | Value |
|------|-------|
| Independent | Nested `y,x` walk + independent clip compose (`fasm_oracle_blit_clip`) + independent RGB565 pack; production uses LUT/pointer-style stores and FASM `shr ah,2` 16bpp sequence |
| PRNG seed | `0x42333220` (`'B32 '`) |
| PRNG cases | 50,000 (bounded dest; synthetic LFB/win_map/LUTs) |
| Edge cases | clip reject, w=0, h=0, client-relative, 32/24/16 bpp, software vs hardware cursor, win_map miss, check_mouse color rewrite, bpp∉{16,24,32} → 16-path |

### Validation plan

| Layer | Plan |
|-------|------|
| Host tests | `bl32_*` focused + 50k PRNG + exact buffer compare + full suite |
| ABI smoke | Direct `rust_blit_32` with **synthetic** LFB/win_map (not public `blit_32`; live LFB is REG-003/004 class). Marker `'BL32'`. Non-fatal `.fail` (REG-004). |
| QEMU | OFF / ON / A/B / ON×3 — desktop reached, **779380**, `resets=0` |
| Subsystem soak | Desktop syscall-73 blit path; exact-pixel host tests are primary; desktop A/B is live LFB soak |
| Rollback | `USE_RUST_BLIT_32 = 0` |

### Memory impact

**Measured:** blob **2292 B** / 0 reloc. ON end `.bss` `OS_BASE+0x8B503` → assert `0x8C503 < 0x8E000` PASS. Did **not** raise `TMP_STACK_TOP` or move `sys_proc`/`SLOT_BASE`. In-kernel synthetic-LFB smoke fitted.

### Gate

`USE_RUST_BLIT_32 = 1` in `kernel/video/blitter.inc` / `project/build.toml`.

---

**Stop after Cut CP. Do not start Cut CQ.**
