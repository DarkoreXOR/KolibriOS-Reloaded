# Cut CA Implementation — `fsReadCMOS`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-ca-plan.md`](cut-ca-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CA** |
| FASM symbol | `fsReadCMOS` |
| Source | [`kernel/fs/fs_common.inc`](../../kernel/fs/fs_common.inc) |
| Callers | 13× `call fsReadCMOS` (fs_common×7 via legacy fsGetTime path + wrapper, fat×6) |
| Rust symbol | `rust_fs_read_cmos` |
| Pure helper | `kolibri_utils::fs_read_cmos` |
| Composes | Cut BV `fs_read_cmos_bcd` (shared BCD decode) |
| Subsystem | FS calendar / CMOS BCD decode leaf |
| Stage | Stage 3 calendar foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — CMOS/calendar cluster has no Rust-owned RTC subsystem;
port I/O remains FASM via `fs_cmos_raw_read_stdcall`.

Selected `fsReadCMOS` over `ntfs_restore_usa_frs`, `ahci_port_wait`, and
`tcp_mss` for CMOS BCD decode semantic class + highest eligible caller fanout
after fresh post-BZ audit.

---

## Legacy ABI

```text
call/ret (not stdcall)
in:  AL = CMOS register index
out: AX = decoded BCD 0–99 (upper EAX bits preserved)
side effect: OUT 0x70 / IN 0x71
preserves: ECX, EDX (REG-001)
```

Quirks retained:

* `in al, 71h` only modifies AL; upper EAX untouched before decode
* BCD unpack: `xor ah,ah` / `shl ax,4` / `shr al,4` / `aad`
* FAT callers rely on upper-EAX preservation across `ror eax,N` sequences

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_fs_read_cmos` |
| Blob | **30** bytes, **0 relocations** |
| SHA-256 | `38d7f1dc0328f9bee6556f4076675a11c0cb98210195276a6d304b92a928be4c` |
| Callback wrapper | `fs_cmos_raw_read_stdcall` (reg → raw byte via port I/O) |
| Trampoline | save EAX / `stdcall rust_fs_read_cmos` / merge upper EAX |
| Gate | `USE_RUST_FS_READ_CMOS` (prod 1) |
| Rust ABI | `stdcall rust_fs_read_cmos(raw_read, reg); ret 8` → EAX (AX) |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow BCD mirror (`fasm_oracle_fs_read_cmos`) |
| PRNG seed | `0x43555443` (`'CUTC'`) |
| PRNG cases | 50,000 (raw byte domain) |
| Host tests | **PASS** — `639/639` |
| ABI smoke | **PASS** — marker `'CMOS'` |

---

## QEMU regression

| Config | Gate | Result | Non-black |
|--------|------|--------|-----------|
| OFF | `USE_RUST_FS_READ_CMOS=0` | **OK** (`running`) | 779380 |
| ON | `USE_RUST_FS_READ_CMOS=1` | **OK** (`running`) | 779380 |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| Desktop OFF vs ON | **MATCH** (779380 non-black both) |
| `--disk exfat` attach | **PASS** (testdisk auto-attached; FAT CMOS timestamp path) |

---

## Regressions

[REG-005](regression-log.md#reg-005--intermittent-desktop-hang-cut-ca-fsreadcmos-ebx-2026-08-12) — Cut CA trampoline used `pop ebx` for EAX merge, clobbering live EBX in `fat_Write` → `get_time_for_file`. Fixed: stack-only merge + full EBX/ESI/EDI/EBP preserve; smoke EBX canary added.

---

## Production integration

| Item | Value |
|------|-------|
| Gate | `USE_RUST_FS_READ_CMOS = 1` |
| Registry | `project/build.toml` `[[rust.migrations]]` cut CA |
| Final image | `dev_build/test/kernel-20260812-133814.img` |
| Image SHA-256 | `c05613468f58ff695938b5a452c8887d0987759480b49f04a58b11590b30dc65` |

### Rollback

Set `USE_RUST_FS_READ_CMOS = 0` in `kernel/fs/fs_common.inc` (or
`enabled = false` for cut CA in `project/build.toml`) and rebuild.

### Known limitations

* Attach-only exFAT soak does not walk full metadata write paths.
* Live CMOS smoke vectors use range checks, not fixed RTC values.

### Files changed

| Path | Change |
|------|--------|
| `rust_kernel/kolibri_utils/src/fs_read_cmos.rs` | new module + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_fs_read_cmos` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | module wiring |
| `kernel/fs/fs_common.inc` | gate + trampoline + port callback |
| `kernel/rust/fs_read_cmos.inc` | blob embed + smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | smoke call |
| `project/build.toml` | blob + migration entry |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | Cut CA entry |
| `docs/migration/boundaries.md` | LOCAL FACT update |

---

## Final inventory

**82 / 135**

**Stop; do not start Cut CB.**
