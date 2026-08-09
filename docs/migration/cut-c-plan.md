# Cut C Plan — Candidate Audit & Decision Record

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut C** is the next pure-utility migration after Cut B (`cp866toUpper`).  
> Cut A and Cut B remain complete and must not be redone.

---

## Candidate

| Field | Value |
|-------|-------|
| **Function** | `utf16toUpper` |
| **Source** | [`kernel/fs/parse_fn.inc:134–152`](../../kernel/fs/parse_fn.inc) |
| **Purpose** | Convert one UTF-16 code unit in `AX` to uppercase for FS name compare (Latin + Cyrillic BMP ranges only) |

---

## Why this candidate

**LOCAL FACT** — Cut B already ranked `utf16toUpper` as the strongest alternate after `cp866toUpper` ([`cut-b-plan.md`](cut-b-plan.md) §2–3). Repository re-audit confirms that ranking.

| Preference | How `utf16toUpper` scores |
|------------|---------------------------|
| Well-defined ABI | Register `AX` in/out; plain `ret`; no stack args |
| Small surface | ~19 FASM instructions; closed transform |
| Deterministic | Pure function of input word |
| Few dependencies | None (no callees, globals, memory, HW, IRQ, sched) |
| Differential potential | Exhaustive `0..=0xFFFF` (65536) feasible on host |
| Clear rollback | Independent `USE_RUST_UTF16_UPPER` |
| Useful | **11** live FS call sites (FAT/exFAT/NTFS/ISO9660 name paths) |

**Why safer/useful than alternatives (re-audited):**

| Alternate | Why not Cut C |
|-----------|----------------|
| `uni2ansi_char` | Overlaps Cut A CP866 encode; has `.table` → reloc risk unless rewritten |
| `ansi2uni_char` | Reads `uni2ansi_char.table` (absolute load) |
| `strncmp` | stdcall + PE/`n=-1` quirks; larger ABI surface |
| `strlen` (parse_fn) | Memory walk; only 2 callers |
| `utf8to16` / `UTF16to8` | Pointer advances / SF flag ABI; separate from Cut A UTF-8 |
| `checksum_2` | Network-adjacent; quirky `0→FFFF` / byte-swap |
| Allocator / sched / IRQ / paging | Explicitly out of scope |

`utf16toUpper` continues Cut B’s casefold pattern on a **new** symbol, proves the pipeline again with a 16-bit domain, and exercises a wider real caller set without leaving Strategy A.

---

## Candidate inventory (summary)

Audited from live `kernel/` sources. Cut A/B symbols are closed.

| Function | Source | Approx complexity | Callers | Convention | Args | Return | Modified | Preserved | Globals | Memory | External | Static | HW / IRQ / sched | Determinism | Testability | Likely section | Reloc risk | Migration risk |
|----------|--------|-------------------|--------:|------------|------|--------|----------|-----------|---------|--------|----------|--------|------------------|-------------|-----------------|----------------|------------|----------------|
| **`utf16toUpper`** | `parse_fn.inc` | tiny leaf | **11** | reg / `ret` | `AX` | `AX` | `EAX` | none req. | none | none | none | none | none | yes | exhaustive 64K | `.text.rust_utf16_to_upper` | none | **Low — SELECT** |
| `ansi2uni_char` | `parse_fn.inc` | small + table | **5**+ | reg / `ret` | `AL` | `AX` | `EAX` | — | none | table read | none | via other | none | yes | 256 | would need inline table | abs table | Low–med |
| `uni2ansi_char` | `parse_fn.inc` | small + table | **10**+ | reg / `ret` | `AX` | `AL` | `EAX` | — | none | `.table` | none | 8 B | none | yes | 65536 sparse | — | abs `.table` | Low (defer) |
| `strncmp` | `string.inc` | medium | **11**+exports | stdcall `ret 12` | 3 dwords | `EAX` −1/0/1 | ESI/EDI | ESI/EDI pushed | none | yes | none | none | none | yes | corpus | `.text.rust_strncmp` | none | Low–med |
| `strlen` | `parse_fn.inc` | tiny | **2** | ESI→ECX | ESI | ECX | ECX | EDI/EAX | none | yes | none | none | none | yes | corpus | — | none | Low |
| `utf8to16` | `parse_fn.inc` | medium | **16** | ESI↑ / AX | ESI | AX | ESI/EAX | — | none | yes | none | none | none | yes | corpus (≠ Cut A) | — | none | Med |
| `UTF16to8` | `parse_fn.inc` | medium | **5**+ | AX; EDI↑ ECX↓; **SF** | AX/EDI/ECX | flags+mem | EDI/ECX/EAX | — | none | yes | none | none | none | yes | corpus | — | none | Med |
| `checksum_2` | `stack.inc` | tiny | **7** | EDX→DX | EDX | DX | EDX/ECX | — | none | none | none | none | none (net) | yes | 32-bit domain | — | none | Low–med |

