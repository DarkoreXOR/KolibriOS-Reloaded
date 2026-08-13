# Cut CR Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-cr-implementation.md`](cut-cr-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CR** migrates GUI glyph rasterization —
> `drawChar` in `kernel/gui/font.inc`.  
> Cut CQ remains complete and must not be modified. Do not start Cut CS.

---

## Fresh post-CQ repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **98 / 135** (`migration-todo.md`; 98 `[x]` + 37 `[ ]`) |
| Production gates | **98** `[[rust.migrations]]` with `enabled = true` |
| Cut CQ | **complete** — `exFAT_find_lfn` SHA `e146d2a5fbe9f30552ae8f3678c6bd12917c54976907083aef4e2491c11f217d` (**1324 B / 0 reloc** in implementation; plan listed 1301 B pre-REG-019) |
| `USE_RUST_EXFAT_FIND_LFN` | **1** |
| REG-016 | **Fixed** — not reopened |
| REG-017 / REG-018 / REG-019 | **Fixed** — not reopened |
| Final CQ image | `dev_build/test/kernel-20260813-154326.img` |
| `TMP_STACK_TOP` | **`0x008E000`** |
| `sys_proc` | **`0x008E000`** |
| `SLOT_BASE` | **`0x0090000`** (REG-012 pack) |
| End `.bss` (CQ ON) | **`OS_BASE+0x8BB03`** (implementation) |
| Early-stack assert | `0x8CB03 < 0x8E000` → **~7.2 KiB** headroom |

**Do not restore stale `0x008F000` / `SLOT_BASE=0x91000`.** Do not raise `TMP_STACK_TOP` speculatively. Do not reopen REG-016/017/018/019 without a new reproducible failure. Do not modify Cut CQ.

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader orchestration stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD+CP | Clip + syscall-73 blit leaves; LFB / win_map / cursor policy still FASM |
| AH+AI+CL+CQ exFAT | Hash/sector/lookup leaves ≠ plugin ownership (`exFAT_get_name` still FASM) |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| D+BB+BF+BH+CN + `strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| AL+BR+BS+CM EXT | Time pack + one address leaf ≠ EXT FS ownership |
| J + `ntfs_restore_usa_frs` | USA restore is Rust; FRS wrapper is a 2-instruction fallthrough, not NTFS ownership |
| CO `unpack` | Decoder island — **done**; still not subsystem ownership |
| GUI S+CE + `drawChar` | Geometry/clientbox + one glyph leaf ≠ GUI server ownership |

Cut CR remains **Path B**. Cut N `antiAliasing` plus this glyph leaf do not establish a Rust-owned GUI server.

---

## Complete candidate ranking (37 pending)

| Rank | Symbol | Oracle | Memory | Soak | Verdict |
|------|--------|--------|--------|------|---------|
| **1** | **`drawChar`** | **4/5** exact pixels (not desktop non-black); independent bit-walk + blend weights; `syscall_getpixel` injected | KiB blob; ~7.2 KiB `.bss` headroom | desktop `dtext` | **SELECT** |
| 2 | `ntfs_restore_usa_frs` | 5/5 (Cut J replay) | ~8 B | `--disk ntfs` | **REJECT** — `mov eax,[ebp+NTFS.frs_size]` + fallthrough into already-migrated `ntfs_restore_usa`. Four live callers, zero new semantics. |
| 3 | `alloc_page` / `map_page` | 5/5 bitmap / PTE | tiny | every alloc | **DEFER** — Stage 4; CLI + `sys_pgmap` / `invlpg`; not subsystem ownership |
| 4 | `tcp_output` / `ipv4_output*` / `*_SetFileInfo` | weak/write | large | protocol / FS write | **DEFER** — Stage 5–6 orchestration, mutex, disk mutate |
| 5 | `strnlen` | 5/5 | ~20 B | PE export | **REJECT** — PE-export-only; `_strnlen` in taskman is a **private copy** |
| 6 | `tcp_mss` | 5/5 | ~20 B | 1 TCP caller | **REJECT** — thin clamp+store |
| 7 | `mutex_init` | 5/5 | ~20 B | ~33 callers | **REJECT** — thin + fan-out |
| 8 | `enable_irq` / `irq_eoi` | 1/5 | ~80 B | IRQ path | **REJECT** — no deterministic mask/EOI oracle |
| 9 | `mem_test` | 1/5 | ~60 B | boot-only; **skipped when E820 present** | **REJECT** — CR0/cache/`wbinvd`; QEMU E820 skips the body |
| 10 | `get_phys_addr` / `net_ptr_to_num` / `sysfn_*` / `pid_to_appdata` / `socket_check_owner` / `socket_ptr_to_num` | 5/5 | tiny | weak/dead | **REJECT** — thin / wrapper / façade / dead |
| … | remaining Stage 4–7 / ban / unsuitable | varies | High | deferred | unchanged |

### Special scrutiny outcomes

| Target | Outcome |
|--------|---------|
| `drawChar` | **SELECT.** Four live `dtext` call sites (`pushd esi edi 16` UTF-16/UTF-8/866→Uni, `pushd esi edi 9` cp866 6×9). Exact-pixel oracle is independent of desktop non-black. Hidden `[esp+20+deltaToScreen]` / `[esp+20+widthX]` is **dtext frame + `pushd` 12 + retaddr 4**; trampoline snapshots those slots before stdcall (REG-010). `fontSmoothing` injected as a **value**. `syscall_getpixel` injected via a FASM stdcall thunk (fake `pushad` so `SYSCALL_STACK.eax` = +32 still works). Cut N `antiAliasing` inlined. Not Path A. Stage 7 blast is justified by evidence quality after every thinner/orchestration candidate failed the bar. |
| `ntfs_restore_usa_frs` | Evaluated as a Cut J sibling. Body is two instructions and fallthrough. Migrating it would wrap `rust_ntfs_restore_usa` with a field load — inventory inflation, not a leaf. |
| `alloc_page` / `map_page` | Strong synthetic bitmap/PTE oracles, but CLI + global `sys_pgmap` / `invlpg` / `pages_free` is Stage 4 allocator ownership, which is not established. Catastrophic blast. |
| `strnlen` | Public `proc strnlen` is PE-export only. `taskman.inc` `_strnlen` is a distinct private leaf. Thin `repne scasb`. Unchanged vs CQ. |
| IRQ / `mem_test` / thin wrappers | Unchanged rejects. |

### Selection rationale

Post-CQ, the remaining named Path B leaf with independent semantic substance, live production callers, and a host-checkable oracle is **`drawChar`**. `ntfs_restore_usa_frs` was evaluated seriously and rejected as a fallthrough. Stage 4–7 orchestration (`alloc_page`, `tcp_output`, `*_SetFileInfo`) still lacks ownership and has write-path / TLB / mutex blast that the evidence bar does not cover. Thin export/clamp/IRQ leaves remain rejects.

---

## Selected target: `drawChar`

| Field | Value |
|-------|-------|
| Source | `kernel/gui/font.inc` ~305–844 |
| Subsystem | GUI / glyph rasterization (Stage-7 leaf; Cut N compose) |
| Stage | Stage 2 / Stage 7 GUI foothold |
| Path | **B** |
| Callers | **4** live (`dtext` UTF-16, UTF-8, cp866 6×9, cp866→Uni) |
| Callees | `antiAliasing` (N, inlined); `syscall_getpixel` (injected thunk) |
| Globals | `fontSmoothing` — **injected value** |

### Legacy ABI

```text
drawChar  (plain `call` / `ret`, not stdcall)
  in:  EBP = font color
       ESI = font multiplier (SSS+1, typically 1)
       EDI → 32bpp buffer (row left edge)
       EBX → glyph bitmap (one byte per row)
       [esp+4]  = row count (16 or 9; dtext `pushd … 16/9`)
       [esp+24] = widthX (pitch in bytes)   ; dtext `widthX=8` + 16-byte overhead
       [esp+44] = deltaToScreen             ; dtext `deltaToScreen=28` + 16-byte overhead
       EDX high bytes leak into first-row `bsf eax, edx` after `mov dl,[ebx]`
  out: ESI = multiplier (restored)
       EBP = color (preserved)
       EBX = glyph+rows (clobbered; callers reload)
       EDI advanced then discarded by caller `pop`
       [esp+4] decremented to 0 (caller pops into a scratch edi)
  preserves: EBP, ESI (multiplier)
  clobbers: EAX, ECX, EDX, EBX, EDI, flags
  DF: unchanged (no cld)
  flags: not an observable return
