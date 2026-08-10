# Cut B Plan — Candidate Audit & Decision Record

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for Phase B-1  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** In this document, **Cut B** means the next pure-utility migration after Cut A (user/stage terminology).  
> It is **not** `boundaries.md`’s allocator “Cut B”, and **not** the historical FASM `init.inc` restoration titled “Cut B” in [`fasm-baseline-restoration.md`](fasm-baseline-restoration.md).

---

## 1. Current baseline

**LOCAL FACT** — Cut A is complete and audited ([`cut-a-final-architecture.md`](cut-a-final-architecture.md)):

```text
Rust (freestanding i686)
  → dedicated .text.* section
  → relocation validation
  → raw blob extraction
  → FASM `file`
  → FASM ABI trampoline
  → existing kernel callers
```

| Function | Section | Size | Relocs | Switch |
|----------|---------|-----:|-------:|--------|
| `crc_32` | `.text.rust_crc_32` | 226 B | 0 | `USE_RUST_CRC` |
| `unicode.utf16.encode` | `.text.rust_unicode_utf16_encode` | 85 B | 0 | `USE_RUST_UTF16` |
| `unicode.cp866.encode` | `.text.rust_unicode_cp866_encode` | 294 B | 0 | `USE_RUST_CP866` |
| `unicode.utf8.decode` | `.text.rust_unicode_utf8_decode` | 318 B | 0 | `USE_RUST_UTF8` |

Reference floppy SHA-256 (immutable):

```text
1901F3A8D7CA0DA23DBB6259D85579F09ED36EBAA58B972AAD16E7059B47C8BA
```

Build path remains PowerShell extract scripts + vendored `tools/fasm/FASM.EXE` + `tools/kolibri_img` CoW + QEMU.

### Repository integrity — `kernel/init.inc`

**LOCAL FACT:** Current `kernel/init.inc` is the restored early-init source (SHA-256 `F7391BA4F36B9C4D569286FF6971E6E83A0385E1E906BD2BF041BC0437BB419D`, 14219 bytes). It defines `mem_test` / memory bring-up symbols required by `kernel.asm`, not USB init.

**LOCAL FACT:** USB init lives at `kernel/bus/usb/init.inc` (separate include via `kernel32.inc`). Historical corruption is documented in [`fasm-baseline-restoration.md`](fasm-baseline-restoration.md).

**Decision:** Do not use early-boot / memory-init symbols as Cut B candidates. Do not “repair” `init.inc` as part of Cut B (already restored; out of scope to touch).

---

## 2. Candidate table

Audited from live sources under `kernel/`. Cut A leftovers in `crc.inc` / `unicode.inc` / assemble-time `encoding.inc` are closed or unsuitable.

| Candidate | Source | Callers | ABI | Dependencies | Relocs risk | Strategy | Risk | Recommendation |
|-----------|--------|--------:|-----|--------------|-------------|----------|------|----------------|
| **`cp866toUpper`** | `fs/parse_fn.inc:87–113` | **1** (`fat.inc:518`) | AL/EAX in → AL/EAX out; plain `ret` | none | none (no table) | **A** | **Low** | **SELECT** |
| `utf16toUpper` | `fs/parse_fn.inc:115–133` | **11** (ntfs/iso9660/fat/exfat) | AX/EAX in/out; plain `ret` | none | none | A | Low | Strong alternate |
| `uni2ansi_char` | `fs/parse_fn.inc:135–180` | **10** live + rollback CP866 path | AX→AL; plain `ret` | `.table` 8 B | abs `.table` | A/D (reuse existing CP866 blob) | Low | Defer — overlaps Cut A logic |
| `ansi2uni_char` | `fs/parse_fn.inc:182–216` | **5** + internal | AL→AX; plain `ret` | reads `uni2ansi_char.table` | abs table load | A (stack/inline table) | Low | Next after leaves |
| `strncmp` | `core/string.inc:58–84` | **11** + PE export | stdcall `ret 12`; EAX −1/0/1 | none | none | A | Low–med | Good; export contract |
| `strnlen` / `strncpy` / `strrchr` | `core/string.inc` | 0–1 each + exports | stdcall | none | none | A | Low | Package later |
| `strlen` (parse_fn) | `fs/parse_fn.inc:330–341` | **2** | ESI in; ECX=len; saves EDI/EAX | none | none | A | Low | Viable |
| `utf8to16` | `fs/parse_fn.inc:295–328` | **16** | ESI↑; AX out; ≠ Cut A utf8.decode | none | none | A | Med | Separate oracle required |
| `UTF16to8` | `fs/parse_fn.inc:254–293` | **5** + internal | AX in; EDI↑ ECX↓; **SF** on exhaust | none | none | A + careful trampoline | Med | Flags ABI |
| `checksum_2` | `network/stack.inc:746–765` | **7** + macros | EDX in; DX out (0→FFFF quirk) | none | none | A | Low–med | Network-adjacent |
| `checksum_1` | `network/stack.inc:668–734` | checksum family | EDX/ESI/ECX partial sum | none | none | A | Med | Pair with checksum_2 |
| `is_region_userspace` | `kernel.asm:4456+` | **~30** | stdcall; result in **ZF** | `OS_BASE` | constant | A | Med | Security surface — defer |
| `memmove` | `kernel.asm:3215+` | **24** | EAX/EBX/ECX; forward-only | none | none | Reject early | Med–high | Hot path; misnamed |
| Allocator exports | `core/memory.inc` etc. | many | various | globals | high | Reject | High | `boundaries.md` mid cut |
| `encoding.inc` macros | assemble-time only | n/a | n/a | n/a | n/a | Reject | — | No runtime symbol |

