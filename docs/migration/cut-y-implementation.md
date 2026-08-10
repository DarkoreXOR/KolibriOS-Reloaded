# Cut Y Implementation — `fix_coff_relocs`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-y-plan.md`](cut-y-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fix_coff_relocs` |
| Source | [`kernel/core/dll.inc`](../../kernel/core/dll.inc) |
| Callers | 1 live (`load_library`); dead site in `ext_lib.inc` |
| Rust symbol | `rust_fix_coff_relocs` |
| Pure helper | `kolibri_utils::fix_coff_relocs` |
| Subsystem | PE/COFF driver/DLL loader |

---

## Candidate comparison (post-X audit)

| Candidate | Outcome |
|-----------|---------|
| `fix_coff_relocs` | **Selected** — first PE reloc buffer-patch + section walk |
| `is_partition_table_entry` | Deferred #2 — strong disk/device class; heavier CF+ESI/EBP |
| `get_coff_sym` | Deferred #3 — PE name→Value; thinner than reloc patch |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |
| `net_ptr_to_num4` / `is_string_userspace` | Deferred — thin / after-P |
| `mutex_init` | Deferred Stage-4 sync |
| `pci_make_config_cmd` / `coff_get_align` | Rejected — trivial scalar |
| `xfs_hashname` / `blit_clip` / `strtoint_dec` | Rejected — anti-cluster / banned / dead |

---

## Why selected

Cut Y’s research question: does Strategy A + C remain viable for a **PE/COFF relocation applicator** that walks section reloc tables and mutates loaded-image dwords in-place (DIR32/REL32) — distinct from TSS privilege bits, XFS, TCP timers, calendar, GUI, and FAT naming?

| Preference | Result |
|------------|--------|
| Materially new vs A–X | Yes — first PE reloc / buffer-patch leaf |
| Anti-cluster after X | Yes — not CPU/TSS; not XFS |
| Strategy A feasible | Pure structure walk + wrapping dword math; no globals |
| Clear ABI | stdcall(coff, sym, delta); void; ret 12 |
| Testability | Synthetic COFF corpora; DIR32/REL32/unknown; PRNG |
| Limited blast radius | 1 live `load_library` caller; independent switch |

---

## Original implementation

FASM leaf in `dll.inc` (retained under `USE_RUST_FIX_COFF_RELOCS=0`):

* `stdcall uses ebx esi` with stack args `coff`, `sym`, `delta`
* Walk `nSections` headers at `coff+20`
* For each reloc: resolve `SymIndex * 18` into symbol table
* Type 6 DIR32: `[VA + sec.VA + delta] += Value`
* Type 20 REL32: `[VA + sec.VA + delta] += Value - (VA + sec.VA) - 4`
* Other types skipped
* Void return

Locked layout (`const.inc`):

| Struct | Size | Notes |
|--------|-----:|-------|
| `COFF_HEADER` | 20 | `nSections` @2 |
| `COFF_SECTION` | 40 | `VirtualAddress` @12; `PtrReloc` @24; `NumReloc` @32 |
| `COFF_RELOC` | 10 | `VA` @0; `SymIndex` @4; `Type` @8 |
| `COFF_SYM` | 18 | `Value` @8 |

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/coff_reloc.rs`](../../rust_kernel/kolibri_utils/src/coff_reloc.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_fix_coff_relocs` |
| Build | [`rust_kernel/kolibri_utils/build-fix-coff-relocs.ps1`](../../rust_kernel/kolibri_utils/build-fix-coff-relocs.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_fix_coff_relocs.bin` |
| Embed | [`kernel/rust/fix_coff_relocs.inc`](../../kernel/rust/fix_coff_relocs.inc) |

`#![no_std]` freestanding; byte/dword update only (no tables / `.rodata`).

### Blob lock

| Field | Value |
|-------|-------|
| Size | **237** bytes |
| Relocations | **0** |
| SHA-256 | `8686B72C9C8106F649F1ED08467408DCA974589776CF9DC8F8EF404E4DE0BA04` |
| Epilogue | `ret 12` (`c2 0c 00`) |
| Return | void |

### Trampoline

Hand-written stdcall forwarder in `dll.inc`:

```text
proc fix_coff_relocs stdcall uses ebx esi, coff, sym, delta
        stdcall rust_fix_coff_relocs, [coff], [sym], [delta]
        ret
endp
```

Smoke builds a synthetic mini-COFF in uglobal BSS and calls the public ABI (DIR32 + REL32 + delta shift + zero-sections).

---

## Build / package sequence

```powershell
powershell -File rust_kernel/kolibri_utils/build-fix-coff-relocs.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\dev_build\cut-x-final.img ..\..\dev_build\cut-y-on.img
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-y-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **251/251** (includes Cut Y COFF reloc suite) |
| Empty / zero-reloc / unknown type | **PASS** |
| DIR32 / REL32 / sec.VA / delta | **PASS** |
| Multi-section + SymIndex | **PASS** |
| Wrapping addends | **PASS** |
| Deterministic PRNG (50 000 images, seed `0x43555459`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow buffered oracle vs Rust | **PASS** |
| Boundary coverage | nSections=0; NumReloc=0; types 6/20/other; delta≠0; wraparound |

---

## ABI smoke

| Item | Result |
|------|--------|
| `fix_coff_relocs_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C59` hang) |
| Vectors | DIR32; REL32; delta-shifted DIR32; zero sections; EBX/ESI/EBP preserve |
| Marker | `rust_fix_coff_relocs_smoke_result = 'FCRX'` on success |

---

## QEMU validation

Kernels built with Cuts A–X production gates intact (`USE_RUST_SET_IO_ACCESS_RIGHTS=1`, etc.).

Images: CoW from `dev_build/cut-x-final.img`, replace `KERNEL.MNT`.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| OFF | `USE_RUST_FIX_COFF_RELOCS=0` | **OK** (QMP `running` + screendump `dev_build/cut-y-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_FIX_COFF_RELOCS=1` | **OK** (screendump `dev_build/cut-y-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C59`; boot continued to desktop).

**Real subsystem soak:** stock image boot does **not** deliberately force COFF DLL/`load_library` reloc application beyond whatever drivers the image already loads. Leaf validated by differential + ABI smoke on synthetic COFF; generic boot/desktop/network attach is kernel integration regression. Target-specific forced DLL reload was **not** performed.

Production default after completion: **`USE_RUST_FIX_COFF_RELOCS = 1`**.

Production image: `dev_build/cut-y-final.img`.

---

## Kernel sizes

| Artifact | Size |
|----------|------|
| `kernel-cut-y-off.mnt` | 239064 |
| `kernel-cut-y-on.mnt` | 238984 |

---

## Rollback

```text
USE_RUST_FIX_COFF_RELOCS = 0
```

Legacy FASM body remains under the `else` branch.

---

## Known limitations

* Stock QEMU image does not force a dedicated COFF DLL reload soak beyond normal boot drivers.
* Host differential uses offset-into-image addressing (64-bit host cannot place absolute 32-bit patch pointers); production/smoke use absolute addresses matching FASM.
* Incorrect reloc application can break COFF `.obj`/driver load (Stage-8 surface).
* Legacy FASM enters `.fix_sec` before testing `nSections`, so `nSections=0` infinite-loops / faults. No production caller passes 0. Rust exits cleanly on 0 (intentional safe divergence; documented). ABI smoke never uses `nSections=0` against the public symbol.

---

## Out of scope

* Migrating `get_coff_sym` / `rebase_coff` / `fix_coff_symbols` / `memmove`  
* Migrating `is_partition_table_entry`  
* Cut Z  
