# Cut E Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut E** is the controlled complexity-increase migration after Cut D (`strncmp`).  
> Cuts A–D remain complete and must not be redone. Proof-of-life is diagnostic only — not a Cut E dependency.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `checksum_1` |
| **Source** | [`kernel/network/stack.inc:668–734`](../../kernel/network/stack.inc) |
| **Purpose** | Partial Internet checksum (RFC1071-style): accumulate 16-bit network-order words from a buffer into `EDX`, with 8/4/2/1 remainder paths decoded via `pushf`/`popf` on shifted length bits. Paired with FASM `checksum_2` (fold / one’s-complement / byte-swap) which is **not** migrated in this cut. |

---

## Complexity

Why this is more complex than previous cuts:

| Prior cut | Shape |
|-----------|-------|
| B / C | Tiny register leaf; closed casefold; no loops over memory |
| D | Linear `cmpsb` string walk; simple −1/0/+1 result |
| **E** | Multi-stage length decode (`shr` + `pushf` ×3), 8-byte ADC stride loop, three remnant paths, **carry chain as internal state** across DL/DH/EDX |

This exercises branching, loops, remnant edge cases, and carry semantics while remaining a pure leaf.

---

## Alternatives

| Candidate | Source | Reason rejected |
|-----------|--------|-----------------|
| `fsCalculateTime` | `fs/fs_common.inc` | Calendar leaf with month loop — strong alternate, but absolute `months`/`months2` loads → reloc risk unless inlined (solved once for CP866; not the best “complexity under Strategy A” probe). Same FS calendar family as a later pair with `fsTime2bdfe`. |
| `strtoint_dec` | `core/conf_lib.inc` | Two-pass decimal parse with place-value state; stdcall. Viable but weaker complexity step-up than ADC/carry networking leaf; fewer live call sites. |
| `checksum_2` alone | `network/stack.inc` | Too small (~20 instr, no loop); quirks only. Kept as FASM twin — Cut E migrates **one** function. |
| `fsTime2bdfe` | `fs/fs_common.inc` | Inverse calendar; write-back + `EDI+=8` ABI; table relocs; better as a follow-on pair. |
| `blit_clip` / `block_clip` | `video/blitter.inc` | Branch-heavy geometry; **CF return** trampoline risk; mutate-in-place. |
| `strchr` / other `string.inc` | `core/string.inc` | Algorithmically interesting but **same subsystem as Cut D** — rejected for Cut E narrative. |
| Remaining `parse_fn` / Unicode | `fs/parse_fn.inc` | Cuts B/C already covered that locality; tables / SF ABI. |
| `unpack` / alloc / sched / IRQ / paging | various | Explicitly out of scope |

### Why selected

| Preference | How `checksum_1` scores |
|------------|-------------------------|
| Complexity increase | ADC loop + carry + `pushf` remnant decode — clearly above B/C/D |
| New subsystem | **network** (`stack.inc`), not string / parse_fn / unicode |
| Clear ABI | Documented regcall leaf; callers already `call checksum_1` |
| Deterministic | Pure function of seed + buffer |
| Few hidden deps | None (no globals, HW, IRQ, sched, tables) |
| Strong oracle | FASM-faithful host oracle; length/seed corpus + PRNG |
| Safe smoke | Static buffers in `high_code`; hang-on-fail |
| Easy rollback | Independent `USE_RUST_CHECKSUM_1` |
| Link strategy | Strategy A expected (no `.rodata`); Strategy B/C only if evidence requires |

---

## ABI

**LOCAL FACT** — body `stack.inc:668–734`; representative callers:

```asm
; icmp.inc — seed 0, length in ECX, data in ESI
        xor     edx, edx
        call    checksum_1
        call    checksum_2

; udp.inc macro — non-zero seed from pseudoheader, then payload
        call    checksum_1
        call    checksum_2
```

| Item | Contract |
|------|----------|
| **Inputs** | `EDX` = seed / partial sum; `ESI` = data pointer; `ECX` = byte length |
| **Outputs** | `EDX` = updated partial sum |
| **Registers** | Clobbers `ECX`, `ESI`, flags. Does **not** touch `EBX`/`EDI`/`EBP`/`EAX` in the FASM body |
| **Stack** | Uses stack only for `pushf`/`popf` frames (0–3). Plain `ret` (no stdcall cleanup) |
| **Caller-saved** | As usual for `call`; flags clobbered |
| **Callee-saved** | FASM body preserves `EBX`/`EDI`/`EBP`/`EAX` (and does not require preserving `ESI`/`ECX`) |
| **Cleanup** | Caller; leaf `ret` |

### Algorithm (LOCAL FACT — FASM body)

1. `shr ecx,1` / `pushf` → CF = odd trailing byte; if no words → `.no_2`.  
2. `shr ecx,1` / `pushf` → CF = leftover 2-byte after 4-byte units; if none → `.no_4`.  
3. `shr ecx,1` / `pushf` → CF = leftover 4-byte after 8-byte units; if none → `.no_8`.  
4. Loop: 8-byte ADC stride (`add dl,[esi+1]` / `adc dh,[esi+0]` ×4) + `adc edx,0`; advance `esi` by 8.  
5. Post-loop `adc edx,0`; then remnant paths `.no_8` / `.no_4` / `.no_2` via `popf`/`jnc`.  
6. Return with sum in `EDX`.