---

## 3. Selected candidate

### `cp866toUpper`

**Why selected**

1. **Pure transformation** — maps one CP866 byte to uppercase; no memory, no globals, no callees.
2. **Tiny, closed domain** — 256 inputs → exhaustive differential oracle.
3. **Minimal blast radius** — single live caller (`fat.inc` 8.3 name store path).
4. **Clear ABI** — register in/out, plain `ret`, no stack cleanup.
5. **No `.rodata`** — Strategy A reloc-free blob is the natural fit (no forced rewrite for tables).
6. **Greenfield Cut B** — does not merely trampoline the already-migrated CP866 encode blob (unlike `uni2ansi_char`).
7. **Proves repeatability** — same pipeline as Cut A on a *new* function outside `crc.inc`/`unicode.inc`.

**Why safer than alternatives**

| Alternate | Why not first |
|-----------|----------------|
| `utf16toUpper` | Equally pure, but **11** FS callers → larger immediate surface |
| `uni2ansi_char` | Already covered algorithmically by Cut A CP866; migrating it is mostly wiring reuse |
| `strncmp` | stdcall + PE export + `n=-1` “unlimited” quirks; more ABI surface |
| `utf8to16` / `UTF16to8` | Different from Cut A UTF-8; flags / pointer advances; higher trampoline risk |
| checksum / userspace checks | Adjacent to net or security call density |
| allocator / sched / IRQ | Explicitly out of scope for this cut |

---

## 4. Integration strategy

**Selected: Strategy A — reloc-free raw `.text` extraction**  
combined with **Strategy D — FASM ABI trampoline** (callers unchanged).

```text
A + D: rust_cp866_to_upper in .text.rust_cp866_to_upper
       → extract (0 relocs, symbol @0)
       → kernel/rust/cp866_upper.inc `file`
       → cp866toUpper trampoline under USE_RUST_CP866_UPPER=1
```

**Why not B (`rust-lld`):** Function needs no `.rodata`, helpers, or cross-section refs. Introducing a linker for a 8-bit map would expand kernel assumptions without benefit.

**Why not C (source rewrite alone):** Algorithm is already a faithful FASM leaf; rewrite into Rust is fine as the *body*, but the integration path remains A+D. No FASM-specific absolute addresses require a different architecture.

**Why not pure D without A:** The Rust body still needs a delivery mechanism; Cut A’s extract+`file` path is proven and sufficient.

---

## 5. ABI contract

**LOCAL FACT** — `kernel/fs/parse_fn.inc:87–113` and sole caller `kernel/fs/fat.inc:517–519`:

```asm
; caller
        lodsb
        ...
        call    cp866toUpper
        stosb
```

