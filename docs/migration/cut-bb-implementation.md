# Cut BB Implementation — `strrchr`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bb-plan.md`](cut-bb-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `strrchr` |
| Source | [`kernel/core/string.inc`](../../kernel/core/string.inc) |
| Callers | 1 kernel (`fs_execute` / process name) + PE export `strrchr` |
| Rust symbol | `rust_strrchr` |
| Pure helper | `kolibri_utils::strrchr` |
| Subsystem | core string reverse character search |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BA audit: XFS/NTFS/network/AHCI/PE/PCI/Stage-3 Path A
still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. BA #2 `strtoint_dec` is **dead** (`conf_lib.inc` not linked).
Selected **`strrchr`** — new reverse-search algorithm in `core/string.inc`
(not Cut D compare deepen); clean stdcall; excellent differential; exercised
on every `fs_execute` / app launch when building `APPDATA.appname`.

REG-001: trampoline preserves **EDX** (FASM leaf never touched it).

REG-003: ABI smoke uses **iglobal synthetic C strings only** — never mutates
live `path_string` / process slots.

Reloc-free note: an early draft returned `*mut u8` and failed to inline into
the `link_section`, emitting PIC + call relocs. Production helper returns
`usize` address (`0` = NULL) and inlines into `.text.rust_strrchr`.

---

## Candidate comparison (post-BA audit)

| Candidate | Outcome |
|-----------|---------|
| `strrchr` | **Selected** — reverse char search |
| `strtoint_dec` | Reject — not linked |
| `fat_name_is_legal` | #2 — mild FAT deepen + charset table |
| `tcp_outflags` | #3 — mild M/V TCP deepen |
| `get_proc_ex` | #4 — PE ban stretch |
| `is_string_userspace` | #5 — thin P sibling |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_STRRCHR=0`:

```text
stdcall strrchr(s, c)
out: EAX = pointer to last byte (c as u8), or NULL
preserves: EDX, EBX, ESI, EDI, EBP
clobbers: ECX, flags
leaves: DF = 0 (explicit cld)
ret 8
```

Quirks retained:

* Only low 8 bits of `c` participate (`scasb` / `AL`)
* `c == 0` returns pointer to the terminating NUL
* Empty string + non-NUL needle → NULL

---

## Rust ABI

```text
stdcall rust_strrchr(s, c) → EAX = address or 0
  ret 8
```

Trampoline: `push edx` / `stdcall rust_strrchr` / `pop edx` / `cld`.

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_strrchr` |
| Size | **84 bytes** |
| Relocations | **0** |
| SHA-256 | `5604785C86EF8B0AB38A37E1EC0911C25C38B420190D925144EF765ECC073F23` |
| Epilogue | `ret 8` (`c2 08 00`) |

---

## Differential tests

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle | **PASS** |
| Named vectors (empty / path / first-mid-last / wide `c`) | **PASS** |
| 50k PRNG seed `0x43554242` (`CUBB`) | **PASS** |

---

## ABI smoke

| Check | Result |
|-------|--------|
| Marker | `STRR` (`rust_strrchr_smoke_result`) |
| Direct `rust_strrchr` vectors | **PASS** |
| Public trampoline + EDX/EBX/ESI/EDI/EBP canaries | **PASS** |
| Live process/path mutation | **none** (synthetic iglobals only) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_STRRCHR=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_STRRCHR=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — same 779380 non-black; 16 differing bytes (≈0.0007%, clock/timing noise) |
| Desktop app-launch path | **PASS** both OFF and ON (`fs_execute` → `strrchr`) |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Process create / `fs_execute` path name extract (desktop boots and launches `/sys` apps) | **PASS** |
| FS `--disk` soak | **NOT REQUIRED** for this leaf (not an FS algorithm) |
| PE-export `strrchr` consumer matrix | **NOT AVAILABLE** (no dedicated driver soak harness) |

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
| Production gate | `USE_RUST_STRRCHR = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bb-final.img` |
| Rollback | `USE_RUST_STRRCHR = 0` or `[[rust.migrations]]` `cut = "BB"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/string.rs` (`strrchr` + oracle + 50k tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/core/string.inc` (gate + trampoline)
* `kernel/rust/strrchr.inc` (new)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-bb-plan.md`
* `docs/migration/cut-bb-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/migration-todo.md`

---

## Known limitations

* Does not migrate `strchr` / `strncpy` / `strlen` / `strnlen`.
* No Path A claim for core/string or process-create ownership.
* PE-export consumers not separately soaked beyond desktop boot.
* Stop; do not start Cut BC.
