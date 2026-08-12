# Cut BY Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-by-implementation.md`](cut-by-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BY** migrates BDFE time dword → DOS FAT packed time —
> `bdfe_to_fat_time` in `kernel/fs/fat.inc`.  
> Cuts A–BX remain complete. Cut BT/BU/BV/BW/BX are closed — do not modify.
> Do not start Cut BZ.

---

## Fresh post-BX migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **79** |
| `[[rust.migrations]]` entries | **79** (Cut A = 4 symbols) |
| `[ ]` pending | **56** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut BT (`ntfsGetTime`) | **closed** — gate `USE_RUST_NTFS_GET_TIME = 1` |
| Cut BU (`fix_coff_symbols`) | **closed** — gate `USE_RUST_FIX_COFF_SYMBOLS = 1` |
| Cut BV (`fsGetTime`) | **closed** — gate `USE_RUST_FS_GET_TIME = 1` |
| Cut BW (`fat_date_to_bdfe`) | **closed** — gate `USE_RUST_FAT_DATE_TO_BDFE = 1` |
| Cut BX (`bdfe_to_fat_date`) | **closed** — gate `USE_RUST_BDFE_TO_FAT_DATE = 1` |

Baseline before this cut: **79 / 135**. Target after: **80 / 135**.

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

BX completing `bdfe_to_fat_date` does **not** establish calendar subsystem
ownership; `bdfe_to_fat_time` is a distinct write-path time pack leaf (BY), not
Path A.

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | **`bdfe_to_fat_time`** | BDFE→DOS **time** pack (write) | 5 (FAT×2 + exFAT×3) | `--disk exfat` | Excellent (AO round-trip + u32 PRNG) | Low | **SELECT** |
| 2 | `uni2ansi_char` | CP866 encode | 11 | multi-FS | Excellent | Low | Defer — AN inverse ban |
| 3 | `fsReadCMOS` | CMOS BCD port read | 12 | CMOS HW | port mock | Med | Defer — calendar port I/O |
| 4 | `ahci_port_wait` | AHCI busy poll | 2 | AHCI boot | Med (timer) | Med | Defer — AV HW orchestration |
| 5 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 6 | `tcp_mss` | TCP MSS clamp | 1 | partial net | Good | Low | Reject — TCP deepen / 1 caller |
| 7 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |

### Why #1 wins

* **Novel leaf vs AO:** Cut AO migrated **time unpack** (read); `bdfe_to_fat_time`
  is the distinct **write-path pack** inverse on `bdfe_to_fat_entry` metadata paths.
* Five live `call bdfe_to_fat_time` sites (FAT×2 + exFAT×3) on metadata write.
* Excellent independent FASM-flow oracle + 50k PRNG (`seed 0x43554259` / `'CUBY'`)
  + exhaustive u16 round-trip via migrated AO `fat_time_to_bdfe`.
* Same `--disk exfat` attach-only soak class as BX (boot FAT + attached exFAT).
* Pure register leaf — reloc-free; legacy body preserves EDX via push/pop;
  trampoline preserves ECX+EDX (REG-001).

### Why alternatives lose

* `uni2ansi_char`: highest caller count but unicode cluster deepen; AN inverse ban.
* `fsReadCMOS`: port I/O leaf; BV keeps orchestration in Rust while port stays FASM.
* `ahci_port_wait`: `timer_ticks` + MMIO poll; AV orchestration deepen.
* `ntfs_restore_usa_frs`: one `mov` + fallthrough to migrated Cut J.
* `tcp_mss` / export-only strings: thin / zero kernel callers.

### ABI target

Legacy ABI:

```text
regcall bdfe_to_fat_time()
  in:  EAX = BDFE time dword (hours<<16 | minutes<<8 | seconds)
  out: EAX = FAT packed time (callers store AX only)
  body push/pop EDX internally
preserves: EBX, ESI, EDI, EBP, ECX typical across callers
trampoline must preserve ECX+EDX (REG-001)
```

Rust ABI:

```text
stdcall rust_bdfe_to_fat_time(bdfe_time) -> EAX; ret 4
trampoline push/pop ECX+EDX
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554259` (`'CUBY'`).
* Exhaustive u16 round-trip: `bdfe_to_fat_time(fat_time_to_bdfe(t)) == t`.
* ABI smoke: direct `rust_*` + public `bdfe_to_fat_time`; marker `B2FT`.
* QEMU OFF/ON desktop smoke; A/B framebuffer compare.
* Real subsystem soak: `--disk exfat` attach-only (entry metadata write path).
