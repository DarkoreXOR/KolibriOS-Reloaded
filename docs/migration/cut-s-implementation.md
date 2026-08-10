# Cut S Implementation — `window._.check_window_position`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-s-plan.md`](cut-s-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `window._.check_window_position` |
| Source | [`kernel/gui/window.inc`](../../kernel/gui/window.inc) |
| Callers | 2 (`window._.set_window_box`, `window._.sys_set_window`) — fn0 / fn67 path |
| Rust symbol | `rust_check_window_position` |
| Pure helper | `kolibri_utils::check_window_position` / `WindowBox` |
| Subsystem | GUI / window management |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `window._.check_window_position` | **Selected** — GUI screen-fit policy; EDI→WDATA*; strong desktop QEMU |
| `fsTime2bdfe` | Deferred — EDI+=8 novelty; calendar-family; weak stock FAT live path |
| `fat_gen_short_name` | Deferred — composition with K/B; FAT create only |
| `tcp_set_persist` | Deferred — Cut M sibling; less novelty |
| `blit_clip` | Deferred — Cut H composition; weak fn73 live path |
| `memmove` | Deferred — Stage-4 fanout |

---

## Why selected

Cut S’s research question: does Strategy A + C remain viable for a **GUI window screen-fit policy leaf** that mutates `EDI→WDATA.box` using `_display` dimensions injected by the trampoline (reloc-free), on the **live desktop** create/move path?

| Preference | Result |
|------------|--------|
| New class vs Cut R | Yes — not EBP/MOVBE/SHRD; GUI policy |
| Distinct from Cut H | Yes — shrink+reposition vs RECT∩RECT+CF |
| Real kernel callers | fn0 `sys_set_window`, fn67 `set_window_box` |
| Strategy A feasible | No tables; dims as args |
| Testability | Independent FASM-flow oracle + grid + 200k PRNG |
| QEMU observability | Strong (desktop paint) |

---

## Special ABI handling

| Item | Contract |
|------|----------|
| Convention | Regcall leaf, plain `ret` |
| In | **EDI → `WDATA*`** (`box` at +0) |
| Implicit | `[_display.width]`, `[_display.height]` |
| Out | In-place `box.{left,top,width,height}` |
| Preserved | EAX EBX ECX EDX ESI (push/pop); EDI |
| Flags | unused |

Trampoline (production):

```text
push eax ebx ecx edx esi
stdcall rust_check_window_position, edi, [_display.width], [_display.height]
pop  esi edx ecx ebx eax
ret
```

Rust never references `_display` (Cut L/O pattern).

### Compare semantics (locked)

* Width/height: **unsigned** `jae` → clamp to `display - 1`
* Left/top low: **signed** `jl` → 0
* Left/top high: wrapping `ADD` then **signed** `jge` → `display - size - 1`

---

## Original implementation

FASM leaf retained under `USE_RUST_CHECK_WINDOW_POSITION=0` in `window.inc`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/window.rs`](../../rust_kernel/kolibri_utils/src/window.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_check_window_position` |
| Build | [`rust_kernel/kolibri_utils/build-check-window-position.ps1`](../../rust_kernel/kolibri_utils/build-check-window-position.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_check_window_position.bin` |
| Embed | [`kernel/rust/check_window_position.inc`](../../kernel/rust/check_window_position.inc) |

`#![no_std]` freestanding; explicit `i32` / unsigned size compares; no globals.

### Blob lock

| Field | Value |
|-------|-------|
| Section | `.text.rust_check_window_position` |
| Size | **182** bytes |
| Relocations | **0** |
| SHA-256 | `DD90D7C1FEA19341B3DB9065E43ED77EC4C5A6959E3E59FB403ABC27F056BA35` |
| Epilogue | `ret 12` (`c2 0c 00`) |

---

## Differential tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **193/193** (incl. prior A–R) |
| Inside / oversize / left-low / left-high / top / width==display | **PASS** |
| Oversize-then-left-high uses clamped width | **PASS** |
| Display-zero wrap / wrapping ADD signed compare | **PASS** |
| Boundary grid (coords × sizes × displays) | **PASS** |
| PRNG 200k vs independent FASM-flow oracle (`0x4357_5031`) | **PASS** |

---

## In-kernel smoke

`check_window_position_rust_smoke_test` (wired after Cut R smoke):

* Save/restore `_display.width/height`; install 800×600
* Vectors: inside unchanged; oversize→799×599; left-low; left-high→749; top-high→559
* Asserts EDI/EAX/EBX/ECX/EDX/ESI preservation
* Fail hang: `EAX=0xDEAD0C53`, `EBX='CWPS'`, `ECX='FAIL'`

---

## QEMU validation

Kernels built with Cuts A–R production gates intact (`USE_RUST_XFS_EXTENT_UNPACK=1`, etc.).

Images: copied from `tmp_images/cut-r-final.img`, then freed clusters via authorized deletes only (`DEVELOP/FASM`, `3D/VIEW3DS`, `GAMES/DINO` — see `.cursor/rules/image-handling.mdc`; `DOCPACK` already absent on Cut R). `kolibri_img delete` now accepts nested paths for those files.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| ON | `USE_RUST_CHECK_WINDOW_POSITION=1` | **OK** (QMP `running` + screendump `tmp_images/cut-s-on.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| OFF | `=0` (original FASM body) | **OK** (screendump `tmp_images/cut-s-off.ppm`, same non-black sample count) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C53`; boot continued to desktop). Window create/move path exercises this leaf on stock desktop (fn0).

Production default after completion: **`USE_RUST_CHECK_WINDOW_POSITION = 1`**.

Production image: `tmp_images/cut-s-final.img`.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel-cut-s-on.mnt` | 234216 | `1C0824E6F6A4770A476A7320B4152825612E099382B88BD422DB23BB51300CDA` |
| `kernel-cut-s-off.mnt` | 234312 | `F4D631B088B80D70508034DAD205A872CA2CFD239CDB150523CD1BC561849C57` |

---

## Rollback

```text
USE_RUST_CHECK_WINDOW_POSITION = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/check_window_position.inc`. Independent of Cuts A–R.

---

## Evidence summary

### PROVEN

* GUI screen-fit leaf with EDI→BOX mutation  
* Display globals injected by trampoline (reloc-free Rust)  
* Bit-exact vs independent host FASM-flow oracle + 200k PRNG  
* Freestanding 182-byte blob, 0 relocs  
* In-kernel smoke hang-on-fail (registers + box fields)  
* QEMU ON/OFF desktop regression (fn0 path live)  

### NOT PROVEN

* Exhaustive interactive maximize/rollup matrix beyond stock boot paint  

### OUT OF SCOPE

* `set_window_clientbox` / `check_window_draw`  
* `blit_clip` / `fsTime2bdfe` / `memmove`  
* Cut T  

---

## Files touched

* `rust_kernel/kolibri_utils/src/window.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-check-window-position.ps1` (new)
* `kernel/rust/check_window_position.inc` (new)
* `kernel/gui/window.inc` (gate + trampoline)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml` (blob entry)
* `docs/migration/cut-s-plan.md`
* `docs/migration/cut-s-implementation.md`
* `docs/migration/migration-plan.md`
