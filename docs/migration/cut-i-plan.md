# Cut I Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut I** is the first migration of an **NTFS packed codec** leaf — variable-length MCB (data-run) decode with dual memory regions and inverted CF polarity vs Cut H.  
> Cuts A–H remain complete and must not be redone. Proof-of-life is diagnostic only — not a Cut I dependency.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `ntfs_decode_mcb_entry` |
| **Source** | [`kernel/fs/ntfs.inc:1116–1161`](../../kernel/fs/ntfs.inc) |
| **Subsystem** | Filesystem / NTFS data-run (MCB) codec |
| **Purpose** | Decode one variable-length NTFS MCB entry from a packed stream at `ESI` into a caller-provided 16-byte stack buffer (`run size` + signed `cluster delta`). Advance `ESI`. **CF=1** more / **CF=0** end. |

---

## Candidate comparison

### Candidate 1: `ntfs_decode_mcb_entry` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `fs/ntfs.inc:1116–1161` |
| Purpose | VLE MCB entry → 16 B `{run_size, cluster_delta}`; advance ESI; CF more/end |
| Complexity | Nibble header; bounded `rep movsb`/`stosb`; length high-bit reject; cluster sign-extend via `cmc`/`sbb`; partial buffer writes on early fail |
| Callers | ≥7 sites in `ntfs.inc` (MFT scan, attr read loop, extend/shrink paths) |
| ABI | Regcall: `ESI` in/out; 16 B buffer below return addr on stack; **CF=1** more / **CF=0** end; preserves `EAX`/`ECX`/`EDI` (push/pop); plain `ret` |
| Deps | Read packed stream; write 16 B stack buffer; no tables/HW/IRQ/sched/alloc |
| Reloc risk | **None** — no static data |
| Risk | Med (stack-buffer ABI + ESI advance + inverted CF + partial mutate) |

### Candidate 2: `blit_clip` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `video/blitter.inc` (~121–222) |
| Purpose | Build src/dst RECTs; call `block_clip` ×2; remap; CF draw/reject |
| Why rejected | Strong **composition** follow-on after Cut H, but CF + geometry already proven. Prefer a new algorithmic family over stacking another video clip leaf. Natural later cut. |

### Candidate 3: `ntfs_restore_usa` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `fs/ntfs.inc:1085–1114` |
| Purpose | Validate/restore Update Sequence Array across sectors; CF fail |
| Why rejected | Same NTFS island and good multi-region loop, but less packed/VLE novelty than MCB decode. Keep as alternate NTFS leaf. |

### Candidate 4: `fat_next_short_name` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `fs/fat.inc` (~395–485) |
| Purpose | In-place 8.3 name collision mutate; uses `std`/`cld`; CF=error |
| Why rejected | Novel **DF** flag surface, but narrower narrative than NTFS VLE codec. |

### Candidate 5: `fsTime2bdfe` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `fs/fs_common.inc:116–163` |
| Purpose | Inverse calendar: seconds → BDFE; `EDI += 8` |
| Why rejected | Calendar twin of Cut G — fails preference for a **new** algorithm class. |

### Candidate 6: `fat_time_to_bdfe` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `fs/fat.inc` (~1073–1088) |
| Purpose | DOS FAT packed time word → BDFE time dword |
| Why rejected | Safe EAX bitfield unpack, but too small vs Cut I prefs (no multi-region / VLE loops). |

### Candidate 7: `antiAliasing` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `gui/font.inc` (~846–862) |
| Purpose | `(3*dst+src)/4` color blend; quirky `EBP` contract |
| Why rejected | Unusual EBP ABI is novel, but tiny + trampoline pain; Cut H already deferred it. |

### Candidate 8: `strtoint_dec` — rejected for Cut I

| Field | Detail |
|-------|--------|
| Source | `core/conf_lib.inc:116–163` |
| Why rejected | Viable stdcall leaf; weaker complexity / conf-only fanout after A–D string work. |

---

## Why Cut I is a meaningful next step

