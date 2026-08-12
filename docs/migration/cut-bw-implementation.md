# Cut BW Implementation — `fat_date_to_bdfe`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bw-plan.md`](cut-bw-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BW** |
| FASM symbol | `fat_date_to_bdfe` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 6× `call fat_date_to_bdfe` (FAT×3, exFAT×3 entry→BDFE read paths) |
| Rust symbol | `rust_fat_date_to_bdfe` |
| Pure helper | `kolibri_utils::fat_date_to_bdfe` |
| Subsystem | FAT/exFAT DOS packed-date → BDFE unpack |
| Stage | Stage 5 FS metadata foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — FAT calendar cluster has no Rust-owned FS plugin;
entry orchestration remains FASM.

Selected `fat_date_to_bdfe` over `uni2ansi_char`, `bdfe_to_fat_time`, and
`ahci_port_wait` for distinct **date** unpack semantic (AO migrated **time**
only) + live read-path fanout + excellent u16 oracle after fresh post-BV audit.

---

## Legacy ABI

```text
regcall fat_date_to_bdfe()
  in:  EAX = FAT packed date (body uses full EAX — high bits affect year)
  out: EAX = BDFE date dword (year<<16 | month<<8 | day)
  body push/pop ECX+EDX internally
preserves: EBX, ESI, EDI, EBP typical across callers
trampoline must preserve ECX+EDX (REG-001)
```

Quirks retained:

* `shr eax, 9` then `add ax, 1980` (16-bit year add)
* Day from `edx & 0x1F`; month from `(ecx>>5) & 0xF`
* No calendar clamping (same as FASM)

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_fat_date_to_bdfe` |
| Blob | **38** bytes, **0 relocations** |
| SHA-256 | `ab0b13dbae9f3581642a0f520d022176accd821ec347baf4a47b0f64b5f5c82f` |
| Trampoline | `push ecx` / `push edx` / `stdcall rust_fat_date_to_bdfe, eax` / `pop edx` / `pop ecx` / `ret` |
| Gate | `USE_RUST_FAT_DATE_TO_BDFE` (prod 1) |
| Rust ABI | `stdcall rust_fat_date_to_bdfe(fat_date); ret 4` → EAX |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_fat_date_to_bdfe`) |
| PRNG seed | `0x43554257` (`'CUBW'`) |
| PRNG cases | 50,000 |
| Host tests | **PASS** — `626/626` (includes 3 Cut BW tests + 50k PRNG) |
| ABI smoke | **PASS** — marker `'FDTB'` |

---

## QEMU regression

| Config | Gate | Result | Non-black |
|--------|------|--------|-----------|
| OFF | `enabled = false` | **OK** (`running`) | 779380 |
| ON | `USE_RUST_FAT_DATE_TO_BDFE=1` | **OK** (`running`) | 779380 |

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
| Dedicated FAT date metadata browse harness | **NOT AVAILABLE** |

---

## Regression status

| Item | Status |
|------|--------|
| Regressions discovered | **NONE** |
| regression-log entry | Not required |

Initial smoke fixture defect (`0x158CF` high-bit vector) fixed before closure — not a kernel regression.

---

## Production

| Item | Value |
|------|-------|
| Gate | `USE_RUST_FAT_DATE_TO_BDFE = 1` |
| Rollback | `USE_RUST_FAT_DATE_TO_BDFE = 0` and/or `enabled = false` in `build.toml` |
| Final image | `dev_build/test/kernel-20260812-130241.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/fs/fat.inc`
* `kernel/rust/fat_date_to_bdfe.inc` (new)
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/cut-bw-plan.md` (new)
* `docs/migration/cut-bw-implementation.md` (new)

---

## Known limitations

* Write-path calendar siblings (`bdfe_to_fat_time` / `bdfe_to_fat_date`) remain FASM (ban-list).
* No dedicated harness that exercises only FAT date unpack on metadata write.
* Smoke does not cover full EAX high-bit year pollution paths (fixture removed after false-positive).

---

## Inventory

**78 / 135**
