# Cut BW Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bw-implementation.md`](cut-bw-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BW** migrates DOS FAT packed-date unpack —
> `fat_date_to_bdfe` in `kernel/fs/fat.inc`.  
> Cuts A–BV remain complete. Cut BT/BU/BV are closed — do not modify.
> Do not start Cut BX.

---

## Fresh post-BV migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **77** |
| `[[rust.migrations]]` entries | **77** (Cut A = 4 symbols) |
| `[ ]` pending | **58** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut BT (`ntfsGetTime`) | **closed** — gate `USE_RUST_NTFS_GET_TIME = 1` |
| Cut BU (`fix_coff_symbols`) | **closed** — gate `USE_RUST_FIX_COFF_SYMBOLS = 1` |
| Cut BV (`fsGetTime`) | **closed** — gate `USE_RUST_FS_GET_TIME = 1` |

Baseline before this cut: **77 / 135**. Target after: **78 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BN/BO/time/dir leaves only — no Rust-owned mount |
| NTFS / MCB / FRS | **No** | AF/BT/BU/BV pack leaves only — BT/BU/BV closed |
| Networking / TCP / IPv4 | **No** | timer/flag leaves ≠ protocol ownership |
| PE / COFF loader | **No** | Y/AT/BK/BU leaves only — BU closed |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT / exFAT calendar | **No** | AO/BW date/time unpack leaves only — entry orchestration FASM |
| EXT | **No** | AL/BR/BS leaves; no `--disk ext` |
| Unicode / strings | **No** | AN/BQ closed; uni2ansi inverse ban-listed |
| Calendar / CMOS | **No** | BV/BT/BS/BR leaves; port I/O stays FASM |
| HID / ISO9660 / Stage-3 | **No** | prior rejections unchanged |
| paging / V86 / PCI | **No** | AQ/BL/BA leaves only |

BV migrating `fsGetTime` does **not** establish calendar subsystem ownership;
`fsReadCMOS` and `fsCalculateTime` boundaries remain FASM-owned.

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | **`fat_date_to_bdfe`** | DOS packed **date** unpack | 6 (FAT+exFAT read) | `--disk exfat` | Excellent u16 | Low | **SELECT** |
| 2 | `uni2ansi_char` | CP866 encode | 11 | multi-FS | Excellent | Low | Defer — unicode deepen / AN inverse ban |
| 3 | `bdfe_to_fat_time` | BDFE→DOS time pack (write) | 5 | `--disk exfat` | Good | Low | Defer — AO calendar pair ban |
| 4 | `ahci_port_wait` | AHCI busy poll | 2 | AHCI boot | Med (timer) | Med | Defer — AV HW orchestration |
| 5 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 6 | `tcp_mss` | TCP MSS clamp | 1 | partial net | Good | Low | Reject — TCP deepen / 1 caller |
| 7 | `fsReadCMOS` | CMOS BCD port read | 12 | CMOS HW | port mock | Med | Defer — calendar side effect |
| 8 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |
| 9 | `blit_clip` | blit composition | 1 | desktop | Good | Low | Defer — H sibling glue |

### Why #1 wins

* **Novel leaf vs AO:** Cut AO migrated **time** unpack; `fat_date_to_bdfe` is the
  distinct **date** bitfield layout (year<<9 / month<<5 / day) on FAT+exFAT entry
  read paths — not a thin wrapper.
* Six live `call fat_date_to_bdfe` sites (FAT×3 + exFAT×3) on metadata read.
* Excellent independent u16 exhaustive oracle + 50k PRNG (`seed 0x43554257` / `'CUBW'`).
* Same `--disk exfat` attach-only soak class as AO (boot FAT + attached exFAT).
* Pure register leaf — reloc-free, REG-001 ECX+EDX trampoline discipline.

### Why alternatives lose

* `uni2ansi_char`: highest caller count but unicode cluster deepen; AN inverse
  ban-list; Cut A already owns encode via `unicode.cp866.encode`.
* `bdfe_to_fat_time`: write-path inverse pack; AO calendar pair ban-list.
* `ahci_port_wait`: `timer_ticks` + MMIO poll; AV orchestration deepen.
* `ntfs_restore_usa_frs`: one `mov` + fallthrough to migrated Cut J.
* `fsReadCMOS`: lower-level port I/O; BV keeps orchestration in Rust while
  port leaf stays FASM.
* `tcp_mss` / export-only strings: thin / zero kernel callers.

### ABI target

Legacy ABI:

```text
regcall fat_date_to_bdfe()
  in:  EAX = FAT packed date (callers often movzx word — body uses full EAX)
  out: EAX = BDFE date dword (year<<16 | month<<8 | day)
  body push/pop ECX+EDX internally
preserves: EBX, ESI, EDI, EBP typical across callers
trampoline must preserve ECX+EDX (REG-001)
```

Rust ABI:

```text
stdcall rust_fat_date_to_bdfe(fat_date) -> EAX; ret 4
trampoline push/pop ECX+EDX
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554257` (`'CUBW'`).
* ABI smoke: direct `rust_*` + public `fat_date_to_bdfe`; marker `FDTB`.
* QEMU OFF/ON desktop smoke; A/B framebuffer compare.
* Real subsystem soak: `--disk exfat` attach-only (entry metadata read path).
