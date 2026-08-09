# Cut J Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut J** is the first migration of an **NTFS Update Sequence Array (USA) integrity leaf** — multi-sector strided word validate/restore with same-buffer USA↔sector-tail aliasing and CF fail polarity.  
> Cuts A–I remain complete and must not be redone. Proof-of-life is diagnostic only — not a Cut J dependency.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `ntfs_restore_usa` |
| **Source** | [`kernel/fs/ntfs.inc:1085–1114`](../../kernel/fs/ntfs.inc) |
| **Subsystem** | Filesystem / NTFS record integrity (USA) |
| **Purpose** | Validate USN at each sector end-word; restore original words from the USA array into those locations. **CF=0** OK / **CF=1** fail. |

Thin fall-through wrapper `ntfs_restore_usa_frs` (loads `EAX` from `[EBP+NTFS.frs_size]`) is **not** migrated; it continues to enter `ntfs_restore_usa`.

---

## Candidate comparison

### Candidate 1: `ntfs_restore_usa` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `fs/ntfs.inc:1085–1114` |
| Purpose | USA size check → USN walk → strided `0x200` sector-end word restores |
| Complexity | Multi-region same-buffer aliasing; word R/W; bounded sector loop; mid-loop partial mutate on USN mismatch |
| Callers | 8 sites (`ntfs_restore_usa` ×4 + `ntfs_restore_usa_frs` ×4) on FRS/index read paths |
| ABI | Regcall: `EBX`→record, `EAX`=size bytes; **CF=0** OK / **CF=1** fail; `pushad`/`popad`; plain `ret` |
| Deps | Read/write within caller record only; no tables/HW/IRQ/sched/alloc |
| Reloc risk | **None** — no static data |
| Compiler helper risk | Word loops may tempt LLVM `memcpy`/`memset` if coded poorly — explicit `u16` loads/stores required (Cut I lesson) |
| Risk | Med (partial mutate + same-buffer USA/tail aliasing + exact stride) |

### Candidate 2: `fat_next_short_name` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Source | `fs/fat.inc:395–485` |
| Purpose | In-place 8.3 collision mutate; `std`/`cld`; CF=error |
| Why rejected | Strong **DF** novelty and viable leaf, but narrower fanout (2 callers) and less multi-region / aliasing proof than USA. Keep as next-cut alternate. |

### Candidate 3: `memmove` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Source | `kernel.asm:3230–3262` |
| Purpose | Forward dword/`movsb` copy (`EAX`/`EBX`/`ECX`) |
| Why rejected | Best **memcpy-class helper-risk** probe after Cut I, but algorithmically thin and huge blast radius (~24 callers). Prefer integrity leaf with richer mutation semantics first. |

### Candidate 4: `blit_clip` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Source | `video/blitter.inc:121–222` |
| Why rejected | Composition over Cut H; CF/geometry already proven. Natural later cut. |

### Candidate 5: `xfs._.extent_unpack` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Source | `fs/xfs.asm:1465–1497` |
| Why rejected | New FS + BE bitfields, but no multi-region loop; thinner than USA for Cut J prefs. |

### Candidate 6: `fsTime2bdfe` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Source | `fs/fs_common.inc:116–163` |
| Why rejected | Calendar twin of Cut G — fails new-algorithm-class bar. |

### Candidate 7: `fat_time_to_bdfe` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Why rejected | Tiny EAX bitfield unpack; too small for Cut J bar. |

### Candidate 8: `antiAliasing` / `strtoint_dec` — rejected for Cut J

| Field | Detail |
|-------|--------|
| Why rejected | Tiny / conf-only / quirky EBP — weak architectural narrative. |

---

## Why Cut J is a meaningful next step

Cuts A–I proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry (CF+mutate)
/ NTFS VLE MCB codec (ESI + stack buf + inverted CF)
```

Cut I specifically proved packed VLE decode + stack out-buffer + inverted CF. Cut J must answer a **different** question:

> Does Strategy A + C remain viable for a **multi-region integrity leaf** that validates a USA signature stream and **strides word restores** across sector tails **inside the same record buffer**, with CF fail polarity matching Cut H (not Cut I), exact partial mutation on mid-loop reject, and zero tables — without compiler `memset`/`memcpy`/GOT?

`ntfs_restore_usa` is the right probe:

1. **Technical safety** — leaf; no HW; deterministic; rollback switch  
2. **New algorithm class** — USA integrity restore (not VLE codec / geometry / calendar / string)  
3. **New memory surface** — same-buffer USA array ↔ sector-end words; strided `+0x200` mutation  
4. **Interacting boundaries** — size>>9 vs `updateSequenceSize`; USN mismatch mid-loop partial restore  
5. **Kernel relevance** — 8 live NTFS callers (FRS + index paths)  
6. **Testability** — synthesize multi-sector records; oracle compares CF + full buffer bytes  
7. **Limited blast radius** — independent switch; original retained; wrapper `*_frs` untouched  

---

## ABI

**LOCAL FACT** — body `ntfs.inc:1085–1114`; typical caller:

```asm
        mov     ebx, record
        mov     eax, size_bytes   ; or via ntfs_restore_usa_frs
        call    ntfs_restore_usa
        jc      .usa_fail
