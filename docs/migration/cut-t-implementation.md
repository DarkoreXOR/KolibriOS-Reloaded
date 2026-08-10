# Cut T Implementation — `fsTime2bdfe`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-t-plan.md`](cut-t-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fsTime2bdfe` |
| Source | [`kernel/fs/fs_common.inc`](../../kernel/fs/fs_common.inc) |
| Callers | 4 — `xfs.asm` ×2 (`conv_time_to_kos_epoch`, `conv_bigtime_to_kos_epoch`); `ext.inc` `ext_read_time`; `ntfs.inc` `ntfs_datetime_to_bdfe` (`jmp`) |
| Rust symbol | `rust_fs_time2bdfe` |
| Pure helper | `kolibri_utils::fs_time2bdfe` / `BdfeTime` |
| Subsystem | Filesystem / calendar |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `fsTime2bdfe` | **Selected** — EDI+=8 inverse calendar; completes Cut G pair |
| `set_window_clientbox` | Deferred — GUI depth after Cut S |
| `pci_make_config_cmd` | Deferred — first PCI, but four-instruction body |
| `coff_get_align` | Deferred — PE/COFF foothold; less ABI novelty |
| `blit_clip` / `fat_time_to_bdfe` / `memmove` / `mutex_init` / `set_io_access_rights` | Deferred — see plan |

---

## Why selected

Cut T’s research question: does Strategy A + C remain viable for a **pointer-advancing write-back leaf** (`EDI += 8`) that is the calendar inverse of Cut G, with stack-local month tables and a trampoline-owned pointer advance?

| Preference | Result |
|------------|--------|
| New class vs A–S | Yes — callee advances EDI (not just mutate-in-place / preserve) |
| Completes G pair | Yes — `months`/`months2` reserved for this leaf since Cut G |
| Real kernel callers | XFS / ext / NTFS timestamp → BDFE |
| Strategy A feasible | Stack month tables (Cut G pattern) |
| Testability | Independent FASM-flow oracle + grid + 200k PRNG + G↔T roundtrip |
| QEMU observability | Weak on stock FAT (no XFS/ext/NTFS list); compensated by smoke + differential |

---

## Special ABI handling

| Item | Contract |
|------|----------|
| Convention | Regcall leaf, plain `ret` |
| In | **EAX** = seconds since 2001-01-01; **EDI** → BDFE out |
| Out | 8 bytes at original EDI; **EDI = EDI + 8** |
| Layout | `+0 sec, +1 min, +2 hour (word), +4 day, +5 month, +6 year u16 LE` |
| Clobbers | EAX, EBX, ECX, EDX, flags |
| Preserved | ESI, EBP |

Trampoline (production):

```text
stdcall rust_fs_time2bdfe, eax, edi
add edi, 8
ret
```

Rust never references `months`/`months2` iglobals (stack-materialized). Trampoline owns the public `EDI+=8` contract so a stdcall bug that failed to advance EDI is independently detectable by smoke.

### Algorithm quirks (locked)

* Hour written as **word** at +2 (clears pad +3) — matches FASM `mov [edi+2], dx`
* After `days/365`, leap adjust uses **signed** `jns` on `sub edx, years/4`
* Month peel uses 16-bit `DX` (`sub dl` / `dec dh` / `jns`) over `months` or `months2`
* Leap table when `(year & 3) == 0`

---

## Original implementation

