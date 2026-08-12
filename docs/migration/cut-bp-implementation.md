# Cut BP Implementation — `UTF16to8_string`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bp-plan.md`](cut-bp-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BP** |
| FASM symbol | `UTF16to8_string` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 5 live (`fat.inc`, `ntfs.inc`, `exfat.inc` ×2, `fs_lfn.inc`) |
| Rust symbol | `rust_utf16_to_8_string` |
| Pure helper | `kolibri_utils::utf16_to_8` (loop + encode inlined in FFI) |
| Subsystem | FS / Unicode string streaming encode |
| Stage | Stage-2 Path B (broadens Cut Q char leaf) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — Unicode string wrapper does not establish Rust-owned subsystem
state. Cut Q + Cut BP remain complementary Path B leaves.

Selected `UTF16to8_string` over `fix_coff_symbols`, `ext_write_time`, `ahci_port_wait`,
and `tcp_mss` for new string-scale semantic class + five cross-FS callers.

---

## Candidate comparison (post-BO audit)

| Rank | Candidate | Outcome |
| ---- | --------- | ------- |
| 1 | `UTF16to8_string` | **SELECT** — UTF-16 string streaming encode |
| 2 | `cp866toUTF8_string` | Defer — wrapper / fewer callers |
| 3 | `fix_coff_symbols` | Defer — PE deepen |
| 4 | `ext_write_time` | Defer — no `--disk ext` |
| 5 | `ahci_port_wait` | Defer — AHCI deepen |

---

## Legacy ABI

```text
register call UTF16to8_string
in:  ESI -> UTF-16 string
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

* Loop starts with implicit `xor eax,eax` equivalent (first `lodsw` after zero init).
* `lodsw` updates only AX; high `EAX` bits accumulate across iterations.
* Each code unit uses Cut Q `UTF16to8` semantics (including INT_MIN ECX escape).
* Overflow returns via `js` with failing code unit restored in `EAX`.
* NUL wchar returns with `test eax,eax` → ZF=1 after encoding NUL byte.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_utf16_to_8_string` |
| Blob | **275** bytes, **0 relocations** |
| SHA-256 | `6DF852C95CF03C7E6FD5E30F21F070410CD038AE08052C94BFC476C91ABB077E` |
| Trampoline | `kernel/fs/parse_fn.inc` under `USE_RUST_UTF16_TO_8_STRING` |
| Gate | `USE_RUST_UTF16_TO_8_STRING` (prod 1) |
| Rust ABI | `stdcall rust_utf16_to_8_string(src, dest, ecx, src_out, dest_out, ecx_out); ret 24` |
| Encode | Inlined `utf16_to_8` inside FFI (no call to `rust_utf16_to_8` blob) |

Trampoline note: out-pointer addresses use `sub esp,12` + `lea` slots (not `push` +
`esp+4` stdcall args) to avoid stack skew during argument push.

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow loop oracle (`lodsw` + per-char encode) separate from `utf16_to_8_string` |
| Host tests | **PASS** — `580/580` (includes 4 Cut BP tests + 50k PRNG) |
| Seed | `0x43555042` (`'CUPB'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `utf16_to_8_string_rust_smoke_test` | **PASS** |
| Marker | `rust_utf16_to_8_string_smoke_result = 'UTBP'` |
| Coverage | public `UTF16to8_string` NUL + overflow vectors; `EBX`/`EDX` canaries; buffer mutation |
| Live state | isolated synthetic `iglobal` src/buf only (REG-003 safe) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` | **OK** (`running`, 779380 non-black) | `dev_build/bp-off.ppm` |
| ON | `USE_RUST_UTF16_TO_8_STRING=1` | **OK** (`running`, 779380 non-black) | `dev_build/bp-on.ppm` |

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
| Scripted exFAT LFN / NTFS filename walk | **NOT AVAILABLE** |

Precision: callers include exFAT volume-label and FAT/NTFS/LFN name paths, but no
scripted browse harness exists beyond attach-only boot smoke.

---

## Regressions

| Item | Result |
|------|--------|
| Live regressions discovered | **none** |
| Regression-log entry | none |

Pre-ship: smoke overflow vector had wrong `ESI`/`EDI` expectations and trampoline
used unsafe `esp+N` stdcall out-args — fixed before closure (fixture/trampoline
defects, not production leaf bugs; REG-003 class).

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_UTF16_TO_8_STRING = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/test/kernel-20260812-105024.img` |
| Rollback | `USE_RUST_UTF16_TO_8_STRING = 0` or `[[rust.migrations]]` `cut = "BP"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/utf16_to_8.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `kernel/rust/utf16_to_8_string.inc`
* `kernel/fs/parse_fn.inc`
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/cut-bp-plan.md`
* `docs/migration/cut-bp-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/migration-todo.md`
* `docs/migration/boundaries.md`

## Known limitations

* String loop only — does not migrate `cp866toUTF8_string` wrapper (separate candidate).
* No scripted exFAT/NTFS filename regression walk (attach-only soak).
* Host differential compares flags/buffers; `ESI`/`EDI` pointer compares are `#cfg(32)` only.

---

## Inventory

**71 / 135** — one new `[x]` (`UTF16to8_string`).
