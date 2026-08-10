# Cut AH Implementation — `calculate_SetChecksum_field`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ah-plan.md`](cut-ah-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `calculate_SetChecksum_field` |
| Source | [`kernel/fs/exfat.inc`](../../kernel/fs/exfat.inc) |
| Callers | 1 live (exFAT dirent write path; also stores AX to `[edi+2]`) |
| Rust symbol | `rust_calculate_set_checksum_field` |
| Pure helper | `kolibri_utils::calculate_set_checksum_field` |
| Shared core | `kolibri_utils::exfat_rolling_checksum` (future NameHash) |
| Subsystem | exFAT / directory-entry SetChecksum |
| Migration kind | **Single-function cut** (Path B; cluster rejected) |

---

## Cluster audit summary

Path A (cluster) was **rejected** under the raised post-AF/AG bar. Live-tree
re-audit after AG confirmed no multi-function group establishes a genuine
Rust-owned internal subsystem. Rejected: socket membership (FASM-owned
`net_sockets`/`socket_mutex`, lock-free vs mutex ABI asymmetry, dead
`ptr_to_num`/`check_owner`), NTFS AE+AF+AG (no shared internals), disk scan
orchestration, FAT datetime siblings, XFS/unicode/IPv4 anti-clusters, empty
taskman follow-ons. Closest *future* Path A: exFAT SetChecksum + extracted
NameHash sharing `exfat_rolling_checksum` — NameHash is still inlined FASM,
not a callable. See [`cut-ah-plan.md`](cut-ah-plan.md).

---

## Candidate comparison (post-AG audit)

| Candidate | Outcome |
|-----------|---------|
| `calculate_SetChecksum_field` | **Selected** — exFAT skip-index rolling checksum; standing #3 after AG |
| `socket_check` | Deferred #2 — socket-list ZF; Stage-5 foothold; blockers unchanged |
| FAT datetime ×4 / thin hashes / P-family | Rejected — low novelty / thin |
| `createMcbEntry` | Deferred — FRS mutation / high blast |
| `memmove` | Deferred Stage-4 — high blast |

---

## Legacy ABI

FASM leaf in `exfat.inc` (retained under `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD=0`):

```text
call / ret
in:  EBP → exFAT
     (internal) ESI = &file_dir_entry
     (internal) ECX = fname_extdir_offset − ESI
out: AX = checksum; [file_dir_entry+2] = AX
preserves: EBX, ECX, EDX, ESI, EDI (push/pop); EBP
```

Critical quirks retained:

* Per-byte: `((c & 1) ? 0x8000 : 0) + (c >> 1) + byte` (16-bit wrap)
* Absolute indices 2 and 3 skipped (SetChecksum field itself)
* Always stores AX to `file_dir_entry+2` after the loop
* FASM do-while hangs if `ECX==0` (callers use `len ≥ 32`); Rust treats
  `len==0` as empty sum (documented; not exercised by live callers)

---

## Rust ABI

```text
stdcall rust_calculate_set_checksum_field(buf, len) -> EAX
ret 8
EAX low 16 = checksum; writes LE u16 to [buf+2]
```

Trampoline: `lea buf=[ebp+file_dir_entry]`; `len=fname_extdir_offset−buf`;
preserves EBX/ECX/EDX/ESI/EDI around stdcall.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `exfat_checksum.rs` + `ffi.rs` section `.text.rust_calculate_set_checksum_field` |
| Extract | `extract_reloc_free_text.py` → `rust_calculate_set_checksum_field.bin` |
| Embed | `kernel/rust/calculate_set_checksum_field.inc` `file` directive |
| Trampoline | `exfat.inc` under `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD` |
| Gate | `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD` (dev 0 → prod 1) |
| Smoke | `calculate_set_checksum_field_rust_smoke_test` in `exfat.inc` (needs struct offsets) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_calculate_set_checksum_field` |
| Blob/object size | 149 bytes |
| Relocations | 0 |
| SHA-256 | `2D8B6EA1E5974639DB1F72A4E7AB26EC035D1C0113DB27E1DD356BBC0EED0DCD` |

Prior Cut AG blob unchanged:
`0CB05C0C90ED4892C1896DD37FE8E690567AC280C0938C99ADB92241DC6C2EC9` (271 B).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | empty/short skip; store side-effect; file+stream; wrap/odd-bit; max 608-byte span |
| NameHash foreshadow | no-skip rolling core matches manual oracle |
| PRNG | 50 000 vectors, seed `0x43555448` (`'CUTH'`) |
| Host tests | **350/350** cargo tests (343 prior AG baseline + 7 AH) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `calculate_set_checksum_field_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | 64-byte file+stream; AX=`[buf+2]`; skip-index invariance; 32-byte file-only; EBX/ECX/EDX/ESI/EDI/EBP canaries; direct `rust_*` match |
| Marker | `rust_calculate_set_checksum_field_smoke_result = 'EXCS'` on success |

---

## QEMU validation

Kernels built with Cuts A–AG production gates intact (`USE_RUST_NTFS_TEST_BOOTSEC=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace.

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD=0` | **OK** (QMP `running` + screendump `dev_build/cut-ah-off.ppm`, 779426 non-black samples) | e1000 + user net present |
| ON | `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD=1` | **OK** (screendump `dev_build/cut-ah-on.ppm`, 779426 non-black samples) | e1000 + user net present |

Smoke (ON): **PASS** (no HLT hang; boot continued).

Real subsystem soak: **NOT AVAILABLE** — stock floppy has no exFAT volume /
dirent-write path that evidences `calculate_SetChecksum_field` beyond the
synthetic ABI smoke. Boot smoke exercises the public symbol on a stand-in
`exFAT` object (ABI path), recorded under ABI smoke above.

Production image: `dev_build/cut-ah-final.img`.

---

## Production gate

```text
USE_RUST_CALCULATE_SET_CHECKSUM_FIELD = 1
```

Rollback: `USE_RUST_CALCULATE_SET_CHECKSUM_FIELD = 0` (or `enabled = false` in `orch/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/exfat_checksum.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-calculate-set-checksum-field.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_calculate_set_checksum_field.bin` (generated)
* `kernel/rust/calculate_set_checksum_field.inc` (new)
* `kernel/fs/exfat.inc` (trampoline + gate + ABI smoke)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml`
* `orch/README.md`
* `orch/src/config.rs` / `main.rs`
* `docs/migration/cut-ah-plan.md`
* `docs/migration/cut-ah-implementation.md`
* `docs/migration/migration-plan.md`
* `.cursor/rules/dev-build.mdc`

---

## Known limitations

* Stock-image exFAT SetChecksum soak not claimed.
* Does not extract/migrate inlined `exFAT_hash_calculate` (NameHash) yet —
  shared Rust core is ready for a future Path A.
* `socket_check` / `memmove` / FAT datetime / `createMcbEntry` remain deferred.
* No Path A cluster claimed — AH is an exFAT foothold leaf only.
* FASM `ECX==0` hang quirk not replicated (callers use `len ≥ 32`).
