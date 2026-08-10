# Cut AD Implementation — `is_protective_mbr`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ad-plan.md`](cut-ad-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `is_protective_mbr` |
| Source | [`kernel/blkdev/disk.inc`](../../kernel/blkdev/disk.inc) |
| Callers | 1 live (`disk_scan_partitions` GPT branch) |
| Rust symbol | `rust_is_protective_mbr` |
| Pure helper | `kolibri_utils::is_protective_mbr` |
| Subsystem | Disk / GPT protective MBR |
| Migration kind | **Single-function cut** (Path B; cluster rejected) |

---

## Cluster audit summary

Path A (cluster) was **rejected**. Proven A–AC infra is sufficient for Stage-2 leaves, but no remaining multi-function cluster eliminates FASM↔Rust boundaries with controlled blast better than a single leaf. Rejected clusters: IPv4 helpers (anti-cluster after AC), unicode map+string (anti-cluster after AB), FAT DOS datetime (low novelty), taskman (empty). See [`cut-ad-plan.md`](cut-ad-plan.md).

---

## Candidate comparison (post-AC audit)

| Candidate | Outcome |
|-----------|---------|
| `is_protective_mbr` | **Selected** — GPT protective-MBR ZF; standing #2; Z cooled three cuts |
| `ntfs_test_bootsec` | Deferred #2 — FS bootsec+CF |
| `socket_check` | Deferred #3 — socket-list ZF membership |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |
| `net_ptr_to_num4` / unicode map | Rejected — anti-cluster after AC / AB |

---

## Legacy ABI

FASM leaf in `disk.inc` (retained under `USE_RUST_IS_PROTECTIVE_MBR=0`):

```text
call / ret
in:  ECX → partition-table array (MBR+0x1BE)
     ESI → DISK (Capacity low dword only)
out: ZF set = protective MBR; ZF clear = not
preserves: ECX, EDI (explicit push/pop); ESI untouched
clobbers: EAX (left 0), other flags unspecified
```

Critical quirks retained:

* `[ecx-2]` word must be 0
* Entry 0: bootable=0 (not 0x80), type=`0xEE`, FirstAbs=1
* Length=`0xFFFFFFFF` **or** wrapping `(-1 + Capacity_lo)`
* Entries 1–3: 48 zero bytes (`repz scasw`)
* Capacity **high** dword ignored

---

## Rust ABI

```text
stdcall rust_is_protective_mbr(pt_array, capacity_lo) -> EAX
ret 8
EAX = 0 protective / 1 not
```

Trampoline injects `[esi+DISK.MediaInfo.Capacity]`; `test eax,eax` restores ZF; preserves ECX/ESI/EDI.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `partition.rs` + `ffi.rs` section `.text.rust_is_protective_mbr` |
| Extract | `extract_reloc_free_text.py` → `rust_is_protective_mbr.bin` |
| Embed | `kernel/rust/is_protective_mbr.inc` `file` directive |
| Trampoline | `disk.inc` under `USE_RUST_IS_PROTECTIVE_MBR` |
| Gate | `USE_RUST_IS_PROTECTIVE_MBR` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_is_protective_mbr` |
| Blob/object size | 513 bytes |
| Relocations | 0 |
| SHA-256 | `20831737E6269493000A63AF6A76BC027724364B5F4F498202BB1979C8AF5748` |

Note: size reflects LLVM unrolling of the 48-byte trailing-zero scan; still reloc-free with trailing `ret 8`.

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | canonical `len=-1`; `Capacity_lo-1`; pre-word reject; bootable 0x80; wrong type; wrong first LBA; nonzero trail; capacity wrap |
| Boundary | capacity 0 / max; length wrap `-1+cap` |
| PRNG | 50 000 vectors, seed `0x43555444` (`'CUTD'`) |
| Host tests | **310/310** cargo tests (includes Cut Z partition suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `is_protective_mbr_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CAD` hang) |
| Vectors | Canonical protective; capacity−1; type/pre/trail/bootable rejects; ECX/ESI/EDI/EBX/EBP canaries |
| Marker | `rust_is_protective_mbr_smoke_result = 'IPMB'` on success |

---

## QEMU validation

Kernels built with Cuts A–AC production gates intact (`USE_RUST_IPV4_ROUTE=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace.

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_IS_PROTECTIVE_MBR=0` | **OK** (QMP `running` + screendump `tmp_images/cut-ad-off.ppm`, 779426 non-black samples) | e1000 + user net present |
| ON | `USE_RUST_IS_PROTECTIVE_MBR=1` | **OK** (QMP `running` + screendump `tmp_images/cut-ad-on.ppm`, 779426 non-black samples) | e1000 + user net present |

Smoke (ON): **PASS** (no `0xDEAD0CAD`; boot continued).

Real subsystem soak: **NOT AVAILABLE** — stock floppy is legacy MBR, not a GPT protective-MBR media; `disk_scan_gpt` path is not evidenced on the reference image. Boot smoke exercises the public `is_protective_mbr` symbol on synthetic buffers (ABI path), recorded under ABI smoke above.

Production image: `tmp_images/cut-ad-final.img`.

---

## Production gate

```text
USE_RUST_IS_PROTECTIVE_MBR = 1
```

Rollback: `USE_RUST_IS_PROTECTIVE_MBR = 0` (or `enabled = false` in `orch/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/partition.rs` (Cut AD helpers + tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-is-protective-mbr.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_is_protective_mbr.bin` (generated)
* `kernel/rust/is_protective_mbr.inc` (new)
* `kernel/blkdev/disk.inc` (trampoline + gate)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml`
* `orch/README.md`
* `orch/src/config.rs` / `main.rs` / `Cargo.toml` (A–AD comments)
* `docs/migration/cut-ad-plan.md`
* `docs/migration/cut-ad-implementation.md`
* `docs/migration/migration-plan.md`
* `tmp_images/README.md`

---

## Known limitations

* Blob is larger than the FASM leaf due to unrolled trailing-zero checks; functionally reloc-free.
* Capacity high dword ignored (legacy quirk preserved).
* Does not migrate `disk_scan_gpt` or rewrite partition scanning.
* Stock-image GPT protective-MBR soak not claimed.
* `memmove` / Stage-4 VA→PA / `ntfs_test_bootsec` / `socket_check` remain deferred.
* Cluster migration remains premature after AD.