Cuts A–H proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry (CF+mutate)
```

Cut H specifically proved CF-return + in-place RECT mutate. Cut I must answer a **different** question:

> Does Strategy A + C remain viable for a **variable-length packed codec** with **stream + stack dual buffers**, **ESI pointer advance**, **signed field extension**, and **CF polarity opposite Cut H**, with zero tables and no allocator/scheduler/IRQ/paging?

`ntfs_decode_mcb_entry` is the right probe:

1. **Technical safety** — leaf; no HW; deterministic; rollback switch  
2. **New algorithm class** — NTFS MCB VLE decode (not string/checksum/calendar/geometry)  
3. **New ABI surfaces** — stack-resident 16 B out-buffer; ESI advanced; CF=1 means *success/more* (inverted vs H)  
4. **Interacting boundaries** — nibble sizes 0..8; length high-bit reject; cluster sign-extend; partial buffer writes  
5. **Kernel relevance** — ≥7 live NTFS callers  
6. **Testability** — synthesize packed entries; oracle compares CF + ESI delta + all 16 buffer bytes  
7. **Limited blast radius** — independent switch; original retained  

---

## ABI

**LOCAL FACT** — body `ntfs.inc:1116–1161`; typical caller:

```asm
        sub     esp, 10h
.scanmcb:
        call    ntfs_decode_mcb_entry
        jnc     .scanmcbend
        mov     eax, [esp]      ; run size (low dword of qword)
        add     edx, [esp+8]    ; cluster delta
        ; ...
        jmp     .scanmcb
.scanmcbend:
        add     esp, 10h
```

| Item | Contract |
|------|----------|
| **Inputs** | `ESI` → packed MCB entry; 16-byte buffer at `[ESP+4]` on entry (below return address) |
| **Outputs** | `ESI` advanced past consumed bytes; buffer `[0..7]` = run size (zero-padded u64 LE); `[8..15]` = signed cluster delta (sign-extended i64 LE); **CF=1** more, **CF=0** end |
| **Calling convention** | Register/stack leaf, plain `ret` |
| **Registers** | Preserves `EAX`/`ECX`/`EDI` (push/pop). Advances `ESI`. Does not touch `EBX`/`EBP`/`EDX` in FASM body (callers rely on `ECX`/`EDX` surviving). |
| **Partial mutate** | Early reject after length copy may leave only prefix bytes written; cluster half may be untouched — oracle must match FASM |

### Algorithm (LOCAL FACT — FASM body)

1. `lodsb` header; if `0` → end (CF=0, no write)  
2. `length_len = header & 0xF`; if `> 8` → end  
3. Copy `length_len` bytes into buffer; if last length byte `>= 0x80` → end (**no zero-pad**)  
4. Zero-pad run size to 8 bytes  
5. `cluster_len = header >> 4`; if `> 8` → end (run size complete)  
6. Copy `cluster_len` bytes; sign-extend remaining bytes (`cmc`/`sbb` from last cluster byte high bit)  
7. `stc`; ret  

---

## Call graph

| Kind | Detail |
|------|--------|
| **Direct callers** | ≥7 sites in `ntfs.inc` (e.g. MFT fragment scan ~389, attr read ~963, extend/shrink ~2872+) |
| **Potential production paths** | NTFS mount / attribute extent walk |

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Static data | **none** |
| External calls | **none** |
| Memory | Read packed stream at ESI; write up to 16 B at caller stack buffer |
| Allocator / Scheduler / IRQ / Paging / Hardware | **none** |

---

## Rust feasibility

| Item | Expectation |
|------|-------------|
| Generated deps | None (byte copies + zero/sign fill) |
| Relocations | **0** under Strategy A |
| `.rodata` | **none** |
| Helpers | Avoid panic / slice bounds helpers; raw pointer loads/stores |
| CF | Return `u32` 0=end / 1=more; FASM trampoline maps to `clc`/`stc` |
| ESI | Pass `*mut *mut u8` inout; trampoline reloads ESI |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_ntfs_decode_mcb_entry in .text.rust_ntfs_decode_mcb_entry
  → extract (0 relocs, symbol @0, ret 8)
  → kernel/rust/ntfs_decode_mcb_entry.inc `file`
  → ntfs_decode_mcb_entry trampoline under USE_RUST_NTFS_DECODE_MCB_ENTRY=1
```

