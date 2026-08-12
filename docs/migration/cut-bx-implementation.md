# Cut BX Implementation — `bdfe_to_fat_date`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bx-plan.md`](cut-bx-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BX** |
| FASM symbol | `bdfe_to_fat_date` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 6× `call bdfe_to_fat_date` (FAT×3, exFAT×3 entry→FAT write paths) |
| Rust symbol | `rust_bdfe_to_fat_date` |
| Pure helper | `kolibri_utils::bdfe_to_fat_date` |
| Subsystem | FAT/exFAT BDFE date → DOS packed-date pack |
| Stage | Stage 5 FS metadata foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — FAT calendar cluster has no Rust-owned FS plugin;
entry orchestration remains FASM.

Selected `bdfe_to_fat_date` over `bdfe_to_fat_time`, `uni2ansi_char`, and
`fsReadCMOS` for distinct **write-path date pack** semantic (BW migrated read
unpack only) + live write-path fanout + excellent BW round-trip oracle after
fresh post-BW audit.

---

## Legacy ABI

```text
regcall bdfe_to_fat_date()
  in:  EAX = BDFE date dword (year<<16 | month<<8 | day)
  out: EAX = FAT packed date (callers store AX only)
  body push/pop EDX internally
preserves: EBX, ESI, EDI, EBP, ECX typical across callers
trampoline must preserve ECX+EDX (REG-001)
```

Quirks retained:

* `sub ax, 1980` (16-bit year subtract)
* Month from `dh & 0xF`; day from `dl & 0x1F`
* No calendar clamping (same as FASM)

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_bdfe_to_fat_date` |
| Blob | **40** bytes, **0 relocations** |
| SHA-256 | `9f5b983136826f1f7dd88dcd7a4b4822a9e8c6b97faa965d77dd6cbf77089f5a` |
| Trampoline | `push ecx` / `push edx` / `stdcall rust_bdfe_to_fat_date, eax` / `pop edx` / `pop ecx` / `ret` |
| Gate | `USE_RUST_BDFE_TO_FAT_DATE` (prod 1) |
| Rust ABI | `stdcall rust_bdfe_to_fat_date(bdfe_date); ret 4` → EAX |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_bdfe_to_fat_date`) |
| PRNG seed | `0x43554258` (`'CUBX'`) |
| PRNG cases | 50,000 |
| Round-trip | Exhaustive u16 via migrated BW `fat_date_to_bdfe` |
| Host tests | **PASS** — `629/629` (includes 3 Cut BX tests + 50k PRNG + 65k round-trip) |
| ABI smoke | **PASS** — marker `'B2FD'` |

---

## QEMU regression

| Config | Gate | Result | Non-black |
|--------|------|--------|-----------|
| OFF | `enabled = false` | **OK** (`running`) | 779380 |
| ON | `USE_RUST_BDFE_TO_FAT_DATE=1` | **OK** (`running`) | 779380 |

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
| `--disk exfat` attach-only (boot FAT + AHCI exFAT) | **PASS** — 779380 non-black |
| Dedicated FAT date metadata write harness | **NOT AVAILABLE** |

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
| Gate | `USE_RUST_BDFE_TO_FAT_DATE = 1` |
| Rollback | `USE_RUST_BDFE_TO_FAT_DATE = 0` and/or `enabled = false` in `build.toml` |
| Final image | `dev_build/test/kernel-20260812-131420.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/fs/fat.inc`
* `kernel/rust/bdfe_to_fat_date.inc` (new)
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/cut-bx-plan.md` (new)
* `docs/migration/cut-bx-implementation.md` (new)

---

## Known limitations

* Read-path calendar siblings remain partially migrated (BW read date; AO read time).
* Write-path time pack (`bdfe_to_fat_time`) remains FASM.
* `uni2ansi_char` remains FASM (AN inverse ban).
* No dedicated harness that exercises only FAT date pack on metadata write.
* exFAT validation is attach-only rather than full metadata write matrix.

---

## Inventory

**79 / 135**
