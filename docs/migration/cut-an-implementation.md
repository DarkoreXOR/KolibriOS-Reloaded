# Cut AN Implementation — `ansi2uni_char`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-an-plan.md`](cut-an-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ansi2uni_char` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 6 sites / 4 files (`parse_fn`, `fat`, `iso9660`×2, `font`) |
| Rust symbol | `rust_ansi2uni_char` |
| Pure helper | `kolibri_utils::cp866_decode` |
| Subsystem | Unicode / CP866 → Unicode decode |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Encode already shipped as Cut A (`cp866_encode` /
`unicode.cp866.encode`); pairing decode with encode does not create a
Rust-owned Unicode subsystem (string orchestrators remain FASM). EXT
write/all-times, XFS W+AM, AH+AI, ISO CD (already AJ-routed), and socket
membership remain non-Path-A. See [`cut-an-plan.md`](cut-an-plan.md).

REG-001 lesson applied: trampoline preserves **ECX+EDX** (FAT/ISO `loop`
counters; stdcall clobber class).

---

## Candidate comparison (post-AM audit)

| Candidate | Outcome |
|-----------|---------|
| `ansi2uni_char` | **Selected** — CP866 decode; ISO `--disk` soak; REG-001 trampoline focus |
| `blit_clip` | #2 — H composition; desktop-only soak |
| `fat_name_is_legal` | #3 — thin after U; no FAT `--disk` |
| EXT write / thin v4 / CD / sockets / memmove / `xfs_hashname` | Reject / defer |

---

## Legacy ABI

FASM leaf in `parse_fn.inc` (retained under `USE_RUST_ANSI2UNI_CHAR=0`):

```text
call/ret (not stdcall)
in:  AL = CP866 byte (body starts with movzx eax, al)
out: AX = Unicode code unit
preserves: ECX, EDX, EBX, ESI, EDI, EBP (body touches EAX only on hot paths)
```

Critical quirks retained:

* `0x14` → `U+00B6`
* `0x80..=0xAF` → `0x0410..=0x043F`
* `0xE0..=0xEF` → `0x0440..=0x044F`
* `0xF0..=0xF7` → `uni2ansi_char.table[i] + 0x400`
* `0xB0..=0xDF` and `>=0xF8` → `'_'`
* High bits above AL ignored (`movzx`)

---

## Rust ABI

```text
stdcall rust_ansi2uni_char(ch) -> EAX
  AX = Unicode; input truncated to u8
  ret 4
```

Trampoline: `push ecx` / `push edx` / `stdcall rust_ansi2uni_char, eax` /
`pop edx` / `pop ecx` / `ret` (REG-001 / Cut D class).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `unicode.rs` `cp866_decode` + `ffi.rs` section `.text.rust_ansi2uni_char` |
| Extract | `extract_reloc_free_text.py` → `rust_ansi2uni_char.bin` |
| Embed | `kernel/rust/ansi2uni_char.inc` `file` directive |
| Trampoline | `parse_fn.inc` under `USE_RUST_ANSI2UNI_CHAR` |
| Gate | `USE_RUST_ANSI2UNI_CHAR` (dev 0 → prod 1) |
| Smoke | `ansi2uni_char_rust_smoke_test` |

Reloc-free note: an early `match`/`if` chain for F0–F7 emitted a `.rodata`
switch table + GOT; fixed with volatile stack table (Cut A `special_40x`
pattern).

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ansi2uni_char` |
| Blob/object size | 142 bytes |
| Relocations | 0 |
| SHA-256 | `DF4A69FBB8D996233A7FDF9023C4407DE2BCD6F8AF7E4D6D1FD813503EB83E7F` |

Trailing instruction is `ret 4` (`C2 04 00`). Reloc-free verified by extractor
(extraction fails if the section has relocations).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | ASCII; `0x14`→¶; Cyrillic ranges; F0–F7 specials; unk → `'_'`; high-bit truncate |
| Exhaustive | all `0..=0xFF` + high-bit sentinels |
| Encode↔decode round-trip | mapped CP866 set (skips non-invertible `_` ranges) |
| PRNG | 50 000 cases, seed `0x4355544E` (`'CUTN'`) |
| Host tests | **401/401** cargo tests (397 AM baseline + 4 new ansi2uni) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ansi2uni_char_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | ASCII; ¶; Cyrillic; F0/F1/F7; unk; direct `rust_*`; ECX loop×8 |
| Canaries | ECX=`0xC1C10001`, EDX=`0xD2D20002` across public call (REG-001) |
| Marker | `rust_ansi2uni_char_smoke_result = 'A2SU'` on success |

---

## QEMU validation

Kernels built with Cuts A–AM production gates intact (`USE_RUST_XFS_GET_BEFORE_BY_HASHVAL=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_ANSI2UNI_CHAR=0` | **OK** (QMP `running` + screendump `dev_build/cut-an-off.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_ANSI2UNI_CHAR=1` | **OK** (screendump `dev_build/cut-an-on.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

### A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop smoke | 779426 non-black | 779426 non-black | Match |
| `--disk iso9660` boot+desktop | 779426 non-black | 779426 non-black | Match |

### Real subsystem soak

`--disk iso9660` A/B: **PASS** (QMP `running`, identical non-black counts with
ATAPI CD attached). Boot ABI smoke covers CP866→Unicode vectors used by ISO
ASCII→UTF-16 name copy (`iso9660.inc` callers).

Scripted ISO directory browse / path-lookup harness: **NOT AVAILABLE** (same
class as Cut AJ). Desktop font CP866 path is exercised by boot UI text.

Production image: `dev_build/cut-an-final.img`.

e1000: **N/A**

---

## Regressions discovered

**NONE** during Cut AN validation.

---

## Production gate

```text
USE_RUST_ANSI2UNI_CHAR = 1
```

Rollback: `USE_RUST_ANSI2UNI_CHAR = 0` (or `enabled = false` in
`project/build.toml` Cut AN migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/unicode.rs` — `cp866_decode` + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_ansi2uni_char`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/fs/parse_fn.inc` — trampoline + gate + FASM rollback body
* `kernel/rust/ansi2uni_char.inc` — blob embed + ABI smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-an-plan.md` / `cut-an-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No scripted ISO path-lookup / Eolite browse soak (attach-only A/B).
* `uni2ansi_char` FASM body remains (Cut A already covers encode via
  `unicode.cp866.encode`); not claimed as Path A pair.
* `cp866toUTF8_string` orchestration remains FASM.
* No Path A claim.
