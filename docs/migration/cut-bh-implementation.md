# Cut BH Implementation — `strlen`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bh-plan.md`](cut-bh-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `strlen` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 2 kernel (`ext.inc` `linkInode` / dirent name paths) |
| Rust symbol | `rust_strlen` |
| Pure helper | `kolibri_utils::strlen` |
| Subsystem | C-string length (`scasb` / EXT name) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BG audit: XFS/NTFS/network/AHCI/PE/FAT/Stage-3/HID
Path A still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. Selected **`strlen`** — first **NUL-terminated length / scasb**
class (vs D compare / BB reverse / BF pad-copy); two live EXT callers; excellent
independent oracle; reloc-free trivial. Soak honestly **NOT AVAILABLE** (no
`--disk ext`). Preferred over side-effect-heavy `set_mouse_data`, ISO glue+ban,
and post-BG AHCI-trivial `ahci_is_sig_known`.

REG-001: trampoline restores **EAX/EBX/EDX/ESI/EDI/EBP**; **ECX = length**
(legacy restores EAX/EDI; leaves ESI/EBX/EDX/EBP alone; Rust stdcall returns
length in EAX).

REG-003: ABI smoke uses **iglobal synthetic C strings only** — never mutates
live EXT inode/name buffers.

DF: legacy has **no `cld`/`std`** — trampoline leaves DF unchanged (smoke
verifies DF=1 survives across empty-string call).

---

## Candidate comparison (post-BG audit)

| Candidate | Outcome |
|-----------|---------|
| `strlen` | **Selected** — length/`scasb` |
| `set_mouse_data` | #2 — HID deepen; side-effect heavy |
| `iso9660_copy_name` | #3 — AJ glue + `uni2ansi` ban |
| `ahci_is_sig_known` | #4 — trivial CMP; AHCI stack after BG |
| `v86_get_lin_addr` | #5 — Stage-4 address math |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_STRLEN=0`:

```text
register strlen
in:  ESI → NUL-terminated C string
out: ECX = length (bytes before NUL)
preserves: EAX, EDI (push/pop); ESI, EBX, EDX, EBP (untouched)
clobbers: flags
DF: unchanged (no cld/std)
plain ret
```

Quirk: algorithm is `or ecx,-1` / `repnz scasb` / `inc ecx` / `not ecx`
(empty → 0). DF is caller-dependent; Rust path is DF-agnostic.

---

## Rust ABI

```text
stdcall rust_strlen(s) -> EAX = length
  ret 4
```

Trampoline: push EAX/EBX/EDX/ESI/EDI/EBP → `stdcall rust_strlen, esi` →
`mov ecx, eax` → pop restore (EAX restored after length moved to ECX).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `string.rs` `strlen` + `ffi.rs` section `.text.rust_strlen` |
| Extract | `extract_reloc_free_text.py` → `rust_strlen.bin` |
| Embed | `kernel/rust/strlen.inc` `file` directive |
| Trampoline | `parse_fn.inc` under `USE_RUST_STRLEN` |
| Gate | `USE_RUST_STRLEN` (prod 1) |
| Smoke | `strlen_rust_smoke_test` (early init with other string smokes) |

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_strlen` |
| Size | **28 bytes** |
| Relocations | **0** |
| SHA-256 | `7214EB80EF6C0662509B06EF59C5F900ADDE67355DFFA94D78B04C887B09CEF3` |
| Epilogue | `ret 4` (`c2 04 00`) |

---

## Differential

| Item | Result |
|------|--------|
| Host `cargo test` | **PASS** (Cut BH suite included) |
| Independent oracle | FASM-flow `or ecx,-1` / scasb / `inc` / `not` (not derived from Rust body) |
| Coverage | empty; short; path; binary/high bytes; **50k PRNG** seed `0x43554248` (`'CUBH'`) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `strlen_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | rust_* empty/3/17; public empty+abc+path + EAX/EBX/EDX/ESI/EDI/EBP canaries; DF=1 preserve on empty |
| Marker | `rust_strlen_smoke_result = 'STRL'` on success |
| Live state | Synthetic `strlen_smoke_*` strings only (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_STRLEN=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body; desktop |
| ON | `USE_RUST_STRLEN=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — 54 differing bytes (clock/timer noise; same non-black count 779380) |
| Desktop boot | **PASS** both OFF and ON |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| EXT dirent / `linkInode` name length | **NOT AVAILABLE** — no `--disk ext` in `scripts/run_qemu.py` |
| Desktop boot (smoke + kernel linked with EXT) | **PARTIAL** — production symbol exercised only via ABI smoke; EXT callers not hit without an EXT volume |

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
| Production gate | `USE_RUST_STRLEN = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bh-final.img` |
| Rollback | `USE_RUST_STRLEN = 0` or `[[rust.migrations]]` `cut = "BH"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/string.rs` — `strlen` + differential tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_strlen`
* `rust_kernel/kolibri_utils/src/lib.rs` — export
* `kernel/rust/strlen.inc` — blob embed + ABI smoke
* `kernel/fs/parse_fn.inc` — trampoline + `USE_RUST_STRLEN`
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call
* `project/build.toml` — blob + migration BH
* `docs/migration/cut-bh-plan.md`
* `docs/migration/cut-bh-implementation.md`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* No `--disk ext` soak — EXT production callers not exercised in QEMU attach path.
* DF leave-alone matches legacy; callers that enter with DF=1 and non-empty strings
  on the FASM rollback path are undefined (same as upstream).

---

## Updated inventory

**Functions completed / functions total: `63 / 135`**
