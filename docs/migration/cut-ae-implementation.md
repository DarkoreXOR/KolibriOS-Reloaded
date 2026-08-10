# Cut AE Implementation — `ntfs_datetime_to_bdfe`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ae-plan.md`](cut-ae-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ntfs_datetime_to_bdfe` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | 6 live (NTFS dirent → BDFE timestamp chains) |
| Rust symbol | `rust_ntfs_datetime_to_bdfe` |
| Pure helper | `kolibri_utils::ntfs_datetime_to_bdfe` |
| Subsystem | NTFS / FILETIME → BDFE |
| Migration kind | **Single-function cut** (Path B; cluster rejected) |

---

## Cluster audit summary

Path A (cluster) was **rejected**. Proven A–AD infra is sufficient for Stage-2 leaves, but no remaining multi-function cluster eliminates FASM↔Rust boundaries with controlled blast better than a single leaf. Rejected: Disk/GPT orchestration after Z+AD, FAT DOS datetime (low novelty vs G/T), NTFS epoch twin as forced same-cut cluster, XFS hash siblings, unicode map anti-cluster, IPv4 helpers anti-cluster. See [`cut-ae-plan.md`](cut-ae-plan.md).

---

## Candidate comparison (post-AD audit)

| Candidate | Outcome |
|-----------|---------|
| `ntfs_datetime_to_bdfe` | **Selected** — 1601×10⁷ epoch + compose Cut T; 6 callers |
| `ntfs_test_bootsec` | Deferred #2 — FS bootsec+CF (avoid validate-stack after AD) |
| `socket_check` | Deferred #3 — Stage-5 socket-list ZF foothold |
| `calculate_SetChecksum_field` | Deferred — exFAT rolling checksum |
| `xfs_hashname` / thin P-family | Rejected — thin / repetition |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |

---

## Legacy ABI

FASM leaf in `ntfs.inc` (retained under `USE_RUST_NTFS_DATETIME_TO_BDFE=0`):

```text
call / ret  (tail-jumps to fsTime2bdfe when FASM body)
in:  EDX:EAX = FILETIME (100ns since 1601-01-01)
     EDI → BDFE outbuf
out: EDI = EDI+8; 8-byte BDFE written
clobbers: EAX, EBX, ECX, EDX (via fsTime2bdfe)
preserves: ESI, EBP (untouched)
```

Critical quirks retained:

* Bias `sub/sbb` with `3365781504` / `29389701`
* If post-bias `EDX >= 10000000` then `EDX := 0` before `div`
* Pre-2001 FILETIME wraps (unsigned underflow)
* Div remainder discarded; calendar via `fsTime2bdfe` (hour as word; pad cleared)

---

## Rust ABI

```text
stdcall rust_ntfs_datetime_to_bdfe(ft_lo, ft_hi, out) ; ret 12
writes 8-byte BDFE at out (inlines fs_time2bdfe; reloc-free)
```

Trampoline: `stdcall …, eax, edx, edi` then `add edi, 8`.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `time.rs` + `ffi.rs` section `.text.rust_ntfs_datetime_to_bdfe` |
| Extract | `extract_reloc_free_text.py` → `rust_ntfs_datetime_to_bdfe.bin` |
| Embed | `kernel/rust/ntfs_datetime_to_bdfe.inc` `file` directive |
| Trampoline | `ntfs.inc` under `USE_RUST_NTFS_DATETIME_TO_BDFE` |
| Gate | `USE_RUST_NTFS_DATETIME_TO_BDFE` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ntfs_datetime_to_bdfe` |
| Blob/object size | 526 bytes |
| Relocations | 0 |
| SHA-256 | `44E05987E751BBD8C6BE4655BE9F747523EE065B70892551808D4B010B805F10` |

Trailing instruction is `ret 12` (`C2 0C 00`). Size includes inlined Cut T calendar (month tables stack-materialized).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | epoch bias; +1s; leap 2004-02-29; 2010-07-04 noon; end-of-day; pre-2001 wrap; EDX clamp; just-below-clamp |
| Boundary | max FILETIME (when div fits); bias−1; clamp edges |
| PRNG | 50 000 vectors, seed `0x43555445` (`'CUTE'`) |
| Host tests | **322/322** cargo tests |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ntfs_datetime_to_bdfe_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CAE` hang) |
| Vectors | Bias epoch; +1s; leap; end-of-day pad; EDI+=16 chain; EDX clamp → 5s |
| Marker | `rust_ntfs_datetime_to_bdfe_smoke_result = 'NTDB'` on success |

---

## QEMU validation

Kernels built with Cuts A–AD production gates intact (`USE_RUST_IS_PROTECTIVE_MBR=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace.

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_NTFS_DATETIME_TO_BDFE=0` | **OK** (QMP `running` + screendump `tmp_images/cut-ae-off.ppm`, 779380 non-black samples) | e1000 + user net present |
| ON | `USE_RUST_NTFS_DATETIME_TO_BDFE=1` | **OK** (QMP `running` + screendump `tmp_images/cut-ae-on.ppm`, 779426 non-black samples) | e1000 + user net present |

Smoke (ON): **PASS** (no `0xDEAD0CAE`; boot continued).

Real subsystem soak: **NOT AVAILABLE** — stock floppy is not an NTFS volume; dirent FILETIME→BDFE path is not evidenced on the reference image. Boot smoke exercises the public `ntfs_datetime_to_bdfe` symbol on synthetic FILETIME values (ABI path), recorded under ABI smoke above.

Production image: `tmp_images/cut-ae-final.img`.

---

## Production gate

```text
USE_RUST_NTFS_DATETIME_TO_BDFE = 1
```

Rollback: `USE_RUST_NTFS_DATETIME_TO_BDFE = 0` (or `enabled = false` in `orch/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs` (Cut AE helpers + oracle tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-ntfs-datetime-to-bdfe.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_ntfs_datetime_to_bdfe.bin` (generated)
* `kernel/rust/ntfs_datetime_to_bdfe.inc` (new)
* `kernel/fs/ntfs.inc` (trampoline + gate)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml`
* `orch/README.md`
* `orch/src/config.rs` / `main.rs` (A–AE comments)
* `docs/migration/cut-ae-plan.md`
* `docs/migration/cut-ae-implementation.md`
* `docs/migration/migration-plan.md`
* `tmp_images/README.md`

---

## Known limitations

* Blob is larger than the FASM leaf because it inlines `fs_time2bdfe` calendar; functionally reloc-free.
* Does not migrate `ntfs_test_bootsec` / dirent enumeration.
* Stock-image NTFS FILETIME soak not claimed.
* `memmove` / Stage-4 VA→PA / `socket_check` remain deferred.
* Cluster migration remains premature after AE.
* Cut AF (`ntfsCalculateTime`) completes the FILETIME twin as a later Path B.
