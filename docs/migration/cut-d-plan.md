# Cut D Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut D** is the next production utility migration after Cut C (`utf16toUpper`).  
> Cut A / B / C remain complete and must not be redone. Proof-of-life is diagnostic only (branch `tmp/rust_pof`) — not a Cut D dependency.

---

## Selected candidate

| Field | Value |
|-------|-------|
| **Function** | `strncmp` |
| **Source** | [`kernel/core/string.inc:58–84`](../../kernel/core/string.inc) |
| **Purpose** | Compare up to `n` bytes of two C strings; return −1 / 0 / 1 (unsigned byte order). Used by PE import resolution, DLL/COFF symbol match, heap name checks; also PE-exported. |

---

## Alternatives considered

### Remaining `parse_fn.inc` candidates

| Candidate | Why rejected |
|-----------|----------------|
| `uni2ansi_char` | Overlaps Cut A CP866 encode; absolute `.table` → Strategy A reloc risk unless inlined/rewritten |
| `ansi2uni_char` | Reads `uni2ansi_char.table` (absolute load); same table-coupling issue |
| `strlen` (parse_fn) | Still `parse_fn` locality after Cuts B/C; only 2 callers; less generalization proof |
| `utf8to16` | Pointer-advance ABI; separate oracle from Cut A UTF-8 decode; medium trampoline risk |
| `UTF16to8` | Pointer + `ECX` countdown + **SF** on exhaust — flags ABI is higher risk for this cut |

### Other subsystems

| Candidate | Why rejected |
|-----------|----------------|
| `strnlen` / `strncpy` / `strrchr` (`string.inc`) | Viable later package; weaker immediate caller density / more quirks (`strrchr` uses `std`/`cld`) than `strncmp` |
| `checksum_2` (`network/stack.inc`) | Network-adjacent; `0→FFFF` / byte-swap quirks; harder real-caller smoke without stack traffic |
| `checksum_1` | Paired with `checksum_2`; larger surface; defer as a pair |
| `is_region_userspace` | ~30 callers; security-surface; `OS_BASE` constant dependency |
| `memmove` | Hot path; misnamed forward-only copy; medium–high risk |
| Allocator / sched / IRQ / paging / drivers | Explicitly out of scope |

---

## Why the selected candidate

Cuts B and C both migrated pure register leaves from `kernel/fs/parse_fn.inc`. Cut D must show the **same architecture generalizes** beyond that file and beyond casefold.

| Preference | How `strncmp` scores |
|------------|----------------------|
| Clear ABI | FASM `proc … stdcall` — 3 stack dwords in, `EAX` −1/0/1 out, callee `ret 12` |
| Moderate scope | ~25 instructions; closed string compare; no callees |
| Deterministic | Pure function of two buffers + `n` |
| Few hidden dependencies | None (no globals, HW, IRQ, sched); reads caller memory only |
| Strong oracle | Host differential vs FASM-faithful control-flow oracle; corpus + `n=0` / `n=-1` |
| Safe execution point | In-kernel smoke with static strings; boot PE path also uses it |
| Easy rollback | Independent `USE_RUST_STRNCMP` |
| Generalization | **New subsystem** (`core/string.inc`); stdcall + memory walk + PE export surface |

**parse_fn vs other subsystems (explicit):** Continuing another `parse_fn` leaf would be locally convenient but would *not* stress a new include / calling style. `strncmp` keeps Strategy A+D while proving the pipeline on `string.inc` stdcall utilities — the evidence-based choice for Cut D.

---

## ABI

**LOCAL FACT** — body `string.inc:58–84`; representative callers:

```asm
; heap.inc — stdcall macro
        stdcall strncmp, edx, ebx, 32

; dll.inc — unlimited compare via n = -1
        stdcall strncmp, eax, edi, -1

; peload.inc — manual pushes + call (same stdcall stack layout)
        push    32
        push    eax
        push    esi
        call    strncmp
```

| Item | Contract |
|------|----------|
| **Inputs** | `s1`, `s2`, `n` — three dwords on stack (stdcall, right-to-left) |
| **Outputs** | `EAX` ∈ {−1, 0, +1} — `(s1>s2) − (s1<s2)` from **unsigned** byte compare |
| **Registers** | FASM body saves/restores `ESI`/`EDI`. Rust `stdcall` must preserve callee-saved regs (`EBX`/`ESI`/`EDI`/`EBP`) |
| **Stack** | Callee cleans **12** bytes (`ret 12`). No extra frame required beyond FASM `proc` / Rust prologue |
| **Caller-saved** | As usual for `call`; flags clobbered |
| **Callee-saved** | `ESI`, `EDI` (and standard stdcall set) |
| **Stack cleanup** | Callee (`ret 12`) |
| **`n == 0`** | Immediate equal → `EAX = 0` |
| **`n == -1` (0xFFFFFFFF)** | Practically “until NUL or mismatch” (counter wraps only after 2³² steps) |
| **Memory side effects** | **None** (read-only) |
| **Direction flag** | Body uses `cld`; must leave DF clear (or restore). Rust must not leave DF set |

