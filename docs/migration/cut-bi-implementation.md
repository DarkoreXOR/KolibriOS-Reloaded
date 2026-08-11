# Cut BI Implementation — `iso9660_copy_name`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bi-plan.md`](cut-bi-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `iso9660_copy_name` |
| Source | [`kernel/fs/iso9660.inc`](../../kernel/fs/iso9660.inc) |
| Callers | 1 live (`iso9660_GetFileInfo` `.rootdir` volume name) |
| Rust symbol | `rust_iso9660_copy_name` |
| Pure helper | `kolibri_utils::iso9660_copy_name` |
| Composes | Cut A/AN `cp866_encode`/`cp866_decode` + Cut Q `utf16_to_8` (inlined) |
| Subsystem | ISO9660 / volume-name encoding copy |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BH audit: XFS/NTFS/network/AHCI/PE/FAT/string/HID/Stage-3
Path A still fail the raised bar; AJ+copy is still two leaves ≠ ISO ownership.
Ban-listed `uni2ansi_char` / deferred `cp866toUTF8_string` are **inlined** via
existing Rust helpers — not cut as production symbols.

Selected **`iso9660_copy_name`** — volume-name encoding dispatch (ASCII/UCS-2 ×
cp866/utf16/utf8) + REG-002 NUL terminate; one live GetFileInfo caller; real
`--disk iso9660` soak. Preferred over thin `is_string_userspace`, Stage-4
address-math `v86_get_lin_addr`, PE-thin `coff_get_align`, and trivial
`ahci_is_sig_known`. HID `set_mouse_data` remains deferred (side-effects).

REG-001: trampoline preserves **EBP**; updates **ESI/EDI** via inout; restores
EAX/EBX for canary hygiene (legacy clobbers them).

REG-002: terminator is **byte** NUL for ASCII volumes, **word** NUL for UCS-2.

REG-003: ABI smoke uses **iglobal synthetic VolumeName/dest/ISO context only**.

---

## Candidate comparison (post-BH audit)

| Candidate | Outcome |
|-----------|---------|
| `iso9660_copy_name` | **Selected** — encoding dispatch + `--disk iso9660` |
| `is_string_userspace` | #2 — thin P sibling |
| `v86_get_lin_addr` | #3 — Stage-4 address math |
| `coff_get_align` | #4 — PE thin |
| `ahci_is_sig_known` | #5 — trivial CMP |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_ISO9660_COPY_NAME=0`:

```text
register iso9660_copy_name
in:  ESI → source name (ASCII or UCS-2 BE VolumeName)
     EDI → dest bdfe.name
     ECX = max byte length (caller: 32)
     EDX = nameenc (1=cp866, 2=utf16, 3=utf8; other → NUL only)
     EBP → ISO9660* (reads type_encoding)
out: EDI → terminator position
     byte[edi]=0; if type_encoding≠0 also word[edi]=0
preserves: EBP
clobbers: EAX, EBX, ECX, EDX, ESI, flags
DF: assumed 0 (lods/stos/movs); Rust path DF-agnostic
plain ret
```

Quirks retained:

* UCS-2 path `shr ecx,1` before char loops
* ASCII→UTF8 uses `cp866toUTF8_string` control flow (may read one byte past
  `max_len` when the window has no embedded NUL; UTF16to8 may store the NUL
  code unit before `.end_copy_name` writes another terminator)
* Invalid `nameenc` → terminator only

---

## Rust ABI

```text
stdcall rust_iso9660_copy_name(esi_inout, edi_inout, max_len, nameenc, type_encoding)
  ret 20
updates *esi_inout / *edi_inout (EDI at terminator)
```

Trampoline: push EAX/EBX/ECX/EDX/ESI/EDI; pass `&ESI`, `&EDI`, ECX, EDX,
`[ebp+type_encoding]`; pop updated EDI/ESI then restore EAX/EBX/ECX/EDX.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `iso9660_copy_name.rs` + `ffi.rs` section `.text.rust_iso9660_copy_name` |
| Extract | `extract_reloc_free_text.py` → `rust_iso9660_copy_name.bin` |
| Embed | `kernel/rust/iso9660_copy_name.inc` `file` directive |
| Trampoline | `iso9660.inc` under `USE_RUST_ISO9660_COPY_NAME` |
| Gate | `USE_RUST_ISO9660_COPY_NAME` (prod 1) |
| Smoke | `iso9660_copy_name_rust_smoke_test` (early init with AJ smoke) |

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_iso9660_copy_name` |
| Size | **1295 bytes** |
| Relocations | **0** |
| SHA-256 | `574BEE33A45D8FA5856A0CAD7CAC5D7DCB240F1EF153A027F60E6223F267EA94` |
| Epilogue | `ret 20` (`c2 14 00`) present (shared epilogue; cold tails after) |

Size reflects inlined CP866 encode/decode + UTF16to8 (reloc-free compose).

---

## Differential

| Item | Result |
|------|--------|
| Host `cargo test` | **PASS** (Cut BI suite included) |
| Independent oracle | FASM-flow dispatch (not a call to the SUT); composes shared encode helpers for char maps |
| Coverage | all 6 encoding paths; invalid nameenc; ASCII/UCS-2 NUL width; **50k PRNG** seed `0x43554249` (`'CUBI'`) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `iso9660_copy_name_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | ASCII→cp866; ASCII→utf16; UCS-2→cp866+word NUL; invalid nameenc; EBP + EAX/EBX canaries |
| Marker | `rust_iso9660_copy_name_smoke_result = 'ICPN'` on success |
| Live state | Synthetic VolumeName/dest/ISO context only (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_ISO9660_COPY_NAME=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body; `--disk iso9660` |
| ON | `USE_RUST_ISO9660_COPY_NAME=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate; `--disk iso9660` |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — 72 differing bytes (clock/timer noise; same non-black count 779380) |
| Desktop boot + CD | **PASS** both OFF and ON |
| Prior image | `dev_build/cut-bh-final.img` retained as baseline |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| `--disk iso9660` (ATAPI `-cdrom`) | **PASS** — CD attached; ISO stack live; OFF/ON desktop non-black match |
| Volume GetFileInfo with `nameenc≠0` | **PARTIAL** — live caller path; not separately automated beyond CD attach + ABI smoke. Directory name compare remains Cut AJ. |

---

## Regressions

| Item | Result |
|------|--------|
| Regressions discovered | **none** |
| Regression log entry | N/A (no live regression) |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_ISO9660_COPY_NAME = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bi-final.img` |
| Rollback | `USE_RUST_ISO9660_COPY_NAME = 0` or `[[rust.migrations]]` `cut = "BI"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/iso9660_copy_name.rs` — leaf + differential tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_iso9660_copy_name`
* `rust_kernel/kolibri_utils/src/lib.rs` — module + export
* `kernel/rust/iso9660_copy_name.inc` — blob embed + smoke result
* `kernel/fs/iso9660.inc` — trampoline + `USE_RUST_ISO9660_COPY_NAME` + ABI smoke
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call
* `project/build.toml` — blob + migration BI
* `docs/migration/cut-bi-plan.md`
* `docs/migration/cut-bi-implementation.md`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Does not migrate `uni2ansi_char` / `cp866toUTF8_string` as production symbols
  (inlined only inside this leaf).
* Does not claim ISO Path A / mount/read ownership.
* Volume-label GetFileInfo with encoding flags is not a separate automated UI
  soak beyond `--disk iso9660` desktop + synthetic ABI smoke.
* DF=1 FASM path would reverse string ops; callers use DF=0; Rust ignores DF.

**Stop; do not start Cut BJ.**
