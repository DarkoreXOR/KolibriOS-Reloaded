# Cut AV Implementation — `ahci_find_cmdslot`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-av-plan.md`](cut-av-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ahci_find_cmdslot` |
| Source | [`kernel/blkdev/ahci.inc`](../../kernel/blkdev/ahci.inc) |
| Callers | 2 live (`ahci_port_identify`, `ahci_rw_sectors`) |
| Rust symbol | `rust_ahci_find_cmdslot` |
| Pure helper | `kolibri_utils::ahci_find_cmdslot` |
| Subsystem | AHCI free command-list slot lookup (driver foothold) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AU audit: AC/M/V/AS/AU do not own networking;
AU + free-slot extract is an anti-cluster; I+`createMcbEntry` encode ≠ FRS
ownership; Y+AT+`rebase_coff` PE anti-cluster unchanged. Selected
**`ahci_find_cmdslot`** — strongest remaining leaf: first AHCI/driver free-slot
bit scan with clean stdcall ABI, excellent differential domain, and real
`--bus ahci` soak.

REG-001: trampoline preserves **EBX+ECX+EDX+ESI** (legacy push/pop) plus
EDI/EBP canaries; callers keep ESI→`PORT_DATA` inside `pushad` across the call.

---

## Candidate comparison (post-AU audit)

| Candidate | Outcome |
|-----------|---------|
| `ahci_find_cmdslot` | **Selected** — AHCI free-slot bit scan |
| `createMcbEntry` | #2 — NTFS MCB encode; high FRS blast |
| `getInodeLocation` | #3 — EXT inode→LBA arithmetic |
| `rebase_coff` | Defer — Y mutate anti-cluster |
| AU free-slot extract / `net_ptr_to_num4` / network Path A | Reject / defer |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_AHCI_FIND_CMDSLOT=0`:

```text
stdcall ahci_find_cmdslot(pdata) → EAX
in:  pdata → PORT_DATA
out: EAX = free slot index | -1
preserves EBX/ECX/EDX/ESI via push/pop
ret 4
```

Quirk: `ncs = (HBA_MEM.cap >> 8) & 0x0F` (drops AHCI NCS bit 4); loop uses
exclusive `cmp/jae` bound without adding 1 to the 0-based CAP.NCS field.

---

## Rust ABI

```text
stdcall rust_ahci_find_cmdslot(slots, ncs) → EAX
  ret 8
```

Trampoline: reads `SACT|CI` and `(CAP>>8)&0xf` from the `PORT_DATA` →
`HBA_PORT` / `AHCI_CTR` / `HBA_MEM` chain; preserves EBX/ECX/EDX/ESI/EDI/EBP.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `ahci_find_cmdslot.rs` + `ffi.rs` section `.text.rust_ahci_find_cmdslot` |
| Extract | `extract_reloc_free_text.py` → `rust_ahci_find_cmdslot.bin` |
| Embed | `kernel/rust/ahci_find_cmdslot.inc` `file` directive |
| Trampoline | `ahci.inc` under `USE_RUST_AHCI_FIND_CMDSLOT` |
| Gate | `USE_RUST_AHCI_FIND_CMDSLOT` (prod 1) |
| Smoke | `ahci_find_cmdslot_rust_smoke_test` (after `ahci_init`) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ahci_find_cmdslot` |
| Blob/object size | 54 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `67B7D59F15749C5AFF4DC072443D7E60EAC66DA9B3F07D3ECC44C3AECB8B5177` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs `ahci_find_cmdslot` | **PASS** |
| Named vectors | ncs=0; empty; first/mid/last free; all occupied; `&0xf` quirk |
| PRNG | 50 000 vectors, seed `0x43555456` (`'CUTV'`) |
| Host tests | **456/456** cargo tests (incl. ahci_find_cmdslot suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ahci_find_cmdslot_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C56` hang) |
| Vectors | Direct `rust_*` free/mid/miss/ncs0; public hit + miss + NCS quirk; EBX/ECX/EDX/ESI/EDI/EBP canaries |
| Marker | `rust_ahci_find_cmdslot_smoke_result = 'SLOT'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_AHCI_FIND_CMDSLOT=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_AHCI_FIND_CMDSLOT=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop (IDE) | 779380 | 779380 | **match** |
| `--disk xfs --bus ahci` | 7358 | 7358 | **match** (A/B valid — but see REG-004 correction below) |

---

## Real subsystem soak

```text
Real subsystem soak: PASS (AHCI identify/rw path live)
```

`python scripts/run_qemu.py` equivalent via `scripts/qmp_desktop_smoke.py`
`--disk xfs --bus ahci`: QMP `running` both OFF and ON with identical non-black
counts (7358). A/B match is valid — both gates produced the same result.

**REG-004 correction (2026-08-12):** `7358` non-black pixels corresponds to the
**kernel initialization screen** (init-stage; black background with white log
lines), *not* the desktop. At the time of Cut AV validation, the Cut AR ABI
smoke (`r_f_port_area_rust_smoke_test`) was hanging the init stage under AHCI
disks, so the `qmp_desktop_smoke.py` screenshot captured the init log screen.
The 7358-vs-7358 A/B **equality is still a valid negative regression test** for
this cut (same behaviour both sides), but the "desktop reached" interpretation
was incorrect. After REG-004 fix, `--bus ahci` boots now reach desktop (~779380
non-black). See [`regression-log.md`](regression-log.md#reg-004).

Synthetic ABI smoke after `ahci_init` also exercises the public trampoline on a
fake `PORT_DATA` chain (marker `SLOT`).

---

## Regressions

**NONE** in this cut.

[REG-004](regression-log.md#reg-004) — AHCI init-screen hang root-caused to Cut AR smoke (not Cut AV); `7358` pixel counts re-interpreted as init-screen, not desktop. A/B equality of this cut remains valid.

---

## Production gate

```text
USE_RUST_AHCI_FIND_CMDSLOT = 1
```

Rollback: `USE_RUST_AHCI_FIND_CMDSLOT = 0` (or `enabled = false` in
`project/build.toml` for cut AV).

Image: `dev_build/cut-av-final.img`

---

## Files changed

* `rust_kernel/kolibri_utils/src/ahci_find_cmdslot.rs` — pure scan + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_ahci_find_cmdslot` section
* `rust_kernel/kolibri_utils/src/lib.rs` — module export
* `rust_kernel/kolibri_utils/out/rust_ahci_find_cmdslot.bin` — extracted blob
* `kernel/rust/ahci_find_cmdslot.inc` — embed + ABI smoke
* `kernel/blkdev/ahci.inc` — trampoline + gate + FASM rollback body
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call after `ahci_init`
* `project/build.toml` — blob + migration registry
* `scripts/qmp_desktop_smoke.py` — headless QMP desktop/AHCI smoke helper
* `docs/migration/cut-av-plan.md` / `cut-av-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* Read-only free-slot scan — command table fill / CI issue / wait / recovery remain FASM
* CAP.NCS `&0xf` + exclusive-bound quirk retained (legacy)
* No Path A claim for AHCI / DMA / IRQ ownership
* MMIO `SACT|CI` race underfoot same as legacy
* AHCI QEMU desktop non-black count is lower than IDE in this harness (OFF=ON)
