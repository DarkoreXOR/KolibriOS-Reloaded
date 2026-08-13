# Cut CR Implementation — `drawChar`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-cr-plan.md`](cut-cr-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CR** |
| FASM symbol | `drawChar` |
| Source | [`kernel/gui/font.inc`](../../kernel/gui/font.inc) |
| Callers | **4** live (`dtext` UTF-16, UTF-8, cp866 6×9, cp866→Uni) |
| Rust symbol | `rust_draw_char` |
| Pure helper | `kolibri_utils::draw_char` / `DrawCharCtx` |
| Subsystem | GUI / glyph rasterization (Stage-7 leaf; Cut N compose) |
| Stage | Stage 2 / Stage 7 GUI foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — post-CQ vacuum; no remaining pending group is a Rust-owned subsystem. GUI S+CE+N plus this glyph leaf still do not own the GUI server. Video H+CD+CP remain clip/blit leaves; LFB / win_map / cursor policy stay FASM.

Selected `drawChar` over `ntfs_restore_usa_frs` (two-instruction load + fallthrough into already-migrated Cut J), `alloc_page`/`map_page` (Stage 4 CLI/`sys_pgmap`/`invlpg`; ownership not established), `tcp_output`/`*_SetFileInfo` (orchestration / write-path), `strnlen` (PE-export-only thin; taskman `_strnlen` is a private copy), `tcp_mss`/`mutex_init` (thin), and `enable_irq`/`irq_eoi`/`mem_test` (no deterministic hardware oracle). Stage 7 blast is justified by an exact-pixel host oracle after every thinner/orchestration candidate failed the bar. **Not Path A.**

**Memory:** A fully inlined scaled diamond (`esi>1`) produced a **20155 B** reloc-free blob and failed `assert $-OS_BASE+PAGE_SIZE < TMP_STACK_TOP` (end `.bss` `OS_BASE+0x90643`). Production blob is therefore the live **1×** path only; `esi != 1` keeps the original FASM body (`drawChar_fasm`). Desktop `dtext` uses multiplier `SSS+1` = **1**. **`TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` unchanged** (`0x8E000` / `0x8E000` / `0x90000`). ON end `.bss` **`OS_BASE+0x8C3C3`**; assert `0x8D3C3 < 0x8E000` PASS. OFF (no trampoline) end `.bss` **`OS_BASE+0x8C343`**.

---

## Legacy ABI

```text
drawChar  (plain `call` / `ret`, not stdcall)
  in:  EBP = font color
       ESI = font multiplier (SSS+1; desktop 1)
       EDI → 32bpp buffer (row left edge)
       EBX → glyph bitmap (one byte per row)
       [esp+4]  = row count (16 or 9; dtext `pushd … 16/9`)
       [esp+24] = widthX (pitch in bytes)
       [esp+44] = deltaToScreen
       EDX high bytes leak into first-row `bsf eax,edx` after `mov dl,[ebx]`
  out: ESI = multiplier (restored)
       EBP = color (preserved)
       EBX = glyph+rows (clobbered; callers reload)
       EDI advanced then discarded by caller `pop`
       [esp+4] decremented to 0
  preserves: EBP, ESI (multiplier)
  clobbers: EAX, ECX, EDX, EBX, EDI, flags
  DF: unchanged (no cld)
  flags: not an observable return
```

`[esp+20+widthX]` / `[esp+20+deltaToScreen]` inside the FASM body are **after** `push edi`. At entry the same slots are `[esp+24]` / `[esp+44]` (retaddr 4 + `pushd` 12 = 16).