FASM leaf retained under `USE_RUST_FS_TIME2BDFE=0` in `fs_common.inc`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/time.rs`](../../rust_kernel/kolibri_utils/src/time.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_fs_time2bdfe` |
| Build | [`rust_kernel/kolibri_utils/build-fs-time2bdfe.ps1`](../../rust_kernel/kolibri_utils/build-fs-time2bdfe.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_fs_time2bdfe.bin` |
| Embed/smoke | [`kernel/rust/fs_time2bdfe.inc`](../../kernel/rust/fs_time2bdfe.inc) |

`#![no_std]` freestanding; month tables via `write_volatile` immediates (no `.rodata`).

---

## Link strategy

**Strategy A + C** (reloc-free raw blob + minimal FASM trampoline/switch).

---

## Artifact extraction

| Field | Value |
|-------|-------|
| Section | `.text.rust_fs_time2bdfe` |
| Size | **434** bytes |
| Relocations | **0** |
| SHA-256 | `A9E4C6FAA070D97A235523D99754EF2D1CC781D632603361DCD32F51C96057C0` |
| Epilogue | `ret 8` (`c2 08 00`) |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 8`.

---

## Differential tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **202/202** (incl. prior A–S) |
| Named vectors (epoch, leap, 2010 noon, EOD, `u32::MAX`) | **PASS** |
| Day grid 0..=4000 × TOD samples | **PASS** |
| PRNG 200k vs independent FASM-flow oracle (`0xC07_72B_FE`) | **PASS** |
| Roundtrip vs Cut G on valid dates (named + 50k PRNG) | **PASS** |
| Hour-word pad byte cleared | **PASS** |

---

## In-kernel smoke

`fs_time2bdfe_rust_smoke_test` (wired after Cut S smoke):

* Vectors: epoch 0; leap 2004-02-29; 2010-07-04 12:00; EOD 86399 with poisoned pad; consecutive EDI chaining (+16)
* Asserts EDI+=8, ESI/EBP preservation, BDFE byte layout
* Fail hang: `EAX=0xDEAD0C54`, `EBX='T2BF'`, `ECX='FAIL'`

---

## QEMU validation

Kernels built with Cuts A–S production gates intact (`USE_RUST_CHECK_WINDOW_POSITION=1`, etc.).

Images: rebuilt `dev_build/cut-s-final.img` lineage from reference + Cut S ON kernel (prior disposable images were absent), authorized deletes (`DOCPACK`, `DEVELOP/FASM`, `3D/VIEW3DS`, `GAMES/DINO`), then CoW → Cut T OFF/ON.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| OFF | `USE_RUST_FS_TIME2BDFE=0` | **OK** (QMP `running` + screendump `dev_build/cut-t-off.ppm`, 2333234 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_FS_TIME2BDFE=1` | **OK** (screendump `dev_build/cut-t-on.ppm`, 2333226 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C54`; boot continued to desktop).

**Real subsystem soak:** stock image is FAT desktop — does **not** exercise XFS/ext/NTFS directory listing paths that call `fsTime2bdfe`. Leaf validated by differential + ABI smoke; generic boot/desktop does not attribute production FS timestamp conversion.

Production default after completion: **`USE_RUST_FS_TIME2BDFE = 1`**.

Production image: `dev_build/cut-t-final.img`.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel-cut-t-on.mnt` | 235112 | `625ECC956AA4A50AA77FA1237B0B1F10BB27DD3ECC46AF2880C50845ADF22A14` |
| `kernel-cut-t-off.mnt` | 235208 | `FCC42531173902FDE7DBBB03177C53FC3CCCF06030AA7EDC616D1C0D9AA2B998` |

---

## Rollback

```text
USE_RUST_FS_TIME2BDFE = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/fs_time2bdfe.inc`. Independent of Cuts A–S.

---

## Evidence summary

### PROVEN

* Pointer-advancing (`EDI+=8`) calendar inverse leaf  
* Reloc-free blob with stack month tables  
* Bit-exact vs independent host FASM-flow oracle + 200k PRNG  
* Roundtrip with Cut G on valid production dates  
* Public ABI smoke (layout, EDI chain, ESI/EBP)  
* QEMU OFF/ON desktop regression  

### NOT PROVEN / NOT AVAILABLE

* Live XFS/ext/NTFS directory timestamp conversion on stock image  
* Exhaustive soak of every NTFS `jmp` path under real disk I/O  

---

## Known limitations

* Stock QEMU image does not exercise `fsTime2bdfe` production callers (XFS/ext/NTFS).  
* Month peel assumes FASM day-of-year domain; pathological out-of-range DX is undefined in FASM too.  
* `months`/`months2` iglobals retained for documentation / potential other FASM readers; Rust does not use them.

---

## Out of scope (unchanged)

* Migrating `set_window_clientbox` / `pci_make_config_cmd` / `blit_clip` / `memmove`  
* Cut U  

---

## Completion

Cut T gates complete → **STOP**. Do not start Cut U.
