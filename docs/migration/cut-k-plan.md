# Cut K Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-k-implementation.md`](cut-k-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut K** is the first migration of a **FAT 8.3 short-name collision leaf** — in-place basename mutate with temporary **`std`/`cld`** direction-flag discipline and **CF** exhausted polarity.  
> Cuts A–J remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `fat_next_short_name` |
| **Source** | [`kernel/fs/fat.inc:395–485`](../../kernel/fs/fat.inc) |
| **Subsystem** | Filesystem / FAT 8.3 short-name collision generation |
| **Purpose** | Mutate an 11-byte 8.3 name buffer in place to the next collision candidate (`~N` digit walk / insert / shrink). **CF=0** OK / **CF=1** exhausted. |

---

## Candidate comparison

### Candidate 1: `fat_next_short_name` — **SELECTED**

| Field | Detail |
|-------|--------|
| Source | `fs/fat.inc:395–485` |
| Purpose | In-place 8.3 collision mutate; reverse `~` search (`std`/`cld`); digit increment / expand / shrink; CF=exhausted |
| Complexity | Bounded 8-byte basename control flow; DF discipline; CF return; pushad preserve |
| Callers | 2 live (`fat_gen_short_name` lossy path; create `.short_name_found`) |
| ABI | Regcall: `EDI`→11-byte name; **CF=0** OK / **CF=1** err; `pushad`/`popad`; plain `ret`; leaves **DF clear** |
| Deps | Mutates caller buffer only; no tables/HW/IRQ/sched/alloc |
| Reloc risk | **None** — no static data |
| Compiler helper risk | Low if coded as explicit byte stores (avoid memset for zero-fill) |
| Risk | Med (subtle DF/CF + digit-carry control flow) |

### Candidate 2: `xfs._.extent_unpack` — rejected for Cut K

| Field | Detail |
|-------|--------|
| Source | `fs/xfs.asm:1465–1497` |
| Why rejected | New FS + BE bitfields are attractive, but EBP→partition scratch ABI is harder and lacks DF/CF/loop novelty after H–J. Inferior narrative vs FAT DF leaf. |

### Candidate 3: `memmove` — rejected for Cut K

| Field | Detail |
|-------|--------|
| Source | `kernel.asm:3232–3264` |
| Why rejected | Best memcpy-class helper-risk probe, but algorithmically thin and ~24 hot callers (huge blast radius). Defer to a dedicated cut. |

### Candidate 4: `xfs_hashname` — rejected for Cut K

| Field | Detail |
|-------|--------|
| Why rejected | Safe XFS leaf, but too small / little ABI pressure after A–J. |

### Candidate 5: `blit_clip` — rejected for Cut K

| Field | Detail |
|-------|--------|
| Why rejected | Video geometry composition after Cut H — banned as neighbor of proven geometry. |

### Candidate 6: NTFS helpers — rejected for Cut K

| Field | Detail |
|-------|--------|
| Why rejected | Cuts I–J already covered NTFS; Cut K must expand subsystem coverage. |

### Candidate 7: `fat_time_to_bdfe` / `fat_date_to_bdfe` / `strtoint_dec` / `strrchr` — rejected

| Field | Detail |
|-------|--------|
| Why rejected | Too small, calendar-adjacent, conf-only, or string-family DF without FAT relevance. |

---

## Why Cut K is a meaningful next step

Cuts A–J proved:

```text
Unicode / casefold / string / checksum / FS calendar / video geometry (CF+mutate)
/ NTFS VLE MCB codec / NTFS USA integrity restore
```

Cut K must answer a **different** question:

> Does Strategy A + C remain viable for a **DF-sensitive FAT 8.3 short-name collision leaf** (`std`/`cld` reverse scan, in-place basename digit arithmetic, CF=exhausted) with zero tables, explicit byte stores, and a byte-exact differential oracle — without compiler `memset`/`memcpy`/GOT?

`fat_next_short_name` is the right probe:

1. **Different subsystem** — FAT naming (not NTFS, not video geometry)  
2. **New ABI class** — temporary **DF** ownership + **CF** error (A–J never owned `std`)  
3. **Strategy A+C fit** — pure leaf; fixed 11-byte buffer; no `.rodata`  
4. **Already parked** by Cut I/J audits as the strongest deferred alternate  
5. **Limited blast radius** — 2 callers; independent rollback switch  
6. **Testability** — synthesize 11-byte vectors; compare CF + full buffer  

---

## ABI

**LOCAL FACT** — body `fat.inc:395–485`; callers:

```asm
        mov     edi, name11
        call    fat_next_short_name
        jc      .exhausted
        ; basename bytes 0..7 mutated; ext 8..10 untouched
```

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EDI` → 11-byte 8.3 name (mutates basename 0..7) |
| Output | mutated basename; **CF=0** OK / **CF=1** exhausted |
| Preserved | all GPRs via `pushad`/`popad` |
| DF | original clears DF before return; trampoline must `cld` |

### Rust FFI

| Item | Contract |
|------|----------|
| Symbol | `rust_fat_next_short_name` |
| Convention | `extern "stdcall"` |
| Args | `(name: *mut u8)` — 11-byte buffer |
| Return | `u32` in `EAX`: `0` OK / `1` fail |
| Epilogue | `ret 4` |

### Trampoline sketch

```asm
fat_next_short_name:
        pushad
        stdcall rust_fat_next_short_name, edi
        test    eax, eax
        popad
        cld
        jz      .rust_fat_ok
        stc
        ret
.rust_fat_ok:
        clc
        ret
```

---

## Rollback

| Switch | `USE_RUST_FAT_NEXT_SHORT_NAME` (`1` default / `0` original FASM) |
|--------|------------------------------------------------------------------|

---

## Intended architectural proof

| Dimension | Intent |
|-----------|--------|
| Subsystem novelty | First FAT leaf; first DF-sensitive leaf |
| Memory | In-place 8-byte basename mutate; extension untouched |
| Flags | CF fail polarity (Cut H/J style) + DF clear-on-exit |
| Compiler | Zero helpers / zero relocs (Cut I invariant) |
| Oracle | FASM-faithful differential on CF + 11 bytes |

---

## Explicitly not proven by Cut K

* Live FAT create/collision on a mounted volume in QEMU  
* Host-assembled FASM binary vs Rust byte differential  
* Pathological OOB FASM walks (all-space basename / `~` at index 0 with all-9s)  
* Migrating `fat_gen_short_name` / `memmove` / XFS / Cut L  

---

## Completion checklist (gates)

| Gate | Expected |
|------|----------|
| Candidate audit | PASS |
| New subsystem (FAT, not NTFS/video) | PASS |
| Dependency / compiler artifact audit | PASS — none |
| Relocations | **0** |
| Differential + PRNG | PASS |
| ABI / DF / CF / registers | PASS |
| Kernel smoke + QEMU ON/OFF | PASS |
| Cuts A–J regression (locked hashes) | PASS |
| Determinism ×2 | PASS |
| Documentation | COMPLETE |

**Do not start Cut L after Cut K verification is green — STOP.**
