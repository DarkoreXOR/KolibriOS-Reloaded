# Cut F Plan

**Date:** 2026-08-09  
**Status:** audit complete — selected target ready for implementation  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut F** is the checksum-family independence validation after Cut E (`checksum_1`).  
> Cuts A–E remain complete and must not be redone. Proof-of-life is diagnostic only — not a Cut F dependency.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `checksum_2` |
| **Source** | [`kernel/network/stack.inc:759–786`](../../kernel/network/stack.inc) |
| **Purpose** | Finalize a partial Internet checksum: fold 32→16, one’s-complement (`not`), zero-sum quirk (`dec` when pre-`not` DX was 0 → `0xFFFE`), then byte-swap to INET order. Production twin of FASM/Rust `checksum_1`. |

---

## Relationship to checksum_1

| Item | Detail |
|------|--------|
| **Shared structure** | Same file (`stack.inc`); same checksum family; always called immediately after `checksum_1` (or after an inline ADC pseudoheader + `checksum_1`) in ICMP/UDP/TCP paths |
| **Differences** | No buffer walk; no length decode; no ADC loop; pure arithmetic on `EDX`; quirky zero path + `xchg`; output is `DX` (INET order), not a growing partial sum |
| **Why this is not simply a duplicate migration** | Distinct algorithm and ABI; independent oracle domain (32-bit scalar, not buffer×length); independent blob/section/switch; must **not** share Rust helpers with Cut E. Cut F answers: *can the same pipeline land a related sibling without coupling?* |

Cut E deferred `checksum_2` as “too small” when the goal was complexity increase. Cut F’s goal is **family-level repeatability / isolation**, for which small + quirky is appropriate.

---

## Alternatives considered

| Candidate | Source | Why rejected |
|-----------|--------|----------------|
| `fsCalculateTime` | `fs/fs_common.inc` | Strong FS calendar leaf, but absolute `months`/`months2` loads → reloc risk unless inlined; **does not answer the checksum-family independence question** that Cut F is designed to ask |
| `strchr` | `core/string.inc` | Interesting `scasb`/`pushf` chunk growth; still **string.inc** after Cut D — weaker as a family-repeatability probe; `std`/`cld` not involved but chunk loop is a different narrative |
| `strtoint_dec` | `core/conf_lib.inc` | Viable stdcall parse leaf; fewer live call sites; unrelated to checksum family |
| `fsTime2bdfe` | `fs/fs_common.inc` | Write-back + `EDI+=8`; table relocs; better as calendar pair with `fsCalculateTime` |

### Why `checksum_2` is preferable

| Preference | How `checksum_2` scores |
|------------|-------------------------|
| Cut F research question | Direct sibling of Cut E — controlled family validation |
| Independence | Separate ABI, oracle, blob, section, rollback switch |
| Clear ABI | Documented regcall leaf; callers already `call checksum_2` |
| Deterministic | Pure function of `EDX` |
| Few hidden deps | None (DEBUGF compiles out when verbose=0) |
| Strong oracle | Full 32-bit scalar domain; fold/zero/byte-swap edge cases |
| Safe smoke | Static partial sums → `checksum_2`; hang-on-fail; optional `checksum_1`→`checksum_2` chain |
| Easy rollback | Independent `USE_RUST_CHECKSUM_2` (must not depend on Cut E switch) |
| Link strategy | Strategy A expected (no memory, no `.rodata`) |

**Rejected for Cut F:** treating “too small for Cut E” as a permanent ban — Cut F’s acceptance criteria explicitly prioritize family independence over complexity step-up.

---

## ABI

**LOCAL FACT** — body `stack.inc:767–786`; representative callers:

```asm
; icmp.inc — after checksum_1
        call    checksum_1
        call    checksum_2
        cmp     dx, si

; udp.inc macro — after checksum_1
        call    checksum_1
        call    checksum_2
        add     [esi+UDP_header.Checksum], dx
```

| Item | Contract |
|------|----------|
| **Inputs** | `EDX` = semi-checksum (partial sum from `checksum_1` or equivalent) |
| **Outputs** | `DX` = final checksum in INET byte order; high half of `EDX` is 0 after FASM body |
| **Registers** | Clobbers `ECX`, `EDX` (high folded away), flags. Does **not** touch `EAX`/`EBX`/`ESI`/`EDI`/`EBP` in the FASM body |
| **Stack** | None (no `pushf`; `DEBUGF` is a compile-time no-op when `DEBUG_NETWORK_VERBOSE=0`) |
| **Preservation** | FASM body preserves `EAX`/`EBX`/`ESI`/`EDI`/`EBP` |
| **Cleanup** | Plain `ret` (caller; no stdcall cleanup) |

### Algorithm (LOCAL FACT — FASM body)

1. `ecx = edx>>16`; `edx &= 0xffff`; `edx += ecx` (first fold).  
2. `ecx = edx>>16`; `add dx, cx` (second fold into 16 bits).  
3. `test dx, dx` (ZF ← DX==0; comment notes CF from `add` is unreliable for this check).  
4. `not dx` (flags unchanged).  
5. If ZF was set (pre-`not` DX was 0): `dec dx` → `0xFFFE`.  
6. `xchg dl, dh` (INET byte order).  
7. `ret`.

**Zero quirk (precise):** folded DX == 0 → after `not`/`dec` → `0xFFFE` (not `0xFFFF`). Prior docs’ shorthand “0→FFFF” is imprecise; Cut F oracle must match `0xFFFE`.

