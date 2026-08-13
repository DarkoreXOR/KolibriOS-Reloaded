# Cut CO Implementation — `unpack`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-co-plan.md`](cut-co-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regressions:** [REG-015](regression-log.md#reg-015--splash-only-desktop-cut-co-lzma-match-literal-planes-2026-08-13), [REG-016](regression-log.md#reg-016--splash-only-desktop-cut-co-e8-not-bswap-2026-08-13).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CO** |
| FASM symbol | `unpack` |
| Source | [`kernel/unpacker.inc`](../../kernel/unpacker.inc) |
| Callers | **2** live (`dll.inc` `load_file`, `load_file_umode`) under `unpack_mutex` |
| Rust symbol | `rust_unpack` |
| Pure helper | `kolibri_utils::unpack::unpack` |
| Subsystem | core / KPCK+LZMA + optional E8/E9 filter (Stage-2 leaf) |
| Stage | Stage 2 / unpack helper |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — post-CN vacuum; no remaining pending group is a Rust-owned subsystem. `unpack` is a decoder island, not FS/GUI/IRQ ownership.

Selected `unpack` over `exFAT_find_lfn` (plugin/callback island), `blit_32` (pixel oracle required), `drawChar` (Stage 7), `strnlen` (export-only thin), `tcp_mss` (thin), and IRQ leaves (no mask/EOI oracle). Nested LZMA labels are not independently callable — the whole `unpack` is the cut.

**Memory:** Embedding the ~9 KiB blob in linear `.text` **does** raise `.bss`. Pitch LUTs `BPSLine_calc_area` / `d_width_calc_area` (`rd MAX_SCREEN_HEIGHT` ×2) moved from tight `.bss` to after `sys_pgmap` (still uninitialized, not in `kernel.mnt`, still inside the first 4 MiB PSE map, still zeroed by the B32 `CLEAN_ZONE→HEAP` wipe). **`TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` unchanged** (`0x8E000` / `0x8E000` / `0x90000`). ON end `.bss` **`OS_BASE+0x8AA83`**; assert `0x8BA83 < 0x8E000` PASS. In-kernel ABI smoke omitted (REG-012 pack).

---

## Legacy ABI

```text
unpack  stdcall(packed, unpacked)
  in:  stack — packed ptr, unpacked ptr
  out: none (void); dest filled on success
  preserves: all GPRs via pushad/popad
  DF: unchanged (FASM has no cld; assumes DF=0 for lods/stos/movs)
  flags: not an observable return
  stack: ret 8
  fail: AL method bits: (al & 0xC0)==0xC0 or (al & ~0xC0)!=1 → dest untouched
        (FASM tests AL only — high flag bytes are ignored)
```

KPCK: `'KPCK'` @0, dest_len @4, flags @8, LZMA @12. Custom range init is LE `lodsd` (not stock 5-byte LZMA). `posState = dest_ptr & 3`. Optional E8/E9: `0x40` = `.c1`, `0x80` = `.c2`/`.ctr1`. Rel32 rewrite uses FASM `shr ax,8` / `ror eax,16` / `xchg al,ah` — **not** `bswap eax`.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_unpack` (embedded in `unpacker.inc` when gated) |
| Blob | **8877** bytes, **0 relocations** |
| SHA-256 | `f42072296df5ed5b8a5e826bd7fe6e0d0a1159ab3a3830fdf5f5ec6c3493a744` |
| Epilogue | `ret 12` |
| Trampoline | snapshot `[esp+36]`/`[esp+40]` then `stdcall rust_unpack, packed, dest, [unpack.p]`; `pushad`/`popad`; **no** `add esp` |
| Gate | `USE_RUST_UNPACK` (prod **1**) |
| Rust ABI | `stdcall rust_unpack(packed, unpacked, p); ret 12` |

`p` is the existing heap probability buffer (`kernel_alloc` of 7990 dwords). Range/rep/prev-byte state is local (not FASM uglobals).

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-semantic decoder (separate range coder / lit-match / E8 scan) vs production |
| PRNG seed | `0x5550434B` (`'UPCK'`) |
| PRNG cases | 50,000 (bounded dest; flags without unbounded E8 scan) |
| Fixtures | `testdata/launcher.kpck` (REG-015), `testdata/taskbar.kpck` (REG-016) |
| Unicorn | OFF-kernel FASM unpacker vs i686 blob: **238/238** KPCK on the floppy byte-identical |
| Cut CO host tests | focused `upck_*` **13/13 PASS** |
| Full host suite | **764/764 PASS** |
| ABI smoke | **N/A in-kernel** (REG-012 headroom); host `upck_*` + trampoline contract |

---

## QEMU validation

| Config | Gate | Image | non-black | resets | Result |
|--------|------|-------|-----------|--------|--------|
| OFF | FASM unpack + LUT move | `kernel-20260813-111308.img` | 779380 | 0 | PASS |
| ON | `USE_RUST_UNPACK=1` | `kernel-20260813-121344.img` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 / 779380 / 779380 | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260813-121344.img`

A RESET is FAIL. Non-black count is supplementary; A/B pixel match is required. Splash-class ~8k non-black is **not** desktop success.

---

## Subsystem soak

**Kernel-live.** `load_file` / `load_PE` unpack boot KPCK: `DRIVERS/PS2MOUSE.SYS`, `/sys/LAUNCHER`, then `@TASKBAR` / `@ICON` / `DEFAULT.SKN` (and further apps). Desktop 779380 A/B is the production soak — not export-only.

---

## Regressions

| ID | Title |
|----|--------|
| REG-015 | LZMA match-literal used `probs[256+symbol]` instead of FASM `CH=match_bit` planes 256/512 |
| REG-016 | E8/Jcc rel32 used Intel `bswap`; FASM uses `shr ax,8`/`ror 16`/`xchg al,ah`. Also flags were 32-bit-widened vs FASM **AL**. |

---

## Rollback

```text
USE_RUST_UNPACK = 0
```

in `kernel/unpacker.inc` (or `enabled = false` for Cut CO in `project/build.toml` then rebuild). Legacy FASM body retained under `else`. LUT move after `sys_pgmap` is independent of the gate (needed so either body can assemble under the REG-012 pack); leave it in place unless reverting the whole cut’s memory work.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/unpack.rs` | production decoder + oracle + `upck_*` |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_unpack` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | `mod unpack` |
| `rust_kernel/kolibri_utils/testdata/launcher.kpck` | REG-015 fixture |
| `rust_kernel/kolibri_utils/testdata/taskbar.kpck` | REG-016 fixture |
| `kernel/unpacker.inc` | gate + trampoline + `file` embed |
| `kernel/data32.inc` | pitch LUTs after `sys_pgmap` |
| `kernel/kernel.asm` | omit in-kernel smoke (REG-012) |
| `project/build.toml` | blob + Cut CO migration `enabled = true` |
