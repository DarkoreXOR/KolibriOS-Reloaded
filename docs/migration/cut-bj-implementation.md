# Cut BJ Implementation — `is_string_userspace`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bj-plan.md`](cut-bj-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `is_string_userspace` |
| Source | [`kernel/kernel.asm`](../../kernel/kernel.asm) |
| Callers | 1 live (`load_library` via `memory.inc` sysfn path) |
| Rust symbol | `rust_is_string_userspace` |
| Pure helper | `kolibri_utils::is_string_userspace` / `is_string_userspace_at` |
| Subsystem | Stage-3 / syscall userspace string gate |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BI audit: XFS/NTFS/network/AHCI/PE/FAT/string/HID/ISO/Stage-3
Path A still fail the raised bar; P+AZ+this leaf are three gates ≠ façade ownership.
ISO Path B exhausted (AJ+BI).

Selected **`is_string_userspace`** — NUL-terminated string userspace membership
(bound + 64K-capped scasb ZF gate); one live `load_library` caller; clean ZF ABI;
strong independent oracle. Preferred over Stage-4 address-math `v86_get_lin_addr`,
PE-thin `coff_get_align`, trivial `ahci_is_sig_known`, and side-effect-heavy
`set_mouse_data`.

REG-001: trampoline preserves **EAX/ECX/EDI** (+ **EDX** canary); ZF via
`cmp eax,1` + flag-neutral pops (Cut P pattern).

REG-002: N/A.

REG-003: ABI smoke is **reject-only** (`base≥OS_BASE`); early-init clears userspace
PDT before smokes — no accept-path dereference, no live table mutation.

---

## Candidate comparison (post-BI audit)

| Candidate | Outcome |
|-----------|---------|
| `is_string_userspace` | **Selected** — Stage-3 NUL-scan ZF gate |
| `v86_get_lin_addr` | #2 — Stage-4 address math |
| `coff_get_align` | #3 — PE thin |
| `ahci_is_sig_known` | #4 — trivial CMP |
| `set_mouse_data` | #5 — HID side-effects |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_IS_STRING_USERSPACE=0`:

```text
stdcall is_string_userspace(base)
in:  base = string linear address
out: ZF=1 iff base≤OS_BASE-1 and a NUL is found within
     min(OS_BASE-base, 0x10000) bytes; else ZF=0
preserves: EAX, ECX, EDI (push/pop); EDX/EBX/ESI/EBP untouched
clobbers: flags (ZF is the result)
DF: assumed 0 for `repnz scasb`; Rust path DF-agnostic forward scan
ret 4
```

Quirks retained:

* `base > OS_BASE-1` → reject with ZF from `SUB` (always 0 on that path)
* Scan window capped at 64K even when more userspace remains
* No NUL in window → ZF=0 (reject), including unterminated buffers

---

## Rust ABI

```text
stdcall rust_is_string_userspace(base) -> EAX ∈ {0,1}
  ret 4
1 = legacy ZF=1; 0 = legacy ZF=0
```

Trampoline: push EDI/ECX/EDX/EAX; call Rust; `cmp eax,1`; flag-neutral pops.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `userspace.rs` + `ffi.rs` section `.text.rust_is_string_userspace` |
| Extract | `extract_reloc_free_text.py` → `rust_is_string_userspace.bin` |
| Embed | `kernel/rust/is_string_userspace.inc` `file` directive |
| Trampoline | `kernel.asm` under `USE_RUST_IS_STRING_USERSPACE` |
| Gate | `USE_RUST_IS_STRING_USERSPACE` (prod 1) |
| Smoke | `is_string_userspace_rust_smoke_test` (early init after Cut P) |

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_is_string_userspace` |
| Size | **56 bytes** |
| Relocations | **0** (extractor reject-on-reloc) |
| SHA-256 | `45E339710101A52FDB88F93EDEC7121D7EE177643A51C7888B64C22275C0F958` |
| Epilogue | `ret 4` (`c2 04 00`) present |

---

## Differential

| Item | Result |
|------|--------|
| Host `cargo test` (`string_*`) | **PASS** |
| Independent oracle | FASM-flow SUB/JB/cap/scasb (not a call to the SUT) |
| Coverage | reject bases; empty/short accept; no-NUL reject; 64K cap; near-`OS_BASE`; **50k PRNG** seed `0x4355424A` (`'CUBJ'`) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `is_string_userspace_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | `OS_BASE` / `OS_BASE+1` / `0xFFFFFFFF` reject; direct `rust_*` on kernel iglobal; EAX/ECX/EDX/EDI/EBX/ESI canaries |
| Marker | `rust_is_string_userspace_smoke_result = 'ISUS'` on success |
| Live state | Reject-only; no userspace map; no process/FS/net mutation (REG-003) |
| Accept scan | **Host differential only** — early-init userspace PDT cleared before smokes |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_IS_STRING_USERSPACE=0` | **OK** (QMP `running` + screendump, 779426 non-black) | FASM body |
| ON | `USE_RUST_IS_STRING_USERSPACE=1` | **OK** (QMP `running` + screendump, 779426 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — 16 differing bytes (clock/timer noise; same non-black count 779426) |
| Desktop boot | **PASS** both OFF and ON |
| Prior image | `dev_build/cut-bi-final.img` retained as baseline |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Desktop boot / early init | **PASS** — ABI smoke on reject path; desktop reached |
| `load_library` string gate | **PARTIAL** — live caller path present; not separately automated beyond desktop smoke + host accept corpus. Library-load UI soak not separately scripted. |

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
| Production gate | `USE_RUST_IS_STRING_USERSPACE = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bj-final.img` |
| Rollback | `USE_RUST_IS_STRING_USERSPACE = 0` or `[[rust.migrations]]` `cut = "BJ"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/userspace.rs` — leaf + differential tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_is_string_userspace`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/rust/is_string_userspace.inc` — blob embed + ABI smoke
* `kernel/kernel.asm` — trampoline + gate + smoke call
* `kernel/kernel32.inc` — include
* `project/build.toml` — blob + migration BJ
* `docs/migration/cut-bj-plan.md`
* `docs/migration/cut-bj-implementation.md`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Does not claim Stage-3 Path A / syscall façade ownership.
* Early-init ABI smoke cannot safely exercise accept/NUL-scan (userspace PDT
  cleared before smokes); accept coverage is host differential + live
  `load_library` path during normal use.
* DF=1 FASM path would reverse `scasb`; callers use DF=0; Rust ignores DF.

**Stop; do not start Cut BK.**
