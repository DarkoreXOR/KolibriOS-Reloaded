# Cut CL Implementation — `exFAT_get_sector`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-cl-plan.md`](cut-cl-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CL** |
| FASM symbol | `exFAT_get_sector` |
| Source | [`kernel/fs/exfat.inc`](../../kernel/fs/exfat.inc) |
| Callers | 3 live (`exFAT_hd_find_lfn.found`, `exFAT_notroot_next_sector`, `exFAT_notroot_first`) |
| Rust symbol | `rust_exfat_get_sector` |
| Pure helper | `kolibri_utils::exfat_get_sector` / `exfat_get_sector_ptr` |
| Subsystem | exFAT cluster/offset → absolute sector (Stage-2 FS util) |
| Stage | Stage 2 / FS address-math leaf |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — no remaining pending symbol establishes Rust-owned subsystem state (PE/USB/video/exFAT-plugin/IRQ/string-export clusters fail ownership or substance bars).

Selected `exFAT_get_sector` over `unpack` (Stage-2 blob blocked by ~253 B headroom / multi-KiB LZMA), `blit_32` (LFB blast), `exFAT_find_lfn` (plugin island), `drawChar` (Stage 7), `getInodeLocation` (no `--disk ext`), and `enable_irq`/`irq_eoi`/`mutex_init` (oracle/thin) after fresh post-CK audit. **AW address-math anti-cluster ban aged out** (CK closed FAT twin); exFAT twin has independent callers + `--disk exfat` soak and is the strongest remaining Stage-2 Path B leaf that fits memory.

Memory: end `.bss` @ `OS_BASE+0x8CF43`; assert needs `0x8DF43 < TMP_STACK_TOP`. Existing `0x008DF00` failed by **0x43**. Raised **`TMP_STACK_TOP` `0x008DF00` → `0x008DF80`** (+128 B; smallest clean step that clears `0x8DF43` while staying below `sys_proc` @ `0x8E000`). Gap to `sys_proc` remains (`0x80`).

---

## Legacy ABI

```text
exFAT_get_sector  (register ABI; not stdcall)
  in:  EAX → {cluster @0, sector_in_cluster @4}; EBP → exFAT*
  out: EAX = (cluster-2)*SECTORS_PER_CLUSTER + CLUSTER_HEAP_START + sector_in_cluster
  body: push/pop ECX; 32-bit imul wrap; DF unchanged
  preserves: ECX, EBX, EDX, ESI, EDI, EBP
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_exfat_get_sector` |
| Blob | **25** bytes, **0 relocations** |
| SHA-256 | `766a371d747139c9f2520f4b6a55e18e6367fa9fdf6530637902d3a8be374572` |
| Epilogue | `ret 16` (`c2 10 00`) |
| Note | Identical machine code to Cut CK `rust_fat_get_sector` (same math; distinct symbol/section/embed) |
| Trampoline | omit-FP; push EBX/ECX/EDX/ESI/EDI; inject pair + exFAT fields; `stdcall rust_*`; restore; never `add esp` (REG-009) |
| Gate | `USE_RUST_EXFAT_GET_SECTOR` (prod 1) |
| Rust ABI | `stdcall rust_exfat_get_sector(cluster, ofs, spc, cluster_heap_start); ret 16` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent i64 `(cluster-2)*spc + cluster_heap_start + ofs` truncated to u32 |
| PRNG seed | `0x45534543` (`'ESEC'`) |
| PRNG cases | 50,000 |
| Cut CL host tests | focused `esec_*` **10/10 PASS** |
| Full host suite | **734/734 PASS** |
| ABI smoke | **PASS** — marker `'ESEC'` (hang=`DEAD0C6C`); synthetic `sizeof.exFAT` + pair; public trampoline + direct `rust_*`; register canaries |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_EXFAT_GET_SECTOR=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_EXFAT_GET_SECTOR=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-193847.img`

---

## Subsystem soak

`--disk exfat` attach soak: `python scripts/qmp_desktop_smoke.py --wait 90 --disk exfat`  
Result: `query-status: running`, non-black=779380, resets=0. Exercises exFAT partition create/dir walk paths that call `exFAT_get_sector` (`exFAT_hd_find_lfn` / `exFAT_notroot_*`). Primary correctness evidence remains the independent LBA oracle + ABI smoke + A/B desktop.

---

## Regressions

None this cut.

Applied prior lessons: FASM register-ABI outer / Rust `stdcall` only (no double cleanup); REG-001/011 preserve EBX/ECX/EDX/ESI/EDI/EBP; omit-FP keeps EBP→exFAT; REG-003 synthetic exFAT only; hang marker `DEAD0C6C`.

---

## Rollback

```text
USE_RUST_EXFAT_GET_SECTOR = 0
```

in `kernel/fs/exfat.inc` (or `enabled = false` for Cut CL in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/exfat_get_sector.rs` | Pure helper + oracle tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_exfat_get_sector` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | module export |
| `rust_kernel/kolibri_utils/out/rust_exfat_get_sector.bin` | reloc-free blob |
| `kernel/rust/exfat_get_sector.inc` | embed + ABI smoke |
| `kernel/fs/exfat.inc` | gate + trampoline + legacy body |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | smoke call |
| `kernel/const.inc` | `TMP_STACK_TOP` +0x80 |
| `project/build.toml` | blob + migration registry |
| `docs/compatibility/fixed-addresses.md` | TMP update |
| `docs/architecture/memory-model.md` | TMP update |
| `docs/migration/cut-cl-plan.md` | plan |
| `docs/migration/cut-cl-implementation.md` | this file |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | progress |

---

## Inventory after CL

`93 / 135` completed; `42` pending; **93 / 93** production gates enabled.
