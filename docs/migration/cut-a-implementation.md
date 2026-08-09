# Cut A Implementation Note — CRC & Unicode

**Stage:** Stage 2 / Cut A  
**Date:** 2026-08-09  
**Scope:** `kernel/crc.inc` + `kernel/unicode.inc` only.  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Summary

| FASM symbol | Convention | Pure? | Rust symbol | Confidence |
|-------------|------------|-------|-------------|------------|
| `crc_32` | stdcall + **EAX in/out** partial | yes* | `rust_crc_32` (+ FASM trampoline keeps `crc_32`) | **HIGH** |
| `unicode.utf8.decode` | register (`ESI`/`ECX` in-out, `EAX` out) | yes* | `rust_unicode_utf8_decode` | **HIGH** |
| `unicode.utf16.encode` | register (`EAX` in/out) | yes | `rust_unicode_utf16_encode` | **HIGH** |
| `unicode.cp866.encode` | register (`EAX`/`AX` in, `AL` out); calls `uni2ansi_char` | yes | `rust_unicode_cp866_encode` | **HIGH** |

\*Pure with respect to globals: no global reads/writes. Still reads caller-provided memory for CRC/UTF-8.

**Not migrated (out of `crc.inc`/`unicode.inc`):** `utf8to16`, `uni2ansi_char`, `ansi2uni_char`, and other helpers in [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc).  
`unicode.cp866.encode`’s Rust body **embeds** the `uni2ansi_char` mapping so the Rust path does not call back into FASM. FASM `uni2ansi_char` remains for its other call sites.

---

## 1. `crc_32`

### Identity

| Field | Value |
|-------|-------|
| Original symbol | `crc_32` |
| Source | [`kernel/crc.inc:16`](../../kernel/crc.inc) |
| Included from | [`kernel/kernel32.inc`](../../kernel/kernel32.inc) (`include "crc.inc"`) |

### Call sites (LOCAL FACT)

| Site | Notes |
|------|-------|
| `kernel/blkdev/disk.inc` ~916, ~974 | GPT: poly `0xEDB88320`; `mov eax,-1` before; `xor eax,-1` after |
| `kernel/fs/ext.inc` many | poly `EXT_CRC_POLY = 0x82F63B78`; running hash in `EAX` |

### Calling convention (LOCAL FACT)

- Declared as `proc crc_32 _poly, _buffer, _length` with **default** `proc` flags → epilogue `retn 12` (**stdcall** stack cleanup). See [`kernel/proc32.inc`](../../kernel/proc32.inc) `epiloguedef`.
- Stack args (after `call`): `[esp+4]=poly`, `[esp+8]=buffer`, `[esp+12]=length` (plus frame via `push ebp` / `mov ebp,esp`).
- **`EAX` = partial CRC in and updated CRC out.** Not a stack argument. Comment in source: “Partial hash in assumed to be eax”.
- Callee saves/restores: `EBX`, `ECX`, `EDX`, `ESI` (manual). Uses `EBP` frame. Does **not** preserve `EDI` (untouched). Flags clobbered.

### Algorithm (LOCAL FACT)

Bit-oriented reflected CRC: for each byte, for each of 8 bits, `eax >>= 1` and conditionally `eax ^= poly` when `(eax^byte)&1` before shift. No table. Length loop: `dec length; js done` → **length 0 leaves `EAX` unchanged**.

### Edge cases

- Zero length: no memory read; `EAX` unchanged.
- Length treated via signed `js` after `dec` (callers pass small positive lengths).
- 32-bit wrapping XOR/shifts as ordinary x86 dword ops.
- Polynomials observed: `0xEDB88320` (GPT), `0x82F63B78` (ext4 CRC32C-style).

### Proposed Rust FFI

```text
#[no_mangle]
pub unsafe extern "stdcall" fn rust_crc_32(
    partial: u32,
    poly: u32,
    buffer: *const u8,
    length: u32,
) -> u32;
```

**Why not bare `extern "stdcall" fn crc_32(poly, buffer, length)`?**  
A Rust stdcall prologue may clobber `EAX` before the partial hash can be read. **INFERENCE:** explicit `partial` argument + thin FASM trampoline that passes `EAX` is the safe hybrid boundary. Call sites keep `stdcall crc_32, ...`.

### FASM trampoline (Phase C; not applied until hybrid link works)

```asm
proc crc_32 _poly, _buffer, _length
        stdcall rust_crc_32, eax, [_poly], [_buffer], [_length]
        ret
endp
```

Original body kept under rollback (see Rollback).

---

## 2. `unicode.utf8.decode`

### Identity

| Field | Value |
|-------|-------|
| Symbol | `unicode.utf8.decode` |
| Source | [`kernel/unicode.inc:10`](../../kernel/unicode.inc) |

