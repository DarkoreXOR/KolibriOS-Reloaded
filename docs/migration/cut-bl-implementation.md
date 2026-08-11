# Cut BL Implementation — `v86_get_lin_addr`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bl-plan.md`](cut-bl-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `v86_get_lin_addr` |
| Source | [`kernel/core/v86.inc`](../../kernel/core/v86.inc) |
| Callers | 15 live (`v86.inc` exception / simulate-int / iret paths) |
| Rust symbol | `rust_v86_get_lin_addr` |
| Pure helper | `kolibri_utils::v86_get_lin_addr` / `v86_get_lin_addr_ptr` |
| Subsystem | Stage-4 V86 linear address translate |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

Post-BK audit: XFS/NTFS/network/AHCI/PE/FAT/string/HID/ISO/Stage-3/paging Path A
still fail the raised bar. PE thin Path B novelty exhausted (Y+AT+BK).

Selected **`v86_get_lin_addr`** — V86 address → linear via `page_tabs[page]`
PTE frame | offset; 15 live callers; clean “destroys nothing” ABI; strong
independent oracle; reloc-free via trampoline-injected `page_tabs`. Preferred
over trivial `ahci_is_sig_known`, XFS/HID/TCP deepen, `blit_clip` (H glue),
and CMOS/EXT leaves without harness.

**New soak evidence:** `scripts/reference_qemu.py` + hybrid CoW `config.ini`
`biosdisks=on` exposes attached IDE images as guest `/bd*` through
`bd_drv` → `v86_start` (exercises the leaf’s live call sites).

REG-001: trampoline preserves **EBX/ECX/EDX/ESI/EDI/EBP**; EAX = linear.
Legacy leaf push/pop ECX+EDX; ESI handle unused but preserved.

REG-002: N/A.

REG-003: ABI smoke uses **synthetic PTE table only** — never writes live
`page_tabs` / V86 machine state. Public preserve canaries use live
`page_tabs` read-only.

---

## Candidate comparison (post-BK audit)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `v86_get_lin_addr` | V86 PTE→linear | 15 | BIOS `/bd*` | Low–Med | Stage-4 translate | **SELECT** |
| 2 | `ahci_is_sig_known` | Signature CMP→ZF | 2 | `--bus ahci` | Low | AV deepen / trivial | Defer |
| 3 | `xfs._.conv_time_to_kos_epoch` | BE32→`fsTime2bdfe` | 3 | `--disk xfs` | Low | XFS time deepen | Defer |
| 4 | `blit_clip` | Dual `block_clip` | 1 | Desktop GUI | Med | H deepen | Defer |
| 5 | `set_mouse_data` | HID aggregator | 0 in-kernel | Desktop mouse | Med–High | HID deepen | Defer |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_V86_GET_LIN_ADDR=0`:

```text
call v86_get_lin_addr
in:  EAX = V86 address; ESI = handle (unused by leaf)
out: EAX = linear address
preserves: all GPRs (push/pop ECX+EDX; others untouched)
clobbers: flags (callers ignore)
plain ret (not stdcall)
```

Quirks retained:

* ESI=handle documented but never read by the leaf
* PTE flags stripped (`and edx, 0xFFFFF000`); page offset kept (`and eax, 0xFFF`)
* Unmapped PTE (`0`) → linear = page offset only

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_v86_get_lin_addr` |
| Blob | 34 bytes, **0 relocations** |
| SHA-256 | `9BAB1DB9…1BC7690A` |
| Trampoline | `kernel/core/v86.inc` under `USE_RUST_V86_GET_LIN_ADDR` |
| Gate | `USE_RUST_V86_GET_LIN_ADDR` (prod 1) |
| Rust ABI | `stdcall rust_v86_get_lin_addr(v86_addr, page_tabs) -> EAX; ret 8` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow (`fasm_oracle_v86_get_lin_addr`) |
| Host tests | **PASS** — 6 tests including 50k PRNG |
| Seed | `0x4355424C` (`'CUBL'`) |
| Coverage | page0 offset0/0xFFF; mid-page flag strip; unmapped PTE; BIOS-ROM style; 50k PRNG in 1 MiB window |

---

## ABI smoke

| Item | Result |
|------|--------|
| `v86_get_lin_addr_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | synthetic PTE edges + preserve canaries; public live `page_tabs` preserve-only |
| Marker | `rust_v86_get_lin_addr_smoke_result = 'VGLA'` on success |
| Live state | Synthetic PTE table for functional asserts; no `page_tabs` writes (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_V86_GET_LIN_ADDR=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_V86_GET_LIN_ADDR=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — 68 differing bytes (clock/timer noise; same non-black count 779380) |
| Desktop boot | **PASS** both OFF and ON |
| Prior image | `dev_build/cut-bk-final.img` retained as baseline |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Desktop boot / early init | **PASS** — ABI smoke + desktop reached (`init_sys_v86` always runs) |
| BIOS disks `/bd*` | **PASS** — hybrid CoW `config.ini` `biosdisks=on` + `--disk exfat --bus ide`; QMP `running`, 779380 non-black. `/bd*` Eolite browse not separately scripted beyond boot/attach. |

---

## Regressions

| Item | Result |
|------|--------|
| Regressions discovered | **none** |
| Regression log entry | N/A (no live regression) |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_V86_GET_LIN_ADDR = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bl-final.img` |
| Rollback | `USE_RUST_V86_GET_LIN_ADDR = 0` or `[[rust.migrations]]` `cut = "BL"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/v86_get_lin_addr.rs` — leaf + differential tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_v86_get_lin_addr`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/rust/v86_get_lin_addr.inc` — blob embed + ABI smoke
* `kernel/core/v86.inc` — trampoline + gate
* `kernel/kernel.asm` — smoke call
* `kernel/kernel32.inc` — include
* `project/build.toml` — blob + migration BL
* `docs/migration/cut-bl-plan.md`
* `docs/migration/cut-bl-implementation.md`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Does not claim Path A for paging / V86 machine / allocator ownership.
* Complements Cut AQ without stretching into `get_phys_addr` / `map_page` / `alloc_page`.
* BIOS-disk soak confirms boot+attach with `biosdisks=on`; no automated `/bd*` directory browse harness.
* Stop; do not start Cut BM.
