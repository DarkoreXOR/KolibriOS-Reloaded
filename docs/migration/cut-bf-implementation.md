# Cut BF Implementation — `strncpy`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bf-plan.md`](cut-bf-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `strncpy` |
| Source | [`kernel/core/string.inc`](../../kernel/core/string.inc) |
| Callers | 1 kernel (`shmem_open` / `heap.inc`) + PE export `strncpy` |
| Rust symbol | `rust_strncpy` |
| Pure helper | `kolibri_utils::strncpy` |
| Subsystem | core string bounded padded copy |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BE audit: XFS/NTFS/network/AHCI/PE/FAT/Stage-3/HID
Path A still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. Selected **`strncpy`** — first mutating **bounded padded-copy**
string class (vs D compare / BB reverse search); live `shmem_open` caller + PE
export; excellent independent oracle; desktop shmem path.

REG-001 / Cut BE lesson: trampoline must **not** invent EDX preserve — legacy
FASM **clobbers ECX/EDX**. Preserve **ESI/EDI/EBX/EBP**; restore **DF** via `cld`.

REG-003: ABI smoke uses **iglobal synthetic dst/src only** — never mutates live
`shmem_list` / SMEM nodes.

Reloc-free note: an early pad loop was outlined to `memset` (GOTPC+PLT relocs).
Production uses a single forward pass with `write_volatile` so the extractor
accepts a reloc-free blob.

---

## Candidate comparison (post-BE audit)

| Candidate | Outcome |
|-----------|---------|
| `strncpy` | **Selected** — bounded padded copy |
| `set_mouse_data` | #2 — HID deepen after L+BE; side-effect heavy |
| `strlen` | #3 — EXT-only; no `--disk ext` |
| `iso9660_copy_name` | #4 — AJ glue + `uni2ansi` ban |
| `swap_bytes_in_words` | #5 — AV deepen |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_STRNCPY=0`:

```text
stdcall strncpy(s1, s2, n)
out: EAX = s1
preserves: ESI, EDI, EBX, EBP
clobbers: ECX, EDX, flags
leaves: DF = 0 (explicit cld)
ret 12
```

Quirks retained:

* Always writes exactly `n` bytes (copy including NUL when within `n`, then pad)
* If no NUL in first `n` source bytes → copy `n` bytes with **no** terminator
* `n == 0` → no write; return `s1`

---

## Rust ABI

```text
stdcall rust_strncpy(s1, s2, n) → EAX = s1
  ret 12
```

Trampoline: `stdcall rust_strncpy` / `cld` (no EDX push — legacy clobbers EDX).

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_strncpy` |
| Size | **156 bytes** |
| Relocations | **0** |
| SHA-256 | `40254FA89E31A550C8676D3F030EA72DD87379C645A28990DEDC5D3883EC73F5` |
| Epilogue | `ret 12` (`c2 0c 00`) |

---

## Differential tests

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle | **PASS** |
| Named vectors (n=0 / pad / trunc / shmem 31) | **PASS** |
| 50k PRNG seed `0x43554246` (`CUBF`) | **PASS** |
| Host suite (all `kolibri_utils` tests) | **PASS** (520 passed) |

---

## ABI smoke

| Check | Result |
|-------|--------|
| Marker | `SNCP` (`rust_strncpy_smoke_result`) |
| Direct `rust_strncpy` vectors | **PASS** |
| Public trampoline + ESI/EDI/EBX/EBP canaries | **PASS** |
| ECX/EDX preserve | **not asserted** (legacy clobber; Cut BE lesson) |
| Live shmem mutation | **none** (synthetic iglobals only) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_STRNCPY=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_STRNCPY=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — same 779380 non-black; **0** differing bytes |
| Desktop boot path | **PASS** both OFF and ON |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Desktop boot (kernel ABI smoke + shmem symbol present) | **PASS** |
| Explicit shared-memory create/open exercise | **PARTIAL / NOT AVAILABLE** — no dedicated shmem soak harness; live caller is `shmem_open` (31-byte name window covered by smoke + differentials) |
| PE-export `strncpy` consumer matrix | **NOT AVAILABLE** |
| FS `--disk` soak | **NOT REQUIRED** for this leaf (not an FS algorithm) |

---

## Regressions

| Item | Result |
|------|--------|
| Regressions discovered | **none** |
| Regression log entry | N/A (no live regression) |
| Validation corrections | Early blob rejected (`memset` PLT); fixed with `write_volatile` before closure. Not a REG-* (did not ship). |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_STRNCPY = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bf-final.img` |
| Rollback | `USE_RUST_STRNCPY = 0` or `[[rust.migrations]]` `cut = "BF"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/string.rs` (`strncpy` + oracle + 50k tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/core/string.inc` (gate + trampoline)
* `kernel/rust/strncpy.inc` (new)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-bf-plan.md`
* `docs/migration/cut-bf-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/migration-todo.md`

---

## Known limitations

* Does not migrate `strchr` / `strnlen` / `strncat` / `strlen`.
* No Path A claim for core/string or shmem ownership.
* Reloc-free requires avoiding LLVM `memset` outlining (`write_volatile` stores).
* Dedicated shmem create/open soak harness not available — report PARTIAL.
* Stop; do not start Cut BG.
