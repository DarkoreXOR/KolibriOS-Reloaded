# Cut BZ Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bz-implementation.md`](cut-bz-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BZ** migrates Unicode → CP866 encode —
> `uni2ansi_char` in `kernel/fs/parse_fn.inc`.  
> Cuts A–BY remain complete. Cut AO/BW/BX/BY are closed — do not modify.
> Do not start Cut CA.

---

## Fresh post-BY migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **80** |
| `[[rust.migrations]]` entries | **80** (Cut A = 4 symbols) |
| `[ ]` pending | **55** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut BY (`bdfe_to_fat_time`) | **closed** — gate `USE_RUST_BDFE_TO_FAT_TIME = 1` |
| FAT calendar quartet | **complete** (AO/BW/BX/BY) — no auto-continuation |

Baseline before this cut: **80 / 135**. Target after: **81 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| FAT / exFAT calendar | **No** | AO/BW/BX/BY quartet complete; entry orchestration FASM |
| Unicode encode+decode | **No** | Cut A encode + AN decode already shipped; pairing ≠ Rust-owned subsystem |
| XFS / NTFS / EXT | **No** | prior leaf-only rejections unchanged |
| Networking / TCP / IPv4 | **No** | timer/flag leaves ≠ protocol ownership |
| PE / COFF / AHCI / HID | **No** | prior leaf-only rejections unchanged |
| Calendar / CMOS | **No** | BV port I/O stays FASM |
| paging / V86 / PCI | **No** | AQ/BL/BA leaves only |

Completing the FAT calendar quartet triggers re-evaluation of FAT siblings
(`fat_get_sector` ban-listed) — **not** selected. Unicode `uni2ansi_char` is
the distinct public encode leaf with highest live caller fanout among
remaining Path B candidates.

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | **`uni2ansi_char`** | Unicode→CP866 encode (public leaf) | 11 (7 files) | `--disk iso9660` + `--disk exfat` | Excellent (FASM oracle + exhaustive u16 + AN round-trip) | Low | **SELECT** |
| 2 | `fsReadCMOS` | CMOS BCD port read | 12 | CMOS HW | port mock | Med | Defer — calendar port I/O |
| 3 | `ahci_port_wait` | AHCI busy poll | 2 | AHCI boot | Med (timer) | Med | Defer — AV HW orchestration |
| 4 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 5 | `tcp_mss` | TCP MSS clamp | 1 | partial net | Good | Low | Reject — TCP deepen / 1 caller |
| 6 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |
| 7 | `net_ptr_to_num` | AY wrapper | ~12 | partial net | Good | Low | Reject — thin wrapper |

### Why #1 wins

* **Distinct public symbol:** Cut A migrated `unicode.cp866.encode` (export
  path); **`uni2ansi_char`** remains the direct in-kernel leaf called from
  FAT/exFAT/NTFS/ISO9660/fs_lfn/taskman name-conversion loops.
* Eleven live `call uni2ansi_char` sites across seven files — highest fanout
  among remaining Path B leaves after BY.
* Excellent independent FASM-flow oracle + exhaustive u16 domain + 50k PRNG
  (`seed 0x4355545A` / `'CUTZ'`) + supplementary AN decode round-trip.
* `--disk iso9660` (4 caller sites) + `--disk exfat` attach-only soak.
* Pure register leaf — reloc-free; trampoline preserves ECX+EDX (REG-001).

### Why alternatives lose

* `fsReadCMOS`: CMOS port I/O; calendar cluster deepen after BV/BT/BS/BR.
* `ahci_port_wait`: AV Path A reject — controller wait orchestration.
* `ntfs_restore_usa_frs`: thin J sibling wrapper.
* `tcp_mss`: single caller TCP deepen.
* Export-only strings / `net_ptr_to_num`: zero novelty / thin wrapper.

### Why not automatic FAT calendar continuation

FAT calendar quartet (AO/BW/BX/BY) is **complete**. Remaining FAT items
(`fat_get_sector`) are AW address-math ban-listed siblings — rejected.

### Legacy ABI (expected)

```text
call/ret (not stdcall)
in:  AX = Unicode code unit
out: AL = CP866 byte (callers test/store AL)
preserves: ECX (ISO/NTFS loop counters), EDX (REG-001)
```

### Rust ABI (expected)

```text
stdcall rust_uni2ansi_char(cp) -> EAX (AL = CP866); ret 4
trampoline: push ecx / push edx / stdcall / pop edx / pop ecx / ret
```

### Production gate

`USE_RUST_UNI2ANSI_CHAR = 1` in `kernel/fs/parse_fn.inc`.
