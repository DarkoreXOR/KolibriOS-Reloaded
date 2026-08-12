# Cut BV Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bv-implementation.md`](cut-bv-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BV** migrates CMOS RTC orchestration → KOS seconds —
> `fsGetTime` in `kernel/fs/fs_common.inc`.  
> Cuts A–BU remain complete. Cut BT/BU are closed — do not modify. Do not start Cut BW.

---

## Fresh post-BU migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **76** |
| `[[rust.migrations]]` entries | **76** (Cut A = 4 symbols) |
| `[ ]` pending | **59** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut BT (`ntfsGetTime`) | **closed** — gate `USE_RUST_NTFS_GET_TIME = 1` |
| Cut BU (`fix_coff_symbols`) | **closed** — gate `USE_RUST_FIX_COFF_SYMBOLS = 1` |

Baseline before this cut: **76 / 135**. Target after: **77 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BN/BO/time/dir leaves only — no Rust-owned mount |
| NTFS / MCB / FRS | **No** | AF/BT/BU pack leaves only — BT/BU closed |
| Networking / TCP / IPv4 | **No** | timer/flag leaves ≠ protocol ownership |
| PE / COFF loader | **No** | Y/AT/BK/BU leaves only — BU closed |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT / exFAT | **No** | AO calendar siblings ban-listed |
| EXT | **No** | AL/BR/BS leaves; no `--disk ext` |
| Unicode / strings | **No** | BQ closed; AN inverse banned |
| Calendar / CMOS | **No** | BV is a Path B leaf; port I/O stays FASM |
| HID / ISO9660 / Stage-3 | **No** | prior rejections unchanged |
| paging / V86 / PCI | **No** | AQ/BL/BA leaves only |

Previous migrations (BT ntfsGetTime pack, BU fix_coff_symbols) do not change
the ownership bar — they deepen Path B leaves without shared Rust subsystem state.

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | **`fsGetTime`** | CMOS orchestration → BDFE → KOS secs | 9+ (`ntfs`/`ext`/smokes) | desktop partial | Excellent mock CMOS | Med | **SELECT** |
| 2 | `fsReadCMOS` | CMOS BCD port read | 12 | CMOS HW | port I/O | Med–High | Defer — lower-level side effect |
| 3 | `tcp_mss` | TCP MSS clamp store | 1 | partial net | Good | Low | Reject — thin / TCP deepen |
| 4 | `ahci_port_wait` | AHCI busy poll | 2 | `--bus ahci` | timer HW | Med | Defer — AV orchestration |
| 5 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 6 | `bdfe_to_fat_time` | BDFE → DOS time | 5 | `--disk exfat` | Good | Low | Defer — AO ban-list |
| 7 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |
| 8 | `uni2ansi_char` | CP866 encode inverse | 10+ | multi-FS | Good | Low | Defer — AN ban-list |

### Why #1 wins

* **New semantic class:** first CMOS RTC orchestration leaf (six-register read +
  FASM `ror`/`add 2000` BDFE pack + Cut G calendar) — complements BT/BS/BR
  metadata paths without claiming calendar subsystem ownership.
* Nine live `call fsGetTime` sites across NTFS/EXT + BT smoke composition.
* Port I/O stays FASM (`fsReadCMOS` via injected `fs_read_cmos_stdcall`).
* Strong independent mock-CMOS oracle + 50k PRNG (`seed 0x43554256` / `'CUBV'`).
* Desktop partial soak via CMOS-backed boot + NTFS metadata compose (Cut BT).

### Why alternatives lose

* `fsReadCMOS`: lower-level port I/O; BV subsumes its orchestration role for
  time query while keeping the port leaf in FASM.
* `tcp_mss`: three-instruction TCP deepen; single caller; zero novelty.
* `ntfs_restore_usa_frs`: `mov eax,[frs_size]` + fallthrough to migrated Cut J.
* AO calendar / AN inverse / export-only string: explicit ban-list.
* `ahci_port_wait`: hardware timer poll; AV sibling orchestration.

### ABI target

Legacy ABI:

```text
regcall fsGetTime()
  six × fsReadCMOS (regs 7/8/9/0/2/4) → stack BDFE
  fallthrough fsCalculateTime → EAX = KOS secs since 2001-01-01
clobbers: EAX, ECX?, EDX?, ESI (stack BDFE ptr during legacy fallthrough)
preserves: EBX, EDI, EBP typical across callers
```

Rust ABI:

```text
stdcall rust_fs_get_time(read_cmos) -> EAX; ret 4
trampoline injects fs_read_cmos_stdcall
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554256` (`'CUBV'`).
* ABI smoke: direct `rust_*` + public `fsGetTime`; marker `FSGT`.
* QEMU OFF/ON desktop smoke; A/B framebuffer compare.
* Real subsystem soak: desktop partial (CMOS boot + NTFS compose) — attach-only.
