# Cut BS Implementation — `ext_write_time`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bs-plan.md`](cut-bs-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BS** |
| FASM symbol | `ext_write_time` |
| Source | [`kernel/fs/ext.inc`](../../kernel/fs/ext.inc) |
| Callers | 5× `stdcall ext_write_time` (`writeInode`, create/delete paths) |
| Rust symbol | `rust_ext_write_time_pack` |
| Pure helper | `kolibri_utils::ext_write_time_pack_ptr` |
| Composes | inverse pack of Cut AL `ext_unix_to_secs` epoch math |
| Subsystem | EXT / write-time inode field pack |
| Stage | Stage 5 FS plugin foothold (EXT write leaf) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — EXT AL/BR/BS read+pack leaves do not establish Rust-owned EXT
mount/write subsystem.

Selected `ext_write_time` over `fix_coff_symbols`, `fsGetTime`, and `ext_SetFileInfo`
for write-path semantic class + five live callers + deterministic pack oracle after
`fsGetTime` split.

---

## Legacy ABI

```text
stdcall ext_write_time(time_ptr, extra_time_ptr)
  call fsGetTime → EAX = KOS secs since 2001-01-01
  xor EDX,0; add UNIXTIME_TO_KOS_OFFSET; adc EDX,0
  test EAX / jns / inc EDX
  [time_ptr] = EAX
  if extra_time_ptr != -1: [extra_ptr] = EDX & 3
preserves: EBX, ESI, EDI (proc `uses`)
clobbers: EAX, ECX, EDX
stack: stdcall ret 8
```

Quirks retained:

* `UNIXTIME_TO_KOS_OFFSET` = `978307200`
* `adc EDX,0` after 32-bit add (extra epoch high bits)
* signed-negative `inc EDX` ext4 sign-extension on write
* `extra_time_ptr == -1` skips extra write
* `and EDX, 3` before extra store

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_ext_write_time_pack` |
| Blob | **48** bytes, **0 relocations** |
| SHA-256 | `c8b2d7ac9cc133f987a4ab4ee7c30efee48ff80cd8539f4a550cf93fe1ea3d6f` |
| Trampoline | `call fsGetTime` → `stdcall rust_ext_write_time_pack, eax, [time_ptr], [extra_time_ptr]` |
| Gate | `USE_RUST_EXT_WRITE_TIME` (prod 1) |
| Rust ABI | `stdcall rust_ext_write_time_pack(kos_secs, time_ptr, extra_time_ptr); ret 12` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow `add/adc/test/jns/inc` mirror (literal offset constant) |
| Host tests | **PASS** — `600/600` (includes 7 Cut BS tests + 50k PRNG) |
| Seed | `0x43554253` (`'CUBS'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ext_write_time_rust_smoke_test` | **PASS** |
| Marker | `rust_ext_write_time_smoke_result = 'EXBS'` |
| Coverage | direct Rust kos=0/1 + extra sentinel + public `ext_write_time` trampoline; ESI/EDI/EBP canaries |
| Live state | isolated synthetic `iglobal` field buffers only (REG-003 safe) |

Initial smoke drafts failed when combining a standalone `fsGetTime` oracle vector with
a subsequent public `ext_write_time` call (double-CMOS / ordering interaction). Final
smoke uses deterministic direct-Rust vectors plus a single public-trampoline vector.

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` | **OK** (`running`, 779380 non-black) | A/B capture |
| ON | `USE_RUST_EXT_WRITE_TIME=1` | **OK** (`running`, 779380 non-black) | A/B capture |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |
| attach-only exFAT secondary disk (default testdisk) | **PASS** — implicit via standard QEMU boot path |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| `--disk ext` EXT write-time / `writeInode` path | **NOT AVAILABLE** |
| attach-only exFAT A/B | **PASS** (desktop equivalence) |

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
| Production gate | `USE_RUST_EXT_WRITE_TIME = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/test/kernel-20260812-113614.img` |
| Rollback | `USE_RUST_EXT_WRITE_TIME = 0` or Cut BS `enabled = false` in `project/build.toml` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/ext_write_time.rs` — new
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `kernel/rust/ext_write_time.inc` — new
* `kernel/fs/ext.inc`
* `kernel/kernel32.inc`
* `kernel/kernel.asm`
* `project/build.toml`
* `docs/migration/cut-bs-plan.md` — new
* `docs/migration/cut-bs-implementation.md` — new
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* CMOS read remains FASM (`fsGetTime`); only the deterministic pack tail is Rust.
* No `--disk ext` harness for live `writeInode` / create-delete caller soak.
* Round-trip with Cut AL is lossless only outside AL clamp domains (documented in tests).

**Stop; do not start Cut BT.**
