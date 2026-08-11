# Cut AX Implementation — `createMcbEntry`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-ax-plan.md`](cut-ax-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `createMcbEntry` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | 5 (NTFS create/resize/attr write paths) |
| Rust symbol | `rust_ntfs_create_mcb_entry` |
| Pure helper | `kolibri_utils::ntfs_create_mcb_entry_ptr` / fixture |
| Subsystem | NTFS MCB (data-run) VLE encode |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AW audit: XFS AK+AM+AP+R+W+AW complementary leaves ≠
ownership; AHCI/network/PE Path A unchanged rejects; address-math siblings
(`exFAT_get_sector`, `getInodeLocation`) lose to the raised bar after AW;
`rebase_coff` remains Y anti-cluster. Selected **`createMcbEntry`** — Cut I
encode twin; permanent #2 through AU–AW now that safer leaves shipped.

REG-001: trampoline preserves **EBX**; EBP→NTFS omit-FP; `cld` only when FRS
slide bit0 set. Legacy sizeWithHeader-before-FRS-full quirk retained.

---

## Candidate comparison (post-AW audit)

| Candidate | Outcome |
|-----------|---------|
| `createMcbEntry` | **Selected** — MCB VLE encode (I inverse) |
| `getInodeLocation` | #2 — EXT inode→LBA; no `--disk ext`; address-math ban |
| `exFAT_get_sector` | Reject — AW address-math sibling |
| `xfs._.get_last_dirblock` | Reject — XFS fatigue / thin over R |
| `rebase_coff` | Defer — Y mutate anti-cluster |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_NTFS_CREATE_MCB_ENTRY=0`:

```text
register call createMcbEntry
in:  [ebp+NTFS.fileDataStart/Size], edi→dest, esi→attr header
out: edi advanced to terminator byte (not past)
may mutate attr sizeWithHeader + FRS (slide)
no uses; clobbers eax/ecx/edx/esi; ebx/ebp untouched
DF→0 iff FRS std/cld slide path; ret 0
```

Quirks retained:

* start width via `shl 1` / `not` (signed zig-zag)
* size width via `shl 1` only
* `add word [sizeWithHeader], 8` **before** FRS space check (early-out leaves bump)
* terminator written at final EDI without advancing past it

---

## Rust ABI

```text
stdcall rust_ntfs_create_mcb_entry(
  start, size, dest, attr, frs, out_dest) → EAX
  EAX bit0 = need_cld (FRS slide ran)
  *out_dest = new EDI (terminator address)
  ret 24
```

Trampoline: omit-FP; extracts NTFS fields from `EBP`; preserves EBX; `cld` when bit0 set.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `ntfs_create_mcb.rs` + `ffi.rs` section `.text.rust_ntfs_create_mcb_entry` |
| Extract | `extract_reloc_free_text.py` → `rust_ntfs_create_mcb_entry.bin` |
| Embed | `kernel/rust/ntfs_create_mcb_entry.inc` `file` directive |
| Trampoline | `ntfs.inc` under `USE_RUST_NTFS_CREATE_MCB_ENTRY` |
| Gate | `USE_RUST_NTFS_CREATE_MCB_ENTRY` (prod 1) |
| Smoke | `ntfs_create_mcb_rust_smoke_test` (after Cut AW smoke) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ntfs_create_mcb_entry` |
| Blob/object size | 656 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `85DABFF1C90A9EBCCEC81A91B91827B8535D0B0CC07E8983DAD1147C5B963E1B` |
| Epilogue | `ret 24` (`c2 18 00`) |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs fixture/ptr | **PASS** |
| Named vectors | no-extend; extend+slide; FRS-full sizeWithHeader quirk; negative start roundtrip vs Cut I decode |
| PRNG | 50 000 vectors, seed `0x43555458` (`'CUTX'`) |
| Host tests | **473/473** cargo tests (incl. ntfs_create_mcb suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ntfs_create_mcb_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C58` hang) |
| Vectors | rust_* no-extend; rust_* extend+slide; public trampoline; FRS-full early-out |
| Marker | `rust_ntfs_create_mcb_smoke_result = 'CMCE'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_NTFS_CREATE_MCB_ENTRY=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_NTFS_CREATE_MCB_ENTRY=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop (IDE) | 779380 | 779380 | **match** |
| `--disk ntfs` | 779380 | 779380 | **match** |

---

## Real subsystem soak

```text
Real subsystem soak: PARTIAL — --disk ntfs attach A/B PASS
```

`python scripts/qmp_desktop_smoke.py --disk ntfs`: QMP `running` both OFF and ON
with identical non-black counts (779380). NTFS mount/browse proves no read-path
regression. **`createMcbEntry` itself is a write/create leaf** — default browse
does not reach it; encode+FRS behavior is covered by host differentials and
boot ABI smoke (marker `CMCE`) on synthetic FRS/attr fixtures. No forced
CreateFile harness in this cut.

---

## Regressions

```text
NONE
```

(No live REG-NNN append for this cut.)

---

## Production gate

```text
USE_RUST_NTFS_CREATE_MCB_ENTRY = 1
```

Rollback: `USE_RUST_NTFS_CREATE_MCB_ENTRY = 0` (or `enabled = false` in
`project/build.toml` for cut AX).

Image: `dev_build/cut-ax-final.img`

---

## Files changed

* `rust_kernel/kolibri_utils/src/ntfs_create_mcb.rs` — encode + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_ntfs_create_mcb_entry` section
* `rust_kernel/kolibri_utils/src/lib.rs` — module export
* `rust_kernel/kolibri_utils/out/rust_ntfs_create_mcb_entry.bin` — extracted blob
* `kernel/rust/ntfs_create_mcb_entry.inc` — embed + ABI smoke
* `kernel/fs/ntfs.inc` — trampoline + gate + FASM rollback body
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-ax-plan.md` / `cut-ax-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* Encode leaf only — NTFS space alloc / FRS ownership / write orchestration remain FASM
* Default `--disk ntfs` browse does not exercise write-path callers
* Legacy sizeWithHeader-before-space-check quirk retained
* No Path A claim for NTFS ownership
* Does not migrate `getInodeLocation` / `exFAT_get_sector` / `rebase_coff`
