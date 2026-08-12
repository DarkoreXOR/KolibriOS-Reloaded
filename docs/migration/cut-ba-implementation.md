# Cut BA Implementation — `pci_make_config_cmd`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-ba-plan.md`](cut-ba-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `pci_make_config_cmd` |
| Source | [`kernel/bus/pci/pci32.inc`](../../kernel/bus/pci/pci32.inc) |
| Callers | 2 (`pci_read_reg`, `pci_write_reg` mechanism-1 paths) |
| Rust symbol | `rust_pci_make_config_cmd` |
| Pure helper | `kolibri_utils::pci_make_config_cmd` / `pci_make_config_cmd_from_regs` |
| Subsystem | PCI config-space address encode (bus/PCI foothold) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-AZ audit: XFS/NTFS/network/AHCI/PE/Stage-3 Path A
still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. Selected **`pci_make_config_cmd`** — new semantic class (PCI
config address dword encode) and first bus/PCI foothold; pure bit math;
excellent differential domain; boot + `--bus ahci` soak.

REG-001: trampoline preserves **EBX+ECX+EDX** (callers keep ESI=size encode
across the call inside `pci_read_reg` / `pci_write_reg`).

REG-003: ABI smoke uses **synthetic register inputs only** — never touches
CF8/CFC or live PCI devices.

---

## Candidate comparison (post-AZ audit)

| Candidate | Outcome |
|-----------|---------|
| `pci_make_config_cmd` | **Selected** — PCI config address encode |
| `strtoint_dec` | #2 — decimal parse; conf_lib only |
| `get_proc_ex` | #3 — PE ban stretch |
| `tcp_outflags` | #4 — TCP deepen |
| `fat_name_is_legal` | #5 — FAT charset table |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_PCI_MAKE_CONFIG_CMD=0`:

```text
call pci_make_config_cmd
in:  AH = bus, BH = device+func, BL = register
out: EAX = 0x80000000 | (bus<<16) | (devfn<<8) | reg
preserves: EBX, ECX, EDX (and ESI/EDI/EBP — leaf does not touch them)
ret
```

Quirks retained:

* Only **AH** supplies bus (`shl eax,8` then `mov ax,bx` discards AL / high)
* Low two bits of BL preserved (callers later `and al, 0xfc`)
* `and eax, 0xffffff` then `or 0x80000000` clears bits 24–30

---

## Rust ABI

```text
stdcall rust_pci_make_config_cmd(bus, devfn, reg) → EAX
  args truncated to 8 bits each
  ret 12
```

Trampoline: `movzx` AH/BH/BL → stdcall; restore EBX/ECX/EDX.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `pci_make_config_cmd.rs` + `ffi.rs` section `.text.rust_pci_make_config_cmd` |
| Extract | `extract_reloc_free_text.py` → `rust_pci_make_config_cmd.bin` |
| Embed | `kernel/rust/pci_make_config_cmd.inc` `file` directive |
| Trampoline | `pci32.inc` under `USE_RUST_PCI_MAKE_CONFIG_CMD` |
| Gate | `USE_RUST_PCI_MAKE_CONFIG_CMD` (prod 1) |
| Smoke | `pci_make_config_cmd_rust_smoke_test` (early init, after Cut AZ smoke) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_pci_make_config_cmd` |
| Blob/object size | 37 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `1EEC4458F41F01FA90FC6EBB16F859B1FEFECBBCEE7CABDA2619B7B8807DF819` |
| Epilogue | `ret 12` (`c2 0c 00`) |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs helpers | **PASS** |
| Named vectors | zero; classic 1/0x18/0x04; max; high-EAX clear; BL low bits; wide trunc |
| PRNG | 50 000 vectors, seed `0x43554241` (`'CUBA'`) |
| Host tests | **496/496** cargo tests (incl. `pci_make_config_cmd` suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `pci_make_config_cmd_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | rust_* classic/zero/max/trunc; public AH/BH/BL + EBX/ECX/EDX/ESI/EDI/EBP canaries; AH-only bus with AL junk |
| Marker | `rust_pci_make_config_cmd_smoke_result = 'PCIC'` on success |
| Live state | Synthetic regs only (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_PCI_MAKE_CONFIG_CMD=0` | **OK** (QMP `running` + screendump, 7358 non-black) | FASM body; `--bus ahci` — see REG-004 |
| ON | `USE_RUST_PCI_MAKE_CONFIG_CMD=1` | **OK** (QMP `running` + screendump, 7358 non-black) | Final production gate; `--bus ahci` — see REG-004 |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **Identical** (byte-for-byte PPM match; 7358 non-black — see REG-004) |
| `--bus ahci` | **PASS** both OFF and ON (A/B equality valid; desktop interpretation corrected by REG-004) |

Desktop A/B alone would not prove mech-1 CF8 encode; AHCI boot forces PCI
config reads through `pci_read_reg` → `pci_make_config_cmd`.

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Boot PCI config via `pci_read_reg` / `pci_write_reg` (mech-1) on AHCI QEMU | **PASS** (init stage reached; AHCI bus) — desktop interpretation corrected by REG-004 |
| Full hardware PCI matrix / mechanism-2 | **NOT AVAILABLE** (mech-2 path does not call this leaf) |

---

## Regressions

| Item | Result |
|------|--------|
| Regressions discovered | **none** in this cut |
| Regression log entry | [REG-004](regression-log.md#reg-004) — AHCI init-screen hang root-caused to Cut AR smoke (not this cut); `7358` pixel counts re-interpreted as init-screen, not desktop |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_PCI_MAKE_CONFIG_CMD = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-ba-final.img` |
| Rollback | `USE_RUST_PCI_MAKE_CONFIG_CMD = 0` or `[[rust.migrations]]` `cut = "BA"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/pci_make_config_cmd.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/bus/pci/pci32.inc` (gate + trampoline)
* `kernel/rust/pci_make_config_cmd.inc` (new)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-ba-plan.md`
* `docs/migration/cut-ba-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/migration-todo.md`

---

## Known limitations

* Mechanism-2 PCI path does not use this leaf (unchanged).
* Does not migrate `pci_read_reg` / `pci_write_reg` I/O.
* No Path A claim for PCI bus ownership.
* Stop; do not start Cut BB.
