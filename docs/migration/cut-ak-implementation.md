# Cut AK Implementation — `xfs._.conv_bigtime_to_kos_epoch`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ak-plan.md`](cut-ak-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs._.conv_bigtime_to_kos_epoch` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 3 indirect (`xfs_get_inode_info` ctime/atime/mtime via v5 fn ptr) |
| Rust symbol | `rust_xfs_bigtime_to_secs` |
| Pure helper | `kolibri_utils::xfs_bigtime_to_secs` |
| Composes | Cut T `fsTime2bdfe` (FASM call after Rust secs) |
| Subsystem | XFS / v5 bigtime → BDFE |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. AH+AI helper reuse still ≠ Rust-owned exFAT subsystem.
ISO compare pair is pattern reuse; live `cd_compare_name` already trampolines
to the AJ Rust leaf when `USE_RUST_ISO9660_COMPARE_NAME=1`. Socket membership
blockers unchanged. Pairing bigtime with thin v4 `conv_time_to_kos_epoch` is an
anti-cluster. See [`cut-ak-plan.md`](cut-ak-plan.md).

---

## Candidate comparison (post-AJ audit)

| Candidate | Outcome |
|-----------|---------|
| `xfs._.conv_bigtime_to_kos_epoch` | **Selected** — XFS v5 ns epoch math; compose T; 3 indirect callers |
| `fat_name_is_legal` | #2 — real validate leaf; novelty diluted after Cut U |
| `socket_check` | Deferred — FASM mutex/list ownership unchanged |
| `cd_compare_name` | Not a cut — already AJ-routed when gate ON |
| FAT datetime / thin hashes / `createMcbEntry` / `memmove` | Reject / defer |

---

## Legacy ABI

FASM leaf in `xfs.asm` (retained under `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH=0`):

```text
call / ret
in:  ECX → DQ bigtime (hi_be @+0, lo_be @+4); EDI → BDFE out
out: EDI = EDI+8; 8-byte BDFE written
preserves: ESI, EBP
clobbers: EAX, EBX, ECX, EDX (via fsTime2bdfe)
```

Critical quirks retained:

* `movbe` loads BE dwords into native EDX:EAX
* Bias `sub`/`sbb` with `BIGTIME_TO_KOS_OFFSET_NS` (`0x2B610A3711350000`)
* Pre-KOS-epoch underflow → clamp `{edx,eax}=0`
* Post-bias `edx >= 1e9` → clamp `{edx,eax}={999999999,0xFFFFFFFF}`
* `div 1e9` remainder discarded; calendar via `fsTime2bdfe`

---

## Rust ABI

```text
stdcall rust_xfs_bigtime_to_secs(bt_lo, bt_hi) -> EAX secs ; ret 8
```

Trampoline: `movbe` from `[ecx+DQ.*]`, `stdcall …, eax, edx`, then
`call fsTime2bdfe` (Cut T) for BDFE write + `EDI+=8`. Calendar stays in the
proven Cut T path so the omit-FP XFS call chain does not host a large inlined
calendar blob.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `time.rs` + `ffi.rs` section `.text.rust_xfs_bigtime_to_secs` |
| Extract | `extract_reloc_free_text.py` → `rust_xfs_bigtime_to_secs.bin` |
| Embed | `kernel/rust/xfs_conv_bigtime_to_kos_epoch.inc` `file` directive |
| Trampoline | `xfs.asm` under `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH` |
| Gate | `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH` (dev 0 → prod 1) |
| Smoke | `xfs_conv_bigtime_to_kos_epoch_rust_smoke_test` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_xfs_bigtime_to_secs` |
| Blob/object size | (see `out/rust_xfs_bigtime_to_secs.bin`) |
| Relocations | 0 |

Trailing instruction is `ret 12` (`C2 0C 00`). Size includes inlined Cut T calendar (month tables stack-materialized).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | epoch bias; +1s; leap 2004-02-29; 2010-07-04 noon; end-of-day; pre-epoch clamp; subsec discard; high-edx clamp; just-below-clamp |
| PRNG | 50 000 vectors, seed `0x4355544B` (`'CUTK'`) |
| Host tests | **377/377** cargo tests (364 AJ baseline + 13 new XFS bigtime) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_conv_bigtime_to_kos_epoch_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | bias epoch; +1s; EOD; zero→clamp; EDI+=8; ESI/EBP canaries |
| Marker | `rust_xfs_conv_bigtime_to_kos_epoch_smoke_result = 'XFBT'` on success |

---

## QEMU validation

Kernels built with Cuts A–AJ production gates intact (`USE_RUST_ISO9660_COMPARE_NAME=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH=0` | **OK** (QMP `running` + screendump `dev_build/cut-ak-off.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH=1` | **OK** (screendump `dev_build/cut-ak-on.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

Real subsystem soak: **NOT AVAILABLE** — no scripted XFS v5 inode-time harness; attaching `images/exfat-image.img` does not evidence XFS bigtime conversion.

Production image: `dev_build/cut-ak-final.img`.

e1000: **N/A**

---

## Production gate

```text
USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH = 1
```

Rollback: `USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH = 0` (or `enabled = false` in `project/build.toml` Cut AK migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs` (XFS bigtime + oracle + tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/out/rust_xfs_conv_bigtime_to_kos_epoch.bin` (generated)
* `kernel/rust/xfs_conv_bigtime_to_kos_epoch.inc` (new)
* `kernel/fs/xfs.asm` (trampoline + gate; legacy body retained)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-ak-plan.md`
* `docs/migration/cut-ak-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Stock-image / attached-media XFS v5 inode-time soak not claimed
* Thin v4 `xfs._.conv_time_to_kos_epoch` remains FASM (intentional anti-cluster)
* `socket_check` / `memmove` / FAT datetime / `createMcbEntry` remain deferred
* No Path A cluster claimed
* Current default `qemu.args` do not include e1000 (desktop regression only)
