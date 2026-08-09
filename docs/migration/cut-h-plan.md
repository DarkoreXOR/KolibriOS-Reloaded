# Cut H Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut H** is the first migration of a kernel utility in the **video / geometry** family, and the first leaf that returns success/failure via **CF** rather than `EAX`.  
> Cuts A–G remain complete and must not be redone. Proof-of-life is diagnostic only — not a Cut H dependency.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `block_clip` |
| **Source** | [`kernel/video/blitter.inc:34–90`](../../kernel/video/blitter.inc) |
| **Subsystem** | Video / rectangle geometry |
| **Purpose** | Clip a mutable `RECT` at `EDI` against a clip `RECT` at `ESI`. Mutate in place; **CF=0** draw / **CF=1** reject. Used by `blit_clip` (×2) on the blit path. |

---

## Candidate comparison

### Candidate 1: `block_clip` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `video/blitter.inc:34–90` |
| Purpose | Axis-aligned rect clip; mutate target; CF draw/reject |
| Complexity | Signed compare chains; independent X then Y; early reject; partial mutate on Y-fail after X clip |
| Callers | Direct: `blit_clip` ×2; production via `blit_32` → `blit_clip` |
| ABI | Regcall: `ESI` clip, `EDI` mutable rect; **CF** out; preserves `EBX`; plain `ret` |
| Deps | Read clip + R/W target RECT (16 B each); no tables/HW/IRQ/sched/alloc |
| Reloc risk | **None** — no static data |
| Risk | Med (CF trampoline novelty) |

### Candidate 2: `fat_time_to_bdfe` — rejected for Cut H

| Field | Detail |
|-------|--------|
| Source | `fs/fat.inc` (~1073–1088) |
| Purpose | DOS FAT packed time word → BDFE time dword (bitfield unpack) |
| Why rejected | Strong new algorithmic family and safer EAX ABI, but Cut H’s research question is the **CF-return + mutate-in-place** surface that Cut G explicitly deferred. Prefer proving that ABI class over another FS-adjacent packer. Keep as low-risk alternate if CF trampoline evidence forces a rethink. |

### Candidate 3: `fsTime2bdfe` — rejected for Cut H

| Field | Detail |
|-------|--------|
| Source | `fs/fs_common.inc:116–163` |
| Purpose | Inverse calendar: seconds → BDFE; `EDI += 8` |
| Why rejected | Same calendar family as Cut G — fails preference for a **new** subsystem/algorithm class. Natural follow-on pair later. |

### Candidate 4: `strtoint_dec` — rejected for Cut H

| Field | Detail |
|-------|--------|
| Source | `core/conf_lib.inc:116–163` |
| Purpose | ASCII decimal → `u32` (two-pass place-value) |
| Why rejected | Viable stdcall leaf, but weaker narrative / conf-only fanout; parse is string-adjacent after Cuts A–D. |

### Candidate 5: `blit_clip` — rejected for Cut H

| Field | Detail |
|-------|--------|
| Source | `video/blitter.inc:94–195` |
| Why rejected | Near-leaf that **calls** `block_clip` ×2; migrate the leaf first. Follow-on after Cut H. |

### Candidate 6: `antiAliasing` — rejected for Cut H

| Field | Detail |
|-------|--------|
| Source | `gui/font.inc` (~846–862) |
| Why rejected | Color blend is a new family, but quirky `EBP` contract is a worse first ABI probe than CF+RECT. |

---

## Why Cut H is a meaningful next step

Cuts A–G proved:

```text
Unicode / casefold / string / checksum / FS calendar
```

Cut G specifically proved calendar control flow + stack-materialized tables. Cut H must answer a **different** question:

> Does Strategy A + C remain viable for a **CF-returning, in-place mutate** leaf in a **new subsystem** (video geometry), with zero `.rodata` and no allocator/scheduler/IRQ/paging?

`block_clip` is the right probe:

1. **Technical safety** — leaf; no HW; deterministic; rollback switch  
2. **New ABI class** — CF success/fail (never proven in A–G)  
3. **New subsystem** — video/geometry, not string/net/FS calendar  
4. **Kernel relevance** — live blit path via `blit_clip`  
5. **Testability** — synthesize RECTs; strong host oracle; assert rect bytes + CF  
6. **Limited blast radius** — independent switch; originals retained  

---

## ABI

**LOCAL FACT** — body `blitter.inc:34–90`; callers:

```asm
; blit_clip — ESI/EDI set to stack RECTs; CF checked immediately
        lea     edi, [esp + .sx0]
        lea     esi, [ebx + BLITTER.sc]
        call    block_clip
        jc      .done
        ; ...
        lea     edi, [esp + .dx0]
        lea     esi, [ebx + BLITTER.dc]
        call    block_clip
        jc      .done
```

