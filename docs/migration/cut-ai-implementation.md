# Cut AI Implementation — `exFAT_hash_calculate` (NameHash)

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ai-plan.md`](cut-ai-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `exFAT_hash_calculate` (extracted from former inline in `exFAT_find_lfn`) |
| Source | [`kernel/fs/exfat.inc`](../../kernel/fs/exfat.inc) |
| Callers | 1 live (`exFAT_find_lfn` after UTF-8→UTF-16 upper; stores `[exFAT.current_hash]`) |
| Rust symbol | `rust_exfat_hash_calculate` |
| Pure helper | `kolibri_utils::exfat_hash_calculate` |
| Shared core | `kolibri_utils::exfat_rolling_checksum` (same as Cut AH; `skip=false`) |
| Subsystem | exFAT / NameHash |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Cut AH already shipped SetChecksum with the shared
rolling core; migrating NameHash reuses that helper but does **not** create a
new Rust-owned exFAT subsystem (no shared mutable Rust state, no multi-crossing
collapse beyond one leaf). Socket membership blockers unchanged. See
[`cut-ai-plan.md`](cut-ai-plan.md).

---

## Candidate comparison (post-AH audit)

| Candidate | Outcome |
|-----------|---------|
| `exFAT_hash_calculate` (NameHash) | **Selected** — clean extractable boundary; AH core ready |
| Path A SetChecksum+NameHash | Rejected — AH already done; helper reuse ≠ Path A |
| `socket_check` | Deferred — FASM mutex/list ownership unchanged |
| FAT datetime / thin hashes | Rejected — low novelty / thin |
| `createMcbEntry` / `memmove` | Deferred — high blast / Stage-4 |

---

## Legacy ABI

FASM leaf in `exfat.inc` (retained under `USE_RUST_EXFAT_HASH_CALCULATE=0`):

```text
call / ret
in:  ESI → NameUTF16 bytes
     ECX = byte length (excl. UTF-16 terminator; caller does edi−esi−2)
out: AX = NameHash
legacy FASM: burns ECX to 0; advances ESI by ECX; uses EBX scratch (push/pop)
call site: push ebx ecx … call … pop ecx ebx; mov [current_hash], eax
```

Critical quirks retained:

* Per-byte: `((c & 1) ? 0x8000 : 0) + (c >> 1) + byte` (16-bit wrap)
* **No** skip of indices 2–3 (unlike SetChecksum)
* Byte-oriented (odd lengths allowed)
* FASM do-while hangs if `ECX==0`; Rust returns 0 (documented)
* No store inside the leaf — caller writes `current_hash`

---

## Rust ABI

```text
stdcall rust_exfat_hash_calculate(buf, len) -> EAX
ret 8
EAX low 16 = NameHash; no memory store
```

Trampoline: `stdcall rust_*(esi, ecx)`; preserves EBX/ECX/EDX/ESI/EDI.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `exfat_checksum.rs` + `ffi.rs` section `.text.rust_exfat_hash_calculate` |
| Extract | `extract_reloc_free_text.py` → `rust_exfat_hash_calculate.bin` |
| Embed | `kernel/rust/exfat_hash_calculate.inc` `file` directive |
| Trampoline | `exfat.inc` under `USE_RUST_EXFAT_HASH_CALCULATE` |
| Gate | `USE_RUST_EXFAT_HASH_CALCULATE` (dev 0 → prod 1) |
| Smoke | `exfat_hash_calculate_rust_smoke_test` in `exfat.inc` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_exfat_hash_calculate` |
| Blob/object size | 140 bytes |
| Relocations | 0 |
| SHA-256 | `BC6C94693B0A78FD12A914BCA802AFB34797CB093BC9BFD0445D4A3AA9DF221E` |

Prior Cut AH blob unchanged:
`2D8B6EA1E5974639DB1F72A4E7AB26EC035D1C0113DB27E1DD356BBC0EED0DCD` (149 B).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow NameHash oracle vs Rust | **PASS** |
| Named vectors | empty; UTF-16 `"AB"`; odd length; wrap/odd-bit; differs from SetChecksum when field bytes nonzero |
| PRNG | 50 000 vectors, seed `0x43555449` (`'CUTI'`) |
| Host tests | **354/354** cargo tests (350 AH baseline + 4 new NameHash) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `exfat_hash_calculate_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | UTF-16 `"AB"` len=4; AX matches `rust_*`; empty len→0; odd len=3; EBX/EDX/EDI/EBP canaries; ECX/ESI preserve under Rust ON |
| Marker | `rust_exfat_hash_calculate_smoke_result = 'EXNH'` on success |

---

## QEMU validation

Kernels built with Cuts A–AH production gates intact (`USE_RUST_CALCULATE_SET_CHECKSUM_FIELD=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_EXFAT_HASH_CALCULATE=0` | **OK** (QMP `running` + screendump `dev_build/cut-ai-off.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_EXFAT_HASH_CALCULATE=1` | **OK** (screendump `dev_build/cut-ai-on.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

Real subsystem soak: **NOT AVAILABLE** — attaching `images/exfat-image.img` does not by itself evidence `exFAT_find_lfn` / NameHash beyond boot smoke. No scripted exFAT path lookup was run.

Production image: `dev_build/cut-ai-final.img`.

---

## Production gate

```text
USE_RUST_EXFAT_HASH_CALCULATE = 1
```

Rollback: `USE_RUST_EXFAT_HASH_CALCULATE = 0` (or `enabled = false` in `project/build.toml` Cut AI migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/exfat_checksum.rs` (NameHash API + oracle + tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/out/rust_exfat_hash_calculate.bin` (generated)
* `kernel/rust/exfat_hash_calculate.inc` (new)
* `kernel/fs/exfat.inc` (extract leaf + trampoline + gate + ABI smoke; call site)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `tools/migration_gates/apply_gates.py` (ASCII arrow for Windows cp1251 consoles)
* `docs/migration/cut-ai-plan.md`
* `docs/migration/cut-ai-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Stock-image / attached-disk exFAT NameHash soak not claimed.
* Legacy FASM `ECX==0` hang retained; Rust returns 0.
* `socket_check` / `memmove` / FAT datetime / `createMcbEntry` remain deferred.
* No Path A cluster claimed — AI completes the AH shared-core foreshadow as Path B.
* Current default `qemu.args` do not include e1000 (desktop regression only).
