# Cut AO Implementation — `fat_time_to_bdfe`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-ao-plan.md`](cut-ao-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `fat_time_to_bdfe` |
| Source | [`kernel/fs/fat.inc`](../../kernel/fs/fat.inc) |
| Callers | ~22 sites (`fat_entry_to_bdfe2`, `exFAT_entry_to_bdfe2`, `exFAT_bdfe_to_fat_entry` paths) |
| Rust symbol | `rust_fat_time_to_bdfe` |
| Pure helper | `kolibri_utils::fat_time_to_bdfe` |
| Subsystem | FAT/exFAT DOS packed-time → BDFE unpack |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. FAT datetime pack/unpack siblings, Unicode
encode/decode, ISO AJ+AN, XFS W+AM+AK, EXT AL+write, AH+AI, H+`blit_clip`,
and socket membership do not meet the raised Path A bar. See
[`cut-ao-plan.md`](cut-ao-plan.md).

REG-001 lesson applied: trampoline preserves **ECX+EDX** (legacy body
`push`/`pop`; entry→BDFE callers keep ESI/EDI across the call).

---

## Candidate comparison (post-AN audit)

| Candidate | Outcome |
|-----------|---------|
| `fat_time_to_bdfe` | **Selected** — DOS pack layout; `--disk exfat` soak; REG-001 trampoline focus |
| `window._.set_window_clientbox` | #2 — GUI policy; desktop-only |
| `blit_clip` | #3 — H composition; desktop-only |
| `coff_get_align` / EXT write / CD / sockets / memmove / unicode inverse | Reject / defer |

---

## Legacy ABI

FASM leaf in `fat.inc` (retained under `USE_RUST_FAT_TIME_TO_BDFE=0`):

```text
call/ret (not stdcall)
in:  EAX = FAT packed time (callers usually movzx word)
out: EAX = BDFE time dword (hours high word; AH=minutes; AL=seconds×2)
preserves: ECX, EDX (push/pop); EBX/ESI/EDI/EBP untouched
```

Critical quirks retained:

* Full 32-bit input (not truncated to u16) — high bits affect `shr eax,11`
* Seconds field ×2 via `add edx,edx` (even seconds only)
* No hour/min/sec calendar clamping on malformed fields

---

## Rust ABI

```text
stdcall rust_fat_time_to_bdfe(fat_time) -> EAX
  EAX = BDFE time dword
  ret 4
```

Trampoline: `push ecx` / `push edx` / `stdcall rust_fat_time_to_bdfe, eax` /
`pop edx` / `pop ecx` / `ret` (REG-001 / Cut D class).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `time.rs` `fat_time_to_bdfe` + `ffi.rs` section `.text.rust_fat_time_to_bdfe` |
| Extract | `extract_reloc_free_text.py` → `rust_fat_time_to_bdfe.bin` |
| Embed | `kernel/rust/fat_time_to_bdfe.inc` `file` directive |
| Trampoline | `fat.inc` under `USE_RUST_FAT_TIME_TO_BDFE` |
| Gate | `USE_RUST_FAT_TIME_TO_BDFE` (dev 0 → prod 1) |
| Smoke | `fat_time_to_bdfe_rust_smoke_test` |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_fat_time_to_bdfe` |
| Blob/object size | 34 bytes |
| Relocations | 0 |
| SHA-256 | `1FCE905F98482DAB61B633D3B78FD940A60C23D46B8A8DD498F6B43997025B60` |

Trailing instruction is `ret 4` (`C2 04 00`). Reloc-free verified by extractor
(extraction fails if the section has relocations).

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | midnight; 00:00:02; 00:01:00; 01:00:00; 23:59:58; high-bit u32 |
| Exhaustive | all `0..=0xFFFF` |
| Pack round-trip | legal h/m/s2 domain via independent `bdfe_to_fat_time` oracle |
| PRNG | 50 000 cases, seed `0x4355544F` (`'CUTO'`) |
| Host tests | **405/405** cargo tests (401 AN baseline + 4 new fat_time) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `fat_time_to_bdfe_rust_smoke_test` | **PASS** (boot reached desktop; no HLT hang) |
| Vectors | midnight; 00:00:02; 00:01:00; 01:00:00; 23:59:58; high junk; direct `rust_*`; ECX loop×8 |
| Canaries | ECX=`0xC0C00001`, EDX=`0xD0D00002` across public call (REG-001) |
| Marker | `rust_fat_time_to_bdfe_smoke_result = 'FTBD'` on success |

---

## QEMU validation

Kernels built with Cuts A–AN production gates intact (`USE_RUST_ANSI2UNI_CHAR=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (`dev_build/test/`).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_FAT_TIME_TO_BDFE=0` | **OK** (QMP `running` + screendump `dev_build/cut-ao-off.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |
| ON | `USE_RUST_FAT_TIME_TO_BDFE=1` | **OK** (screendump `dev_build/cut-ao-on.ppm`, 779380 non-black samples) | Not attached in current `qemu.args` |

Smoke (ON): **PASS** (no HLT hang; boot continued).

### A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop smoke | 779380 non-black | 779380 non-black | Match |
| `--disk exfat` boot+desktop | 779380 non-black | 779380 non-black | Match |

### Real subsystem soak

`--disk exfat` A/B: **PASS** (QMP `running`, identical non-black counts with
exFAT HD attached). Boot ABI smoke covers DOS packed-time vectors used by
FAT/exFAT `*_entry_to_bdfe2` paths. Default `[testdisk]` also attaches exFAT
on normal boots.

Scripted Eolite exFAT directory browse / timestamp-field harness:
**NOT AVAILABLE** (attach + boot smoke only; same class as prior FS cuts without
scripted browse).

Production image: `dev_build/cut-ao-final.img`.

e1000: **N/A**

---

## Regressions discovered

**NONE** during Cut AO validation.

---

## Production gate

```text
USE_RUST_FAT_TIME_TO_BDFE = 1
```

Rollback: `USE_RUST_FAT_TIME_TO_BDFE = 0` (or `enabled = false` in
`project/build.toml` Cut AO migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/time.rs` — `fat_time_to_bdfe` + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_fat_time_to_bdfe`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/fs/fat.inc` — trampoline + gate + FASM rollback body
* `kernel/rust/fat_time_to_bdfe.inc` — blob embed + ABI smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-ao-plan.md` / `cut-ao-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No scripted Eolite browse / timestamp-field soak (attach-only A/B).
* `fat_date_to_bdfe` / `bdfe_to_fat_time` / `bdfe_to_fat_date` remain FASM;
  not claimed as Path A pair.
* Entry→BDFE orchestration remains FASM.
* No Path A claim.
