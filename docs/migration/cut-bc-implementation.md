# Cut BC Implementation — `fat_name_is_legal`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bc-plan.md`](cut-bc-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fat_name_is_legal` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 1 (`fat_CreateFile` / `.notfound` short-name generate) |
| Rust symbol | `rust_fat_name_is_legal` |
| Pure helper | `kolibri_utils::fat_name::fat_name_is_legal` |
| Subsystem | FAT LFN charset legality |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BB audit: XFS/NTFS/network/AHCI/PE/FAT/string Path A
still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. `strchr` is export-only (no kernel callers). Selected
**`fat_name_is_legal`** — new LFN charset-validate semantic (bit0 of
`fat_legal_chars`), distinct from Cuts U/K short-name generation and AO time.

REG-001: trampoline preserves **ECX** and **EDX**.

REG-003: ABI smoke uses **iglobal synthetic C strings only**.

Reloc-free note: legality uses an if/else predicate (no `.rodata` table, no
`match` jump tables — Cut AZ lesson).

---

## Candidate comparison (post-BB audit)

| Candidate | Outcome |
|-----------|---------|
| `fat_name_is_legal` | **Selected** — LFN charset validate |
| `strchr` | Reject — PE export only, no kernel callers |
| `tcp_outflags` | #2 — mild M/V TCP deepen |
| `swap_bytes_in_words` | #3 — AV-adjacent endian thin |
| `get_proc_ex` | #4 — PE ban stretch |
| `is_string_userspace` | #5 — thin P sibling |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_FAT_NAME_IS_LEGAL=0`:

```text
call fat_name_is_legal
in:  ESI → NUL-terminated UTF-8 (byte) name
out: CF=1 legal, CF=0 illegal
preserves: ESI, EBX, ECX, EDX, EDI, EBP
clobbers: EAX, flags
ret (plain)
```

Quirks retained:

* High-bit bytes (`AL` SF) skip the table and continue (UTF-8 multi-byte)
* Table bit0: values `1` (LFN-only) and `3` (both) accept; `0` rejects
* Space (`0x20`) is LFN-legal (table value `1`) — first printable-row entry
* `"` / `*` / `/` / `|` / `?` reject; `{` / `}` / `~` accept
* Empty string (immediate NUL) → legal (`stc`)

---

## Rust ABI

```text
stdcall rust_fat_name_is_legal(name) → EAX = 1 legal / 0 illegal
  ret 4
```

Trampoline: `push ecx` / `push edx` / `stdcall rust_…, esi` / `test eax,eax` /
`pop edx` / `pop ecx` / `jz → clc` else `stc`.

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_fat_name_is_legal` |
| Size | **168 bytes** |
| Relocations | **0** |
| SHA-256 | `E0A816CA13731FD81586CD7AE91C2D39B9BB050CD3AC636189712684DA4E8AAC` |
| Epilogue | `ret 4` (`c2 04 00`) |

---

## Differential tests

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle (full table bit0) | **PASS** |
| Predicate exhaust 0..127 vs table | **PASS** |
| Named vectors (empty / space / LFN-only / illegal / UTF-8) | **PASS** |
| 50k PRNG seed `0x43554342` (`CUBC`) | **PASS** |

---

## ABI smoke

| Check | Result |
|-------|--------|
| Marker | `FNIL` (`rust_fat_name_is_legal_smoke_result`) |
| Direct `rust_fat_name_is_legal` vectors | **PASS** |
| Public trampoline CF + ECX/EDX/EBX/ESI/EDI/EBP canaries | **PASS** |
| Live FAT mount / directory mutation | **none** (synthetic iglobals only) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_FAT_NAME_IS_LEGAL=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_FAT_NAME_IS_LEGAL=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — same 779380 non-black; 16 differing bytes (≈0.0007%, clock/timing noise) |
| Desktop boot | **PASS** both OFF and ON |

---

## Real subsystem soak

| Check | Result |
|-------|--------|
| Boot FAT floppy desktop | **PASS** (volume present; apps launch) |
| `fat_CreateFile` → `fat_name_is_legal` create path | **NOT AVAILABLE** as automated harness (leaf only runs on LFN create / short-name generate) |

---

## Regressions

**NONE** discovered this cut. No new `REG-*` entry.

---

## Production gate

| Item | Value |
|------|-------|
| Gate | `USE_RUST_FAT_NAME_IS_LEGAL = 1` |
| Rollback | `USE_RUST_FAT_NAME_IS_LEGAL = 0` or `[[rust.migrations]]` `cut = "BC"` `enabled = false` |
| Image | `dev_build/cut-bc-final.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/fat_name.rs` — helper + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_fat_name_is_legal`
* `kernel/fs/fat.inc` — gate + trampoline
* `kernel/rust/fat_name_is_legal.inc` — blob embed + smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-bc-plan.md` / `cut-bc-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md` / `migration-todo.md`

---

## Known limitations

* Does not migrate short-name generate (`fat_gen_short_name`) callers beyond the
  legality gate.
* Does not claim FAT Path A ownership.
* Create-path soak remains manual / harness-absent (see above).
* Stop; do not start Cut BD.
