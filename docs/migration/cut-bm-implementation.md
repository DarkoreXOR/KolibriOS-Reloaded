# Cut BM Implementation — `ahci_is_sig_known`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bm-plan.md`](cut-bm-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ahci_is_sig_known` |
| Source | [`kernel/blkdev/ahci.inc`](../../kernel/blkdev/ahci.inc) |
| Callers | 2 live (`ahci_port_init` link-ready + wait-ready paths) |
| Rust symbol | `rust_ahci_is_sig_known` |
| Pure helper | `kolibri_utils::ahci_is_sig_known` |
| Subsystem | AHCI port PxSIG validation |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

Post-BL audit: V86 translate complete (Cut BL). Selected **`ahci_is_sig_known`**
— four-way PxSIG compare with ZF-out contract on AHCI port bring-up; complements
Cut AV (cmdslot) + Cut BG (endian swap) without claiming controller ownership.

REG-001: trampoline preserves **EBX/ECX/EDX/ESI/EDI/EBP**; maps Rust `1/0` to
legacy ZF via `cmp eax, 1`.

REG-002: N/A.

REG-003: ABI smoke uses **synthetic signatures only**.

---

## Candidate comparison (post-BL audit)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `ahci_is_sig_known` | AHCI PxSIG→ZF | 2 | `--bus ahci` | Low | AV/BG AHCI | **SELECT** |
| 2 | `xfs._.conv_time_to_kos_epoch` | BE32→`fsTime2bdfe` | 3 | `--disk xfs` | Low | XFS time deepen | Defer |
| 3 | `blit_clip` | Dual `block_clip` | 1 | Desktop GUI | Med | H deepen | Defer |
| 4 | `fsGetTime` | CMOS→BDFE | 6+ | Partial | Med | Calendar caution | Defer |
| 5 | `tcp_mss` | MSS clamp | 1 | Partial net | Low | TCP deepen | Defer |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_AHCI_IS_SIG_KNOWN=0`:

```text
call ahci_is_sig_known
in:  EAX = HBA_PORT.signature (PxSIG)
out: ZF=1 if known (SATA/ATAPI/SEMB/PM); ZF=0 if unknown
preserves: all GPRs
clobbers: flags only
plain ret (not stdcall)
```

Known signatures (`ahci.inc`):

| Constant | Value |
|----------|-------|
| `SATA_SIG_ATA` | `0x00000101` |
| `SATA_SIG_ATAPI` | `0xEB140101` |
| `SATA_SIG_SEMB` | `0xC33C0101` |
| `SATA_SIG_PM` | `0x96690101` |

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_ahci_is_sig_known` |
| Blob | 57 bytes, **0 relocations** |
| SHA-256 | `9511D73E…CD58AD7` |
| Trampoline | `kernel/blkdev/ahci.inc` under `USE_RUST_AHCI_IS_SIG_KNOWN` |
| Gate | `USE_RUST_AHCI_IS_SIG_KNOWN` (prod 1) |
| Rust ABI | `stdcall rust_ahci_is_sig_known(sig) -> EAX (1/0); ret 4` |
| ZF map | Trampoline `cmp eax, 1` before `ret` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent four-`cmp`/`je` chain oracle |
| Host tests | **PASS** — 3 tests including 50k PRNG |
| Seed | `0x4355544D` (`'CUTM'`) |
| Coverage | four known sigs; near-miss; zero/0xFFFFFFFF; 50k PRNG |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ahci_is_sig_known_rust_smoke_test` | **PASS** (boot reached QMP `running`) |
| Vectors | direct rust_* 1/0; public trampoline jz/jnz; GPR canaries |
| Marker | `rust_ahci_is_sig_known_smoke_result = 'ASIG'` on success |
| Live state | Synthetic signatures only (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` in `build.toml` | **OK** (QMP `running`, 7358 non-black) | FASM body — see REG-004 |
| ON | `USE_RUST_AHCI_IS_SIG_KNOWN=1` | **OK** (QMP `running`, 7358 non-black) | Production gate — see REG-004 |

Tooling: `python scripts/qmp_desktop_smoke.py --bus ahci --wait 15`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — **0** differing bytes (identical PPM; see REG-004 for pixel-count interpretation) |
| Desktop boot | **PASS** both OFF and ON (A/B equality valid; init-screen was reached, desktop interpretation corrected by REG-004) |
| Prior image | `dev_build/cut-bl-final.img` retained as baseline |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| `--bus ahci` | **PASS** — QMP `running`; exercises `ahci_port_init` signature checks on attached exFAT test disk (`images/exfat-image.img` → guest `/sd0/1`). Browse not separately scripted. |
| Desktop-only substitute | **NOT USED** — AHCI bus harness is the applicable soak. |

---

## Regressions

| Item | Result |
|------|--------|
| Regressions discovered | **none** in this cut |
| Regression log entry | [REG-004](regression-log.md#reg-004) — AHCI init-screen hang root-caused to Cut AR smoke (not this cut); `7358` pixel counts re-interpreted as init-screen, not desktop |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_AHCI_IS_SIG_KNOWN = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bm-final.img` |
| Rollback | `USE_RUST_AHCI_IS_SIG_KNOWN = 0` or `[[rust.migrations]]` `cut = "BM"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/ahci_is_sig_known.rs` — leaf + differential tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_ahci_is_sig_known`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/rust/ahci_is_sig_known.inc` — blob embed + ABI smoke
* `kernel/blkdev/ahci.inc` — trampoline + gate
* `kernel/kernel.asm` — smoke call after `ahci_init`
* `kernel/kernel32.inc` — include
* `project/build.toml` — blob + migration BM
* `docs/migration/cut-bm-plan.md`
* `docs/migration/cut-bm-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/migration-todo.md`
* `docs/migration/boundaries.md`

---

## Inventory

**68 / 135** — one new `[x]` (`ahci_is_sig_known`).