Trampoline sketch:

```asm
ntfs_decode_mcb_entry:
        push    eax ecx edx edi
        push    esi
        mov     eax, esp
        lea     edx, [esp+24]   ; caller 16 B buffer
        stdcall rust_ntfs_decode_mcb_entry, eax, edx
        pop     esi
        pop     edi edx ecx
        test    eax, eax
        pop     eax
        jz      .end
        stc
        ret
.end:
        clc
        ret
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Preferred** — no tables |
| **B** `rust-lld` | Only if extract shows unavoidable relocs |
| **C** Rust + FASM glue | Minimal trampoline (ESI inout + stack buf → stdcall → EAX → CF) |

If relocs/`.rodata` appear: classify → remove or document Strategy B — **do not weaken the extractor**.

---

## Testing

| Suite | Plan |
|-------|------|
| **Rust** | Named cases: end marker; length/cluster sizes 0..8; positive/negative deltas; length high-bit reject; oversized nibbles; partial mutate |
| **Differential** | FASM-faithful host oracle; exhaustive header×payload grids where practical + PRNG corpus; compare CF, ESI delta, all 16 buffer bytes |
| **ABI** | In-kernel smoke via public `ntfs_decode_mcb_entry`; ESI advance; EAX/ECX/EDI/EDX/EBP/EBX sentinels; CF via `jc`/`jnc` |
| **Kernel** | Hang-on-fail smoke from `high_code` |
| **QEMU** | Rust ON and OFF both reach desktop |

Real caller: smoke calls the public symbol. Live NTFS mount/extent walk under real disk I/O = **NOT PROVEN** unless separately exercised.

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_NTFS_DECODE_MCB_ENTRY` (`1` default / `0` original FASM) |
| **Original** | Retained in `else` branch of `ntfs.inc` |
| **Independence** | Must not depend on Cut A–H switch values |

---

## Risks

| Risk | Mitigation |
|------|------------|
| Stack buffer address wrong in trampoline | Count pushes carefully; smoke reads `[esp]` after call |
| ESI not advanced | Inout pointer; smoke asserts ESI delta |
| Inverted CF vs Cut H | Explicit 0=end→`clc`, 1=more→`stc`; smoke with `jc`/`jnc` |
| Partial mutate on early reject | Oracle asserts all 16 bytes even when CF=0 |
| Length high-bit reject quirk | Named tests for last length byte `>= 0x80` |
| Cluster_len=0 sign from prior byte | Match FASM `[esi-1]` / `cmc`/`sbb` exactly |
| Accidental prior-blob change | Hash-lock Cut B–H blobs |
| Claiming live NTFS coverage | Explicit **NOT PROVEN** |

---

## Decision record (summary)

```text
Candidate:              ntfs_decode_mcb_entry
Why selected:           first NTFS VLE codec leaf; dual buffers; ESI advance; inverted CF
Why not blit_clip:      composition after H; CF/geometry already proven
Why not ntfs_restore_usa: same island; less packed/VLE novelty
Why not fat_next_short_name: DF novel but narrower
Why not fsTime2bdfe:    calendar twin of G
Why not fat_time_to_bdfe: too small for Cut I bar
Why not antiAliasing:   quirky EBP
Why not strtoint_dec:   weaker complexity / conf-only
ABI:                    ESI inout + stack 16 B → CF more/end; trampoline → stdcall → clc/stc
Dependencies:           none
Chosen linking:         Strategy A + C (pending extract evidence)
Test oracle:            grids + PRNG vs FASM-faithful host oracle (CF + ESI + 16 bytes)
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_NTFS_DECODE_MCB_ENTRY=0 keeps FASM body
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut J after Cut I verification is green — **STOP**.
