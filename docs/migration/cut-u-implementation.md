# Cut U Implementation — `fat_gen_short_name`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-u-plan.md`](cut-u-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fat_gen_short_name` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | 1 — `fat.inc` create path (`.notfound` → short-name gen) |
| Rust symbol | `rust_fat_gen_short_name` |
| Pure helper | `kolibri_utils::fat_gen_short_name` |
| Subsystem | Filesystem / FAT naming |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `fat_gen_short_name` | **Selected** — UTF-8→8.3 state machine; composes Cuts B+K |
| `tcp_set_persist` | Deferred — strong #2; TCP-timer family after M |
| `coff_get_align` | Deferred — PE/COFF foothold; thinner body |
| `set_window_clientbox` | Deferred — GUI depth after Cut S |
| `pci_make_config_cmd` / `blit_clip` / `fat_time_to_bdfe` | Deferred/rejected — thin or low novelty |
| `memmove` / `mutex_init` / `set_io_access_rights` | Deferred — Stage-4 / high risk |

---

## Why selected

Cut U’s research question: does Strategy A + C remain viable for a **non-trivial string state machine** that orchestrates already-migrated leaves (`cp866toUpper`, `fat_next_short_name`) with multi-dot rewind and legality gating — without `.rodata` / cross-blob calls?

| Preference | Result |
|------------|--------|
| New class vs A–T | Yes — LFN→8.3 generator; not calendar/GUI/XFS/trivial pack |
| Composes prior cuts | Inlines Cut B + Cut K into one reloc-free blob |
| Real kernel callers | FAT create short-name path |
| Strategy A feasible | `fat_short_char_ok` if-chain (no table reloc) |
| Testability | Independent FASM-flow oracle + named + 50k PRNG |
| QEMU observability | Weak stock create path; compensated by smoke + differential |

---

## Special ABI handling

| Item | Contract |
|------|----------|
| Convention | Regcall leaf wrapped in `pushad`/`popad`, plain `ret` |
| In | **ESI** → UTF-8 NUL name; **EDI** → out (≥12 bytes; caller `sub esp,12`) |
| Out | 11-byte 8.3 at EDI; FASM also writes a 12th space (`stosd`×3) |
| Clobbers | None visible (pushad/popad) |
| Callees (legacy) | `cp866toUpper`; conditional `fat_next_short_name` |

Trampoline (production):

```text
pushad
stdcall rust_fat_gen_short_name, esi, edi
popad
ret
```

Rust inlines `cp866_to_upper` and `fat_next_short_name` (no cross-blob calls).

### Algorithm quirks (locked)

* High-bit UTF-8 bytes → lossy skip (`test al,al` / `js .space`)
* `fat_legal_chars` bit2 gates short-name store (Rust: `fat_short_char_ok`)
* BH flags: bit0 lossy, bit2 had-dot; second+ dot sets `BH=3` and rewinds extension into basename
* Leading `.` is lossy; field overflow is lossy
* Lossy → `fat_next_short_name` on basename
* All-space basename + lossy hits Cut K’s known FASM OOB family (excluded from differential)

---

## Original implementation

FASM leaf retained under `USE_RUST_FAT_GEN_SHORT_NAME=0` in `fat.inc`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/fat_name.rs`](../../rust_kernel/kolibri_utils/src/fat_name.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_fat_gen_short_name` |
| Build | [`rust_kernel/kolibri_utils/build-fat-gen-short-name.ps1`](../../rust_kernel/kolibri_utils/build-fat-gen-short-name.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_fat_gen_short_name.bin` |
| Embed/smoke | [`kernel/rust/fat_gen_short_name.inc`](../../kernel/rust/fat_gen_short_name.inc) |

`#![no_std]` freestanding; legality via immediate if-chain (no `.rodata`).

---

## Link strategy

Strategy **A + C**: freestanding extract → FASM `file` embed; thin trampoline; `USE_RUST_FAT_GEN_SHORT_NAME` gate.

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_fat_gen_short_name` |
| Blob size | 1248 bytes |
| Relocations | **0** |
| SHA-256 | `280A00613E49E71808F6F32B3A494225C2CF8B567B67226D849B3A80598531C6` |
| Epilogue | `ret 8` (shared; extractor-accepted) |

---

## Differential tests

Host `cargo test -p kolibri_utils` (213+ tests including Cut U):

* Named vectors: `file.txt`, long LFN, multi-dot, leading-dot, spaces, LFN-only `+`, etc.
* `fat_short_char_ok` vs full `fat_legal_chars` bit2 table (0..127)
* Independent FASM-flow oracle vs production
* 50k PRNG corpus (skips Cut K all-space OOB family)

**PASS**

---

## In-kernel smoke

`fat_gen_short_name_rust_smoke_test` (wired after Cut T smoke):

* Vectors: `file.txt` → `FILE    TXT`; `longfilename.txt` → `LONGFI~1TXT`; `a.b.c` → `AB~1    C  `
* Asserts pushad GPR preserve, 12th space byte, 8.3 layout
* Fail hang: `EAX=0xDEAD0C55`, `EBX='FGSN'`, `ECX='FAIL'`

**PASS** (ON boot reached desktop; no hang)

---

## QEMU validation

Kernels built with Cuts A–T production gates intact (`USE_RUST_FS_TIME2BDFE=1`, etc.).

Images: CoW from `dev_build/cut-t-final.img`, replace `KERNEL.MNT`.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| OFF | `USE_RUST_FAT_GEN_SHORT_NAME=0` | **OK** (QMP `running` + screendump `dev_build/cut-u-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_FAT_GEN_SHORT_NAME=1` | **OK** (screendump `dev_build/cut-u-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C55`; boot continued to desktop).

**Real subsystem soak:** stock image boot does **not** deliberately exercise FAT create/rename short-name generation. Leaf validated by differential + ABI smoke; generic boot/desktop is kernel integration regression only.

Production default after completion: **`USE_RUST_FAT_GEN_SHORT_NAME = 1`**.

Production image: `dev_build/cut-u-final.img`.

---

## Kernel sizes

| Artifact | Size |
|----------|------|
| `kernel-cut-u-off.mnt` | 236792 |
| `kernel-cut-u-on.mnt` | 236680 |

---

## Rollback

```text
USE_RUST_FAT_GEN_SHORT_NAME = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/fat_gen_short_name.inc`. Independent of Cuts A–T.

---

## Evidence summary

### PROVEN

* UTF-8→FAT 8.3 string state machine (BH flags, multi-dot rewind)  
* Reloc-free blob composing inlined Cuts B+K  
* Bit-exact vs independent host FASM-flow oracle + 50k PRNG  
* Public ABI smoke (layout, GPR preserve, lossy→~1)  
* QEMU OFF/ON desktop regression  

### NOT PROVEN / NOT AVAILABLE

* Live FAT create/rename short-name generation on stock image  
* All-space basename + lossy path (Cut K FASM OOB family)

---

## Known limitations

* Stock QEMU image does not exercise `fat_gen_short_name` production caller (FAT create).  
* All-space lossy basename inherits Cut K pathological OOB (not differentially compared under host bounds checks).  
* `fat_name_is_legal` remains FASM (natural follow-on pair; out of scope).

---

## Out of scope (unchanged)

* Cut V  
* `tcp_set_persist` / `coff_get_align` / `set_window_clientbox`  
* `fat_name_is_legal` / `fat_time_to_bdfe`
