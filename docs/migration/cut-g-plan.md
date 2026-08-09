# Cut G Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut G** is the first migration of a kernel utility **outside** the already-proven string / Unicode / filesystem-casefold / checksum families.  
> Cuts A–F remain complete and must not be redone. Proof-of-life is diagnostic only — not a Cut G dependency.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `fsCalculateTime` |
| **Source** | [`kernel/fs/fs_common.inc:42–87`](../../kernel/fs/fs_common.inc) (tables at 89–92) |
| **Subsystem** | Filesystem / calendar |
| **Purpose** | Convert a BDFE-style datetime block at `ESI` into seconds since **2001-01-01** in `EAX`. Shared by NTFS / ext set-info paths (and fall-through from `fsGetTime` after CMOS read). |

---

## Candidate comparison

### Candidate 1: `fsCalculateTime` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `fs/fs_common.inc:42–87` |
| Purpose | BDFE datetime → seconds since 2001-01-01 |
| Complexity | Year clamp, leap-year table select, month-day accumulation loop, then days→h→m→s via `mul` |
| Callers | Direct: `ntfsCalculateTime` (`ntfs.inc`), `ext_SetFileInfo` ×2 (`ext.inc`); fall-through from `fsGetTime` |
| ABI | Regcall: `ESI` → block; `EAX` out; clobbers `EBX`/`ECX`/`EDX`; plain `ret`; **must preserve `ESI`** (callers `add esi, 8`) |
| Deps | Read-only input; static `months`/`months2` (24 B); no HW/IRQ/sched/alloc on this label |
| Reloc risk | Med if absolute table loads kept; **solved by stack-materialized / immediate month lengths** (CP866 lesson) |
| Risk | Low–med |

### Candidate 2: `block_clip` — rejected for Cut G

| Field | Detail |
|-------|--------|
| Source | `video/blitter.inc:34–90` |
| Purpose | Clip `RECT` at `EDI` against clip at `ESI`; mutate in place; **CF** draw/reject |
| Why rejected | Strong non-FS / non-network alternate with zero tables, but **CF return ABI** is a higher trampoline risk than Cut G needs for its first out-of-family cut. Prefer calendar math (clearer oracle, richer control flow) over flag-return geometry. |

### Candidate 3: `fsTime2bdfe` — rejected for Cut G

| Field | Detail |
|-------|--------|
| Source | `fs/fs_common.inc:94–141` |
| Purpose | Inverse: seconds → BDFE block; **`EDI += 8`** |
| Why rejected | Same calendar family / table issue as #1, but write-back + `EDI` advance is a harder first ABI. Natural **follow-on pair** after `fsCalculateTime` proves table-inlining. |

### Candidate 4: `strtoint_dec` — rejected for Cut G

| Field | Detail |
|-------|--------|
| Source | `core/conf_lib.inc:116–163` |
| Purpose | ASCII decimal → `u32` (two-pass place-value) |
| Why rejected | Viable stdcall leaf with no tables, but weaker complexity / narrative step than calendar; conf-only fanout. Keep as low-risk fallback if calendar reloc evidence forces a rethink. |

---

## Why Cut G is a meaningful next step

Cuts A–F proved:

```text
Unicode / casefold / string / checksum
```

Cut F specifically proved two related checksum functions can compose through the existing ABI. Cut G must answer a **different** question:

> Does the freestanding blob + trampoline architecture remain viable for a **new kernel utility family** (FS calendar), with non-trivial control flow (leap/month/`mul` chain), without allocator/scheduler/IRQ/paging?

`fsCalculateTime` is the right probe:

1. **Technical safety** — leaf; no HW on the label; deterministic; rollback switch  
2. **Complexity increase** — calendar math ≠ checksum fold; loops + leap + multi-stage `mul`  
3. **Kernel relevance** — live NTFS/ext set-info callers  
4. **Testability** — synthesize BDFE blocks; strong host oracle  
5. **ABI clarity** — documented regcall; ESI preservation audited  
6. **Limited blast radius** — independent switch; tables inlined; originals retained  

---

## ABI

**LOCAL FACT** — body `fs_common.inc:42–87`; representative callers:

```asm
; ext.inc — ESI advances by 8 between calls; EAX result used immediately
        call    fsCalculateTime
        add     eax, UNIXTIME_TO_KOS_OFFSET
        mov     [edi+INODE.aTime], eax
        add     esi, 8
        call    fsCalculateTime

; ntfs.inc — via ntfsCalculateTime wrapper
        call    fsCalculateTime
        ; then scale to 100ns intervals since 1601
```

| Item | Contract |
|------|----------|
| **Inputs** | `ESI` → 8-byte BDFE block: `[0]=sec`, `[1]=min`, `[2]=hour` (low byte used), `[4]=day`, `[5]=month`, `[6]=year` (word) |
| **Outputs** | `EAX` = seconds since 2001-01-01 |
| **Calling convention** | Register leaf, plain `ret` |
| **Registers** | Clobbers `EBX`, `ECX`, `EDX`, flags. Leaves `ESI`/`EDI`/`EBP` alone (FASM body). |
| **Stack** | None in body |
| **Callee-saved (for trampoline)** | Preserve `ESI` (required by callers), `EDI`, `EBP`; `EBX` may be clobbered (matches FASM) |
| **Caller-saved** | `EAX` (return), `ECX`, `EDX`, flags |