### Algorithm (LOCAL FACT — FASM body)

1. If `n == 0` → return 0.  
2. Else loop: `cmpsb`; on inequality → `seta`/`setb` → `movsx` → return.  
3. On equality: if last byte was `0` → return 0.  
4. `dec n`; if non-zero continue; else return 0 (length exhausted, all equal).

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Memory | Read-only at `s1`/`s2` for up to `n` bytes (or until NUL) |
| Static data | **none** |
| External calls | **none** |
| Hardware | **none** |
| Other | PE export name `strncmp` must remain the same symbol |

---

## Oracle

| Item | Plan |
|------|------|
| Existing FASM oracle | Control-flow faithful host oracle mirroring `string.inc` |
| Differential strategy | Named corpus + deterministic pseudo-random buffers; explicit `n=0`, `n=1`, `n=-1`, equal/prefix/mismatch/embedded-NUL |
| Boundary cases | Empty vs empty; empty vs non-empty; first-byte differ; last-of-`n` differ; unsigned `0xFF` vs `0x00`; identical then NUL |
| Exhaustive possibility | **No** (unbounded string domain) — corpus + PRNG with fixed seed |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_strncmp in .text.rust_strncmp
  → extract (0 relocs, symbol @0, ret 12)
  → kernel/rust/strncmp.inc `file`
  → strncmp trampoline under USE_RUST_STRNCMP=1
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Selected** — no `.rodata`, no helpers, no cross-section refs (same Cut A/B/C path) |
| **B** `rust-lld` | **Rejected** — unnecessary for a leaf that only uses registers + arg memory |
| **C** Rust + FASM glue | Trampoline + `USE_RUST_*` only (minimal); keeps PE export / `call strncmp` ABI |
| **D** reject candidate | **Not applicable** — candidate is suitable |

Why A still works: like `rust_crc_32`, pointer args are stack parameters; the body needs no absolute data references if logic stays inline in the dedicated section.

---

## Kernel execution point

| Item | Plan |
|------|------|
| Where | `high_code` smoke after Cut C smoke; production callers remain `dll` / `heap` / `peload` / exports |
| Why safe | Smoke uses static iglobal strings; hang-on-fail before mutex init (same pattern as A/B/C) |
| How exercised | Smoke: equal / unequal / `n=0` / `n=-1` via real `strncmp` symbol. Boot PE import resolution also calls `strncmp` when loading images (**instrumented real-caller: attempt via smoke + document PE path**) |

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_STRNCMP` (`1` default / `0` original FASM body) |
| **Original FASM body** | Retained in `else` branch of `string.inc` |
| **Independence** | Does not require enabling/disabling Cut A/B/C switches |

---

## Risks

| Risk | Mitigation |
|------|------------|
| Unsigned vs signed byte compare | Match FASM `cmpsb` / `seta`/`setb` exactly in oracle + Rust |
| `n = -1` unlimited paths | Explicit corpus vectors; do not truncate to `isize` blindly |
| ESI/EDI preservation | Rely on Rust stdcall callee-saved; ABI smoke checks regs around call |
| DF / `cld` | Rust implementation must not set DF; smoke assumes DF clear |
| PE export breakage | Keep public symbol name `strncmp`; only swap body behind switch |
| Accidental Cut A/B/C regress | Independent switch; full `kolibri_utils` tests + re-extract prior blobs |
| Unexpected relocations | Extractor hard-fails; do not weaken |

---

## Decision record (summary)

```text
Candidate:              strncmp
Why selected:           new subsystem (string.inc); stdcall+memory; PE-useful; strong oracle
Why not parse_fn again: Cuts B/C already proved parse_fn leaves; Cut D must generalize
ABI:                    stdcall 3 dwords; EAX -1/0/1; ret 12
Dependencies:           read-only caller memory only
Global state:           none
Chosen linking:         Strategy A (reloc-free blob) + C (FASM trampoline/switch)
Rejected:               B (no lld need); reject-candidate (not needed)
Test oracle:            corpus + deterministic PRNG vs FASM-faithful host oracle
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_STRNCMP=0 keeps FASM body
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut E after Cut D verification is green — **STOP**.
