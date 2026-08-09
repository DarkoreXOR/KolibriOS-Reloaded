# Cut Q Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-q-implementation.md`](cut-q-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut Q** is the first migration of a **SF-out streaming UTF-16→UTF-8 encode leaf** — `UTF16to8`, whose legacy ABI returns via **SF**, mutates **EDI/ECX**, and preserves **EBX/EDX/ESI/EBP**.  
> Cuts A–P remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `UTF16to8` |
| **Source** | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| **Subsystem** | FS path / Unicode streaming encode |
| **Purpose** | Encode one UTF-16 code unit to UTF-8 with signed ECX byte budget and SF abort |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `UTF16to8` | **Selected** — SF-out ABI novelty after Cut P ZF-out; real ISO/NTFS/FAT/exFAT/LFN callers |
| `memmove` | Deferred — Stage-4 memory class; ~24 hot callers; thin algorithm |
| `xfs._.extent_unpack` | Deferred — XFS-only soak; EBP partition mutate |
| `window._.check_window_position` | Deferred — second GUI leaf after Cut N |
| `coff_get_align` / `pci_make_config_cmd` / `mutex_init` | Rejected — too thin |
| `strtoint_dec` | Rejected — `conf_lib` not linked |

---

## Why Cut Q is a meaningful next step

Cuts A–P proved utils / FS / video / HID / TCP / GUI font / process header / ZF syscall gate.

Cut Q answers:

> Does Strategy A + C remain viable for a **register-streaming SF-out encode leaf** (AX/EDI/ECX; partial ECX burn-down; INT_MIN escape; surrogate-as-UCS-2 3-byte) with a bit-exact differential oracle?

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; trampoline preserves public register/SF ABI; `USE_RUST_UTF16_TO_8` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Inputs | `AX` code unit; `EDI` dest; `ECX` signed remaining bytes |
| Result | **SF=1** abort / **SF=0** success |
| Mutates | `EDI` (+L on success only); `ECX` (pre-decrement schedule) |
| Preserved | **EBX, EDX, ESI, EBP**; DF unchanged |
| EAX | zero-extended input on fail/ASCII; 2/3-byte encoding residues |

Overflow / edge quirks retained: partial ECX burn-down; negative ECX decrements once; `ECX=INT_MIN` may encode; surrogates encoded independently.

---

## Out of scope

* Migrating `UTF16to8_string` / `cp866toUTF8_string` wrappers  
* “Fixing” INT_MIN / surrogate / SF semantics  
* Claiming NTFS/exFAT/ISO live validation on stock floppy (no attached volumes)  
* Cut R  

---

## Completion rule

Complete Cut Q gates → document → **STOP**. Do not start Cut R.
