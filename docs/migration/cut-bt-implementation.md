# Cut BT Implementation — `ntfsGetTime`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bt-plan.md`](cut-bt-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BT** |
| FASM symbol | `ntfsGetTime` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | 4× `call ntfsGetTime` (GetFileInfo/create/resize metadata stamp) |
| Rust symbol | `rust_ntfs_get_time_pack` |
| Pure helper | `kolibri_utils::ntfs_get_time_pack` |
| Composes | `filetime_from_secs_2001` (Cut AF bias constants) |
| Subsystem | NTFS / CMOS metadata FILETIME pack |
| Stage | Stage 5 FS plugin foothold (NTFS write/metadata leaf) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — NTFS AF/BT pack leaves do not establish Rust-owned NTFS
mount/write subsystem.

Selected `ntfsGetTime` over `fix_coff_symbols`, `fsGetTime`, and `fsReadCMOS`
for CMOS-metadata FILETIME semantic class + four live callers + `--disk ntfs`
soak after fresh post-BS audit.

---

## Legacy ABI

```text
regcall ntfsGetTime()
  call fsGetTime → EAX = KOS secs since 2001-01-01
  mov edx, 10000000
  mul edx
  add eax, 3365781504
  adc edx, 29389701
  ret → EDX:EAX FILETIME
clobbers: EAX, ECX, EDX, ESI (fsGetTime stack BDFE ptr in ESI)
preserves: EBX, EDI, EBP (callers advance EDI, not ESI)
```

Quirks retained:

* `NTFS_FILETIME_PER_SEC` = `10_000_000`
* `NTFS_FILETIME_BIAS_LO/HI` = `3365781504` / `29389701`
* 32-bit `mul edx` on KOS seconds (EDX:EAX product)
* `adc` carry into hi word after bias add
* Independent of Cut AF `ntfsCalculateTime` entry (BDFE path)

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_ntfs_get_time_pack` |
| Blob | **23** bytes, **0 relocations** |
| SHA-256 | `bda4113281eb2bb689764188f52e7a143aee891ae6fccf2ff78568e72071437b` |
| Trampoline | `call fsGetTime` → `stdcall rust_ntfs_get_time_pack, eax` |
| Gate | `USE_RUST_NTFS_GET_TIME` (prod 1) |
| Rust ABI | `stdcall rust_ntfs_get_time_pack(kos_secs); ret 4` → EDX:EAX |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow `mul`/`add`/`adc` mirror (literal bias constants) |
| Host tests | **PASS** — `606/606` (includes 6 Cut BT tests + 50k PRNG) |
| Seed | `0x43554254` (`'CUBT'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ntfs_get_time_rust_smoke_test` | **PASS** |
| Marker | `rust_ntfs_get_time_smoke_result = 'NTGT'` |
| Coverage | direct Rust kos=0/1; fsGetTime+Rust vs FASM pack oracle; public `ntfsGetTime` non-zero |
| Live state | no live NTFS structures (REG-003 safe) |
| Lesson | Do **not** assert ESI preserve across `ntfsGetTime` — `fsGetTime` clobbers ESI |

Initial vector 3 (ESI/EDI/EBP canaries + double-CMOS compare) failed pre-desktop;
replaced with fsGetTime-oracle vector + separate public-trampoline non-zero check.

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` | **OK** (`running`, 779380 non-black) | A/B capture |
| ON | `USE_RUST_NTFS_GET_TIME=1` | **OK** (`running`, 779380 non-black) | A/B capture |
| ON + NTFS | `--disk ntfs` | **OK** (`running`, 779380 non-black) | attach-only soak |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25` (+ `--disk ntfs`).

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |
| ON `--disk ntfs` attach | **PASS** — 779380 non-black |

---

## Real subsystem soak

| Harness | Result |
|---------|--------|
| `--disk ntfs` attach-only desktop | **PASS** |
| NTFS write/create metadata stamp paths | **PARTIAL / NOT AVAILABLE** — no automated browse/write harness |

---

## Production gate

| Item | Value |
|------|-------|
| Gate | `USE_RUST_NTFS_GET_TIME = 1` |
| Registry | `project/build.toml` `[[rust.migrations]]` cut = `"BT"` `enabled = true` |

### Rollback

1. Set `USE_RUST_NTFS_GET_TIME = 0` in `kernel/fs/ntfs.inc`, **or**
2. Set `enabled = false` for cut `"BT"` in `project/build.toml` and rebuild.

---

## Regressions

None. No `REG-*` entry required.

---

## Files changed

| Path | Change |
|------|--------|
| `rust_kernel/kolibri_utils/src/ntfs_get_time.rs` | Pure pack + differential tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_ntfs_get_time_pack` stdcall blob |
| `rust_kernel/kolibri_utils/src/lib.rs` | module + re-export |
| `kernel/rust/ntfs_get_time.inc` | embed + ABI smoke (`NTGT`) |
| `kernel/fs/ntfs.inc` | `USE_RUST_NTFS_GET_TIME` gate + trampoline |
| `kernel/kernel32.inc` | include embed |
| `kernel/kernel.asm` | smoke call |
| `project/build.toml` | blob + migration registry |
| `docs/migration/cut-bt-plan.md` | this plan |
| `docs/migration/cut-bt-implementation.md` | this report |
| `docs/migration/migration-todo.md` | inventory 75/135 |
| `docs/migration/migration-plan.md` | Cut BT entry |
| `docs/migration/boundaries.md` | post-BT baseline |

---

## Known limitations

* `fsGetTime`/CMOS remains FASM-owned; Rust pack only after KOS seconds available.
* Smoke cannot deterministically compare two consecutive public `ntfsGetTime` calls (CMOS may tick).
* No dedicated NTFS write-path automated soak beyond attach-only `--disk ntfs`.

---

## Inventory

**Functions completed / functions total:** `75 / 135`

**Stop; do not start Cut BU.**
