# Cut AZ Implementation — `file_system_is_operation_safe`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-az-plan.md`](cut-az-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `file_system_is_operation_safe` |
| Source | [`kernel/fs/fs_lfn.inc`](../../kernel/fs/fs_lfn.inc) |
| Callers | 2 (`sys_file_system_lfn` / sysfn70, `sys_fileSystemUnicode` / sysfn80) |
| Rust symbol | `rust_file_system_is_operation_safe` |
| Pure helper | `kolibri_utils::file_system_is_operation_safe` / `fs_op_safe_buffer_len(_ex)` |
| Subsystem | Syscall-70/80 FS operation safety gate (Stage-3) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-AY audit: XFS/NTFS/network/AHCI/PE Path A still fail
the raised bar; AO/AN/address-math/socket/USB leftovers stay ban-listed.
Selected **`file_system_is_operation_safe`** — new Stage-3 semantic class:
sysfn70 subfn → buffer-byte length → ZF userspace gate (inlined Cut P arithmetic;
no cross-section call).

REG-001: trampoline preserves **EAX+EBX+ECX+EDX**; reconstructs **ZF** via
`cmp eax, 1` with flag-neutral pops (Cut P class).

REG-003: ABI smoke uses **uglobal synthetic** info structs only — never mutates
live FS mounts.

Reloc-free note: an early draft used Rust `match` on subfn and emitted a jump
table in `.rodata` + GOT relocs. Production body uses an if/else chain matching
FASM control flow (same lesson as Cut A `.rodata` ban).

---

## Candidate comparison (post-AY audit)

| Candidate | Outcome |
|-----------|---------|
| `file_system_is_operation_safe` | **Selected** — Stage-3 size→ZF gate |
| `get_proc_ex` | #2 — PE ban stretch after Y+AT |
| `tcp_outflags` | #3 — mild M/V TCP deepen |
| `fat_name_is_legal` | #4 — charset table |
| `is_string_userspace` | #5 — thin P sibling |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE=0`:

```text
stdcall file_system_is_operation_safe(inf_struct_ptr)
out: ZF = 1 safe / ZF = 0 unsafe
preserves: EAX, EBX, ECX, EDX
clobbers: flags (via is_region_userspace or cmp ecx,ecx)
ret 4
```

Quirks retained:

* Unknown subfn (4, >6, …) → **ZF=1 without region check** (`.switch_none`)
* Subfn 1: encoding ≤1 → BDVK 304; else 560; `imul`×count + 32 (wrapping)
* Subfn 5 → fixed 40; subfn 6 → fixed 32
* Cut P overflow-to-zero accept quirk when region check runs

---

## Rust ABI

```text
stdcall rust_file_system_is_operation_safe(inf) → EAX
  EAX = 1 (legacy ZF=1) / 0 (legacy ZF=0)
  ret 4
```

Trampoline: `cmp eax, 1`; restore EAX/EBX/ECX/EDX with flag-neutral pops.

Region gate is **inlined** Cut P arithmetic inside the AZ blob (no call into
`.text.rust_is_region_userspace`).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `fs_operation_safe.rs` + `ffi.rs` section `.text.rust_file_system_is_operation_safe` |
| Extract | `extract_reloc_free_text.py` → `rust_file_system_is_operation_safe.bin` |
| Embed | `kernel/rust/file_system_is_operation_safe.inc` `file` directive |
| Trampoline | `fs_lfn.inc` under `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE` |
| Gate | `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE` (prod 1) |
| Smoke | `file_system_is_operation_safe_rust_smoke_test` (early init, after Cut P smoke) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_file_system_is_operation_safe` |
| Blob/object size | 215 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `389C79FE3BEB259CFE2CF228478CBFA19E840B2FF42798DEF18775D034946A51` |
| Epilogue | `ret 4` (`c2 04 00`) |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs helpers | **PASS** |
| Named vectors | subfn 0/1/2/3/5/6; encoding ≤1 vs >1; unknown accept; P quirk; wrap imul |
| PRNG | 50 000 vectors, seed `0x4355545A` (`'CUTZ'`) |
| Host tests | **489/489** cargo tests (incl. `fs_operation_safe` suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `file_system_is_operation_safe_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | rust_* safe/unsafe/unicode/unknown; public trampoline ZF + EAX/EBX/ECX/EDX canaries |
| Marker | `rust_file_system_is_operation_safe_smoke_result = 'FSOS'` on success |
| Live state | Synthetic uglobal inf only (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop (IDE) | 779380 | 779380 | **match** |
| `--disk exfat` (ON) | — | 779380 / `running` | **PASS** |
| `--disk xfs` (ON) | — | 779380 / `running` | **PASS** |

---

## Real subsystem soak

```text
Real subsystem soak: PARTIAL
```

Attached `--disk exfat` and `--disk xfs` boots reach QMP `running` with
non-black screendumps (sysfn70/80 gate is on the path for any FS file op once
apps browse). Interactive Eolite browse / directory listing was **not**
automated in this cut — report PARTIAL, not full interactive soak.

---

## Regressions

```text
None discovered during Cut AZ.
```

No new `REG-NNN` entry. Lessons applied: REG-001 ZF/regs; REG-003 synthetic
smoke fixtures; reloc-free if/else (no `match` jump table).

---

## Production gate

| Item | Value |
|------|-------|
| Gate | `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE = 1` |
| `project/build.toml` | `[[rust.migrations]]` cut `AZ`, `enabled = true` |
| Rollback | `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE = 0` or `enabled = false` |

---

## Image

`dev_build/cut-az-final.img` (CoW copy of ON production kernel).

---

## Files changed

* `rust_kernel/kolibri_utils/src/fs_operation_safe.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/rust/file_system_is_operation_safe.inc` (new)
* `kernel/fs/fs_lfn.inc` (gate + trampoline)
* `kernel/kernel32.inc` / `kernel/kernel.asm` (include + smoke call)
* `project/build.toml` (blob + migration AZ)
* `docs/migration/cut-az-plan.md` / `cut-az-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* Unknown-subfn accept quirk retained (security surface exists in legacy; not
  “fixed” by this cut).
* Interactive Eolite `--disk` browse not automated (PARTIAL soak).
* Region gate duplicated arithmetically vs Cut P blob (by design for reloc-free).

---

**COMPLETE — STOP.** Do not start Cut BA.
