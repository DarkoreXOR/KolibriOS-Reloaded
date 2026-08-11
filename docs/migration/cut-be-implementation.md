# Cut BE Implementation — `hotkey_do_test`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-be-plan.md`](cut-be-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `hotkey_do_test` |
| Source | [`kernel/hid/keyboard.inc`](../../kernel/hid/keyboard.inc) |
| Callers | 3 (one hotkey loop in `send_scancode` / `.writekey`) |
| Rust symbol | `rust_hotkey_do_test` |
| Pure helper | `kolibri_utils::hotkey::hotkey_do_test` |
| Subsystem | HID hotkey field match |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BD audit: XFS/NTFS/network/AHCI/PE/FAT/string/Stage-3
Path A still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. Thin leftovers (`is_string_userspace`, `v86_get_lin_addr`,
`coff_get_align`, `swap_bytes_in_words`) ranked below. Selected
**`hotkey_do_test`** — new HID semantic (`kb_state` × nibble predicate match),
distinct from Cut L mouse acceleration. Prior “reloc-hostile” reject mitigated
by branchless inlined `hotkey_test0..4` (no PC-relative call table / jump table).

REG-001: trampoline preserves **EAX** (list node), **EBX/ECX/EDX**; **CF** is OUT.

REG-003: ABI smoke uses **stack synthetic hotkey node** + **save/restore
`kb_state`** — never mutates `hotkey_list` / `hotkey_scancodes`.

Reloc-free note: LLVM jump-table for sequential `if test_id == N` was rejected by
the extractor (`.rodata` + GOTOFF). Fixed with branchless mul/add select.

---

## Candidate comparison (post-BD audit)

| Candidate | Outcome |
|-----------|---------|
| `hotkey_do_test` | **Selected** — hotkey predicate match |
| `is_string_userspace` | #2 — thin P sibling |
| `v86_get_lin_addr` | #3 — Stage-4 address math |
| `swap_bytes_in_words` | #4 — AV deepen |
| `coff_get_align` | #5 — thin PE glue |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_HOTKEY_DO_TEST=0`:

```text
call hotkey_do_test
in:  EAX → hotkey list node; CL ∈ {0,2,4} (Shift/Ctrl/Alt field)
out: CF clear = pass (predicate matched); CF set = fail
preserves: EAX, EBX, ESI, EDI, EBP
clobbers: EDX; doubles CL (`add cl, cl`); flags
ret (plain)
```

Quirks retained:

* `funcs` dword at `[eax+4]`; nibble selected by doubled CL
* `kb_state` shifted by original CL; low 2 bits = modifier field
* test id ≥ 5 → `.fail` / `stc`
* Predicates 0..4: neither / odd-parity / both / left / right

---

## Rust ABI

```text
stdcall rust_hotkey_do_test(funcs, kb_state, cl) → EAX = 0 pass / ≠0 fail
  ret 12
```

Trampoline: push EBX/ECX/EDX/EAX; `stdcall` with `[eax+4]`, `[kb_state]`,
zero-extended CL; restore EAX; `test` → `clc`/`stc`.

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_hotkey_do_test` |
| Size | **123 bytes** |
| Relocations | **0** |
| SHA-256 | `90E2B6EBA0169FE9637E928DCC348003B36D4ED4230E47B140C81D593336EA47` |
| Epilogue | `ret 12` (`c2 0c 00`) |

---

## Differential tests

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle (5 inlined predicates) | **PASS** |
| All test ids 0..4 × fields 0..3 | **PASS** |
| Named vectors (CL 0/2/4 field select) | **PASS** |
| Out-of-range nibble ≥5 → fail | **PASS** |
| 50k PRNG seed `0x43554245` (`CUBE`) | **PASS** |

---

## ABI smoke

| Check | Result |
|-------|--------|
| Marker | `HKDT` (`rust_hotkey_do_test_smoke_result`) |
| Direct `rust_hotkey_do_test` vectors | **PASS** |
| Public path CF out + EAX/EBX/ESI/EDI canaries | **PASS** |
| EDX canary | **not asserted** (legacy FASM clobbers EDX; OFF-gate must pass) |
| Live `hotkey_list` / `hotkey_scancodes` mutation | **none** |
| `kb_state` | **save/restore** around smoke |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_HOTKEY_DO_TEST=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_HOTKEY_DO_TEST=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — same 779380 non-black; 132 differing bytes (≈0.006%, clock/timing noise) |
| Desktop boot | **PASS** both OFF and ON |
| Prior `cut-bd-final.img` | Present as rollback/baseline reference |

---

## Real subsystem soak

| Check | Result |
|-------|--------|
| Desktop boot with keyboard/hotkey path present | **PASS** (smoke + desktop non-black) |
| Full system-hotkey matrix / typed hotkey soak | **PARTIAL / NOT AVAILABLE** as automated harness — leaf is on the scancode→hotkey loop; stock desktop smoke does not force a complete hotkey registration matrix |

---

## Regressions

**NONE** discovered this cut that survived into production. During OFF-gate
validation, ABI smoke initially asserted EDX preserve (trampoline-only); that
failed black-screen on FASM body. Fixed smoke to match legacy clobber set
before closing the cut. No new `REG-*` entry (host/smoke harness issue caught
before production ON image).

---

## Production gate

| Item | Value |
|------|-------|
| Gate | `USE_RUST_HOTKEY_DO_TEST = 1` |
| Rollback | `USE_RUST_HOTKEY_DO_TEST = 0` or `[[rust.migrations]]` `cut = "BE"` `enabled = false` |
| Image | `dev_build/cut-be-final.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/hotkey.rs` — helper + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` / `lib.rs` — `rust_hotkey_do_test` export
* `kernel/hid/keyboard.inc` — gate + trampoline
* `kernel/rust/hotkey_do_test.inc` — blob embed + smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-be-plan.md` / `cut-be-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md` / `migration-todo.md`

---

## Known limitations

* Branchless predicate select (not a literal `call [hotkey_tests+…]`) — same
  boolean results for defined ids 0..4.
* Trampoline preserves ECX/EDX more strictly than legacy FASM; callers only
  rely on EAX + CF.
* Full hotkey matrix soak remains manual / PARTIAL.
