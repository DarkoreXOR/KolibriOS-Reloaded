# Cut CA Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-ca-implementation.md`](cut-ca-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut CA** migrates CMOS BCD decode leaf —
> `fsReadCMOS` in `kernel/fs/fs_common.inc`.  
> Cuts A–BZ remain complete. Cut BZ is closed — do not modify. Do not start Cut CB.

---

## Fresh post-BZ migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **81** |
| `[[rust.migrations]]` entries | **81** (Cut A = 4 symbols) |
| `[ ]` pending | **54** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (`conf_lib.inc` not linked) |
| Cut BZ (`uni2ansi_char`) | **closed** — gate `USE_RUST_UNI2ANSI_CHAR = 1` |
| All prior gates | **81/81 enabled** |

Baseline before this cut: **81 / 135**. Target after: **82 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| Calendar / CMOS | **No** | BV orchestration + CA decode leaves only; port I/O stays FASM |
| Unicode encode+decode | **No** | BZ closed; string orchestrators remain FASM |
| XFS / NTFS / EXT | **No** | prior leaf-only rejections unchanged |
| FAT / exFAT calendar | **No** | AO/BW/BX/BY quartet complete |
| Networking / TCP / IPv4 | **No** | timer/flag leaves ≠ protocol ownership |
| PE / COFF / AHCI / HID | **No** | prior leaf-only rejections unchanged |
| paging / V86 / PCI | **No** | AQ/BL/BA leaves only |

### Ranked candidates (54 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | **`fsReadCMOS`** | CMOS BCD decode leaf | 13 (fs_common×7, fat×6) | desktop + `--disk exfat` attach | Excellent (BCD oracle + mock port) | Med | **SELECT** |
| 2 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 3 | `ahci_port_wait` | AHCI busy poll | 2 | `--bus ahci` | timer HW | Med | Defer — AV orchestration |
| 4 | `tcp_mss` | TCP MSS clamp store | 1 | partial net | Good | Low | Reject — TCP deepen |
| 5 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS plugin island |
| 6 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |
| 7 | `net_ptr_to_num` | AY wrapper | 0 direct | partial net | Good | Low | Reject — thin wrapper |

### Why #1 wins

* **Distinct leaf after BV:** Cut BV migrated CMOS orchestration (`fsGetTime`);
  **`fsReadCMOS`** remains the shared BCD decode leaf called from FAT timestamp
  packers (`get_date_for_file` / `get_time_for_file`) and the BV callback wrapper.
* Thirteen live `call fsReadCMOS` sites — highest fanout among eligible Path B
  candidates after BZ.
* Excellent independent BCD oracle (already proven in Cut BV tests) + 50k PRNG
  on raw byte domain (`seed 0x43555443` / `'CUTC'`).
* Port I/O stays FASM via injected `fs_cmos_raw_read_stdcall`.
* Upper-EAX preservation quirk retained (FAT `ror eax,N` pattern).

### Why alternatives lose

* `ntfs_restore_usa_frs`: one-instruction J sibling wrapper.
* `ahci_port_wait`: AV Path A reject — controller wait orchestration.
* `tcp_mss`: three-instruction TCP deepen; single caller.
* `exFAT_find_lfn`: large FS plugin island; explicit deferral.
* Export-only strings / `net_ptr_to_num`: zero novelty / thin wrapper.

### Legacy ABI

```text
call/ret (not stdcall)
in:  AL = CMOS register index (0x70 port)
side effect: OUT 0x70, IN 0x71
out: AX = decoded BCD value 0–99
preserves: upper 16 bits of EAX (IN/shl/shr/aad only touch AX)
preserves: ECX, EDX (REG-001)
```

### Rust ABI

```text
stdcall rust_fs_read_cmos(raw_read, reg) -> EAX (AX); ret 8
trampoline injects fs_cmos_raw_read_stdcall; merges upper EAX from caller
```

### Production gate

`USE_RUST_FS_READ_CMOS = 1` in `kernel/fs/fs_common.inc`.