### Call sites (LOCAL FACT)

Always paired with an encode step; `ESI`/`ECX` set up by caller to a UTF-8 name buffer + length:

- `kernel/fs/xfs.asm` — `xfs._.copy_filename`
- `kernel/fs/ext.inc` — directory/volume name conversion paths

### Calling convention (LOCAL FACT)

| Reg | In | Out |
|-----|----|-----|
| `ESI` | ptr to next UTF-8 byte | advanced by consumed bytes |
| `ECX` | remaining byte count | decreased by consumed bytes |
| `EAX` | (scratch) | Unicode scalar (or `0xFFFD`) |

- **Not** stdcall. Plain `call` / `ret` (callee does not touch stack args).
- Clobbers: `EAX`, `ESI`, `ECX`, flags. Does not use `EBX`/`EDX`/`EDI`.

### Behavior

- `ECX == 0` on entry: jump to `.done` without changing `EAX`/`ESI` (**LOCAL FACT** — `test ecx,ecx; jz .done`). Callers must not rely on `EAX` when length was already 0.
- 1–4 byte sequences via leading-bit tests (`shl al`).
- Truncated / invalid lead or continuation: `EAX = 0xFFFD`, consume **1** byte (`dec ecx; inc esi`).
- **No** explicit overlong or `> U+10FFFF` rejection beyond what the bit parsing produces (**LOCAL FACT**).

### Proposed Rust FFI

```text
#[no_mangle]
pub unsafe extern "stdcall" fn rust_unicode_utf8_decode(
    ptr_inout: *mut *const u8,
    len_inout: *mut u32,
) -> u32;   // codepoint
```

FASM trampoline preserves register ABI at call sites:

```asm
unicode.utf8.decode:
        push    ecx
        push    esi
        push    esp          ; &esi
        lea     eax, [esp+4] ; &ecx  (after pushes: careful layout)
        ; preferred clearer form:
```

Clearer trampoline:

```asm
unicode.utf8.decode:
        push    ecx
        mov     eax, esp      ; pointer to length
        push    esi
        mov     edx, esp      ; pointer to ptr
        stdcall rust_unicode_utf8_decode, edx, eax
        pop     esi
        pop     ecx
        ret
```

---

## 3. `unicode.utf16.encode`

### Identity

| Field | Value |
|-------|-------|
| Symbol | `unicode.utf16.encode` |
| Source | [`kernel/unicode.inc:99`](../../kernel/unicode.inc) |

### Call sites

Same XFS/ext name paths after `unicode.utf8.decode`. Caller does `stosw` then `shr eax,16` / optional second `stosw` for surrogate pairs.

### Calling convention (LOCAL FACT)

| Reg | In | Out |
|-----|----|-----|
| `EAX` | Unicode code point | BMP char in low 16 bits, high 16 = 0; **or** packed surrogates |

Packed supplementary form (**LOCAL FACT**): after encode, low word = high surrogate, high word = low surrogate (`or eax, 0xDC00D800` after `ror eax,16`). Invalid / surrogate-range BMP → `EAX = 0xFFFD`.

- Clobbers: `EAX`, flags only.

### Proposed Rust FFI

```text
#[no_mangle]
pub unsafe extern "stdcall" fn rust_unicode_utf16_encode(cp: u32) -> u32;
```

Trampoline:

```asm
unicode.utf16.encode:
        stdcall rust_unicode_utf16_encode, eax
        ret
```

---

## 4. `unicode.cp866.encode`

### Identity

| Field | Value |
|-------|-------|
| Symbol | `unicode.cp866.encode` |
| Source | [`kernel/unicode.inc:94`](../../kernel/unicode.inc) — `call uni2ansi_char; ret` |
| Real logic | [`kernel/fs/parse_fn.inc:135`](../../kernel/fs/parse_fn.inc) `uni2ansi_char` |

### Calling convention (LOCAL FACT via `uni2ansi_char`)

| Reg | In | Out |
|-----|----|-----|
| `AX` / `EAX` | Unicode character (compared as `ax`) | `AL` = CP866 byte |

`uni2ansi_char` may temporarily use `ECX`/`EDI` (pushed/popped) for the 8-byte special table. Unknown chars → `'_'`. `U+00B6` → `20`.

### Proposed Rust FFI

```text
#[no_mangle]
pub unsafe extern "stdcall" fn rust_unicode_cp866_encode(cp: u32) -> u32; // AL meaningful
```

Trampoline:

```asm
unicode.cp866.encode:
        stdcall rust_unicode_cp866_encode, eax
        ret
```

---

## Differential testing status

