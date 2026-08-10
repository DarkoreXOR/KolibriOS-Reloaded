# Cut AJ Implementation — `iso9660_compare_name`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-aj-plan.md`](cut-aj-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `iso9660_compare_name` |
| Source | [`kernel/fs/iso9660.inc`](../../kernel/fs/iso9660.inc) |
| Callers | 1 live (`iso9660_find_file` directory walk) |
| Rust symbol | `rust_iso9660_compare_name` |
| Pure helper | `kolibri_utils::iso9660_compare_name` |
| Composes | Cut AB `utf8to16` + Cut C `utf16_to_upper` (inlined) |
| Subsystem | ISO9660 / path-component name match |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. AH+AI helper reuse still ≠ Rust-owned exFAT subsystem.
Socket membership blockers unchanged. Pairing `iso9660_compare_name` with
`cd_compare_name` would be pattern reuse, not a Rust-owned ISO core. See
[`cut-aj-plan.md`](cut-aj-plan.md).

---

## Candidate comparison (post-AI audit)

| Candidate | Outcome |
|-----------|---------|
| `iso9660_compare_name` | **Selected** — first ISO leaf; meaningful match SM; 1 caller |
| `xfs._.conv_bigtime_to_kos_epoch` | #2 — strong but calendar stack already deep |
| `fat_name_is_legal` | #3 — real validate leaf; novelty diluted after Cut U |
| `cd_compare_name` | Follow-up near-dupe; not Path A with #1 |
| `socket_check` | Deferred — FASM mutex/list ownership unchanged |
| FAT datetime / thin hashes / `createMcbEntry` / `memmove` | Reject / defer |

---

## Legacy ABI

FASM leaf in `iso9660.inc` (retained under `USE_RUST_ISO9660_COMPARE_NAME=0`):

```text
call / ret
in:  ESI → UTF-8 path component
     EDI → ISO9660_DIRECTORY_RECORD
     EBP → ISO9660 (reads type_encoding)
out: CF=0 match — ESI advanced to '/' or NUL (not past '/')
     CF=1 miss  — ESI restored to entry value
preserves: EAX, ECX, EDI (via push/pop); EBP untouched
clobbers: EDX
```

Critical quirks retained:

* ASCII (`type_encoding==0`): per-byte via `shl ax,8` / `dec edi` / `xchg` / `add edi,2`
* UCS-2 BE (nonzero): `xchg` endianness per code unit
* End: ASCII `';` byte, UCS-2 BE word `0x3B00`, or `edi == name_start+name_len`
* Composes live `utf8to16` / `utf16toUpper` on the FASM path

---

## Rust ABI

```text
stdcall rust_iso9660_compare_name(esi_inout, dir_record, type_encoding) -> EAX
ret 12
EAX=0 match / 1 miss
on match: *esi_inout advanced; on miss: unchanged
```

Trampoline: push EAX/ECX/EDX/EBX/EDI/ESI; pass `&ESI`, EDI, `[ebp+type_encoding]`;
map EAX→`clc`/`stc`; restore regs (ESI updated from slot).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `iso9660_compare.rs` + `ffi.rs` section `.text.rust_iso9660_compare_name` |
| Extract | `extract_reloc_free_text.py` → `rust_iso9660_compare_name.bin` |
| Embed | `kernel/rust/iso9660_compare_name.inc` `file` directive |
| Trampoline | `iso9660.inc` under `USE_RUST_ISO9660_COMPARE_NAME` |
| Gate | `USE_RUST_ISO9660_COMPARE_NAME` (dev 0 → prod 1) |
| Smoke | `iso9660_compare_name_rust_smoke_test` in `iso9660.inc` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_iso9660_compare_name` |
| Blob/object size | 485 bytes |
| Relocations | 0 |
| SHA-256 | `D26685FEE80CCF355F075CAC475151A67DD23AD529F8B144D74B39BB70D2D529` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow ISO compare oracle vs Rust | **PASS** |
| Named vectors | ASCII exact; `;1` version; `/` component; mismatch; short; UCS-2 BE; UCS-2 `;` |
| PRNG | 50 000 vectors, seed `0x4355544A` (`'CUTJ'`) |
| Host tests | **363/363** cargo tests (354 AI baseline + 9 new ISO) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `iso9660_compare_name_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | ASCII match + ESI@NUL; mismatch ESI restore; `FILE;1`; `dir/` component; EAX/EBX/ECX/EDI/EBP canaries |
| Marker | `rust_iso9660_compare_name_smoke_result = 'ISOC'` on success |

---

## QEMU validation

Kernels built with Cuts A–AI production gates intact (`USE_RUST_EXFAT_HASH_CALCULATE=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_ISO9660_COMPARE_NAME=0` | **OK** (QMP `running` + screendump `dev_build/cut-aj-off.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_ISO9660_COMPARE_NAME=1` | **OK** (screendump `dev_build/cut-aj-on.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

Real subsystem soak: **NOT AVAILABLE** — no scripted ISO9660 path-lookup harness; attaching `images/exfat-image.img` does not evidence CD/ISO name match.

Production image: `dev_build/cut-aj-final.img`.

---

## Production gate

```text
USE_RUST_ISO9660_COMPARE_NAME = 1
```

Rollback: `USE_RUST_ISO9660_COMPARE_NAME = 0` (or `enabled = false` in `project/build.toml` Cut AJ migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/iso9660_compare.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/out/rust_iso9660_compare_name.bin` (generated)
* `kernel/rust/iso9660_compare_name.inc` (new)
* `kernel/fs/iso9660.inc` (trampoline + gate + ABI smoke; legacy body retained)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-aj-plan.md`
* `docs/migration/cut-aj-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* Stock-image / attached-media ISO9660 path-lookup soak not claimed at cut time;
  post-cut regression fix routes ATAPI `cd_compare_name` through the same Rust
  leaf (Joliet UCS-2 BE) and hardens `utf16toUpper` to preserve ECX/EDX.
* `socket_check` / `memmove` / FAT datetime / `createMcbEntry` remain deferred.
* No Path A cluster claimed.
* Current default `qemu.args` do not include e1000 (desktop regression only).