| Item | Contract |
|------|----------|
| **Arguments** | CP866 character in **AL** (conceptually); FASM arithmetic uses **EAX** |
| **Return** | Uppercased CP866 character in **AL** |
| **Register inputs** | `AL` (character). Upper bits of `EAX` are not part of the documented contract; sole caller loads via `lodsb` |
| **Register outputs** | `AL` updated. Rust trampoline path will return **zero-extended** `EAX` (AL significant) |
| **Callee-saved** | None required beyond plain `call`/`ret`. FASM body pushes nothing. Does not preserve flags |
| **Stack cleanup** | Caller cleanup — plain `ret` (0 bytes). Rust FFI uses `stdcall` with **1 dword arg** → Rust `ret 4`; trampoline absorbs that |
| **Flags** | Clobbered; callers must not rely on flags |
| **Memory side effects** | **None** |
| **Global state** | **None** |

### Algorithm (LOCAL FACT — FASM body)

| Input AL | Action |
|----------|--------|
| `< 'a'` (0x61) | unchanged |
| `'a'..'z'` | `AL -= 32` |
| `0xA0..0xAF` | `AL -= 32` → `0x80..0x8F` |
| `0xE0..0xEF` | `AL -= 0x50` → `0x90..0x9F` |
| `0xF0..0xF7` | `AL &= ~1` (pair ё→Ё etc.) |
| otherwise | unchanged |

### Proposed Rust FFI

```text
#[no_mangle]
#[link_section = ".text.rust_cp866_to_upper"]
pub extern "stdcall" fn rust_cp866_to_upper(ch: u32) -> u32;
```

Trampoline:

```asm
cp866toUpper:
        stdcall rust_cp866_to_upper, eax
        ret
```

Rollback switch (independent of Cut A switches):

```asm
USE_RUST_CP866_UPPER = 1   ; 0 → original FASM body
```

---

## 6. Verification plan

| Gate | Method |
|------|--------|
| **Rust unit tests** | Pure `cp866_to_upper(u8) -> u8` vs documented ranges; edge bytes `0x00`, `0x60`, `0x61`, `0x7A`, `0x7B`, `0x9F`, `0xA0`, `0xAF`, `0xB0`, `0xDF`, `0xE0`, `0xEF`, `0xF0`–`0xF8`, `0xFF` |
| **Differential** | **Exhaustive 0..=255** FASM-oracle table vs Rust (host `cargo test`) |
| **ABI / trampoline** | In-kernel smoke: `mov al, <case>; call cp866toUpper; cmp al, expect` through real trampoline |
| **Kernel smoke** | Hang-on-fail helper in `kernel/rust/cp866_upper.inc`; called from `high_code` after existing Cut A smokes |
| **QEMU Rust ON** | `USE_RUST_CP866_UPPER=1` (+ Cut A defaults) → desktop |
| **QEMU Rust OFF** | `USE_RUST_CP866_UPPER=0` → desktop; FASM body used |
| **Relocation audit** | Extractor: `SHT_PROGBITS`, exact section, **0** REL/RELA, symbol @ offset 0, `--expect-ret-imm 4` |
| **Reproducibility** | Clean freestanding rebuild ×2; blob SHA-256 identical |
| **Cut A regression** | `cargo test -p kolibri_utils` still green; existing blobs still extract |

### Smoke corpus (minimum)

- Latin: `'a' → 'A'`, `'z' → 'Z'`, `'A'` unchanged, `'@'` unchanged  
- CP866 Cyrillic: `0xA0 → 0x80`, `0xE0 → 0x90`  
- Specials: `0xF1 → 0xF0` (ё→Ё), `0xF0` unchanged  

---

## 7. Out of scope

- Migrating any second function in this cut  
- Rewiring `uni2ansi_char` / remaining `parse_fn.inc` leaves  
- Allocator / scheduler / IRQ / paging / GUI  
- Touching `kernel/init.inc`  
- Introducing `rust-lld`  
- Removing FASM originals or Cut A switches  
- Vendoring `kerpack` / changing `DOCPACK` workflow  

---

## 8. Decision record (summary)

```text
Candidate:              cp866toUpper
Why selected:           pure, 256-domain, 1 caller, no tables, greenfield
Why safer:              smaller surface than utf16toUpper / strncmp / utf8to16
ABI:                    AL in/out; plain ret; no memory
Dependencies:           none
Global state:           none
Chosen linking:         Strategy A (reloc-free blob) + D (FASM trampoline)
Rejected:               B (no need for lld); C-only (still need embed path)
Test oracle:            exhaustive 0..=255 differential
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_CP866_UPPER=0 keeps FASM body
```

**Phase B-1 may proceed only after this document is committed to the tree.**  
Do not start Cut C after implementation — stop when Cut B verification matrix is green.
