# Cut AW Implementation — `xfs._.blkrel2sectabs`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-aw-plan.md`](cut-aw-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs._.blkrel2sectabs` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 1 direct (`xfs._.read_blocks`; fan-out via every XFS block read) |
| Rust symbol | `rust_xfs_blkrel2sectabs` |
| Pure helper | `kolibri_utils::xfs_blkrel2sectabs` |
| Subsystem | XFS AG-relative block → absolute sector translation |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AV audit: AHCI wait/IRQ/issue peers do not
transfer controller ownership; AC/M/V/AS/AU network Path A still fails;
I+`createMcbEntry` encode ≠ FRS ownership (high write blast); Y+AT+`rebase_coff`
PE anti-cluster unchanged; XFS R+W+AM+AP+AK+blkrel complementary leaves ≠ Rust-
owned FS. Selected **`xfs._.blkrel2sectabs`** — strongest remaining leaf: new
AG→sector address-math class with excellent differential domain and real
`--disk xfs` soak (every `read_blocks` path).

REG-001: trampoline preserves **EBX+ESI** (legacy `uses esi`; EBX live buffer
across `read_blocks`) plus ECX/EDI; EBP→XFS must not be framed over.

---

## Candidate comparison (post-AV audit)

| Candidate | Outcome |
|-----------|---------|
| `xfs._.blkrel2sectabs` | **Selected** — AG→sector address math |
| `createMcbEntry` | #2 — NTFS MCB encode; high FRS blast |
| `getInodeLocation` | #3 — EXT inode→LBA; no `--disk ext` harness |
| `rebase_coff` | Defer — Y mutate anti-cluster |
| AHCI wait / endian / sig / network Path A | Reject / defer |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_XFS_BLKREL2SECTABS=0`:

```text
register call xfs._.blkrel2sectabs
in:  EDX:EAX = AG-relative block bitfield; EBP → XFS
out: EDX:EAX = absolute sector
uses esi; clobbers ecx; ret 0
```

Quirks retained:

* x86 shift count masked to 5 bits (`agblklog` / `sectpblog` `& 31`)
* `mul agblocks` uses **only EAX** of the shifted AG number (hi discarded)

---

## Rust ABI

```text
stdcall rust_xfs_blkrel2sectabs(
  block_lo, block_hi, agblklog, agblocks,
  mask_lo, mask_hi, sectpblog, out_hi) → EAX = sector_lo
  *out_hi = sector_hi
  ret 32
```

Trampoline: omit-FP; extracts XFS fields from `EBP`; preserves EBX/ECX/ESI/EDI.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `xfs_blkrel2sectabs.rs` + `ffi.rs` section `.text.rust_xfs_blkrel2sectabs` |
| Extract | `extract_reloc_free_text.py` → `rust_xfs_blkrel2sectabs.bin` |
| Embed | `kernel/rust/xfs_blkrel2sectabs.inc` `file` directive |
| Trampoline | `xfs.asm` under `USE_RUST_XFS_BLKREL2SECTABS` |
| Gate | `USE_RUST_XFS_BLKREL2SECTABS` (prod 1) |
| Smoke | `xfs_blkrel2sectabs_rust_smoke_test` (after Cut AP smoke) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_xfs_blkrel2sectabs` |
| Blob/object size | 57 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `EE69B9AFF45C1917EA2BBC9764FC1A66398DCE9BA11C170C08A2738790CC4144` |
| Epilogue | `ret 32` (`c2 20 00`) |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs `xfs_blkrel2sectabs` | **PASS** |
| Named vectors | AG0; AG boundary; sectpblog 0/3; `&31` quirk; mul discards AG hi |
| PRNG | 50 000 vectors, seed `0x43555457` (`'CUTW'`) |
| Host tests | **466/466** cargo tests (incl. xfs_blkrel2sectabs suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_blkrel2sectabs_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C57` hang) |
| Vectors | AG0+shift; AG1+shift; direct `rust_*`; sectpblog=0; EDI canary |
| Marker | `rust_xfs_blkrel2sectabs_smoke_result = 'BL2S'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_XFS_BLKREL2SECTABS=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_XFS_BLKREL2SECTABS=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop (IDE) | 779380 | 779380 | **match** |
| `--disk xfs` | 779380 | 779380 | **match** |

---

## Real subsystem soak

```text
Real subsystem soak: PASS (--disk xfs read path)
```

`python scripts/qmp_desktop_smoke.py --disk xfs`: QMP `running` both OFF and ON
with identical non-black counts (779380). Attaching an XFS volume exercises
`xfs` mount → inode/dir block reads → `xfs._.read_blocks` →
`xfs._.blkrel2sectabs` on every filesystem block I/O. Synthetic ABI smoke also
exercises the public trampoline on a fake `XFS` object (marker `BL2S`).

---

## Regressions

```text
NONE
```

(No live REG-NNN append for this cut.)

---

## Production gate

```text
USE_RUST_XFS_BLKREL2SECTABS = 1
```

Rollback: `USE_RUST_XFS_BLKREL2SECTABS = 0` (or `enabled = false` in
`project/build.toml` for cut AW).

Image: `dev_build/cut-aw-final.img`

---

## Files changed

* `rust_kernel/kolibri_utils/src/xfs_blkrel2sectabs.rs` — pure translate + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_xfs_blkrel2sectabs` section
* `rust_kernel/kolibri_utils/src/lib.rs` — module export
* `rust_kernel/kolibri_utils/out/rust_xfs_blkrel2sectabs.bin` — extracted blob
* `kernel/rust/xfs_blkrel2sectabs.inc` — embed + ABI smoke
* `kernel/fs/xfs.asm` — trampoline + gate + FASM rollback body
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-aw-plan.md` / `cut-aw-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* Address math only — disk I/O / inode / dir orchestration remain FASM
* x86 `&31` shift-count and mul-EAX-only AG# quirks retained (legacy)
* No Path A claim for XFS ownership
* Does not migrate `createMcbEntry` / `getInodeLocation` / `rebase_coff`
