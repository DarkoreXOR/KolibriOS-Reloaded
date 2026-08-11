# Cut BK Implementation — `coff_get_align`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bk-plan.md`](cut-bk-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `coff_get_align` |
| Source | [`kernel/core/dll.inc`](../../kernel/core/dll.inc) |
| Callers | 2 live (`load_library` size loop + section layout loop) |
| Rust symbol | `rust_coff_get_align` |
| Pure helper | `kolibri_utils::coff_get_align_from_characteristics` / `coff_get_align_ptr` |
| Subsystem | Stage-8 PE/COFF section alignment mask |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED — no remaining cluster meets the Rust-owned subsystem ownership bar.**

Post-BJ audit: XFS/NTFS/network/AHCI/PE/FAT/string/HID/ISO/Stage-3/paging Path A
still fail the raised bar. Stage-3 Path B novelty exhausted (P+AZ+BJ).

Selected **`coff_get_align`** — COFF `Characteristics` high-nibble → `(1<<n)-1`
alignment mask with 4K default/clamp; two live `load_library` callers; clean
register ABI; strong independent oracle; desktop `.sys` soak. Preferred over
Stage-4 address-math `v86_get_lin_addr` (weak V86 soak), trivial
`ahci_is_sig_known`, and HID/TCP/XFS deepen leaves.

REG-001: trampoline preserves **ECX+EDX** (+ **EBX/ESI/EDI/EBP** canaries);
EAX = mask. Legacy leaf push/pop ECX; EDX section cursor untouched.

REG-002: N/A.

REG-003: ABI smoke uses **synthetic `COFF_SECTION` only** — no live DLL list
mutation.

---

## Candidate comparison (post-BJ audit)

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `coff_get_align` | COFF Characteristics→align mask | 2 | Desktop `.sys` | Low | PE thin glue | **SELECT** |
| 2 | `v86_get_lin_addr` | PTE→linear | 15 | BIOS/V86 weak | Low–Med | Stage-4 / AW theme | Defer |
| 3 | `ahci_is_sig_known` | Signature CMP→ZF | 2 | `--bus ahci` | Low | AV deepen / trivial | Defer |
| 4 | `xfs._.conv_time_to_kos_epoch` | BE32→`fsTime2bdfe` | 3 | `--disk xfs` | Low | XFS time deepen | Defer |
| 5 | `set_mouse_data` | HID aggregator | 0 in-kernel | Desktop mouse | Med–High | HID deepen | Defer |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_COFF_GET_ALIGN=0`:

```text
call coff_get_align
in:  EDX → COFF_SECTION
out: EAX = alignment mask ((1<<n)-1)
preserves: ECX (push/pop); EDX untouched; EBX/ESI/EDI/EBP untouched
clobbers: EAX (result); flags (callers ignore)
plain ret (not stdcall)
```

Quirks retained:

* Characteristics align field `0` → default 4K (`mask=0xFFF`)
* Field `1` (1-byte) → `mask=0`
* Field `13` (4K) → `mask=0xFFF`
* Field `14`/`15` (>4K) → clamp to 4K
* Only bits 20..23 of Characteristics matter (byte at `+2`, high nibble)

---

## Rust ABI

```text
stdcall rust_coff_get_align(section) -> EAX = mask
  ret 4
```

Trampoline: push ECX/EDX; `stdcall rust_coff_get_align, edx`; pop EDX/ECX; `ret`.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `coff_get_align.rs` + `ffi.rs` section `.text.rust_coff_get_align` |
| Extract | `extract_reloc_free_text.py` → `rust_coff_get_align.bin` |
| Embed | `kernel/rust/coff_get_align.inc` `file` directive |
| Trampoline | `kernel/core/dll.inc` under `USE_RUST_COFF_GET_ALIGN` |
| Gate | `USE_RUST_COFF_GET_ALIGN` (prod 1) |
| Smoke | `coff_get_align_rust_smoke_test` (after Cut AT / near Cut Y) |

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_coff_get_align` |
| Size | **36 bytes** |
| Relocations | **0** (extractor reject-on-reloc) |
| SHA-256 | `F1C39E3AE291D5E93442914ADC0B2B69D84CA88DE63971ECD26CFD4435C84E7B` |
| Epilogue | `ret 4` (`c2 04 00`) present |

---

## Differential

| Item | Result |
|------|--------|
| Host `cargo test` (`coff_get_align::*`) | **PASS** |
| Independent oracle | FASM-flow SHR/DEC/JS/CMP/SHL/DEC (not a call to the SUT) |
| Coverage | field 0 default; 1-byte→0; 2B..4K; >4K clamp; junk low bits; **50k PRNG** seed `0x4355424B` (`'CUBK'`) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `coff_get_align_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | chars=0/1B/2B/4K/8K clamp; public trampoline ECX/EDX/EBX/ESI/EDI/EBP canaries |
| Marker | `rust_coff_get_align_smoke_result = 'CGAL'` on success |
| Live state | Synthetic `COFF_SECTION` only; no DLL list mutation (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_COFF_GET_ALIGN=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_COFF_GET_ALIGN=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — 16 differing bytes (clock/timer noise; same non-black count 779380) |
| Desktop boot | **PASS** both OFF and ON |
| Prior image | `dev_build/cut-bj-final.img` retained as baseline |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Desktop boot / early init | **PASS** — ABI smoke + desktop reached |
| `load_library` / `.sys` align path | **PARTIAL** — live caller path present (size + section layout loops); not separately automated beyond desktop smoke + host oracle. Library-load UI soak not separately scripted. |

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
| Production gate | `USE_RUST_COFF_GET_ALIGN = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bk-final.img` |
| Rollback | `USE_RUST_COFF_GET_ALIGN = 0` or `[[rust.migrations]]` `cut = "BK"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/coff_get_align.rs` — leaf + differential tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_coff_get_align`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/rust/coff_get_align.inc` — blob embed + ABI smoke
* `kernel/core/dll.inc` — trampoline + gate
* `kernel/kernel.asm` — smoke call
* `kernel/kernel32.inc` — include
* `project/build.toml` — blob + migration BK
* `docs/migration/cut-bk-plan.md`
* `docs/migration/cut-bk-implementation.md`
* `docs/migration/migration-todo.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Does not claim PE Path A / loader / export-directory ownership.
* Complements Cuts Y+AT without stretching into `get_proc_ex` / `rebase_coff`.
* Desktop soak exercises `load_library` indirectly; no dedicated `.sys`-load A/B harness.

**COMPLETE — STOP. Do not start Cut BL.**
