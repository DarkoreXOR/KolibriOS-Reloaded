# Cut AM Implementation — `xfs._.get_before_by_hashval`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-am-plan.md`](cut-am-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs._.get_before_by_hashval` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 2 (`xfs._.lookup_node`, `xfs._.lookup_btree`) |
| Rust symbol | `rust_xfs_get_before_by_hashval` |
| Pure helper | `kolibri_utils::xfs_get_before_by_hashval` |
| Subsystem | XFS / DA interior-node first-match-by-hash |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. EXT AL foothold does not make EXT write/all-times a
subsystem. XFS v4 `conv_time_to_kos_epoch` remains a thin anti-cluster wrapper.
ISO `cd_compare_name` is already AJ-routed when ON. Cut W leaf binary search +
this node linear search are complementary Path B leaves — XFS orchestration
state remains FASM-owned. Socket membership blockers unchanged. See
[`cut-am-plan.md`](cut-am-plan.md).

---

## Candidate comparison (post-AL audit)

| Candidate | Outcome |
|-----------|---------|
| `xfs._.get_before_by_hashval` | **Selected** — DA node first-match; BE + v4/v5; EBX quirk; EAX+ZF |
| `ansi2uni_char` | #2 — CP866 decode; deferred table-reloc / unicode anti-cluster |
| `blit_clip` | #3 — glue around Cut H |
| `fat_name_is_legal` / EXT write / thin v4 / CD / sockets / memmove | Reject / defer |

---

## Legacy ABI

FASM leaf in `xfs.asm` (retained under `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL=0`):

```text
stdcall (_base, _count, _hash) / retn 12
in:  EBX = intnode base (LIVE — stdcall _base is unused/dead)
     _count = host-endian entry count (callers xchg BE count)
     _hash = search key
     EBP → XFS; reads [ebp+XFS.version] (5 → v5 layout)
out: EAX = BE `before` on hit, or ERROR_FILE_NOT_FOUND (5) on miss
     ZF = 1 found / 0 miss (cmp esp,esp / test esp,esp)
preserves: EBX, EDX, ESI, EDI (`uses`); ECX clobbered
```

Critical quirks retained:

* **EBX=node, not `_base`** — callers pass `lea eax,[ebx+sizeof.header]` as
  `_base`, but the body indexes from EBX via `xfs_da_intnode.btree` /
  `xfs_da3_intnode.btree`
* Unsigned `jae` — first `hashval >= target` wins (not exact-only)
* v4 btree offset 16 / v5 btree offset 64
* `count == 0` miss **hangs** in legacy FASM (documented); Rust returns ERROR

---

## Rust ABI

```text
stdcall rust_xfs_get_before_by_hashval(entries, count, hash) -> EDX:EAX
  EDX:EAX = (zf << 32) | result
  ret 12
```

Trampoline: omit-FP; `entries = EBX + (version==5 ? 64 : 16)`; `cmp edx,1` /
flag-neutral `pop eax`; `retn 12`.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `xfs_node_hash.rs` + `ffi.rs` section `.text.rust_xfs_get_before_by_hashval` |
| Extract | `extract_reloc_free_text.py` → `rust_xfs_get_before_by_hashval.bin` |
| Embed | `kernel/rust/xfs_get_before_by_hashval.inc` `file` directive |
| Trampoline | `xfs.asm` under `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL` |
| Gate | `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL` (dev 0 → prod 1) |
| Smoke | `xfs_get_before_by_hashval_rust_smoke_test` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_xfs_get_before_by_hashval` |
| Blob/object size | 71 bytes |
| Relocations | 0 |
| SHA-256 | `8840863BA8B8C70D23BAE13FB28D43F7C1035D7603C148484FF364F4035648FE` |

Trailing instruction is `ret 12` (`C2 0C 00`). Reloc-free verified by extractor
(extraction fails if the section has relocations).

### Live-XFS A/B (2026-08-11) — Cut AM cleared

**Reported symptom after AM:** directories shown as files; all sizes 0 bytes.

**A/B result:**
- Full AM rollback (gate OFF + include/smoke removed): **same symptom**
- `dev_build/cut-al-final.img`: **same symptom**
- `dev_build/cut-ak-final.img`: **worse** — blank names (pre-ECX-preserve unicode path)

**Conclusion:** not a Cut AM regression. Root shortform `readdir` never calls
`get_before_by_hashval`; mode/size come from unchanged `xfs_get_inode_info`.
Attrs/sizes bug is pre-existing (or never soak-verified). Cut AM remains
production ON.

**Later root cause (same symptom):** REG-001 in
[`regression-log.md`](regression-log.md) — Rust unicode `stdcall` clobbered
EDX across `xfs._.copy_filename` (BDFE base). Fixed in `xfs.asm` (`uses eax
edx` + `.` entry `cur_inode_save`). Volume-label junk after that fix:
REG-002.

**Trampoline:** FASM-identical — `entries = EBX + (version==5 ? 64 : 16)`;
stdcall `_base` ignored (matches legacy body / `lookup_btree .next_level`).


---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | v4/v5 layouts; jae first-match; hit first/mid/last; miss above; duplicates; BE max |
| PRNG | 50 000 tables × present+miss, seed `0x4355544D` (`'CUTM'`) |
| Host tests | **399/399** cargo tests (389 AL baseline + 10 new node-hash) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_get_before_by_hashval_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | mid hit; jae first-match; first/last; miss; v5 hit/miss; EBX/ESI/EDI/EDX canaries |
| Marker | `rust_xfs_get_before_by_hashval_smoke_result = 'XFBF'` on success |
| Note | Smoke omits `count==0` (legacy FASM hang); host tests cover Rust safe miss |

---

## QEMU validation

Kernels built with Cuts A–AL production gates intact (`USE_RUST_EXT_READ_TIME=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL=0` | **OK** (QMP `running` + screendump `dev_build/cut-am-off.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL=1` | **OK** (screendump `dev_build/cut-am-on.ppm`, 779426 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

Real subsystem soak: **NOT AVAILABLE** — no scripted XFS DA node-walk harness;
attaching `images/exfat-image.img` does not evidence XFS hash lookup.

Production image: `dev_build/cut-am-final.img`.

e1000: **N/A**

---

## Production gate

```text
USE_RUST_XFS_GET_BEFORE_BY_HASHVAL = 1
```

Rollback: `USE_RUST_XFS_GET_BEFORE_BY_HASHVAL = 0` (or `enabled = false` in
`project/build.toml` Cut AM migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/xfs_node_hash.rs` (new — algorithm + oracle + tests)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/out/rust_xfs_get_before_by_hashval.bin` (generated)
* `kernel/rust/xfs_get_before_by_hashval.inc` (new)
* `kernel/fs/xfs.asm` (trampoline + gate; legacy body retained)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-am-plan.md`
* `docs/migration/cut-am-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* No XFS DA node-walk / directory-lookup soak on stock images
* Legacy FASM `count == 0` miss hang remains documented; Rust returns ERROR
* `xfs_hashname` remains FASM
* No Path A claim for XFS hash helpers