```

`[esp+20+widthX]` / `[esp+20+deltaToScreen]` inside the FASM body are **after** `push edi` (bit-column save). At function entry the same slots are `[esp+16+widthX]` = `[esp+24]` and `[esp+44]`. Overhead: retaddr 4 + `pushd` 12 = 16, matching the `+20` once `push edi` is on the stack.

### Rust ABI

```text
stdcall rust_draw_char(ctx); ret 4
  ctx = trampoline-built DrawCharCtx (40 bytes)
  void; pixels written through ctx.buffer
```

Trampoline: snapshot row count / widthX / deltaToScreen / EDX **before** stdcall (REG-010); inject `fontSmoothing` value and getpixel thunk (EBX=index, EAX=color, plain ret);
; preserve EBX/ESI/EDI/EBP; **no** `add esp` for Rust args (REG-009). `add esp, 40` is **local ctx only**. No `cld`.

### Oracle

| Item | Value |
|------|-------|
| Independent | Bit-walk with first-row EDX quirk; M×M square fill; Cut N AA formula `(3·neigh+font)>>2` vs production rotate; subpixel channel weights `(11a+5b)>>4` / `(7n+f)>>3` derived from `lea`/`shl` independently of production stores; neighbor `bt` geometry transcribed separately |
| PRNG seed | `0x44434852` (`'DCHR'`) |
| PRNG cases | 50,000 |
| Edge cases | empty glyph, single bit, full row, bit0/bit7 edges, dirty EDX high bits, smoothing 0/1/2, multiplier 1–4, rows 9/16, delta=0 vs mock getpixel, padded neighbors |

### Validation plan

| Layer | Plan |
|-------|------|
| Host tests | `dch_*` focused + 50k PRNG + canaries + full suite |
| ABI smoke | `rust_draw_char` with **synthetic** buffer (not live LFB; REG-003). smoothing=0, delta=0, one-bit glyph. Marker `'DCHR'` / fail `DEAD0C72`. |
| QEMU | OFF / ON / A/B / ON×3 — desktop reached, **779380**, `resets=0` |
| Subsystem soak | desktop `dtext` (icons/taskbar/clock); exact-pixel host oracle is primary |
| Rollback | `USE_RUST_DRAW_CHAR = 0` |

### Memory impact

| Blob expected low-KiB (1× glyph walk + smoothing). Scaled `esi!=1` stays FASM — a fully inlined diamond was **20155 B** and failed the REG-012 assert. No `.bss` glyph buffer — caller owns the dtext temp bitmap. Do **not** raise `TMP_STACK_TOP`.

### Gate

`USE_RUST_DRAW_CHAR = 1` in `kernel/gui/font.inc` / `project/build.toml`.

---

**Stop after Cut CR. Do not start Cut CS.**
