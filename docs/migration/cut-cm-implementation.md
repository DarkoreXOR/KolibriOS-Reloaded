# Cut CM Implementation — `getInodeLocation`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-cm-plan.md`](cut-cm-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CM** |
| FASM symbol | `getInodeLocation` |
| Source | [`kernel/fs/ext.inc`](../../kernel/fs/ext.inc) |
| Callers | 2 live (`readInode`, `writeInode`) |
| Rust symbol | `rust_get_inode_location` |
| Pure helper | `kolibri_utils::get_inode_location` / `get_inode_location_ptr` |
| Subsystem | EXT inode number → absolute sector + in-sector offset (Stage-2 FS util) |
| Stage | Stage 2 / FS address-math leaf |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — no remaining pending symbol establishes Rust-owned subsystem state (PE/USB/video/exFAT-plugin/IRQ/string-export clusters fail ownership or substance bars).

Selected `getInodeLocation` over `unpack` (Stage-2 memory-blocked / ~32 KiB LZMA state), `blit_32` (LFB blast), `exFAT_find_lfn` (plugin island), `drawChar` (Stage 7), and `enable_irq`/`irq_eoi`/`mutex_init` (oracle/thin) after fresh post-CL audit. Prior “no `--disk ext`” deferral treated as a **tooling gap**: Docker+e2fsprogs creates Kolibri-compatible plain EXT2 (SB@1024); `scripts/mkfs.py ext` + `qmp_desktop_smoke.py --disk ext` close the soak.

Memory: Stage-2 headroom after CL was only **0x80** to `sys_proc` @ `0x8E000`. An earlier attempt raised `SLOT_BASE` to `0x91000` so the full smoke would fit — that **overlapped `VGABasePtr`** and caused **REG-012** (no taskbar / apps won't launch).

**Final (REG-012) pack — do not move `SLOT_BASE`:**

| Symbol | Value |
|--------|-------|
| `TMP_STACK_TOP` | **`0x008E000`** (raised from CL `0x008DF80`) |
| `sys_proc` | **`OS_BASE+0x008E000`** (unchanged from pre-CM pack) |
| `SLOT_BASE` | **`OS_BASE+0x0090000`** (must end at `VGABasePtr`) |

Fit strategy: compact ABI smoke (one direct `rust_*` vector; `DEAD0C6D` via iglobal default). End `.bss` @ `OS_BASE+0x8CFC3` → assert `0x8DFC3 < 0x8E000` (**PASS**). Note `data32.inc` `align 16` after `endofcode` — small code deltas can jump end-`.bss` by a full 16 B cliff.

---

## Legacy ABI

```text
getInodeLocation  (register ABI; not stdcall; omit-FP)
  in:  EAX = inode number (1-based); EBP → EXTFS*
  out: EDX:EAX = partition-relative inode sector
       ECX = byte offset within that 512-byte sector (0..511)
  body: load_bgd_64 + 32-bit imul/mul/adc wrap; DF unchanged
  clobbers: EAX, EBX, ECX, EDX, EFLAGS
  preserves: ESI, EDI, EBP
  stack: balanced; ret 0
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_get_inode_location` |
| Blob | **101** bytes, **0 relocations** |
| SHA-256 | `8dd04514d23f7448e300dfa833c33e6f2139683be8f7b80c515f741bd30a3b2a` |
| Epilogue | `ret 32` (`c2 20 00`) |
| Trampoline | omit-FP; preserve ESI/EDI; BGD load in FASM; inject scalars; `stdcall rust_*`; restore; never `add esp` for Rust args (REG-009); EBX left clobbered |
| Gate | `USE_RUST_GET_INODE_LOCATION` (prod 1) |
| Rust ABI | `stdcall rust_get_inode_location(inode, ipg, table_lo, table_hi, spb, inode_size, out_hi*, out_ofs*); ret 32` → EAX=`sector_lo`; writes `*out_hi` / `*out_ofs` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent wrapping math after BGD: `(table*{spb}) + ((inode-1)%ipg)*inode_size` → sector + ofs |
| PRNG seed | `0x47494C4F` (`'GILO'`) |
| PRNG cases | 50,000 |
| Cut CM host tests | focused `gilo_*` **11/11 PASS** |
| Full host suite | **745/745 PASS** |
| ABI smoke | **PASS** — marker `'GILO'` (stamp-only after REG-013 size fit; host `gilo_*` + EXT Eolite browse cover ABI); hang=`DEAD0C6D` unused |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_GET_INODE_LOCATION=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_GET_INODE_LOCATION=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-210330.img`

REG-012 note: a prior CM image with `SLOT_BASE=0x91000` reported non-black=750708 (desktop without taskbar). After reverting the slot pack, counts match Cut CL again.

---

## Subsystem soak

`--disk ext` attach soak: `python scripts/qmp_desktop_smoke.py --wait 90 --disk ext`  
Result: `query-status: running`, non-black=779380, resets=0. Image: `images/ext-image.img` (64M plain EXT2, SB@1024; `python scripts/mkfs.py ext`). Exercises EXT mount → `readInode` → `getInodeLocation`. Primary correctness evidence remains the independent LBA oracle + ABI smoke + A/B desktop.

---

## Regressions

**REG-012** (no taskbar / apps won't launch): caused by moving `SLOT_BASE` into VGA; fixed by restoring `SLOT_BASE=0x90000` / `sys_proc=0x8E000` and compacting CM smoke. See [`regression-log.md`](regression-log.md).

**REG-013** (EXT Eolite empty names / 0 B): not Cut CM — latent Cut A unicode EDX clobber in `ext_ReadFolder` (REG-001 class), exposed by first `--disk ext` browse. Fixed with `push`/`pop` EDX around unicode name encode. See [`regression-log.md`](regression-log.md).

Applied prior lessons: FASM register-ABI outer / Rust `stdcall` only (no double cleanup); REG-001/011 preserve ESI/EDI/EBP; omit-FP keeps EBP→EXTFS; REG-003 synthetic only; hang marker `DEAD0C6D` via iglobal default.

---

## Rollback

```text
USE_RUST_GET_INODE_LOCATION = 0
```

in `kernel/fs/ext.inc` (or `enabled = false` for Cut CM in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/get_inode_location.rs` | Pure helper + oracle tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_get_inode_location` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | module export |
| `kernel/fs/ext.inc` | gate + trampoline + retained FASM |
| `kernel/rust/get_inode_location.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` / `kernel/kernel.asm` | include + smoke hook; `SLOT_BASE`-relative clear |
| `kernel/const.inc` | `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` pack |
| `project/build.toml` | blob + migration CM |
| `tools/mkfs_utils/create_ext_image.py` | EXT2 image creator |
| `tools/mkfs_utils/docker_populate_ext.sh` | Docker e2fsprogs helper |
| `scripts/mkfs.py` | `ext` target |
| `docs/migration/cut-cm-*.md` | plan + impl |
| `docs/migration/migration-todo.md` / `migration-plan.md` | inventory |
| `docs/compatibility/fixed-addresses.md` | TMP / sys_proc / SLOT_BASE |
| `docs/architecture/memory-model.md` | same |

---

## Inventory after Cut CM

`94 / 135` completed; `41` pending; `94 / 94` production gates enabled.