When `USE_RUST_DRAW_CHAR=1` and `esi != 1`, the trampoline jumps to `drawChar_fasm` (full original body, including `jnz drawChar_fasm` row loop — not back into the trampoline).

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_draw_char` |
| Blob | **1958** bytes, **0 relocations** |
| SHA-256 | `9fe4d8e9636149563e1f0b65cfa9b0df76453abe3a61e0dcf10e56c864e6f15f` |
| Epilogue | `ret 4` (extractor `--expect-ret-imm 4`; trailing `c20400`) |
| Trampoline | `cmp esi,1` / `jne drawChar_fasm`; snapshot rows/widthX/delta/EDX **before** stdcall (REG-010); inject `fontSmoothing` **value** and getpixel thunk; `mov eax, esp` / `stdcall rust_draw_char, eax`; **no** `add esp` for Rust args (REG-009); `add esp, 40` is **local ctx only**; preserve EBX/ESI/EDI/EBP; no `cld` |
| Gate | `USE_RUST_DRAW_CHAR` (prod **1**) |
| Rust ABI | `stdcall rust_draw_char(ctx); ret 4` |
| Ctx | 40-byte `DrawCharCtx` (i686, 10 dwords): color, multiplier, buffer, glyph, rows, widthX, deltaToScreen, smoothing, getpixel, edx_in |

Getpixel thunk is **register ABI**, not stdcall: `EBX = index`, `EAX = color`, plain `ret`, `pushad`/`call syscall_getpixel`/`popad` so `SYSCALL_STACK.eax` (+32) lands in the saved EAX slot. A callee `ret 4` inside Rust `call edx` is invisible to LLVM (ESP drift). Rust pins fn/index to EDX/ECX, `mov ebx, ecx`, `lateout` ECX/EDX (REG-017/019). Cut N `antiAliasing` is inlined (no cross-blob reloc).

`rust_draw_char` → `draw_char_ptr` → `draw_char_1x` only. Host `draw_char()` still runs the scaled diamond for `dch_multiplier_square`.

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent 1× unsmoothed bit-walk (first-row EDX quirk); independent subpixel weights `(11a+5b)>>4` / `(7n+f)>>3` vs production `lea`/`shl`; Cut N AA `(3·neigh+font)>>2` vs production rotate. Smoothing neighbor `bt` geometry is a FASM-faithful transcription (shared control flow; blend math is independent). |
| PRNG seed | `0x44434852` (`'DCHR'`) |
| PRNG cases | **50,000** (multiplier forced to **1** — desktop default; scaled OOB wrapping is host-address-dependent and is not the live path) |
| Edge cases | empty glyph, single bit, full row, bit0/bit7, dirty EDX, smoothing 0/1/2, rows 9/16, delta=0 vs constant mock getpixel, M=2 square (host `draw_char`, not the kernel blob), canaries |
| Cut CR host tests | focused `dch_*` **16/16 PASS** |
| Full host suite | **806/806 PASS** |
| ABI smoke | **PASS** — public `rust_draw_char` with **synthetic** buffer (not live LFB; REG-003). smoothing=0, delta=0, glyph bit3 → pixel at +12. Marker `'DCHR'` / fail `DEAD0C72` (hang). EBX canary `0xB10000C1`. |

Desktop non-black counts are **not** the glyph oracle. Primary evidence is exact pixels in `dch_*`.

---

## QEMU validation

| Config | Gate | Image | non-black | resets | Result |
|--------|------|-------|-----------|--------|--------|
| OFF | FASM `drawChar` | `kernel-20260813-162309.img` (disposable; deleted after A/B) | 779380 | 0 | desktop-reached PASS |
| ON | `USE_RUST_DRAW_CHAR=1` | `kernel-20260813-162513.img` | 779380 | 0 | desktop-reached PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 / 779380 / 779380 | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
(splash-class ≤20000 non-black is FAIL; desktop floor 100000. RESET is FAIL.)  
Final image: `dev_build/test/kernel-20260813-162513.img`

---

## Subsystem soak

**Kernel-live.** Desktop `dtext` (icons / taskbar / clock) is the production path: four `call drawChar` sites with `pushd … 16` or `9` and multiplier 1. Default `fontSmoothing = 2` (subpixel) is the Rust 1× blend. OFF/ON desktop A/B is the live glyph soak. Primary correctness remains host exact-pixel tests, not framebuffer totals.

---

## Regressions

None new. REG-016/017/018/019 were **not** reopened. Applied prior lessons: REG-009 (no extra `add esp` after `ret 4`); REG-010 (snapshot dtext slots before stdcall); REG-012 (1× blob instead of raising `TMP_STACK_TOP`; scaled stays FASM); REG-003 (synthetic buffer smoke); REG-017/019 (`lateout` ECX/EDX around getpixel; no `in("esi")`).

A fully inlined scaled blob was **not** recorded as REG-NNN — it never shipped. The assert failure was caught at assemble time.

---

## Production gate

`USE_RUST_DRAW_CHAR = 1` in `kernel/gui/font.inc` (via `project/build.toml` migration registry).

---

## Rollback

```text
USE_RUST_DRAW_CHAR = 0
```

in `kernel/gui/font.inc` (or `enabled = false` for Cut CR in `project/build.toml` then rebuild). Legacy FASM body is always assembled as `drawChar_fasm` (and is `drawChar` when the gate is off).

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/draw_char.rs` | production 1× + host scaled + oracle + `dch_*` |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_draw_char` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | `mod draw_char` |
| `kernel/gui/font.inc` | gate + trampoline + FASM `drawChar_fasm` fallback |
| `kernel/rust/draw_char.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | gated smoke call |
| `project/build.toml` | blob + Cut CR migration `enabled = true` |
| `docs/migration/cut-cr-plan.md` | plan |
| `docs/migration/cut-cr-implementation.md` | this report |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | Cut CR entry |

