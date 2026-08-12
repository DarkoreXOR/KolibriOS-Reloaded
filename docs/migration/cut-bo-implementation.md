# Cut BO Implementation — `xfs._.get_last_dirblock`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bo-plan.md`](cut-bo-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BO** |
| FASM symbol | `xfs._.get_last_dirblock` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 2 live (`xfs_readdir` extents path; `xfs._.get_inode_short`) |
| Rust symbol | `rust_xfs_get_last_dirblock` |
| Pure helper | `kolibri_utils::xfs_get_last_dirblock` |
| Subsystem | XFS directory extents / final data-dirblock selection |
| Stage | Stage-5 FS foothold (complements R/W/AM/AP/AW/AK/BN) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

Selected `xfs._.get_last_dirblock` as the strongest post-BN Path B leaf:
live callers, deterministic arithmetic oracle, low blast radius, and real
`--disk xfs` attach-only soak without claiming XFS ownership.

---

## Candidate comparison (post-BN audit)

| Rank | Candidate | Outcome |
| ---- | --------- | ------- |
| 1 | `xfs._.get_last_dirblock` | **SELECT** — last data dirblock arithmetic |
| 2 | `fix_coff_symbols` | Defer — PE deepen / mutate |
| 3 | `ahci_port_wait` | Defer — controller poll orchestration |
| 4 | `ext_write_time` | Defer — no `--disk ext` |
| 5 | `ext_read_all_times` | Defer — AL compose |
| 6 | `tcp_mss` | Defer — TCP deepen / thin |

---

## Legacy ABI

```text
register call xfs._.get_last_dirblock
in:  EBX -> inode buffer
     EBP -> XFS
out: EDX:EAX = last data directory block
preserves: EBX, ECX
clobbers: EAX, EDX, flags
stack: plain ret
DF: unchanged
```

Quirks retained:

* `nextents` loaded with `movbe` from inode metadata.
* Final record offset uses `nextents << 4` because `sizeof.xfs_bmbt_rec = 16`.
* `dirblklog` shift count masked to 5 bits (`& 31`).
* `dec eax` after the shift preserves zero-block underflow semantics.
* Only `br_startoff` / `br_blockcount` fields are decoded from the final record.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_xfs_get_last_dirblock` |
| Blob | 114 bytes, **0 relocations** |
| SHA-256 | `B6E0C3F22A99933F4A9F8055640FF78CE7BC791541E8B085FC1207FA98EC591C` |
| Trampoline | `kernel/fs/xfs.asm` under `USE_RUST_XFS_GET_LAST_DIRBLOCK` |
| Gate | `USE_RUST_XFS_GET_LAST_DIRBLOCK` (prod 1) |
| Rust ABI | `stdcall rust_xfs_get_last_dirblock(inode, nextents_offset, inode_core_size, dirblklog, out_hi); ret 20` |
| Public ABI map | Trampoline preserves `EBX`/`ECX`, extracts XFS fields from `EBP`, returns `EDX:EAX` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow oracle mirroring `movbe`/record offset/extent decode/`shr`/`dec`/64-bit add |
| Host tests | **PASS** — named vectors, edge cases, pointer-layout checks, 50k PRNG |
| Seed | `0x4355424F` (`'CUBO'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_get_last_dirblock_rust_smoke_test` | **PASS** (after inode0 fixture reset between direct/public vectors) |
| Marker | `rust_xfs_get_last_dirblock_smoke_result = 'XFBO'` |
| Coverage | direct `rust_*`, public trampoline, `EBX`/`ECX`/`ESI`/`EDI`/`EBP` preservation, `EDX:EAX` results |
| Live state | synthetic inode/XFS fixtures only |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` in `build.toml` | **OK** (`running`, 779380 non-black) | `dev_build/bo-off.ppm` |
| ON | `USE_RUST_XFS_GET_LAST_DIRBLOCK=1` | **OK** (`running`, 779380 non-black) | `dev_build/bo-on.ppm` |
| ON rerun | production gate | **OK** (`running`, 779380 non-black) | `dev_build/bo-on-rerun.ppm` |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 20`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |
| OFF vs ON desktop raw PPM bytes | 66 bytes differ |
| OFF vs ON `--disk xfs` raw PPM bytes | 68 bytes differ |

Interpretation: OFF/ON deltas are within rerun-jitter scale on fresh `prepare_image.py`
packaged kernels; desktop and attach-only XFS A/B are treated as stable equivalence.

Note: an initial smoke fixture reused `inode0` after the direct `rust_*` vector
without reset, causing a pre-ship boot hang on gate ON. This was a **test-fixture
contamination** issue (REG-003 class), fixed before closure; not a production-leaf
regression and no `REG-NNN` entry was required.

---

## Real subsystem soak

| Path | Result |
|------|--------|
| `--disk xfs` attach-only A/B smoke | **PASS** — OFF and ON both `running`, both 779380 non-black |
| Scripted browse / inode-walk soak | **NOT AVAILABLE** |

Precision note: this cut sits on the XFS extents-format readdir / short-lookup
path, but no scripted Eolite browse or inode-metadata walk harness exists yet.
The real-soak claim is limited to project `--disk xfs` attach-only A/B boot evidence.

---

## Regressions

| Item | Result |
|------|--------|
| Live regressions discovered | **none** |
| Regression-log entry | none |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_XFS_GET_LAST_DIRBLOCK = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/test/kernel-20260812-101838.img` |
| Rollback | `USE_RUST_XFS_GET_LAST_DIRBLOCK = 0` or `[[rust.migrations]]` `cut = "BO"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/xfs_get_last_dirblock.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `kernel/rust/xfs_get_last_dirblock.inc`
* `kernel/fs/xfs.asm`
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/cut-bo-plan.md`
* `docs/migration/cut-bo-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/migration-todo.md`
* `docs/migration/boundaries.md`

## Known limitations

* Directory block arithmetic only — XFS mount/inode/dir orchestration remain FASM.
* No scripted XFS directory/inode metadata walk harness (attach-only `--disk xfs` soak only).
* ABI smoke must reset synthetic inode fixtures between mutating vectors (direct `rust_*`
  before public-trampoline reuse).

---

## Inventory

**70 / 135** — one new `[x]` (`xfs._.get_last_dirblock`).