### EAX / ECX liveness across callers

Call sites inspected (`icmp.inc`, `udp.inc`, `tcp_subr.inc`): after `checksum_2` they use `DX` (`cmp`/`add`). None rely on `EAX` or `ECX` surviving `checksum_2`. Trampoline may clobber `EAX` (stdcall return) and will restore result into `EDX`/`DX`.

---

## Dependencies

| Kind | Value |
|------|-------|
| Global state | **none** |
| Memory | **none** (pure register) |
| Static data | **none** (DEBUGF string only if verbose debug enabled — default off) |
| External calls | **none** |
| Hardware | **none** (network-*adjacent* only) |
| Other | Independent of `USE_RUST_CHECKSUM_1`; callers keep `call checksum_2` |

---

## Link strategy

**Selected: Strategy A + C** (reloc-free raw blob + minimal FASM trampoline / switch).

```text
rust_checksum_2 in .text.rust_checksum_2
  → extract (0 relocs, symbol @0, ret 4)
  → kernel/rust/checksum2.inc `file`
  → checksum_2 trampoline under USE_RUST_CHECKSUM_2=1
```

Trampoline maps register ABI → Rust stdcall → `EDX`:

```asm
checksum_2:
        stdcall rust_checksum_2, edx   ; EAX = final DX value (low 16 meaningful)
        mov     edx, eax
        ret
```

| Strategy | Decision |
|----------|----------|
| **A** reloc-free | **Preferred** — pure arithmetic; no buffers → no bounds-check temptation |
| **B** `rust-lld` | Only if extract evidence shows unavoidable relocs |
| **C** Rust + FASM glue | Minimal trampoline (register ↔ stdcall) |
| **D** reject | Not applicable — leaf is suitable |

### Compiler dependency risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Bounds checks / panic | Low (no slices) | Keep scalar `u32` API; inspect ELF |
| `.rodata` | Low | No string/format in freestanding body |
| Compiler helpers (`__udiv*`, etc.) | Low (shifts/adds/not) | Inspect for unexpected calls |
| Accidental coupling to `checksum_1` | Process risk | Separate FFI symbol/section; no shared Rust helpers |

If any appear: classify → remove safely or document Strategy B — **do not weaken the extractor**.

---

## Oracle

| Item | Plan |
|------|------|
| Original implementation | `stack.inc` `checksum_2` body |
| Differential strategy | Host oracle mirroring fold / `test`+`not`+conditional `dec` / `xchg`; compare Rust pure helper vs oracle |
| Coverage | Named quirks (0, `0xFFFF`, `0x1FFFE`, max `u32`); carry folds; byte-swap pairs; exhaustive structured grids (all low×sample high, all high×sample low); deterministic PRNG over `u32` (large fixed seed) |
| Exhaustive possibility | Full 2³² impractical in CI; structured exhaustive + PRNG **yes** |

Do **not** reuse Cut E buffer vectors — domain is scalar.

---

## Kernel execution

| Item | Plan |
|------|------|
| Smoke path | `high_code` after Cut E smoke |
| Real caller | In-kernel smoke calls public `checksum_2` (same symbol production uses); optionally chain `checksum_1`→`checksum_2` with a static buffer to mirror ICMP/UDP sequence |
| Limitations | Boot does not run live ICMP/TCP/UDP — live traffic = **NOT PROVEN** unless separately exercised |
| Safety | Static constants; hang-on-fail; no new network drivers/packet generators |

---

## Rollback

| Item | Value |
|------|-------|
| **Switch** | `USE_RUST_CHECKSUM_2` (`1` default / `0` original FASM body) |
| **Original FASM body** | Retained in `else` branch of `stack.inc` |
| **Independence** | Must work with any `USE_RUST_CHECKSUM_1` setting; does not require toggling Cut A–E switches |

---

## Risks

| Risk | Mitigation |
|------|------------|
| Zero quirk off-by-one (`FFFE` vs `FFFF`) | FASM-faithful oracle; named case `edx=0` |
| Byte-swap mismatch | Named vectors checking `xchg` |
| Shared helper coupling with Cut E | Separate module section / no reuse of ADC helpers |
| Trampoline clobbers | Document EAX/ECX clobber; preserve EBX/ESI/EDI/EBP; return in EDX |
| Accidental Cut E blob change | Hash-lock `checksum_1` blob before/after |
| DEBUGF if verbose ever enabled | Rust path omits debug print (behavioral delta only under debug builds — document) |
| Claiming live network coverage | Explicit **NOT PROVEN** |

---

## Decision record (summary)

```text
Candidate:              checksum_2
Why selected:           checksum-family independence after Cut E; pure; Strategy A likely
Why not fsCalculateTime: wrong research question; table reloc
Why not strchr:         string.inc again; not family validation
ABI:                    EDX in → DX out (INET); plain ret; trampoline → stdcall
Dependencies:           none (register-only)
Chosen linking:         Strategy A + C (pending extract evidence)
Test oracle:            structured exhaustive + PRNG vs FASM-faithful host oracle
Kernel smoke:           hang-on-fail via high_code; optional checksum_1→checksum_2 chain
Rollback:               USE_RUST_CHECKSUM_2=0 keeps FASM body (independent of Cut E)
```

**Implementation may proceed after this document is in the tree.**  
Do not start Cut G after Cut F verification is green — **STOP**.