### Algorithm (LOCAL FACT — FASM body)

1. `years = max(0, year − 2001)`  
2. Select `months` vs `months2` (Feb 29) when `(years + 1) & 3 == 0`  
3. Sum days in months before `month` (loop over table)  
4. `days = years*365 + years/4 + month_sum + day − 1`  
5. `seconds = ((days*24 + hour)*60 + minute)*60 + second`  

**Quirk domain:** FASM `mov bl, …` leaves `BH` from prior `shr ebx,2` (`years/4`). For `years < 1024` (year &lt; 3025) `BH=0` — production domain. Oracle/tests must cover this range explicitly; extreme years may diverge from “pure math” if BH pollutes adds.

**Fall-through:** `fsGetTime` ends by setting `ESI=esp` then falls into `fsCalculateTime`. Trampoline at the label preserves that entry.

---

## Call graph

| Kind | Detail |
|------|--------|
| **Direct callers** | `ntfsCalculateTime`; `ext_SetFileInfo` (×2); fall-through from `fsGetTime` |
| **Important callers** | `ntfs_SetFileInfo` (×3 via wrapper); ext set-info aTime/mTime |
| **Potential production paths** | File timestamp write on NTFS/ext; CMOS “now” via `fsGetTime` → this label |

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** (read-only input buffer) |
| Static data | `months` / `months2` — **must not** become absolute `.rodata` loads in the blob |
| External calls | **none** |
| Memory | Read 8 bytes at `ESI` |
| Allocator / Scheduler / IRQ / Paging / Hardware | **none** on this label (CMOS is only in `fsReadCMOS` / `fsGetTime` prologue) |

---

## Rust feasibility

| Item | Expectation |
|------|-------------|
| Generated deps | None if month lengths are immediates / stack-local |
| Relocations | **0** under Strategy A (fail extract if any) |
| `.rodata` | **none** — stack-materialize 24-byte month tables if needed |
| Helpers | Avoid `__udiv*` / panic / slice bounds; prefer shifts/muls on scalars |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_fs_calculate_time in .text.rust_fs_calculate_time
  → extract (0 relocs, symbol @0, ret 4)
  → kernel/rust/fs_calculate_time.inc `file`
  → fsCalculateTime trampoline under USE_RUST_FS_CALCULATE_TIME=1
```

Trampoline:

```asm
fsCalculateTime:
        stdcall rust_fs_calculate_time, esi
        ret
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Preferred** — inline/stack month tables |
| **B** `rust-lld` | Only if extract shows unavoidable relocs |
| **C** Rust + FASM glue | Minimal trampoline (ESI → stdcall → EAX) |

If relocs/`.rodata` appear: classify → remove (table inlining) or document Strategy B — **do not weaken the extractor**.

---

## Testing

| Suite | Plan |
|-------|------|
| **Rust** | Named BDFE cases: epoch, leap Feb 29, year&lt;2001 clamp, month edges, end-of-day |
| **Differential** | FASM-faithful host oracle; structured grids (years×months×days) + PRNG corpus |
| **ABI** | In-kernel smoke via public `fsCalculateTime`; ESI/EDI/EBP sentinels |
| **Kernel** | Hang-on-fail smoke from `high_code` |
| **QEMU** | Rust ON and OFF both reach desktop |

Real caller: smoke calls the public symbol (same as production). Live NTFS/ext set-info under disk I/O = **NOT PROVEN** unless separately exercised.

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_FS_CALCULATE_TIME` (`1` default / `0` original FASM) |
| **Original** | Retained in `else` branch of `fs_common.inc` |
| **Independence** | Must not depend on Cut A–F switch values |

---

## Risks

| Risk | Mitigation |
|------|------------|
| Absolute `months` reloc | Stack-materialize / immediates; extractor hard-fail on relocs |
| ESI not preserved | stdcall callee-saved + smoke sentinel |
| `fsGetTime` fall-through broken | Keep label at trampoline entry; do not insert code between `add esp,8` and label |
| Year≥3025 BH quirk | Document domain; oracle/tests focus on years&lt;1024 |
| Accidental prior-blob change | Hash-lock Cut B–F blobs |
| Claiming live FS coverage | Explicit **NOT PROVEN** |

---

## Decision record (summary)

```text
Candidate:              fsCalculateTime
Why selected:           first non-leaf-family utility; FS calendar; real callers; Strategy A via table inlining
Why not block_clip:     CF ABI risk; less algorithmic weight for this cut's question
Why not fsTime2bdfe:    harder write-back ABI; follow-on pair
Why not strtoint_dec:   weaker complexity / narrative
ABI:                    ESI in → EAX out; plain ret; trampoline → stdcall
Dependencies:           month tables only (inlined)
Chosen linking:         Strategy A + C (pending extract evidence)
Test oracle:            structured grids + PRNG vs FASM-faithful host oracle
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_FS_CALCULATE_TIME=0 keeps FASM body
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut H after Cut G verification is green — **STOP**.