Memory docs (`fixed-addresses.md`, `memory-model.md`) **unchanged** — pack addresses did not move.

---

## Known limitations

* Kernel blob is **1× only**. `esi != 1` (user font size > default) uses FASM `drawChar_fasm`. Host tests still cover the scaled diamond via `draw_char()`.
* In-kernel smoke uses a synthetic buffer with smoothing=0 and delta=0; it does not call `syscall_getpixel` (REG-003).
* Smoothing neighbor `bt` control flow is a FASM transcription; independent evidence is the bit-walk, AA formula, and subpixel weights.
* `invoke` getpixel exists only on `i686-none`; host `dch_*` inject a constant mock.

---

## Updated inventory

**99 / 135** (`99` enabled production migrations = Cut A four symbols + B–CR).

---

## Ranked candidates for Cut CS

Path A remains **rejected** unless a future audit proves a real Rust-owned subsystem.

After CR, the named Path B leaf inventory that meets the CQ/CR evidence bar is largely exhausted. Do **not** inflate the count.

| Rank | Symbol | Verdict for CS audit |
|------|--------|----------------------|
| 1 | `alloc_page` / `map_page` | **DEFER** unless Stage 4 ownership is explicitly accepted (CLI, `sys_pgmap`, `invlpg`) |
| 2 | `tcp_output` / `ipv4_output*` | **DEFER** — protocol island, mutex, no isolated oracle |
| 3 | `*_SetFileInfo` | **DEFER** — FS write-path |
| 4 | `ntfs_restore_usa_frs` | **REJECT** — 2-instruction fallthrough into Cut J |
| 5 | `strnlen` / `tcp_mss` / `mutex_init` | **REJECT** — thin; `_strnlen` is a private copy |
| 6 | `enable_irq` / `irq_eoi` / `mem_test` | **REJECT** — no deterministic I/O/hardware oracle |
| 7 | wrappers / dead | **REJECT** — `get_phys_addr`, `net_ptr_to_num`, `sysfn_*`, `pid_to_appdata` (commented-only caller), `socket_check_owner`, `socket_ptr_to_num`, `strtoint_dec` |

**Stop after Cut CR.** Cut CS audit: **BLOCKED** — see [`cut-cs-plan.md`](cut-cs-plan.md).
