# Cut BN Implementation — `xfs._.conv_time_to_kos_epoch`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bn-plan.md`](cut-bn-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs._.conv_time_to_kos_epoch` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 3 live (`xfs_get_inode_info`: ctime / atime / mtime) |
| Rust symbol | `rust_xfs_conv_time_to_kos_epoch` |
| Pure helper | `kolibri_utils::xfs_conv_time_to_kos_epoch` |
| Subsystem | XFS classic inode timestamp -> BDFE |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

Selected `xfs._.conv_time_to_kos_epoch` as the strongest post-BM Path B leaf:
it is live, deterministic, low-side-effect, and sits on a proven composition
boundary (`movbe` high dword -> `fsTime2bdfe`) without claiming XFS ownership.

---

## Candidate comparison (post-BM audit)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `xfs._.conv_time_to_kos_epoch` | XFS classic seconds DQ -> BDFE | 3 | `--disk xfs` | Low | XFS time deepen | **SELECT** |
| 2 | `fsGetTime` | CMOS -> stacked BDFE | 6+ | Desktop only | Med | Calendar caution | Defer |
| 3 | `tcp_mss` | MSS clamp | 1 | Partial net | Low | TCP deepen | Defer |
| 4 | `blit_clip` | dual `block_clip` compose | 1 | Desktop only | Med | GUI glue | Defer |
| 5 | `get_proc_ex` | PE import lookup | 1 | Desktop only | Med | PE deepen | Defer |

---

## Legacy ABI

```text
call [ebp+XFS.conv_time_to_kos_epoch]
in:  ECX -> on-disk DQ
       [ECX+0] hi_be = seconds since 2001-01-01
       [ECX+4] lo_be ignored
     EDI -> 8-byte BDFE output
out: BDFE written at [EDI], then EDI += 8
clobbers: EAX/EBX/ECX/EDX, flags
preserves: ESI, EBP
stack: plain ret
DF: unchanged
```

Behavior notes:

* The legacy leaf is exactly `movbe eax, [ecx+DQ.hi_be] ; call fsTime2bdfe ; ret`.
* The low dword is not consulted.
* All calendar semantics, pad-at-`+3`, and `EDI += 8` behavior are inherited from
  `fsTime2bdfe`.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_xfs_conv_time_to_kos_epoch` |
| Blob | 434 bytes, **0 relocations** |
| SHA-256 | `A9E4C6FAA070D97A235523D99754EF2D1CC781D632603361DCD32F51C96057C0` |
| Trampoline | `kernel/fs/xfs.asm` under `USE_RUST_XFS_CONV_TIME_TO_KOS_EPOCH` |
| Gate | `USE_RUST_XFS_CONV_TIME_TO_KOS_EPOCH` (prod 1) |
| Rust ABI | `stdcall rust_xfs_conv_time_to_kos_epoch(secs, out); ret 8` |
| Public ABI map | Trampoline `movbe`-loads `secs`, calls Rust, then `add edi, 8` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow composition oracle (`fasm_oracle_fs_time2bdfe`) |
| Host tests | **PASS** — named vectors, boundary vectors, pointer-layout checks, 50k PRNG |
| Seed | `0x4355544E` (`'CUTN'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_conv_time_to_kos_epoch_rust_smoke_test` | **PASS** |
| Marker | `rust_xfs_conv_time_to_kos_epoch_smoke_result = 'XFCT'` |
| Coverage | direct `rust_*`, public trampoline, `EDI += 8`, low-dword ignored, `ESI`/`EBP` preservation |
| Live state | synthetic DQ fixtures only |

Note: one initial leap-day fixture typo in the smoke data caused an intentional
boot-time hang during validation. The fixture was corrected before closure; no
production regression log entry was needed.

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` in `build.toml` | **OK** (`running`, 779380 non-black) | `dev_build/bn-off.ppm` |
| ON | `USE_RUST_XFS_CONV_TIME_TO_KOS_EPOCH=1` | **OK** (`running`, 779380 non-black) | `dev_build/bn-on.ppm` |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 15`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |
| OFF vs ON desktop raw PPM bytes | 10 bytes differ |
| ON rerun self-diff | 88 bytes differ |

Interpretation: the 10-byte OFF vs ON delta is smaller than same-image rerun jitter,
so the desktop result is treated as stable A/B equivalence at this granularity.

---

## Real subsystem soak

| Path | Result |
|------|--------|
| `--disk xfs` attach-only A/B smoke | **PASS** — OFF and ON both `running`, both 779380 non-black |
| Scripted browse / inode-walk soak | **NOT AVAILABLE** |

Precision note: this cut uses XFS inode time conversion, but no scripted Eolite
browse / metadata walk harness exists yet, so the real-soak claim is limited to
project `--disk xfs` attach-only A/B boot evidence.

---

## Regressions

| Item | Result |
|------|--------|
| Live regressions discovered | **none** |
| Regression log entry | none |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_XFS_CONV_TIME_TO_KOS_EPOCH = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/test/kernel-20260812-094844.img` |
| Rollback | `USE_RUST_XFS_CONV_TIME_TO_KOS_EPOCH = 0` or `[[rust.migrations]]` `cut = "BN"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `kernel/rust/xfs_conv_time_to_kos_epoch.inc`
* `kernel/fs/xfs.asm`
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/cut-bn-plan.md`
* `docs/migration/cut-bn-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/migration-todo.md`
* `docs/migration/boundaries.md`

---

## Inventory

**69 / 135** — one new `[x]` (`xfs._.conv_time_to_kos_epoch`).
