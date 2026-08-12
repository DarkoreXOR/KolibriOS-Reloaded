# Cut BV Implementation — `fsGetTime`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bv-plan.md`](cut-bv-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BV** |
| FASM symbol | `fsGetTime` |
| Source | [`kernel/fs/fs_common.inc`](../../kernel/fs/fs_common.inc) |
| Callers | 9× `call fsGetTime` (NTFS×4, EXT×5; plus Cut BT smoke compose) |
| Rust symbol | `rust_fs_get_time` |
| Pure helper | `kolibri_utils::fs_get_time` |
| Composes | Cut G `fs_calculate_time` (inlined) |
| Subsystem | FS calendar / CMOS RTC query |
| Stage | Stage 3 calendar foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — CMOS/calendar cluster has no Rust-owned RTC subsystem;
port I/O remains FASM.

Selected `fsGetTime` over `fsReadCMOS`, `tcp_mss`, and `ahci_port_wait` for
CMOS-orchestration semantic class + live caller fanout + strong mock-CMOS oracle
after fresh post-BU audit.

---

## Legacy ABI

```text
regcall fsGetTime()
  mov al,7/8/9 + fsReadCMOS ×3 → ror/add 2000/ror 16 (date dword)
  mov al,0/2/4 + fsReadCMOS ×3 → ror/ror 16 (time dword)
  push date; push time; mov esi,esp; add esp,8
  fallthrough fsCalculateTime(esi) → EAX
out: EAX = KOS seconds since 2001-01-01
```

Quirks retained:

* Upper 16 bits of EAX preserved across `fsReadCMOS` (AX-only clobber)
* Two `ror eax,8` between three CMOS reads (not three)
* `add eax, 2000` on full date dword after year-2-digit read
* BDFE stack layout: time dword @+0, date dword @+4
* Cut G calendar via inlined `fs_calculate_time`

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_fs_get_time` |
| Blob | **502** bytes, **0 relocations** |
| SHA-256 | `790fa9a29875faa52f7131275c7802abdb556b3c789d5966ca4ae251655b96fd` |
| Callback wrapper | `fs_read_cmos_stdcall` (AL reg → decoded AX in EAX) |
| Trampoline | `stdcall rust_fs_get_time, fs_read_cmos_stdcall` |
| Gate | `USE_RUST_FS_GET_TIME` (prod 1) |
| Rust ABI | `stdcall rust_fs_get_time(read_cmos); ret 4` → EAX |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent BCD decode + explicit `ror`/stack BDFE pack + `fasm_oracle_fs_calculate_time` |
| PRNG seed | `0x43554256` (`'CUBV'`) |
| PRNG cases | 50,000 |
| Host tests | **PASS** — `623/623` (includes 8 Cut BV tests + 50k PRNG) |
| ABI smoke | **PASS** — marker `'FSGT'` |

---

## QEMU regression

| Config | Gate | Result | Non-black |
|--------|------|--------|-----------|
| OFF | `enabled = false` | **OK** (`running`) | 779380 |
| ON | `USE_RUST_FS_GET_TIME=1` | **OK** (`running`) | 779380 |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |

---

## Real subsystem soak

| Harness | Result |
|---------|--------|
| Desktop CMOS boot | **PARTIAL** — attach-only via live RTC + Cut BT `ntfsGetTime` compose |
| `--disk ntfs` dedicated harness | **NOT AVAILABLE** |
| `--disk ext` | **NOT AVAILABLE** |

---

## Regression status

| Item | Status |
|------|--------|
| Regressions discovered | **NONE** |
| regression-log entry | Not required |

---

## Production

| Item | Value |
|------|-------|
| Gate | `USE_RUST_FS_GET_TIME = 1` |
| Rollback | `USE_RUST_FS_GET_TIME = 0` and/or `enabled = false` in `build.toml` |
| Final image | `dev_build/test/kernel-20260812-124359.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/fs_get_time.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/fs/fs_common.inc`
* `kernel/rust/fs_get_time.inc` (new)
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/cut-bv-plan.md` (new)
* `docs/migration/cut-bv-implementation.md` (new)

---

## Known limitations

* `fsReadCMOS` port I/O remains FASM (injected callback only).
* No dedicated `--disk ext` / `--disk ntfs` CMOS metadata harness.
* Smoke vector 2 uses live RTC sanity (non-negative EAX), not fixed timestamp.

---

## Inventory

**77 / 135**
