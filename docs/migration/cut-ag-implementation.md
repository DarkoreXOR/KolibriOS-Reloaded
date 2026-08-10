# Cut AG Implementation — `ntfs_test_bootsec`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ag-plan.md`](cut-ag-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ntfs_test_bootsec` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | 2 live (`ntfs_create_partition` boot + mid-partition reads) |
| Rust symbol | `rust_ntfs_test_bootsec` |
| Pure helper | `kolibri_utils::ntfs_test_bootsec` |
| Subsystem | NTFS / mount bootsector validate |
| Migration kind | **Single-function cut** (Path B; cluster rejected) |

---

## Cluster audit summary

Path A (cluster) was **rejected** under the raised post-AF bar. A–AF infra is
sufficient for Stage-2 leaves, but no remaining multi-function group establishes
a genuine Rust-owned internal subsystem (shared state, fewer FASM↔Rust crossings,
coherent gate) better than another single leaf. Rejected: AE+AF as Path A
(inverse pair only), socket membership (dead wrapper + FASM mutex ownership),
disk scan orchestration after Z+AD, FAT datetime siblings, XFS/unicode/IPv4
anti-clusters, empty taskman follow-ons. See [`cut-ag-plan.md`](cut-ag-plan.md).

---

## Candidate comparison (post-AF audit)

| Candidate | Outcome |
|-----------|---------|
| `ntfs_test_bootsec` | **Selected** — FS bootsec multi-rule CF; standing #2 after AE/AF |
| `socket_check` | Deferred #2 — socket-list ZF; Stage-5 foothold |
| `calculate_SetChecksum_field` | Deferred #3 — exFAT rolling checksum novelty |
| FAT datetime ×4 / thin hashes / P-family | Rejected — low novelty / thin |
| `memmove` | Deferred Stage-4 — high blast |

---

## Legacy ABI

FASM leaf in `ntfs.inc` (retained under `USE_RUST_NTFS_TEST_BOOTSEC=0`):

```text
call / ret
in:  EBX → bootsector buffer
     EDX = partition size (sectors)
out: CF set = invalid; CF clear = valid
preserves: EBX, EDX, ECX, ESI, EDI, EBP
clobbers: EAX (EDX briefly pushed during MFT mul checks)
```

Critical quirks retained:

* OEM at +3 must be `'NTFS    '`
* Bytes/sector at +11 == `0x200`
* Sectors/cluster non-zero power of two (`dec`+`js`+`test al,[byte]`)
* FAT-compat fields at +14/+16/+20/+22/+32 zero
* TotalSectors high dword == 0; low <= partition
* `$MFT`/`$MFTMirr` high LCN == 0; `spc*LCN` 32-bit; <= partition
* ClustersPerFRS/Index: `-31..=-9` **or** non-zero power of two

---

## Rust ABI

```text
stdcall rust_ntfs_test_bootsec(boot, partition_sectors) -> EAX
ret 8
EAX = 0 valid / 1 invalid
```

Trampoline pushes EBX/EDX, calls Rust, restores, maps EAX→CF via `test`/`clc`/`stc`.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `ntfs_bootsec.rs` + `ffi.rs` section `.text.rust_ntfs_test_bootsec` |
| Extract | `extract_reloc_free_text.py` → `rust_ntfs_test_bootsec.bin` |
| Embed | `kernel/rust/ntfs_test_bootsec.inc` `file` directive |
| Trampoline | `ntfs.inc` under `USE_RUST_NTFS_TEST_BOOTSEC` |
| Gate | `USE_RUST_NTFS_TEST_BOOTSEC` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ntfs_test_bootsec` |
| Blob/object size | 271 bytes |
| Relocations | 0 |
| SHA-256 | `0CB05C0C90ED4892C1896DD37FE8E690567AC280C0938C99ADB92241DC6C2EC9` |

Prior Cut AF blob unchanged:
`FF7337CA0D3E6699431F6C9812AFF8014EB8F4F51C9AFC90AC6D70A25131CA06` (459 B).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | OEM/bps/spc/FAT/total/MFT overflow/FRS ranges/pow2 |
| Boundary | partition 0; FRS `-31..=-9` vs `-32`/`-8`/`0` |
| PRNG | 50 000 vectors, seed `0x43555447` (`'CUTG'`) |
| Host tests | **343/343** cargo tests (330 prior + 13 AG) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ntfs_test_bootsec_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CA7` hang) |
| Vectors | Canonical valid; OEM/bps/spc/total/FRS rejects; pow2 FRS accept; EBX/EDX/ESI/EDI/EBP canaries |
| Marker | `rust_ntfs_test_bootsec_smoke_result = 'NTBS'` on success |

---

## QEMU validation

Kernels built with Cuts A–AF production gates intact (`USE_RUST_NTFS_CALCULATE_TIME=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace.

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_NTFS_TEST_BOOTSEC=0` | **OK** (QMP `running` + screendump `dev_build/cut-ag-off.ppm`, 779426 non-black samples) | e1000 + user net present |
| ON | `USE_RUST_NTFS_TEST_BOOTSEC=1` | **OK** (QMP `running` + screendump `dev_build/cut-ag-on.ppm`, 779426 non-black samples) | e1000 + user net present |

Smoke (ON): **PASS** (no `0xDEAD0CA7`; boot continued).

Real subsystem soak: **NOT AVAILABLE** — stock floppy has no NTFS volume;
`ntfs_create_partition` / bootsec path is not evidenced on the reference image.
Boot smoke exercises the public `ntfs_test_bootsec` symbol on synthetic buffers
(ABI path), recorded under ABI smoke above.

Production image: `dev_build/cut-ag-final.img`.

---

## Production gate

```text
USE_RUST_NTFS_TEST_BOOTSEC = 1
```

Rollback: `USE_RUST_NTFS_TEST_BOOTSEC = 0` (or `enabled = false` in `orch/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/ntfs_bootsec.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-ntfs-test-bootsec.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_ntfs_test_bootsec.bin` (generated)
* `kernel/rust/ntfs_test_bootsec.inc` (new)
* `kernel/fs/ntfs.inc` (trampoline + gate)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml`
* `orch/README.md`
* `orch/src/config.rs` / `main.rs`
* `docs/migration/cut-ag-plan.md`
* `docs/migration/cut-ag-implementation.md`
* `docs/migration/migration-plan.md`
* `.cursor/rules/dev-build.mdc`

---

## Known limitations

* Stock-image NTFS mount soak not claimed.
* Does not migrate `ntfs_create_partition` orchestration or `createMcbEntry`.
* `socket_check` / `memmove` remain deferred; `calculate_SetChecksum_field`
  completed as Cut AH.
* No Path A cluster claimed — AG is a mount-path foothold leaf only.
