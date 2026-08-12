# Cut BZ Implementation — `uni2ansi_char`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bz-plan.md`](cut-bz-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BZ** |
| FASM symbol | `uni2ansi_char` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 11× `call uni2ansi_char` (7 files: ISO9660×4, NTFS×2, FAT, exFAT, fs_lfn, taskman, unicode fallback) |
| Rust symbol | `rust_uni2ansi_char` |
| Pure helper | `kolibri_utils::cp866_encode` (same algorithm as Cut A `unicode.cp866.encode`) |
| Subsystem | Unicode / CP866 encode (public parse_fn leaf) |
| Stage | Stage 2 unicode / FS name conversion |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — Cut A encode + Cut AN decode do not constitute a
Rust-owned Unicode subsystem; string orchestrators remain FASM.

Selected `uni2ansi_char` over `fsReadCMOS`, `ahci_port_wait`, and
`ntfs_restore_usa_frs` after fresh post-BY audit and FAT calendar quartet
re-evaluation. Distinct **public encode leaf** with highest remaining caller
fanout; Cut A only covered the `unicode.cp866.encode` export trampoline path.

---

## Legacy ABI

```text
call/ret (not stdcall)
in:  AX = Unicode code unit
out: AL = CP866 byte (EAX low byte)
preserves: ECX (FAT/ISO/NTFS `loop` counters), EDX (REG-001)
```

Quirks retained:

* ASCII `< 0x80` passthrough
* `U+00B6` → `0x14`
* Cyrillic `0x0410..=0x043F` → `0x80..=0xAF`
* Cyrillic `0x0440..=0x044F` → `0xE0..=0xEF`
* Special `0x0401..=0x045E` table → `0xF0..=0xF7`
* Unmapped → `'_'`
* Input read as full AX (16-bit)

Shared `uni2ansi_char.table` retained for FASM rollback and
`ansi2uni_char` FASM rollback (Cut AN).

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_uni2ansi_char` |
| Blob | **294** bytes, **0 relocations** |
| SHA-256 | `96e5e05c8701768532b4e106e50b8a51d008965fe07c01f9aa38f895e8c7dac1` |
| Trampoline | `push ecx` / `push edx` / `stdcall rust_uni2ansi_char, eax` / `pop edx` / `pop ecx` / `ret` |
| Gate | `USE_RUST_UNI2ANSI_CHAR` (prod 1) |
| Rust ABI | `stdcall rust_uni2ansi_char(cp); ret 4` → EAX (AL) |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`oracle_cp866` in `unicode.rs`) |
| PRNG seed | `0x4355545A` (`'CUTZ'`) |
| PRNG cases | 50,000 (u16 domain) |
| Round-trip | Supplementary decode→encode on mapped set via Cut AN `cp866_decode` |
| Exhaustive u16 | Pre-existing `cp866_exhaustive_matches_oracle` |
| Host tests | **PASS** — `634/634` |
| ABI smoke | **PASS** — marker `'U2AN'` |

---

## QEMU regression

| Config | Gate | Result | Non-black |
|--------|------|--------|-----------|
| OFF | `USE_RUST_UNI2ANSI_CHAR=0` | **OK** (`running`) | 779380 |
| ON | `USE_RUST_UNI2ANSI_CHAR=1` | **OK** (`running`) | 779380 |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| Desktop OFF vs ON non-black | **779380 vs 779380** — exact match |
| Prior BY baseline | Unchanged (779380) |

---

## Real subsystem soak

| Harness | Result |
|---------|--------|
| `--disk iso9660` attach (4 caller sites) | **PASS** — 779380 non-black |
| `--disk exfat` attach-only | **PASS** — 779380 non-black |
| Dedicated UCS-2→CP866 name conversion harness | **NOT AVAILABLE** (attach-only) |

---

## Regression status

| Item | Status |
|------|--------|
| Regressions discovered | **NONE** |
| regression-log entry | Not required |

---

## Production gate / rollback

| Item | Value |
|------|-------|
| Gate | `USE_RUST_UNI2ANSI_CHAR = 1` |
| Rollback | Set gate `0` in `parse_fn.inc` or `enabled = false` for Cut BZ in `project/build.toml`; re-assemble |
| Final image | `dev_build/test/kernel-20260812-132740.img` |

---

## Files changed

| Path | Change |
|------|--------|
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_uni2ansi_char` FFI entry |
| `rust_kernel/kolibri_utils/src/unicode.rs` | PRNG seed + Cut BZ tests |
| `rust_kernel/kolibri_utils/src/lib.rs` | export `UNI2ANSI_CHAR_PRNG_SEED` |
| `kernel/fs/parse_fn.inc` | gate + trampoline; table hoisted |
| `kernel/rust/uni2ansi_char.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | smoke call |
| `project/build.toml` | blob + migration registry |
| `docs/migration/cut-bz-plan.md` | this plan |
| `docs/migration/cut-bz-implementation.md` | this file |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | cut entry |

---

## Known limitations

* Algorithm shared with Cut A `rust_unicode_cp866_encode` — separate symbol/section
  for the public `uni2ansi_char` leaf boundary, not a second implementation.
* Attach-only ISO9660/exFAT soak does not exercise every in-kernel name path
  (NTFS/taskman require dedicated browse/write harness).
* Does **not** claim Path A Unicode subsystem ownership.

---

## Inventory after close

**81 / 135** — one `[x]` added for `uni2ansi_char`.