```

| Item | Contract |
|------|----------|
| **Inputs** | `EBX` → NTFS record; `EAX` = size in bytes |
| **Outputs** | Sector end-words restored from USA on success; **CF=0** OK / **CF=1** fail |
| **Calling convention** | Register leaf, plain `ret` |
| **Registers** | `pushad`/`popad` — all GPRs preserved |
| **Partial mutate** | On USN mismatch mid-loop, earlier sectors already restored — oracle must match FASM |

### Algorithm (LOCAL FACT — FASM body)

1. `sectors = size >> 9`; require `word [ebx+6] == sectors+1`  
2. `ESI = ebx + word [ebx+4]` (USA); `lodsw` → USN in `DX`  
3. `EDI = ebx + 0x1FE`  
4. Loop `sectors` times: require `[EDI]==DX`; `lodsw`/`stosw` restore; `EDI += 0x1FE` (net `+0x200` from pre-`stosw` EDI)  
5. `clc` / on any fail `stc`

**Pathological note:** `sectors == 0` with matching USA size `1` would enter FASM `loop` with `ECX=0` (wraps to ~4G iterations). Out of scope for differential — only exercise `size >= 512` on the success path; `size < 512` with mismatched USA size remains an early-fail case.

---

## Call graph

| Kind | Detail |
|------|--------|
| **Direct** | `ntfs_restore_usa` ×4; `ntfs_restore_usa_frs` ×4 (fall-through) |
| **Potential production paths** | NTFS FRS / index buffer post-read USA restore |

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Static data | **none** |
| External calls | **none** |
| Memory | Read USA + sector ends; write restored words at sector ends |
| Allocator / Scheduler / IRQ / Paging / Hardware | **none** |

---

## Rust feasibility

| Item | Expectation |
|------|-------------|
| Generated deps | None if word R/W stays explicit (no slice `copy_from_slice` on freestanding path) |
| Relocations | **0** under Strategy A |
| `.rodata` | **none** |
| Helpers | Avoid panic / memcpy / memset; raw `u16` load/store |
| CF | Return `u32` 0=OK / 1=fail; FASM trampoline maps to `clc`/`stc` (Cut H polarity) |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_ntfs_restore_usa in .text.rust_ntfs_restore_usa
  → extract (0 relocs, symbol @0, ret 8)
  → kernel/rust/ntfs_restore_usa.inc `file`
  → ntfs_restore_usa trampoline under USE_RUST_NTFS_RESTORE_USA=1
```

Trampoline sketch:

```asm
ntfs_restore_usa:
        pushad
        stdcall rust_ntfs_restore_usa, ebx, eax
        test    eax, eax
        popad
        jz      .ok
        stc
        ret
.ok:
        clc
        ret
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Preferred** — no tables |
| **B** `rust-lld` | Only if extract shows unavoidable relocs |
| **C** Rust + FASM glue | Minimal trampoline (EBX/EAX → stdcall → EAX → CF) |

If relocs/`.rodata`/GOT/`memset` appear: diagnose → fix implementation — **do not weaken the extractor**.

---

## Testing

| Suite | Plan |
|-------|------|
| **Rust** | Named: 1/2/4-sector OK; USA size mismatch; USN mismatch at first/middle/last; wrong offset; size&lt;512 early fail; sentinel bytes outside mutate range |
| **Differential** | Separately coded FASM-faithful host oracle; structured grids + PRNG; compare CF + entire record buffer |
| **ABI** | In-kernel smoke via public `ntfs_restore_usa`; CF via `jc`/`jnc`; all GPRs via `pushad` contract + sentinel checks |
| **Kernel** | Hang-on-fail smoke from `high_code` |
| **QEMU** | Rust ON and OFF both reach desktop |

Real caller: smoke calls the public symbol. Live NTFS mount/I/O USA restore = **NOT PROVEN** unless separately exercised.

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_NTFS_RESTORE_USA` (`1` default / `0` original FASM) |
| **Original** | Retained in `else` branch of `ntfs.inc` |
| **Independence** | Must not depend on Cut A–I switch values |

---

## Risks

| Risk | Mitigation |
|------|------------|
| Mid-loop partial mutate missed by tests | Oracle asserts full buffer even on CF=1 |
| Stride off-by-2 (`stosw` + `add 0x1FE`) | Explicit `+0x200` from pre-write EDI; named multi-sector tests |
| Compiler `memcpy`/`memset`/GOT | Explicit u16 stores; mandatory artifact audit |
| `sectors==0` LOOP pathology | Document OUT OF SCOPE; never PRNG that case on success path |
| Accidental prior-blob change | Hash-lock Cuts A–I blobs |
| Claiming live NTFS coverage | Explicit **NOT PROVEN** |

---

## Decision record (summary)

```text
Candidate:              ntfs_restore_usa
Why selected:           first USA integrity leaf; strided multi-region same-buffer mutate; CF fail
Why not fat_next_short_name: DF novel but narrower fanout / less multi-region proof
Why not memmove:        helper-risk interesting but thin + high blast radius
Why not blit_clip:      composition after H; CF/geometry already proven
Why not xfs extent:     thinner; no multi-region loop
Why not fsTime2bdfe:    calendar twin of G
Why not fat_time_to_bdfe / antiAliasing / strtoint_dec: too small / weak narrative
ABI:                    EBX+EAX → CF ok/fail; trampoline → stdcall → clc/stc
Dependencies:           none
Chosen linking:         Strategy A + C (pending extract evidence)
Test oracle:            grids + PRNG vs FASM-faithful host oracle (CF + full buffer)
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_NTFS_RESTORE_USA=0 keeps FASM body
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut K after Cut J verification is green — **STOP**.