| Method | Status |
|--------|--------|
| Host unit tests of Rust vs **line-by-line FASM algorithm oracle** (second transcription in test modules) | **PASS — 27/27** (`cargo test -p kolibri_utils`) |
| Cross-check: IEEE CRC-32 of `"123456789"` via FASM-style update + final XOR → `0xCBF43926` | **PASS** |
| Host execution of assembled `crc.inc` / `unicode.inc` | **Not done in Cut A** — FASM is now vendored at [`../../fasm/`](../../fasm/); binary differential PE still not built |
| In-kernel hybrid smoke | **Pass (Phase C + CRC + UTF-16 + CP866 + UTF-8)** — see per-function migration docs; final baseline [`cut-a-final-architecture.md`](cut-a-final-architecture.md) |

**Confidence:** algorithm match **HIGH** (direct port + dual oracle + known CRC vector + exhaustive Unicode sweeps). End-to-end FFI-in-image **PASS** (QEMU desktop + hang-on-fail smokes).

---

## Build integration mechanism

1. Workspace crate [`rust_kernel/kolibri_utils`](../../rust_kernel/kolibri_utils) — `no_std` on `os=none`, no allocator. Cargo workspace root is [`rust_kernel/`](../../rust_kernel/) (separate from FASM [`kernel/`](../../kernel/)).
2. Custom target [`rust_kernel/kolibri_utils/i686-kolibri-none.json`](../../rust_kernel/kolibri_utils/i686-kolibri-none.json) (`i686-unknown-none` not shipped in this rustc).
3. Freestanding build (**verified**):
   ```text
   cd rust_kernel
   cargo +nightly build -Z build-std=core,compiler_builtins -Z json-target-spec \
     -p kolibri_utils --release --target kolibri_utils/i686-kolibri-none.json
   ```
   Produces `libkolibri_utils.a`. Archive contains undecorated symbols:
   `rust_crc_32`, `rust_unicode_utf8_decode`, `rust_unicode_utf16_encode`,
   `rust_unicode_cp866_encode` (**LOCAL FACT** — string scan of `.a`).
4. Helper script: [`rust_kernel/kolibri_utils/build-cut-a.ps1`](../../rust_kernel/kolibri_utils/build-cut-a.ps1).
5. Phase C trampoline sketches: [`rust_kernel/kolibri_utils/fasm/trampolines.inc.example`](../../rust_kernel/kolibri_utils/fasm/trampolines.inc.example).
6. Hybrid link into `kernel.mnt`: Phase C probe + CRC + UTF-16 + CP866 + UTF-8 via reloc-free section extract + FASM `file` ([`phase-c-integration.md`](phase-c-integration.md), [`crc32-migration.md`](crc32-migration.md), [`utf16-migration.md`](utf16-migration.md), [`cp866-migration.md`](cp866-migration.md), [`utf8-migration.md`](utf8-migration.md)).
7. **`kernel/`** Phase C smoke + CRC (`USE_RUST_CRC`) + UTF-16 (`USE_RUST_UTF16`) + CP866 (`USE_RUST_CP866`) + UTF-8 (`USE_RUST_UTF8`) trampolines.

### Blockers (exact)

1. ~~`kernel/init.inc` corrupted~~ — **resolved** (FASM baseline restoration).
2. ~~FASM flat binary cannot consume a Rust `staticlib` directly~~ — **resolved for all Cut A functions** (reloc-free extract + `file`; `rust-lld` not required).
3. (Resolved for tooling) Vendored [`../../fasm/`](../../fasm/) is available; system `fasm` on `PATH` is not required.

---

## Phase status

| Phase | Status |
|-------|--------|
| A — Rust exists; FASM authoritative | **Done** |
| B — Differential (algorithm oracle) passes | **Done** (host binary FASM exec still optional) |
| C — Hybrid kernel calls Rust | **Done** (probe + all Cut A trampolines) |
| D — Remove FASM bodies | **Not started** — originals retained under `USE_RUST_*=0` (rollback is intentional) |

Final architecture summary: [`cut-a-final-architecture.md`](cut-a-final-architecture.md).

---

## Rollback

1. Set `USE_RUST_CRC` / `USE_RUST_UTF16` / `USE_RUST_CP866` / `USE_RUST_UTF8` to `0` in `kernel/crc.inc` / `kernel/unicode.inc` (original FASM bodies remain in `else` branches).
2. Optionally drop corresponding `rust/*.inc` includes and smoke calls; rebuild.
3. No other subsystems depend on `kolibri_utils` beyond these wires.

---

## UNKNOWNs

- Whether any out-of-tree code calls `crc_32` / unicode symbols with different conventions (**UNKNOWN** — no app corpus).
- Whether `unicode.utf8.decode` with `ecx=0` leaving stale `eax` is relied upon (**LOCAL FACT** behavior; Rust returns `0`; in-tree callers test `ecx` in the loop).
