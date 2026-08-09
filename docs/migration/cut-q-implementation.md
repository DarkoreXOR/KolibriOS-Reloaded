# Cut Q Implementation — `UTF16to8`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-q-plan.md`](cut-q-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `UTF16to8` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 7 direct + wrappers (`UTF16to8_string`, `cp866toUTF8_string`) across FAT/NTFS/exFAT/ISO/LFN |
| Rust symbol | `rust_utf16_to_8` |
| Pure helper | `kolibri_utils::utf16_to_8` |
| Subsystem | FS / Unicode streaming encode |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `UTF16to8` | **Selected** — SF-out + streaming encode; complements Cut P ZF-out |
| `memmove` | Deferred — blast radius / thin algorithm |
| `xfs._.extent_unpack` | Deferred — XFS-only soak |
| `window._.check_window_position` | Deferred — GUI after Cut N |
| Thin PE/PCI/sync / dead conf | Rejected |

---

## Why selected

Cut Q’s research question: does Strategy A + C remain viable for a **SF-out register-streaming encode leaf** (AX/EDI/ECX; partial ECX burn-down; INT_MIN escape; surrogate-as-UCS-2) as a reloc-free blob with a byte-exact differential oracle?

---

## Special ABI handling

`UTF16to8` does **not** return via EAX as the primary channel. Legacy callers branch on **SF** (`js`/`jns`). Rust returns packed `(SF<<31)|EAX`; the FASM trampoline:

```text
mov ebx, eax          ; packed
and eax, 0x7FFFFFFF   ; residue
test ebx, ebx         ; SF = bit 31
pop esi / edx / ebx   ; flag-neutral
ret
```

Stack slots for EDI/ECX; `lea esp` (not `add esp`) for flag-neutral cleanup. EBX/EDX/ESI restored after SF reconstruct. EBP/DF untouched.

---

## Original implementation

FASM leaf retained under `USE_RUST_UTF16_TO_8=0` in `parse_fn.inc` (ASCII / 2-byte / 3-byte paths with `dec ecx` / `js .ret`).

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/utf16_to_8.rs`](../../rust_kernel/kolibri_utils/src/utf16_to_8.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_utf16_to_8` |
| Build | [`rust_kernel/kolibri_utils/build-utf16-to-8.ps1`](../../rust_kernel/kolibri_utils/build-utf16-to-8.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_utf16_to_8.bin` |
| Embed | [`kernel/rust/utf16_to_8.inc`](../../kernel/rust/utf16_to_8.inc) |

`#![no_std]` freestanding; unrolled stores; no tables / GOT.

### Blob lock

| Field | Value |
|-------|-------|
| Size | **211** bytes |
| Relocations | **0** |
| SHA-256 | `C171137E49051347AC4B522EC58A62193E9959DB919340C0727BAA9C4FC57A58` |
| Epilogue | `ret 12` |
| CALL/GOT/PLT | none (false-positive `C1 E8` = `shr eax,imm8` only) |

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **169/169** |
| Exhaustive AX × ECX budgets | **PASS** |
| Boundary / exhaustion / surrogate / INT_MIN / negative ECX | **PASS** |
| EAX residues (80C2 / BFDF / 80A0 / BFBF) | **PASS** |
| PRNG 200k (seed `0x43555451`) | **PASS** |
| Trampoline SF/EAX pack model | **PASS** |

---

## In-kernel smoke

`utf16_to_8_rust_smoke_test` (wired after Cut P smoke):

* Public `UTF16to8` → trampoline → Rust  
* ASCII / 2-byte / 3-byte success + mid-exhaust; ECX=-1; INT_MIN; U+D800; residues  
* EBX/EDX/ESI sentinels; 0xA5 no-write-on-fail  
* Fail hang: `EAX=0xDEAD0C51`, `EBX='UTF8'`, `ECX='FAIL'`  

---

## QEMU validation

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| ON | `USE_RUST_UTF16_TO_8=1` | **OK** (QMP `running` + screendump `tmp_images/cut-q-on.ppm`, 3038 non-black samples) | **OK** (e1000 + user net; idle HLT; Cut D EDX path intact) |
| OFF | `=0` (original FASM body) | **OK** (screendump `tmp_images/cut-q-off.ppm`, same non-black sample count) | **OK** (same NIC config; FASM baseline) |

Smoke (ON): **PASS** (no `0xDEAD0C51`; boot continued).

### Live filesystem paths actually exercised

* FAT12 floppy boot root + desktop `/sys` app launch path resolution  
* Stock image names predominantly ASCII / 8.3 (e.g. `FILEMA~1`)

### Not exercised on stock floppy (no attached volumes)

* Direct ISO9660 `UTF16to8` sites  
* Direct NTFS `UTF16to8` site  
* Direct exFAT `UTF16to8_string` sites  
* Visual non-ASCII / 2-byte / 3-byte LFN UI confirmation  

Production default after completion: **`USE_RUST_UTF16_TO_8 = 1`**.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel-cut-q-on.mnt` | 232904 | `108CC1DC3974D0AA545E5505E8BD43CDDE1828094DF5101322896A24D76AA3D6` |
| `kernel-cut-q-off.mnt` | 232936 | `9B9C33C8934691F3D8AF483C4F0BE3BC7B77C368C3667AB348471BCEC9334DA1` |

Size delta (trampoline vs FASM body with blob always embedded) is expected; not a failure criterion.

---

## Rollback

```text
USE_RUST_UTF16_TO_8 = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/utf16_to_8.inc`. Independent of Cuts A–P. Cut D EDX trampoline and Cut P switch untouched.

---

## Evidence summary

### PROVEN

* SF-out trampoline with EAX residue + EDI/ECX update  
* Bit-exact ECX burn-down / INT_MIN / surrogate / partial exhaust  
* Freestanding 211-byte blob, 0 relocs  
* Host differential + 200k PRNG  
* In-kernel smoke hang-on-fail  
* QEMU ON/OFF desktop; NIC attached both builds  

### NOT PROVEN

* Live NTFS / exFAT / ISO UTF16to8 call sites on stock floppy  
* Visual non-ASCII filename rendering under QEMU  

### OUT OF SCOPE

* Wrapper migrations (`UTF16to8_string` / `cp866toUTF8_string`)  
* Unicode scalar “fixes”  
* Cut R  

---

## Cut Q status

**COMPLETE.** Do not start Cut R until explicitly instructed.
