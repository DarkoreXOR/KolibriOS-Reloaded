# Cut BX Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bx-implementation.md`](cut-bx-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BX** migrates BDFE date dword → DOS FAT packed date —
> `bdfe_to_fat_date` in `kernel/fs/fat.inc`.  
> Cuts A–BW remain complete. Cut BT/BU/BV/BW are closed — do not modify.
> Do not start Cut BY.

---

## Fresh post-BW migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **78** |
| `[[rust.migrations]]` entries | **78** (Cut A = 4 symbols) |
| `[ ]` pending | **57** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut BT (`ntfsGetTime`) | **closed** — gate `USE_RUST_NTFS_GET_TIME = 1` |
| Cut BU (`fix_coff_symbols`) | **closed** — gate `USE_RUST_FIX_COFF_SYMBOLS = 1` |
| Cut BV (`fsGetTime`) | **closed** — gate `USE_RUST_FS_GET_TIME = 1` |
| Cut BW (`fat_date_to_bdfe`) | **closed** — gate `USE_RUST_FAT_DATE_TO_BDFE = 1` |

Baseline before this cut: **78 / 135**. Target after: **79 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| FAT / exFAT calendar | **No** | AO/BW/BX pack-unpack leaves only — entry orchestration FASM |
| XFS | **No** | BN/BO/time/dir leaves only — no Rust-owned mount |
| NTFS / MCB / FRS | **No** | AF/BT/BU/BV pack leaves only |
| Networking / TCP / IPv4 | **No** | timer/flag leaves ≠ protocol ownership |
| PE / COFF loader | **No** | Y/AT/BK/BU leaves only |
| AHCI | **No** | AV/BG/BM leaves only |
| EXT | **No** | AL/BR/BS leaves; no `--disk ext` |
| Unicode / strings | **No** | AN/BQ closed; uni2ansi inverse ban-listed |
| Calendar / CMOS | **No** | BV/BT/BS/BR leaves; port I/O stays FASM |
| HID / ISO9660 / Stage-3 | **No** | prior rejections unchanged |
| paging / V86 / PCI | **No** | AQ/BL/BA leaves only |

BW completing `fat_date_to_bdfe` does **not** establish calendar subsystem
ownership; `bdfe_to_fat_date` is a distinct write-path pack leaf (BX), not
Path A.

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | **`bdfe_to_fat_date`** | BDFE→DOS **date** pack (write) | 6 (FAT×3 + exFAT×3) | `--disk exfat` | Excellent (BW round-trip + u32 PRNG) | Low | **SELECT** |
| 2 | `bdfe_to_fat_time` | BDFE→DOS time pack (write) | 5 | `--disk exfat` | Good (AO round-trip) | Low | Defer — AO calendar sibling |
| 3 | `uni2ansi_char` | CP866 encode | 11 | multi-FS | Excellent | Low | Defer — AN inverse ban |
| 4 | `fsReadCMOS` | CMOS BCD port read | 12 | CMOS HW | port mock | Med | Defer — calendar port I/O |
| 5 | `ahci_port_wait` | AHCI busy poll | 2 | AHCI boot | Med (timer) | Med | Defer — AV HW orchestration |
| 6 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 7 | `tcp_mss` | TCP MSS clamp | 1 | partial net | Good | Low | Reject — TCP deepen / 1 caller |
| 8 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |

### Why #1 wins

* **Novel leaf vs BW:** Cut BW migrated **date unpack** (read); `bdfe_to_fat_date`
  is the distinct **write-path pack** inverse on `bdfe_to_fat_entry` metadata paths.
* Six live `call bdfe_to_fat_date` sites (FAT×3 + exFAT×3) on metadata write.
* Excellent independent FASM-flow oracle + 50k PRNG (`seed 0x43554258` / `'CUBX'`)
  + exhaustive u16 round-trip via migrated BW.
* Same `--disk exfat` attach-only soak class as BW (boot FAT + attached exFAT).
* Pure register leaf — reloc-free; legacy body preserves EDX via push/pop;
  trampoline preserves ECX+EDX (REG-001).

### Why alternatives lose

* `bdfe_to_fat_time`: AO calendar sibling; write-path time pack — valid next cut
  but ranked below BX after BW closed the date unpack pair.
* `uni2ansi_char`: highest caller count but unicode cluster deepen; AN inverse ban.
* `fsReadCMOS`: port I/O leaf; BV keeps orchestration in Rust while port stays FASM.
* `ahci_port_wait`: `timer_ticks` + MMIO poll; AV orchestration deepen.
* `ntfs_restore_usa_frs`: one `mov` + fallthrough to migrated Cut J.
* `tcp_mss` / export-only strings: thin / zero kernel callers.

### ABI target

Legacy ABI:

```text
regcall bdfe_to_fat_date()
  in:  EAX = BDFE date dword (year<<16 | month<<8 | day)
  out: EAX = FAT packed date (callers store AX only)
  body push/pop EDX internally
preserves: EBX, ESI, EDI, EBP, ECX typical across callers
trampoline must preserve ECX+EDX (REG-001)
```

Rust ABI:

```text
stdcall rust_bdfe_to_fat_date(bdfe_date) -> EAX; ret 4
trampoline push/pop ECX+EDX
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554258` (`'CUBX'`).
* Exhaustive u16 round-trip: `bdfe_to_fat_date(fat_date_to_bdfe(d)) == d`.
* ABI smoke: direct `rust_*` + public `bdfe_to_fat_date`; marker `B2FD`.
* QEMU OFF/ON desktop smoke; A/B framebuffer compare.
* Real subsystem soak: `--disk exfat` attach-only (entry metadata write path).
