# Cut BY Implementation — `bdfe_to_fat_time`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-by-plan.md`](cut-by-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BY** |
| FASM symbol | `bdfe_to_fat_time` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 5× `call bdfe_to_fat_time` (FAT×2, exFAT×3 entry→FAT write paths) |
| Rust symbol | `rust_bdfe_to_fat_time` |
| Pure helper | `kolibri_utils::bdfe_to_fat_time` |
| Subsystem | FAT/exFAT BDFE time → DOS packed-time conversion |
| Stage | Stage 5 FS metadata foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — FAT calendar cluster has no Rust-owned FS plugin;
entry orchestration remains FASM.

Selected `bdfe_to_fat_time` over `uni2ansi_char`, `fsReadCMOS`, and
`ahci_port_wait` for distinct **write-path time pack** semantic (AO migrated read
unpack only) + live write-path fanout + excellent AO round-trip oracle after
fresh post-BX audit.

---

## Legacy ABI

```text
regcall bdfe_to_fat_time()
  in:  EAX = BDFE time dword (hours<<16 | minutes<<8 | seconds)
  out: EAX = FAT packed time (callers store AX only)
  body push/pop EDX internally
preserves: EBX, ESI, EDI, EBP, ECX typical across callers
trampoline must preserve ECX+EDX (REG-001)
```

Quirks retained:

* Hours from high word (`shr eax,16`)
* Minutes from `dh & 0x3F`
* Seconds from `dl >> 1` then `& 0x1F` (FAT stores sec/2)
* No calendar clamping (same as FASM)

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_bdfe_to_fat_time` |
| Blob | **35** bytes, **0 relocations** |
| SHA-256 | `7b7f70e8ab8a00c36240af52067ca1d1d5cebf9a19b66e2f836bcd36f0059d25` |
| Trampoline | `push ecx` / `push edx` / `stdcall rust_bdfe_to_fat_time, eax` / `pop edx` / `pop ecx` / `ret` |
| Gate | `USE_RUST_BDFE_TO_FAT_TIME` (prod 1) |
| Rust ABI | `stdcall rust_bdfe_to_fat_time(bdfe_time); ret 4` → EAX |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_bdfe_to_fat_time`) |
| PRNG seed | `0x43554259` (`'CUBY'`) |
| PRNG cases | 50,000 |
| Round-trip | Exhaustive u16 via migrated AO `fat_time_to_bdfe` |
| Host tests | **PASS** — `632/632` (includes 3 Cut BY tests + 50k PRNG + 65k round-trip) |
| ABI smoke | **PASS** — marker `'B2FT'` |

---

## QEMU regression

| Config | Gate | Result | Non-black |
|--------|------|--------|-----------|
| ON | `USE_RUST_BDFE_TO_FAT_TIME=1` | **OK** (`running`) | 779380 |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| Desktop non-black count (all gates ON incl. BY) | **PASS** — 779380 |

Prior BX A/B baseline unchanged (779380 vs 779380); BY additive gate ON
does not regress desktop framebuffer.

---

## Real subsystem soak

| Harness | Result |
|---------|--------|
| `--disk exfat` attach-only (boot FAT + AHCI exFAT) | **PASS** — 779380 non-black |
| Dedicated FAT time metadata write harness | **NOT AVAILABLE** |

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
| Gate | `USE_RUST_BDFE_TO_FAT_TIME = 1` |
| Rollback | `USE_RUST_BDFE_TO_FAT_TIME = 0` and/or `enabled = false` in `build.toml` |
| Final image | `dev_build/test/kernel-20260812-131922.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/fs/fat.inc`
* `kernel/rust/bdfe_to_fat_time.inc` (new)
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/cut-by-plan.md` (new)
* `docs/migration/cut-by-implementation.md` (new)

---

## Known limitations

* FAT calendar pack/unpack quartet now complete (AO read time, BW read date,
  BX write date, BY write time); entry orchestration remains FASM.
* `uni2ansi_char` remains FASM (AN inverse ban).
* No dedicated harness that exercises only FAT time pack on metadata write.
* exFAT validation is attach-only rather than full metadata write matrix.

---

## Inventory

**80 / 135**
