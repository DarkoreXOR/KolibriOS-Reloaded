# Cut AF Implementation — `ntfsCalculateTime`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-af-plan.md`](cut-af-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ntfsCalculateTime` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | 3 live (`ntfs_SetFileInfo` created/accessed/modified) |
| Related | `ntfsGetTime` shares FASM ×10⁷+bias after `fsGetTime` (not migrated) |
| Rust symbol | `rust_ntfs_calculate_time` |
| Pure helper | `kolibri_utils::ntfs_calculate_time` |
| Subsystem | NTFS / BDFE → FILETIME |
| Migration kind | **Single-function cut** (Path B; cluster rejected) |

---

## Cluster audit summary

Path A (cluster) was **rejected**. Proven A–AE infra is sufficient for Stage-2 leaves, but no remaining multi-function cluster eliminates FASM↔Rust boundaries with controlled blast better than a single leaf. Rejected: NTFS epoch twin as forced same-cut cluster (AE already shipped), GetTime+CalculateTime as Path A (one algorithm), disk orchestration after Z+AD, FAT DOS datetime (low novelty), XFS/unicode/IPv4 anti-clusters, empty taskman follow-ons. See [`cut-af-plan.md`](cut-af-plan.md).

---

## Candidate comparison (post-AE audit)

| Candidate | Outcome |
|-----------|---------|
| `ntfsCalculateTime` | **Selected** — AE inverse; compose Cut G + AE bias; 3 callers |
| `ntfs_test_bootsec` | Deferred #2 — FS bootsec+CF |
| `socket_check` | Deferred #3 — Stage-5 socket-list ZF foothold |
| `calculate_SetChecksum_field` | Deferred — exFAT rolling checksum |
| FAT datetime / thin P-family / `xfs_hashname` | Rejected — low novelty / thin |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |

---

## Legacy ABI

FASM leaf in `ntfs.inc` (retained under `USE_RUST_NTFS_CALCULATE_TIME=0`):

```text
call / ret
in:  ESI → BDFE datetime block
out: EDX:EAX = FILETIME (100ns since 1601-01-01)
clobbers: EAX, EBX, ECX, EDX (via fsCalculateTime + mul)
preserves: ESI, EDI, EBP
```

Critical quirks retained:

* `call fsCalculateTime` → EAX seconds since 2001-01-01
* `mov edx, 10000000` / `mul edx` / `add eax, 3365781504` / `adc edx, 29389701`
* Year clamp via Cut G semantics
* When Rust ON, `ntfsGetTime` keeps an inlined FASM copy of the scale/bias after `fsGetTime` (fall-through split)

---

## Rust ABI

```text
stdcall rust_ntfs_calculate_time(block) -> u64 in EDX:EAX ; ret 4
reads 8-byte BDFE at block (inlines fs_calculate_time + bias; reloc-free)
```

Trampoline: `stdcall …, esi` then `ret` (EDX:EAX already set).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `time.rs` + `ffi.rs` section `.text.rust_ntfs_calculate_time` |
| Extract | `extract_reloc_free_text.py` → `rust_ntfs_calculate_time.bin` |
| Embed | `kernel/rust/ntfs_calculate_time.inc` `file` directive |
| Trampoline | `ntfs.inc` under `USE_RUST_NTFS_CALCULATE_TIME` |
| Gate | `USE_RUST_NTFS_CALCULATE_TIME` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ntfs_calculate_time` |
| Blob/object size | 459 bytes |
| Relocations | 0 |
| SHA-256 | `FF7337CA0D3E6699431F6C9812AFF8014EB8F4F51C9AFC90AC6D70A25131CA06` |

Hot path ends with `ret 4` (`C2 04 00`); LLVM places a cold fallthrough after the epilogue (shared-epilogue note; same class as Cut AC). Size includes inlined Cut G calendar (month tables stack-materialized).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | epoch bias; +1s; leap 2004-02-29; end-of-day; 2010-07-04 noon; pre-2001 year clamp |
| Composition | AF → AE round-trip on production-domain BDFE |
| PRNG | 50 000 vectors, seed `0x43555446` (`'CUTF'`) |
| Host tests | **330/330** cargo tests |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ntfs_calculate_time_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CAF` hang) |
| Vectors | Bias epoch; +1s; leap vs FASM mul path; end-of-day; ESI chaining |
| Marker | `rust_ntfs_calculate_time_smoke_result = 'NTCT'` on success |

---

## QEMU validation

Kernels built with Cuts A–AE production gates intact (`USE_RUST_NTFS_DATETIME_TO_BDFE=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace.

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_NTFS_CALCULATE_TIME=0` | **OK** (QMP `running` + screendump `tmp_images/cut-af-off.ppm`, 779426 non-black samples) | e1000 + user net present |
| ON | `USE_RUST_NTFS_CALCULATE_TIME=1` | **OK** (QMP `running` + screendump `tmp_images/cut-af-on.ppm`, 779426 non-black samples) | e1000 + user net present |

Smoke (ON): **PASS** (no `0xDEAD0CAF`; boot continued).

Real subsystem soak: **NOT AVAILABLE** — stock floppy is not an NTFS volume; SetFileInfo FILETIME write path is not evidenced on the reference image. Boot smoke exercises the public `ntfsCalculateTime` symbol on synthetic BDFE values (ABI path), recorded under ABI smoke above.

Production image: `tmp_images/cut-af-final.img`.

---

## Production gate

```text
USE_RUST_NTFS_CALCULATE_TIME = 1
```

Rollback: `USE_RUST_NTFS_CALCULATE_TIME = 0` (or `enabled = false` in `orch/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs` (Cut AF helpers + oracle tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-ntfs-calculate-time.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_ntfs_calculate_time.bin` (generated)
* `kernel/rust/ntfs_calculate_time.inc` (new)
* `kernel/fs/ntfs.inc` (trampoline + gate; GetTime FASM scale when ON)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml`
* `orch/README.md`
* `orch/src/config.rs` / `main.rs` (A–AF comments)
* `docs/migration/cut-af-plan.md`
* `docs/migration/cut-af-implementation.md`
* `docs/migration/migration-plan.md`
* `tmp_images/README.md`

---

## Known limitations

* Blob is larger than the FASM leaf because it inlines `fs_calculate_time` calendar; functionally reloc-free.
* Does not migrate `ntfsGetTime` CMOS path (keeps FASM scale/bias duplicate when Rust ON).
* Does not migrate `ntfs_test_bootsec` / SetFileInfo orchestration.
* Stock-image NTFS SetFileInfo soak not claimed.
* `memmove` / Stage-4 VA→PA / `socket_check` remain deferred.
* Cluster migration remains premature after AF.
