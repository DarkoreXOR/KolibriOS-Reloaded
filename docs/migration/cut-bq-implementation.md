# Cut BQ Implementation — `cp866toUTF8_string`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bq-plan.md`](cut-bq-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BQ** |
| FASM symbol | `cp866toUTF8_string` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 3 live (`fs_lfn.inc` ×2, `iso9660.inc` ×1) |
| Rust symbol | `rust_cp866_to_utf8_string` |
| Pure helper | `kolibri_utils::cp866_to_utf8_string` (AN+Q loop inlined in FFI) |
| Subsystem | FS / Unicode string streaming encode |
| Stage | Stage-2 Path B (broadens Cut AN + Q char leaves) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — CP866 string wrapper does not establish Rust-owned subsystem
state. Cut AN + Q + BP + BQ remain complementary Path B leaves.

Selected `cp866toUTF8_string` over `fix_coff_symbols`, `ext_write_time`, and
`fsGetTime` for new CP866 string-scale semantic class + three live callers.

---

## Candidate comparison (post-BP audit)

| Rank | Candidate | Outcome |
| ---- | --------- | ------- |
| 1 | `cp866toUTF8_string` | **SELECT** — CP866 string streaming encode |
| 2 | `fix_coff_symbols` | Defer — PE deepen |
| 3 | `ext_write_time` | Defer — no `--disk ext` |
| 4 | `ext_read_all_times` | Defer — AL compose |
| 5 | `fsGetTime` | Defer — CMOS/calendar caution |

---

## Legacy ABI

```text
register call cp866toUTF8_string
in:  ESI -> CP866 string
     EDI -> UTF-8 buffer
     ECX -> signed byte budget
out: SF=1 overflow; ZF=1 on NUL wchar
     ESI/EDI/ECX advanced
preserves: EBX, EDX, EBP
clobbers: EAX, ESI, EDI, ECX, flags
stack: plain ret
DF: unchanged
```

Quirks retained:

* Per-byte loop: `lodsb` → `ansi2uni_char` → push eax → `UTF16to8` → pop eax.
* `test eax,eax` uses **original** Unicode codepoint (pushed value), not encode residue.
* May read one source byte past a fixed window when no embedded NUL (FASM overread;
  shared with Cut BI nameenc=3 path).
* Overflow returns via `js` with original Unicode in `EAX`.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_cp866_to_utf8_string` |
| Blob | **267** bytes, **0 relocations** |
| SHA-256 | `27F934B9ECA4A2860CB545E3A3E3B868B4D779A547B1EFEF7CE8CD37C790EF74` |
| Trampoline | `kernel/fs/parse_fn.inc` under `USE_RUST_CP866_TO_UTF8_STRING` |
| Gate | `USE_RUST_CP866_TO_UTF8_STRING` (prod 1) |
| Rust ABI | `stdcall rust_cp866_to_utf8_string(src, dest, ecx, src_out, dest_out, ecx_out); ret 24` |
| Encode | Inlined `cp866_decode` + `utf16_to_8` inside FFI (no cross-blob calls) |

Trampoline note: same packed SF/ZF/EAX model as Cut BP; out-pointer slots via
`sub esp,12` + `lea` (not `push` + `esp+N` stdcall args).

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow loop (`lodsb` + `cp866_decode` + per-char `utf16_to_8`) |
| Host tests | **PASS** — `585/585` (includes 5 Cut BQ tests + 50k PRNG) |
| Seed | `0x43555051` (`'CUPQ'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `cp866_to_utf8_string_rust_smoke_test` | **PASS** |
| Marker | `rust_cp866_to_utf8_string_smoke_result = 'UTBQ'` |
| Coverage | public `cp866toUTF8_string` NUL + overflow vectors; `EBX`/`EDX` canaries; buffer mutation |
| Live state | isolated synthetic `iglobal` src/buf only (REG-003 safe) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` | **OK** (`running`, 779380 non-black) | `dev_build/bq-off.ppm` |
| ON | `USE_RUST_CP866_TO_UTF8_STRING=1` | **OK** (`running`, 779380 non-black) | `dev_build/bq-on.ppm` |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |
| OFF vs ON attach-only exFAT secondary disk | **PASS** — both `running`, 779380 non-black |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Attach-only exFAT secondary disk (`images/exfat-image.img`) | **PASS** — OFF/ON desktop equivalence |
| ISO9660 volume-name ASCII→UTF8 (`--disk iso9660`) | **NOT AVAILABLE** — no `images/iso9660-image.*` |
| LFN mount_additional_directory CP866 path | **NOT AVAILABLE** — no scripted harness |
| Scripted exFAT/NTFS/LFN filename walk | **NOT AVAILABLE** |

Precision: callers include ISO9660 volume-name (nameenc=3) and LFN set/mount paths,
but no `--disk iso9660` image or scripted browse harness exists beyond attach-only boot smoke.

---

## Regressions

| Item | Result |
|------|--------|
| Live regressions discovered | **none** |
| Regression-log entry | none |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_CP866_TO_UTF8_STRING = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/test/kernel-20260812-110136.img` |
| Rollback | `USE_RUST_CP866_TO_UTF8_STRING = 0` or `[[rust.migrations]]` `cut = "BQ"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/cp866_to_utf8_string.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `kernel/rust/cp866_to_utf8_string.inc`
* `kernel/fs/parse_fn.inc`
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/cut-bq-plan.md`
* `docs/migration/cut-bq-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/migration-todo.md`
* `docs/migration/boundaries.md`

## Known limitations

* String loop only — does not migrate `uni2ansi_char` / `UTF16to8_string` siblings.
* FASM overread quirk when no embedded NUL is retained but not host-differential tested
  (UB-safe oracle requires explicit terminator in test buffers).
* No `--disk iso9660` soak (image missing); LFN mount path **NOT AVAILABLE**.
* Host differential compares flags/buffers; `ESI`/`EDI` pointer compares are `#cfg(32)` only.

---

## Inventory

**72 / 135** — one new `[x]` (`cp866toUTF8_string`).
