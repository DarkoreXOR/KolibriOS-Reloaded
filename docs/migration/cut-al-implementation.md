# Cut AL Implementation — `ext_read_time`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-al-plan.md`](cut-al-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ext_read_time` |
| Source | [`kernel/fs/ext.inc`](../../kernel/fs/ext.inc) |
| Callers | via `ext_read_all_times` only (2 live sites: folder/info paths) |
| Rust symbol | `rust_ext_unix_to_secs` |
| Pure helper | `kolibri_utils::ext_unix_to_secs` |
| Composes | Cut T `fsTime2bdfe` (FASM call after Rust secs) |
| Subsystem | EXT / Unix (+ext4 extra epoch) → BDFE |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. AH+AI helper reuse still ≠ Rust-owned exFAT subsystem.
ISO compare pair is pattern reuse; live `cd_compare_name` already trampolines
to the AJ Rust leaf when `USE_RUST_ISO9660_COMPARE_NAME=1`. XFS v4
`conv_time_to_kos_epoch` remains a thin anti-cluster wrapper after Cut AK.
EXT `ext_write_time` / `ext_read_all_times` are not a Path A boundary (write
depends on `fsGetTime`; all-times is inode orchestration). Socket membership
blockers unchanged. See [`cut-al-plan.md`](cut-al-plan.md).

---

## Candidate comparison (post-AK audit)

| Candidate | Outcome |
|-----------|---------|
| `ext_read_time` | **Selected** — first EXT foothold; epoch-bit + sign + clamp math; compose T |
| `fat_name_is_legal` | #2 — real validate leaf; novelty diluted after Cut U |
| `xfs._.get_before_by_hashval` | #3 — real search leaf; weaker after R+W |
| `ansi2uni_char` | #4 — CP866 decode; deferred table coupling |
| `socket_check` / `cd_compare_name` / thin wrappers / `memmove` | Reject / defer |

---

## Legacy ABI

FASM leaf in `ext.inc` (retained under `USE_RUST_EXT_READ_TIME=0`):

```text
call / ret
in:  EAX = i_*time (Unix secs); EDX = i_*TimeExtra (or 0); EDI → BDFE out
out: EDI = EDI+8; 8-byte BDFE written via fsTime2bdfe
preserves: ECX (critical — ext_read_all_times reuses ECX across calls)
```

Critical quirks retained:

* `and edx, 3` — only ext4 extra epoch bits
* `test eax` / `jns` / `dec edx` — signed `i_time` sign-extension trick
* `sub`/`sbb` with `UNIXTIME_TO_KOS_OFFSET` (`978307200`)
* `js` → clamp `{eax}=0` (pre-2001)
* `jnz` (edx≠0) → clamp `{eax}=0xFFFFFFFF` (past KOS u32 range)
* calendar via `fsTime2bdfe`

---

## Rust ABI

```text
stdcall rust_ext_unix_to_secs(i_time, extra) -> EAX secs ; ret 8
```

Trampoline: `push ecx` / `stdcall …, eax, edx` / `call fsTime2bdfe` /
`pop ecx`. Calendar stays in the proven Cut T path.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `time.rs` + `ffi.rs` section `.text.rust_ext_unix_to_secs` |
| Extract | `extract_reloc_free_text.py` → `rust_ext_unix_to_secs.bin` |
| Embed | `kernel/rust/ext_read_time.inc` `file` directive |
| Trampoline | `ext.inc` under `USE_RUST_EXT_READ_TIME` |
| Gate | `USE_RUST_EXT_READ_TIME` (dev 0 → prod 1) |
| Smoke | `ext_read_time_rust_smoke_test` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ext_unix_to_secs` |
| Blob/object size | 48 bytes |
| Relocations | 0 |
| SHA-256 | `845E9F9A3864BEF278BF415022027FBFFE6976D95288A10C059EA7C9A377C8AB` |

Trailing instruction is `ret 8` (`C2 08 00`).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | KOS epoch; +1s; leap 2004-02-29; EOD; pre-epoch clamp; signed-negative sign-extend; extra&3 mask; high-epoch clamp max |
| PRNG | 50 000 vectors, seed `0x4355544C` (`'CUTL'`) |
| Host tests | **389/389** cargo tests (377 AK baseline + 12 new EXT read-time) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ext_read_time_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | KOS epoch; +1s; EOD; zero→clamp; EDI+=8; ECX/ESI/EBP canaries |
| Marker | `rust_ext_read_time_smoke_result = 'EXTR'` on success |

---

## QEMU validation

Kernels built with Cuts A–AK production gates intact (`USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_EXT_READ_TIME=0` | **OK** (QMP `running` + screendump `dev_build/cut-al-off.ppm`, 112945 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_EXT_READ_TIME=1` | **OK** (screendump `dev_build/cut-al-on.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

Real subsystem soak: **NOT AVAILABLE** — no scripted EXT inode-time harness; attaching `images/exfat-image.img` does not evidence EXT time conversion.

Production image: `dev_build/cut-al-final.img`.

e1000: **N/A**

---

## Production gate

```text
USE_RUST_EXT_READ_TIME = 1
```

Rollback: `USE_RUST_EXT_READ_TIME = 0` (or `enabled = false` in `project/build.toml` Cut AL migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs` (EXT unix→secs + oracle + tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/out/rust_ext_unix_to_secs.bin` (generated)
* `kernel/rust/ext_read_time.inc` (new)
* `kernel/fs/ext.inc` (trampoline + gate; legacy body retained)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-al-plan.md`
* `docs/migration/cut-al-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Stock-image / attached-media EXT inode-time soak not claimed
* `ext_write_time` / `ext_read_all_times` remain FASM (intentional anti-cluster)
* `fat_name_is_legal` / `socket_check` / `memmove` remain deferred
* No Path A cluster claimed
* Current default `qemu.args` do not include e1000 (desktop regression only)