| Item | Contract |
|------|----------|
| **Inputs** | `ESI` → clip `RECT` `{left,top,right,bottom}` (signed dwords); `EDI` → mutable target `RECT` |
| **Outputs** | Target `RECT` clipped in place when overlap exists; **CF=0** draw, **CF=1** reject |
| **Calling convention** | Register leaf, plain `ret` |
| **Registers** | Preserves `EBX` (push/pop). Clobbers `EAX`/`ECX`/`EDX`/flags. Leaves `ESI`/`EDI` **pointers** unchanged. |
| **Stack** | None beyond the `EBX` save |
| **Partial mutate** | If X-axis clips then Y-axis rejects, **X fields may already be written** — oracle must match FASM |

### Algorithm (LOCAL FACT — FASM body)

Comparisons are **signed** (`jge`/`jl`/`jle`):

1. If `left >= clip.right` or `right < clip.left` → fail (no mutate yet)  
2. Else clamp `left`/`right` into clip  
3. If `top >= clip.bottom` or `bottom < clip.top` → fail (**X may already be clamped**)  
4. Else clamp `top`/`bottom`; `clc`; ret  
5. Fail path: `stc`; ret  

---

## Call graph

| Kind | Detail |
|------|--------|
| **Direct callers** | `blit_clip` (×2) |
| **Important callers** | `blit_32` → `blit_clip` |
| **Potential production paths** | Window/client blit clipping |

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Static data | **none** |
| External calls | **none** |
| Memory | Read 16 B clip; read/write 16 B target |
| Allocator / Scheduler / IRQ / Paging / Hardware | **none** |

---

## Rust feasibility

| Item | Expectation |
|------|-------------|
| Generated deps | None (scalar signed compares + stores) |
| Relocations | **0** under Strategy A |
| `.rodata` | **none** |
| Helpers | Avoid panic / slice bounds helpers; use raw pointer loads/stores |
| CF | Return `u32` 0/1 from Rust; FASM trampoline maps to `clc`/`stc` |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_block_clip in .text.rust_block_clip
  → extract (0 relocs, symbol @0, ret 8)
  → kernel/rust/block_clip.inc `file`
  → block_clip trampoline under USE_RUST_BLOCK_CLIP=1
```

Trampoline:

```asm
block_clip:
        stdcall rust_block_clip, esi, edi
        test    eax, eax
        jnz     .fail
        clc
        ret
.fail:
        stc
        ret
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Preferred** — no tables |
| **B** `rust-lld` | Only if extract shows unavoidable relocs |
| **C** Rust + FASM glue | Minimal trampoline (ESI/EDI → stdcall → EAX → CF) |

If relocs/`.rodata` appear: classify → remove or document Strategy B — **do not weaken the extractor**.

---

## Testing

| Suite | Plan |
|-------|------|
| **Rust** | Named cases: full contain, partial clamp each edge, reject outside X/Y, negative coords, empty/degenerate, Y-fail after X clamp |
| **Differential** | FASM-faithful host oracle; structured grids + PRNG corpus; compare CF **and** all 16 RECT bytes |
| **ABI** | In-kernel smoke via public `block_clip`; ESI/EDI/EBP/EBX sentinels; CF checked with `jc`/`jnc` |
| **Kernel** | Hang-on-fail smoke from `high_code` |
| **QEMU** | Rust ON and OFF both reach desktop |

Real caller: smoke calls the public `block_clip` symbol (same as production). Live `blit_32` under GUI paint = **NOT PROVEN** unless separately exercised.

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_BLOCK_CLIP` (`1` default / `0` original FASM) |
| **Original** | Retained in `else` branch of `blitter.inc` |
| **Independence** | Must not depend on Cut A–G switch values |

---

## Risks

| Risk | Mitigation |
|------|------------|
| CF lost across stdcall | Explicit `test`/`clc`/`stc` in trampoline; smoke with `jc`/`jnc` |
| Signed vs unsigned compares | Use `i32` comparisons matching FASM `jge`/`jl`/`jle` |
| Partial mutate on Y-fail | Oracle + tests assert RECT bytes even when CF=1 |
| ESI/EDI/EBX not preserved | stdcall callee-saved + smoke sentinels |
| Accidental prior-blob change | Hash-lock Cut B–G blobs |
| Claiming live blit coverage | Explicit **NOT PROVEN** |

---

## Decision record (summary)

```text
Candidate:              block_clip
Why selected:           first video/geometry leaf; CF-return ABI; mutate-in-place; zero tables
Why not fat_time_to_bdfe: safer EAX but weaker answer to the CF question Cut G deferred
Why not fsTime2bdfe:    calendar twin of Cut G
Why not strtoint_dec:   weaker complexity / conf-only
Why not blit_clip:      depends on block_clip; follow-on
Why not antiAliasing:   quirky EBP ABI
ABI:                    ESI clip + EDI rect → CF; trampoline → stdcall → clc/stc
Dependencies:           none
Chosen linking:         Strategy A + C (pending extract evidence)
Test oracle:            grids + PRNG vs FASM-faithful host oracle (CF + RECT bytes)
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_BLOCK_CLIP=0 keeps FASM body
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut I after Cut H verification is green — **STOP**.