---

## ABI

**LOCAL FACT** — body `parse_fn.inc:134–152`; representative caller `fat.inc:1031–1036`:

```asm
        call    utf8to16
        call    utf16toUpper
        mov     edx, eax
        mov     ax, [edi]
        call    utf16toUpper
        cmp     ax, dx
```

| Item | Contract |
|------|----------|
| **Inputs** | UTF-16 code unit in **AX** (conceptually). Arithmetic uses **EAX** |
| **Outputs** | Uppercased unit in **AX**; Rust path returns **zero-extended EAX** |
| **Stack** | No stack arguments |
| **Registers** | Clobbers `EAX` (and flags). Does not push/pop |
| **Caller/callee saved** | None beyond plain `call`/`ret`. Callers that need other regs save them themselves |
| **Stack cleanup** | Caller cleanup — plain `ret` (0 bytes). Rust FFI `stdcall` 1 dword → Rust `ret 4`; trampoline absorbs |
| **Flags** | Clobbered; callers must not rely on flags |
| **Memory side effects** | **None** |

### Algorithm (LOCAL FACT — FASM body)

| Input AX | Action |
|----------|--------|
| `< 'a'` (0x61) | unchanged |
| `'a'..'z'` | `EAX -= 32` |
| `0x007B..0x042F` | unchanged |
| `0x0430..0x044F` | `EAX -= 32` (Cyrillic а–я → А–Я) |
| `0x0450..0x045F` | `EAX -= 80` (`0x50`) → `0x0400..0x040F` |
| `≥ 0x0460` | unchanged |

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Memory | **none** |
| External functions | **none** |
| Static data | **none** |
| Hardware | **none** |
| Other | **none** |

---

## Testing strategy

| Gate | Method |
|------|--------|
| **Oracle** | Host differential: Rust vs FASM-faithful control-flow oracle |
| **Boundary cases** | `0x00`, `0x60`, `0x61`, `0x7A`, `0x7B`, `0x42F`, `0x430`, `0x44F`, `0x450`, `0x45F`, `0x460`, `0xFFFF`; Latin/`Ё`/`ё` if in range |
| **Exhaustive** | **Yes** — full `0u16..=0xFFFF` |
| **Deterministic corpus** | Named edge vectors in Rust tests |
| **Kernel smoke** | Hang-on-fail via trampoline (`'a'→'A'`, `0x450→0x400`); called from `high_code` |

---

## Link strategy

**Selected: Strategy A + D** (reloc-free raw blob + FASM ABI trampoline).

```text
rust_utf16_to_upper in .text.rust_utf16_to_upper
  → extract (0 relocs, symbol @0, ret 4)
  → kernel/rust/utf16_upper.inc `file`
  → utf16toUpper trampoline under USE_RUST_UTF16_UPPER=1
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Selected** — no `.rodata`, no helpers, no cross-section refs |
| **B** `rust-lld` | **Rejected** — unnecessary for a register leaf |
| **C** Rust + FASM glue | Trampoline only (same as Cut A/B); not a separate architecture |
| **D** reject candidate | **Not applicable** — candidate is suitable |

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_UTF16_UPPER` (`1` default / `0` original FASM) |
| **Original FASM body** | Retained in `else` branch of `parse_fn.inc` |
| **Independence** | Does not require enabling/disabling Cut A or Cut B switches |

---

## Risk assessment

| Known risks | Mitigations |
|-------------|-------------|
| Wider caller set (11 FS sites) vs Cut B’s 1 | Pure leaf; smoke + exhaustive oracle; rollback switch |
| Upper `EAX` bits after FASM `sub eax, N` | Rust returns zero-extended `EAX`; callers compare `AX`/`DX` |
| Accidental regress of Cut A/B | Keep switches independent; run full `kolibri_utils` tests + rebuild all blobs |
| Unexpected relocations | Extractor hard-fails; do not weaken |

---

## Decision record (summary)

```text
Candidate:              utf16toUpper
Why selected:           pure, 64K-domain, no tables, greenfield, FS-useful
Why safer:              smaller ABI than strncmp/utf8to16; no table vs uni2ansi
ABI:                    AX in/out; plain ret; no memory
Dependencies:           none
Global state:           none
Chosen linking:         Strategy A (reloc-free blob) + D (FASM trampoline)
Rejected:               B (no lld need); reject-candidate (not needed)
Test oracle:            exhaustive 0..=0xFFFF differential
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_UTF16_UPPER=0 keeps FASM body
```

**Implementation may proceed only after this document is in the tree.**  
Do not start Cut D after Cut C verification is green — **STOP**.
