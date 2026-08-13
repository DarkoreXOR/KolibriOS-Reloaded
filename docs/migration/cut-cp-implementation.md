# Cut CP Implementation — `blit_32`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-cp-plan.md`](cut-cp-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CP** |
| FASM symbol | `blit_32` |
| Source | [`kernel/video/blitter.inc`](../../kernel/video/blitter.inc) |
| Callers | **1** live (`servetable2[73]` via `i40` / `sysenter` / `syscall` `pushad` frame) |
| Rust symbol | `rust_blit_32` |
| Pure helper | `kolibri_utils::blit_32::blit_32` / `Blit32Ctx` |
| Subsystem | video / syscall-73 LFB blit (Stage-2 leaf; Cut CD compose) |
| Stage | Stage 2 / video hot path |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — post-CO vacuum; no remaining pending group is a Rust-owned subsystem. Video H+CD own clip geometry; LFB / win_map / cursor policy were still FASM until this leaf.

Selected `blit_32` over `exFAT_find_lfn` (plugin/callback island), `drawChar` (Stage 7), `strnlen` (thin PE export), `tcp_mss` / `mutex_init` (thin), and IRQ leaves (no mask/EOI oracle). Cut CO LUT move opened **~9.5 KiB** `.bss` headroom, lifting the prior memory veto on a KiB-class video blob.

**Memory:** Blob + smoke iglobals raise linear `.bss`. **`TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` unchanged** (`0x8E000` / `0x8E000` / `0x90000`). ON end `.bss` **`OS_BASE+0x8B503`**; assert `0x8C503 < 0x8E000` PASS (~7.2 KiB remaining). OFF (FASM body) end `.bss` **`OS_BASE+0x8B703`**. Pitch LUTs remain after `sys_pgmap` (Cut CO).

---

## Legacy ABI

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
  callees: blit_clip (Cut CD, inlined in Rust); [_display.check_mouse]
           on software cursor (EAX=color, ECX=x<<16|y → EAX)
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_blit_32` |
| Blob | **2292** bytes, **0 relocations** |
| SHA-256 | `68e4f58b74effcbb35f3745baf24bb48c08057435ce614f3add9a816ab77ce72` |
| Epilogue | `ret 12` (shared epilogue; extractor confirmed `expect-ret-imm 12`) |
| Trampoline | snapshot ECX/EBX before stdcall (REG-010); `stdcall rust_blit_32, params, flags, ctx`; **no** `add esp` for Rust args (REG-009); `add esp, 68` is **local ctx only** |
| Gate | `USE_RUST_BLIT_32` (prod **1**) |
| Rust ABI | `stdcall rust_blit_32(params, flags, ctx); ret 12` |
| Ctx | 68-byte `Blit32Ctx` (i686): win box/client, slot, bpp, width, pitch, win_map, LFB, LUTs, select_cursor, software_cursor (`select_cursor` label), check_mouse |

Compose inlines Cut CD `blit_clip` (no reloc to `rust_blit_clip`). 16bpp uses FASM `shr ah,2` / `shr ax,3` / `ror 8` / `add al,ah` / `rol 8`.

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent nested `y,x` walk + `fasm_oracle_blit_clip` + `independent_pack_rgb565`; production uses LUT/pointer stores + FASM RGB565 sequence |
| PRNG seed | `0x42333220` (`'B32 '`) |
| PRNG cases | **50,000** (bounded dest; synthetic LFB/win_map/LUTs) |
| Edge cases | clip reject, w=0, h=0, client-relative, 32/24/16 bpp, software vs hardware cursor, win_map miss, check_mouse rewrite, bpp∉{16,24,32} → 16-path |
| Cut CP host tests | focused `bl32_*` **10/10 PASS** |
| Full host suite | **774/774 PASS** |
| ABI smoke | **PASS** — in-kernel `rust_blit_32` with **synthetic** LFB/win_map (not public `blit_32`; live LFB is REG-003/004). Marker `'BL32'` / fail `DEAD0C32`. Non-fatal `.fail` (REG-004). Stack canary + EBX/ESI/EDI/EBP + DF=0 + exact 32bpp pixels. |

Desktop non-black counts are **not** the blit oracle. Exact buffer comparison is host `bl32_*`.

---

## QEMU validation

| Config | Gate | Image | non-black | resets | Result |
|--------|------|-------|-----------|--------|--------|
| OFF | FASM `blit_32` | `kernel-20260813-130011.img` | 779380 | 0 | desktop-reached PASS |
| ON | `USE_RUST_BLIT_32=1` | `kernel-20260813-130209.img` | 779380 | 0 | desktop-reached PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 / 779380 / 779380 | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
(splash-class ≤20000 non-black is FAIL; desktop floor 100000. RESET is FAIL.)  
Final image: `dev_build/test/kernel-20260813-130209.img`

Screendump SHA is not A/B — clock/cursor pixels differ across boots. A/B is desktop-reached + non-black **779380**. Splash-class ~8020/~8021 (stale pre-REG-016 CO runs) is **not** success.

---

## Subsystem soak

**Kernel-live.** Syscall 73 (`blit_32`) is the production LFB blit used by desktop/GUI. OFF/ON desktop A/B is the live LFB soak. Primary correctness remains host exact-pixel tests (bpp, clip, win_map, cursor), not framebuffer non-black totals.

---

## Regressions

None new. REG-016 was **not** reopened (no new unpack failure). Applied: REG-009 (no extra `add esp` after `ret 12`); REG-010 (snapshot args before stdcall); REG-003/004 (synthetic LFB smoke, non-fatal fail); REG-012 pack left intact.

---

## Production gate

`USE_RUST_BLIT_32 = 1` in `kernel/video/blitter.inc` (via `project/build.toml` migration registry).

---

## Rollback

```text
USE_RUST_BLIT_32 = 0
```

in `kernel/video/blitter.inc` (or `enabled = false` for Cut CP in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/blit_32.rs` | production blit + oracle + `bl32_*` |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_blit_32` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | `mod blit_32` |
| `kernel/video/blitter.inc` | gate + trampoline + FASM `else` body |
| `kernel/rust/blit_32.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | gated smoke call |
| `project/build.toml` | blob + Cut CP migration `enabled = true` |
| `scripts/qmp_desktop_smoke.py` | splash-only vs desktop floor (REG-015/016 class) |
| `docs/migration/cut-cp-plan.md` | plan |
| `docs/migration/cut-cp-implementation.md` | this report |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | Cut CP entry |

Memory docs (`fixed-addresses.md`, `memory-model.md`) **unchanged** — pack addresses did not move.

---

## Known limitations

* Software-cursor `check_mouse` is a register-ABI kernel call injected via ctx; host tests use an optional `CheckMouseFn` instead of `asm!`.
* Public `blit_32` is not invoked from boot smoke (live LFB / `current_slot`).

---

## Updated inventory

**97 / 135** (`97` enabled production migrations = Cut A four symbols + B–CP).
