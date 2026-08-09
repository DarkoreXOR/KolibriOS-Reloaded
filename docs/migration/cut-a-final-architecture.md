# Cut A Final Architecture (CRC & Unicode)

**Stage:** Stage 2 / Cut A — **complete** (audited 2026-08-09)  
**Scope:** `crc_32`, `unicode.utf16.encode`, `unicode.cp866.encode`, `unicode.utf8.decode` only.  
**Next migration:** **not started.**

Evidence policy: [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Status

| Gate | Result |
|------|--------|
| FASM baseline | ✓ |
| Phase C Rust probe | ✓ |
| CRC32 Rust | ✓ |
| UTF-16 encode Rust | ✓ |
| CP866 encode Rust | ✓ |
| UTF-8 decode Rust | ✓ |
| Cut A audit / reproducibility | ✓ |
| Cut B (`cp866toUpper`) | ✓ — see [`cut-b-implementation.md`](cut-b-implementation.md) |
| Cut C (`utf16toUpper`) | ✓ — see [`cut-c-implementation.md`](cut-c-implementation.md) |
| Cut D (`strncmp`) | ✓ — see [`cut-d-implementation.md`](cut-d-implementation.md) |
| Cut E (`checksum_1`) | ✓ — see [`cut-e-implementation.md`](cut-e-implementation.md) |
| Cut F (`checksum_2`) | ✓ — see [`cut-f-implementation.md`](cut-f-implementation.md) |
| Cut G (`fsCalculateTime`) | ✓ — see [`cut-g-implementation.md`](cut-g-implementation.md) |
| Next cut | **not started** |
| Phase D (remove FASM bodies) | **not started** — originals kept under `USE_RUST_*=0` |

---

## Final architecture

```text
existing KolibriOS callers
        ↓
existing FASM ABI (unchanged)
        ↓
FASM trampoline (crc.inc / unicode.inc)
        ↓
relocation-free Rust machine-code blob (FASM `file`)
```

Build / embed pipeline:

```text
Rust source (kolibri_utils)
  ↓
freestanding i686 compilation (nightly + build-std)
  ↓
ELF32 object inside libkolibri_utils.a
  ↓
section extraction (exact name + symbol @0 + zero relocs)
  ↓
raw binary blob (*.bin)
  ↓
FASM `file` in kernel/rust/*.inc
  ↓
FASM ABI trampolines
  ↓
kernel.mnt → CoW floppy → QEMU
```

### Important: `rust-lld`

**`rust-lld` is NOT required for the four migrated Cut A functions.**  
They compile to dedicated `.text.*` sections with **zero relocations** and are embedded as raw bytes.

This does **not** mean `rust-lld` is unnecessary for future Rust work. Later functions may need:

* `.rodata` / GOTOFF
* internal cross-section references
* compiler-generated helpers with relocations
* complex data structures

Those cases must be evaluated independently (relocating link at the FASM placement VMA, or rewrite to reloc-free form).

---

## Four migrated functions

| Function | Section | Blob size | Relocs | Public ABI | Switch |
|----------|---------|-----------|--------|------------|--------|
| `crc_32` | `.text.rust_crc_32` | 226 B | 0 | stdcall stack + EAX partial; trampoline `ret` via `proc` (`ret 12` args + Rust `ret 16`) | `USE_RUST_CRC` |
| `unicode.utf16.encode` | `.text.rust_unicode_utf16_encode` | 85 B | 0 | EAX in/out; packed surrogates; `ret` | `USE_RUST_UTF16` |
| `unicode.cp866.encode` | `.text.rust_unicode_cp866_encode` | 294 B | 0 | EAX/AX in, AL out; `ret` | `USE_RUST_CP866` |
| `unicode.utf8.decode` | `.text.rust_unicode_utf8_decode` | 318 B | 0 | ESI/ECX in-out, EAX out; `ret` | `USE_RUST_UTF8` |

Rust stdcall bodies (embedded):

| Symbol | Callee stack cleanup |
|--------|----------------------|
| `rust_crc_32` | `ret 16` |
| `rust_unicode_utf16_encode` | `ret 4` |
| `rust_unicode_cp866_encode` | `ret 4` |
| `rust_unicode_utf8_decode` | `ret 8` |

FASM include wiring: [`kernel/kernel32.inc`](../../kernel/kernel32.inc) → `crc.inc` / `unicode.inc` + `rust/{crc,utf16,cp866,utf8,phase_c}.inc`.

---

## Rollback

Each migration is independently reversible:

```text
USE_RUST_CRC=0
USE_RUST_UTF16=0
USE_RUST_CP866=0
USE_RUST_UTF8=0
```

* Set in [`kernel/crc.inc`](../../kernel/crc.inc) / [`kernel/unicode.inc`](../../kernel/unicode.inc).
* Original FASM bodies remain in `else` branches (not deleted).
* Note: `kernel/rust/*.inc` still embeds the blobs and smoke helpers unless those includes/calls are also removed. Functional rollback of the **call path** does not require deleting embeds.

Verified (audit): all four switches `=0` assembles and boots to desktop; all four `=1` assembles and boots to desktop with smokes passing.

---

## Clean reproduce sequence

Required tools (this tree):

| Tool | Role |
|------|------|
| Rust stable | `cargo test -p kolibri_utils` |
| Rust nightly + `rust-src` | freestanding `build-std` for `i686-kolibri-none` |
| Python 3 | blob extractor |
| Vendored [`fasm/FASM.EXE`](../../fasm/FASM.EXE) | assemble `kernel.mnt` |
| [`tools/kolibri_img`](../../tools/kolibri_img) | CoW / delete / replace (protects reference image) |
| QEMU `qemu-system-i386` | boot smoke (often `C:\Program Files\qemu\…` on Windows) |
| PowerShell | build helpers under `rust_kernel/kolibri_utils/*.ps1` |

```powershell
# 1. Tests + freestanding blobs (all four + Phase C probe)
powershell -File rust_kernel/kolibri_utils/build-utf8.ps1

# 2. Assemble hybrid kernel (switches default to 1)
Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

# 3. Disposable image (never mutate the reference)
cd tools\kolibri_img
cargo build --release
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-a-boot.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-a-boot.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-a-boot.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

# 4. QEMU (adjust QEMU path as needed)
& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-a-boot.img -boot a -m 256 -vga std
```

Reference image SHA-256 (must remain unchanged):

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

---

## Kernel size / floppy fit

| Artifact | Size (audit) |
|----------|----------------|
| Hybrid `kernel.mnt` (all Rust on) | **223080** bytes (~218 KiB) |
| Rollback assemble (all Rust off; FASM bodies + still-embedded blobs) | **223336** bytes |
| Reference `KERNEL.MNT` (kerpack’d) | ~106618 bytes |

* **DOCPACK deletion** on the CoW floppy is still required to free clusters for the uncompressed hybrid kernel.
* **`kerpack`** is still **optional** / not vendored — useful later to shrink distribution images without deleting files.
* Current workflow fits on the 1.44 MiB FAT12 reference layout after `DOCPACK` delete.

---

## Extractor guarantees

[`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py):

* ELF32 / `EM_386` only
* exact section name (no silent fallback to another section)
* reject duplicate section names
* require `SHT_PROGBITS`
* reject any REL/RELA targeting the section
* require named symbol in that section at offset 0
* reject symbol smaller than section
* optional `--expect-ret-imm` for stdcall epilogue presence

Determinism (same nightly + same source, clean rebuild): **byte-identical** blobs observed in the Cut A audit.

---

## Tests (audit baseline)

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **27/27** |
| Differential oracles (CRC + UTF-16 exhaustive + CP866 exhaustive + UTF-8 exhaustive/corpus) | Pass |
| In-kernel smokes (Phase C + CRC + UTF-16 + CP866 + UTF-8) | Pass (hang-on-fail; desktop reached) |
| QEMU all Rust on | Desktop |
| QEMU all Rust off | Desktop |

---

## Remaining issues

**Required for next migration (not Cut A):**

* Decide link strategy per future function: reloc-free extract vs `rust-lld` at fixed VMA vs rewrite.
* Plan Cut B (or next dependency cut) separately after reviewing Cut A lessons.

**Optional future improvement:**

* Host binary differential vs assembled FASM (oracle is algorithm-level today).
* Vendor or document `kerpack` for compressed floppy kernels.
* Gate / remove temporary smoke hang-on-fail helpers once confidence is permanent.
* Optionally skip embedding Rust blobs when the matching `USE_RUST_*=0` (smaller rollback builds).

---

## Related docs

* Per-function notes: [`crc32-migration.md`](crc32-migration.md), [`utf16-migration.md`](utf16-migration.md), [`cp866-migration.md`](cp866-migration.md), [`utf8-migration.md`](utf8-migration.md)
* Design inventory: [`cut-a-implementation.md`](cut-a-implementation.md)
* Phase C probe: [`phase-c-integration.md`](phase-c-integration.md)
* Repo layout: [`../_meta/project-structure.md`](../_meta/project-structure.md)
