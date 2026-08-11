# Cut AP Implementation — `xfs_hashname`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-ap-plan.md`](cut-ap-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs_hashname` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 4 (`lookup_block`, `lookup_leaf`, `lookup_node`, `lookup_btree`) |
| Rust symbol | `rust_xfs_hashname` |
| Pure helper | `kolibri_utils::xfs_hashname` |
| Subsystem | XFS directory name hash (ROL 7 ⊕ byte) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. FAT datetime siblings of AO, Unicode encode/decode,
ISO AJ+AN, XFS W+AM+hashname-as-subsystem, EXT AL+write, AH+AI,
H+`blit_clip`, and socket membership do not meet the raised Path A bar. See
[`cut-ap-plan.md`](cut-ap-plan.md).

REG-001 lesson applied: trampoline preserves **ECX+EDX+ESI** (legacy `uses ecx
esi`; EDX left untouched by FASM body). Callers keep **EBX** live across the
call (dirblock / inode) — stdcall callee-saved; smoke asserts EBX canaries.

---

## Candidate comparison (post-AO audit)

| Candidate | Outcome |
|-----------|---------|
| `xfs_hashname` | **Selected** — ROL7 name hash; `--disk xfs` soak; feeds W/AM |
| `window._.set_window_clientbox` | #2 — GUI policy; desktop-only |
| `blit_clip` | #3 — H composition; desktop-only |
| `coff_get_align` / FAT date siblings / uni2ansi / EXT write / sockets / memmove | Reject / defer |

---

## Legacy ABI

FASM leaf in `xfs.asm` (retained under `USE_RUST_XFS_HASHNAME=0`):

```text
stdcall (_name, _len) / retn 8
in:  _name → byte string; _len = byte count
out: EAX = hash (ROL 7 ⊕ each byte into AL)
preserves: ECX, ESI (`uses`); EDX untouched by body; EBX/EDI/EBP untouched
```

Critical quirks retained:

* `rol eax,7` then `xor al,[esi]` (AL-only XOR)
* `len == 0` do-while hangs in legacy FASM (documented); Rust returns 0
  without reading (same quirk class as Cut AI NameHash)

---

## Rust ABI

```text
stdcall rust_xfs_hashname(name, len) -> EAX
  EAX = hash dword
  ret 8
```

Trampoline (omit-FP leaf shape): `push ecx/edx/esi` / load args from stack /
`call rust_xfs_hashname` / `pop` / `retn 8` (REG-001 / Cut D class).

Note: an earlier FASM `proc … uses` nested-`stdcall` trampoline failed ABI
smoke (black screen / `jmp $`). Manual stack trampoline matches Cut AO/W style.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `xfs_hashname.rs` + `ffi.rs` section `.text.rust_xfs_hashname` |
| Extract | `extract_reloc_free_text.py` → `rust_xfs_hashname.bin` |
| Embed | `kernel/rust/xfs_hashname.inc` `file` directive |
| Trampoline | `xfs.asm` under `USE_RUST_XFS_HASHNAME` |
| Gate | `USE_RUST_XFS_HASHNAME` (dev 0 → prod 1) |
| Smoke | `xfs_hashname_rust_smoke_test` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_xfs_hashname` |
| Blob/object size | 137 bytes |
| Relocations | 0 |
| SHA-256 | `DE71309867AB109405E25301D41971BD7E1CE1F2CE41B432F61738CCCE7CBC9C` |

Trailing instruction is `ret 8` (`C2 08 00`). Reloc-free verified by extractor
(extraction fails if the section has relocations).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | empty; `'a'`; `"ab"`; `README.TXT`; high bytes |
| Exhaustive | all single-byte; all two-byte pairs |
| PRNG | 50 000 cases, seed `0x43555450` (`'CUTP'`) |
| Host tests | **411/411** cargo tests (405 AO baseline + 6 new xfs_hashname) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_hashname_rust_smoke_test` | **PASS** (boot reached desktop; no fail hang) |
| Vectors | `'a'`; `"ab"`; `README.TXT`; high bytes; empty via `rust_*`; direct `rust_*`; ECX loop×8 |
| Canaries | ECX=`0xC0C00001`, EDX=`0xD0D00002`, EBX/ESI/EDI across public call (REG-001) |
| Marker | `rust_xfs_hashname_smoke_result = 'XHSH'` on success |

Empty-len smoke calls **`rust_xfs_hashname` directly** (public FASM body hangs).

---

## QEMU validation

Kernels built with Cuts A–AO production gates intact (`USE_RUST_FAT_TIME_TO_BDFE=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_XFS_HASHNAME=0` | **OK** (QMP `running` + screendump `dev_build/cut-ap-off.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_XFS_HASHNAME=1` | **OK** (screendump `dev_build/cut-ap-on.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no fail hang; boot continued).

### A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop smoke | 779380 non-black | 779380 non-black | Match |
| `--disk xfs` boot+desktop | 779380 non-black | 779380 non-black | Match |

### Real subsystem soak

`--disk xfs` A/B: **PASS** (QMP `running`, identical non-black counts with
XFS HD attached). Boot ABI smoke covers ROL7 hash vectors used by XFS
`lookup_*` paths. Hash feeds Cuts W/AM consumers on non-shortform lookups.

Scripted Eolite XFS directory browse / path-lookup harness:
**NOT AVAILABLE** (attach + boot smoke only; same class as prior FS cuts without
scripted browse).

Production image: `dev_build/cut-ap-final.img`.

e1000: **N/A**

---

## Regressions discovered

**NONE** during Cut AP validation.

(Trampoline shape fixed before production enablement — nested FASM `proc`
forwarder failed ABI smoke; not a live FS regression.)

---

## Production gate

```text
USE_RUST_XFS_HASHNAME = 1
```

Rollback: `USE_RUST_XFS_HASHNAME = 0` (or `enabled = false` in
`project/build.toml` Cut AP migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/xfs_hashname.rs` — hash + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_xfs_hashname`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/fs/xfs.asm` — trampoline + gate + FASM rollback body
* `kernel/rust/xfs_hashname.inc` — blob embed + ABI smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-ap-plan.md` / `cut-ap-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No scripted Eolite browse / path-lookup soak (attach-only A/B).
* Cuts W/AM remain separate Path B leaves; no Path A XFS ownership claim.
* `len == 0` hang avoided only on Rust path (documented quirk).
* No Path A claim.
