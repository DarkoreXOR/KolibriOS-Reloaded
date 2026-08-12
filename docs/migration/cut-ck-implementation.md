# Cut CK Implementation — `fat_get_sector`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-ck-plan.md`](cut-ck-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CK** |
| FASM symbol | `fat_get_sector` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 4 live FAT dir/write paths (`hd_find_lfn.found`, `fat_notroot_next_sector`, `fat_notroot_end_write`, …) |
| Rust symbol | `rust_fat_get_sector` |
| Pure helper | `kolibri_utils::fat_get_sector` / `fat_get_sector_ptr` |
| Subsystem | FAT cluster/offset → absolute sector (Stage-2 FS util) |
| Stage | Stage 2 / FS address-math leaf |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — no remaining pending symbol establishes Rust-owned subsystem state (PE/USB/video/exFAT/IRQ/string-export clusters fail ownership or substance bars).

Selected `fat_get_sector` over `unpack` (Stage-2 blob blocked by ~125 B headroom / multi-KiB LZMA), `blit_32` (LFB blast), `exFAT_find_lfn` (plugin island), `mutex_init` (thin+fanout), and `enable_irq`/`irq_eoi` (I/O oracle) after fresh post-CJ audit. **AW address-math anti-cluster ban aged out** (many cuts since AW); FAT boot-floppy soak is stronger than remaining `exFAT_get_sector` / `getInodeLocation` siblings.

Memory: end `.bss` @ `OS_BASE+0x8CE03`; assert needs `0x8DE03 < TMP_STACK_TOP`. Existing `0x008DD00` failed. Raised **`TMP_STACK_TOP` `0x008DD00` → `0x008DF00`** (+512 B; smallest clean `0x100`-aligned step that clears `0x8DE03`). Gap to `sys_proc` @ `0x8E000` remains (`0x100`).

---

## Legacy ABI

```text
fat_get_sector  (register ABI; not stdcall)
  in:  EAX → {cluster @0, sector_in_cluster @4}; EBP → FAT*
  out: EAX = (cluster-2)*SECTORS_PER_CLUSTER + DATA_START + sector_in_cluster
  body: push/pop ECX; 32-bit imul wrap; DF unchanged
  preserves: ECX, EBX, EDX, ESI, EDI, EBP
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_fat_get_sector` |
| Blob | **25** bytes, **0 relocations** |
| SHA-256 | `766a371d747139c9f2520f4b6a55e18e6367fa9fdf6530637902d3a8be374572` |
| Epilogue | `ret 16` (`c2 10 00`) |
| Trampoline | omit-FP; push EBX/ECX/EDX/ESI/EDI; inject pair + FAT fields; `stdcall rust_*`; restore; never `add esp` (REG-009) |
| Gate | `USE_RUST_FAT_GET_SECTOR` (prod 1) |
| Rust ABI | `stdcall rust_fat_get_sector(cluster, ofs, spc, data_start); ret 16` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent i64 `(cluster-2)*spc + data_start + ofs` truncated to u32 |
| PRNG seed | `0x46534543` (`'FSEC'`) |
| PRNG cases | 50,000 |
| Cut CK host tests | focused `fsec_*` **10/10 PASS** |
| Full host suite | **724/724 PASS** |
| ABI smoke | **PASS** — marker `'FSEC'` (hang=`DEAD0C6B`); synthetic `sizeof.FAT` + pair; public trampoline + direct `rust_*`; register canaries |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_FAT_GET_SECTOR=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_FAT_GET_SECTOR=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-191111.img`

---

## Subsystem soak

Boot floppy is FAT — every desktop boot walks FAT directories via `hd_find_lfn` / `fat_notroot_*` callers of `fat_get_sector`. Soak boots: `query-status: running`, non-black=779380, resets=0. Primary correctness evidence remains the independent LBA oracle + ABI smoke + A/B desktop.

---

## Regressions

None this cut.

Applied prior lessons: FASM register-ABI outer / Rust `stdcall` only (no double cleanup); REG-001/011 preserve EBX/ECX/EDX/ESI/EDI/EBP; omit-FP keeps EBP→FAT; REG-003 synthetic FAT only; hang marker `DEAD0C6B`.

---

## Rollback

```text
USE_RUST_FAT_GET_SECTOR = 0
```

in `kernel/fs/fat.inc` (or `enabled = false` for Cut CK in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/fat_get_sector.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_fat_get_sector` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/fat_get_sector.inc` | Blob embed + ABI smoke |
| `kernel/fs/fat.inc` | Gate + trampoline + legacy body |
| `kernel/kernel32.inc` | Include |
| `kernel/kernel.asm` | Smoke call |
| `kernel/const.inc` | `TMP_STACK_TOP` +0x200 |
| `project/build.toml` | Blob + migration CK |
| `docs/compatibility/fixed-addresses.md` | TMP baseline |
| `docs/architecture/memory-model.md` | TMP baseline |
| `docs/migration/cut-ck-plan.md` | Plan |
| `docs/migration/migration-todo.md` | Inventory 92/135 |
| `docs/migration/migration-plan.md` | Cut CK entry |

---

## Stop

**Cut CK complete. Do not start Cut CL.**
