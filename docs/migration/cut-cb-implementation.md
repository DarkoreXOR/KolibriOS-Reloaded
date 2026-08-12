# Cut CB Implementation — `ahci_port_wait`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-cb-plan.md`](cut-cb-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CB** |
| FASM symbol | `ahci_port_wait` |
| Source | [`kernel/blkdev/ahci.inc`](../../kernel/blkdev/ahci.inc) |
| Callers | 2× `stdcall ahci_port_wait` (identify + sector I/O prep) |
| Rust symbol | `rust_ahci_port_wait` |
| Pure helper | `kolibri_utils::ahci_port_wait` |
| Subsystem | AHCI port TFD busy/DRQ poll |
| Stage | Stage 5 AHCI driver foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — AHCI cluster has no Rust-owned controller; AV/BM/CB are Path B leaves.

Selected `ahci_port_wait` over `ntfs_restore_usa_frs`, `tcp_mss`, and `exFAT_find_lfn` for distinct poll semantics + mockable oracle + live callers.

---

## Legacy ABI

```text
stdcall ahci_port_wait(port, timeout) → EAX; ret 8
in:  port → HBA_PORT*, timeout = timer_ticks offset
out: EAX 0 = TFD clear; EAX 1 = timeout
mask: (task_file_data & (ATA_DEV_BUSY|ATA_DEV_DRQ)) == 0 → success
deadline: timer_ticks_start + timeout (unsigned)
loop: while timer_ticks < deadline (unsigned jb)
preserves: EBX, ECX
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_ahci_port_wait` |
| Blob | **61** bytes, **0 relocations** |
| SHA-256 | `47dfd110b40175f4c220228b459f72c45a85be5297484deff8cc7a597adc3fa9` |
| Callbacks | `ahci_port_wait_read_tfd`, `ahci_port_wait_read_ticks` |
| Trampoline | save GPRs / `stdcall rust_*` / restore GPRs |
| Gate | `USE_RUST_AHCI_PORT_WAIT` (prod 1) |
| Rust ABI | `stdcall rust_ahci_port_wait(read_tfd, read_ticks, port, timeout); ret 16` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow poll mirror (`fasm_oracle_ahci_port_wait`) |
| PRNG seed | `0x43555457` (`'CUTW'`) |
| PRNG cases | 50,000 |
| Cut CB host tests | **7/7 PASS** |
| Full host suite | **646/646 PASS** (after isolated target-dir run) |
| ABI smoke | **PASS** — marker `'WAIT'` |

---

## QEMU validation

| Config | Gate | non-black | Result |
|--------|------|-----------|--------|
| OFF | `USE_RUST_AHCI_PORT_WAIT=0` | 779380 | PASS |
| ON | `USE_RUST_AHCI_PORT_WAIT=1` | 779380 | PASS |
| A/B | match | 779380 = 779380 | PASS |
| ON ×3 consecutive | 1 | 779380 each | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Image: `dev_build/test/kernel-20260812-141218.img`

---

## Subsystem soak

Desktop attach-only PASS (AHCI init + command-prep paths via boot). No dedicated `--bus ahci` regression script; smoke runs after `ahci_init` alongside Cut AV/BM/BG smokes.

---

## Regressions

**REG-006** (fixed): Cut CB smoke hung boot at init logging screen — mock callbacks
used `ret` instead of `ret 4` (stack corruption); public timeout vector used
`timeout=1` with live `timer_ticks` before multitasking enable (infinite poll).
See [`regression-log.md`](regression-log.md).

---

## Production integration

| Item | Value |
|------|-------|
| Gate | `USE_RUST_AHCI_PORT_WAIT = 1` |
| Registry | `project/build.toml` `[[rust.migrations]]` cut CB |
| Inventory | **83 / 135** |
| Rollback | set gate 0 or `enabled = false` in build.toml |

---

## Files changed

- `rust_kernel/kolibri_utils/src/ahci_port_wait.rs` (new)
- `rust_kernel/kolibri_utils/src/ffi.rs`
- `rust_kernel/kolibri_utils/src/lib.rs`
- `kernel/rust/ahci_port_wait.inc` (new)
- `kernel/blkdev/ahci.inc`
- `kernel/kernel32.inc`
- `kernel/kernel.asm`
- `project/build.toml`
- `docs/migration/migration-todo.md`
- `docs/migration/migration-plan.md`
- `docs/migration/boundaries.md`
- `docs/migration/cut-cb-plan.md`
- `docs/migration/cut-cb-implementation.md`

---

**COMPLETE — STOP**

Inventory: **83 / 135**