Network-order word interpretation: low sum byte accumulates `[esi+odd]`, high accumulates `[esi+even]`.

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Memory | Read-only `[ESI .. ESI+ECX)` |
| Static data | **none** |
| External calls | **none** |
| Hardware | **none** (network-*adjacent* only) |
| Other | `checksum_2` remains FASM; callers keep `call checksum_1` then `call checksum_2` |

---

## Oracle

| Item | Plan |
|------|------|
| Original implementation | `stack.inc` `checksum_1` body |
| Differential strategy | Host oracle mirroring FASM control flow (byte ADC + `pushf` remnant CF); compare Rust pure helper vs oracle |
| Coverage | Lengths `0..=64` exhaustive × several seeds; remnant classes (mod 8); named vectors; deterministic PRNG corpus (large, fixed seed); odd/even; empty; max-carry stress |
| Exhaustive possibility | Full `(seed × all buffers)` **no** — length-stratified + PRNG |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_checksum_1 in .text.rust_checksum_1
  → extract (0 relocs, symbol @0, ret 12)
  → kernel/rust/checksum1.inc `file`
  → checksum_1 trampoline under USE_RUST_CHECKSUM_1=1
```

Trampoline maps register ABI → Rust stdcall → `EDX`:

```asm
checksum_1:
        stdcall rust_checksum_1, edx, esi, ecx   ; EAX = sum
        mov     edx, eax
        ret
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Selected after evidence** — first slice-indexing build emitted panic/bounds-check relocs (~1270 B + `.data.rel.ro`); rewritten with raw pointer walks → **0 relocs**, 652 B |
| **B** `rust-lld` | **Not required** once panic paths removed |
| **C** Rust + FASM glue | Minimal trampoline (register ↔ stdcall) |
| **D** reject | Not applicable |

### Compiler-generated dependency gate (Cut E lesson)

A naïve ` &[u8] ` + indexing implementation produced:

* `.rel.text.rust_checksum_1` (many R_386_* to panic/rodata)
* anonymous `.data.rel.ro..Lanon.*` string/metadata sections

**Classification:** Rust bounds-check → `core::panicking` / panic location metadata — not algorithmic need.

**Resolution:** keep Strategy A by using `*const u8` walks (same discipline as `strncmp`), no hidden helper stubs. Documented; not silenced by weakening the extractor.

---

## Kernel execution

| Item | Plan |
|------|------|
| Where | `high_code` smoke after Cut D smoke |
| Why safe | Static iglobal buffers; hang-on-fail before mutex init |
| How exercised | Multiple lengths/seeds via real `checksum_1` symbol; EBX/EDI/EBP preservation check |

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_CHECKSUM_1` (`1` default / `0` original FASM body) |
| **Original FASM body** | Retained in `else` branch of `stack.inc` |
| **Independence** | Does not require toggling Cut A–D switches |

---

## Risks

| Risk | Mitigation |
|------|------------|
| Carry / byte-order mismatch | FASM-faithful oracle; remnant-class corpus |
| `pushf` path imbalance | Mirror early-`jz` exits (1/2/3 flag frames) in oracle |
| Trampoline clobbers | Document that `ESI`/`ECX` remain caller-clobbered; preserve `EBX`/`EDI`/`EBP`; return in `EDX` |
| EAX clobber | FASM body left EAX alone; trampoline/`stdcall` will clobber EAX — **audit callers** for EAX liveness across `call checksum_1` |
| Network not on boot path | In-kernel smoke mandatory; live ICMP/TCP/UDP = **NOT PROVEN** unless separately exercised |
| Compiler helpers (`memcpy`, builtins) | Inspect ELF before extract; document every external |
| Accidental Cut A–D regress | Independent switch; re-extract prior blobs + hash check |

### EAX liveness audit (pre-impl note)

Call sites inspected (`icmp.inc`, `udp.inc` / `tcp_*` macros): immediately after `checksum_1` they call `checksum_2` (uses `EDX`/`ECX`) or compare `DX`. None rely on EAX surviving `checksum_1`. Trampoline may clobber EAX.

---

## Decision record (summary)

```text
Candidate:              checksum_1
Why selected:           network subsystem; ADC/carry complexity; pure; Strategy A likely
Why not fsCalculateTime: table reloc / defer calendar pair
Why not strtoint_dec:   weaker complexity step-up
Why not checksum_2:     too small; keep as FASM twin (one function only)
ABI:                    EDX/ESI/ECX in → EDX out; plain ret; trampoline → stdcall
Dependencies:           read-only caller memory only
Chosen linking:         Strategy A + C (pending extract evidence)
Test oracle:            length-stratified + PRNG vs FASM-faithful host oracle
Kernel smoke:           hang-on-fail via high_code
Rollback:               USE_RUST_CHECKSUM_1=0 keeps FASM body
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut F after Cut E verification is green — **STOP**.
