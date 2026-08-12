# Cut BU Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bu-implementation.md`](cut-bu-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BU** migrates PE/COFF import symbol table resolve —
> `fix_coff_symbols` in `kernel/core/dll.inc`.  
> Cuts A–BT remain complete. Cut BT is closed — do not modify. Do not start Cut BV.

---

## Fresh post-BT migration audit

### Inventory reconciliation

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols:

| Check | Result |
|-------|--------|
| `[x]` checklist items | **75** |
| `[[rust.migrations]]` entries | **75** (Cut A = 4 symbols) |
| `[ ]` pending | **60** |
| Total scoped | **135** |
| `strtoint_dec` | dead / excluded (not counted) |
| Cut BT (`ntfsGetTime`) | **closed** — gate `USE_RUST_NTFS_GET_TIME = 1` |

Baseline before this cut: **75 / 135**. Target after: **76 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BN/BO/time/dir leaves only — no Rust-owned XFS mount |
| NTFS / MCB / FRS | **No** | AF/BT pack leaves only — BT closed |
| Networking / TCP / IPv4 | **No** | Stage-5 timer/flag leaves ≠ protocol ownership |
| PE / COFF loader | **No** | Y/AT/BK leaves only — symbol loop ≠ loader ownership |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT / exFAT | **No** | AO calendar siblings ban-listed |
| EXT | **No** | AL/BR/BS leaves; no `--disk ext` |
| Unicode / strings | **No** | BQ closed string class; AN inverse banned |
| HID / ISO9660 / Stage-3 | **No** | prior rejections unchanged |
| paging / V86 / PCI | **No** | AQ/BL/BA leaves only |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| 1 | `fix_coff_symbols` | COFF sym resolve + internal VA add | 2 | desktop partial | Excellent synthetic | Med | **SELECT** |
| 2 | `fsGetTime` | CMOS → KOS secs compose | 5+ | partial | CMOS hard | Med | Defer — calendar cluster |
| 3 | `fsReadCMOS` | CMOS BCD byte read | 12 | CMOS hard | port I/O | Med–High | Defer — calendar side effect |
| 4 | `tcp_mss` | TCP MSS clamp + store | 1 | partial net | Good | Low | Defer — TCP deepen / 1 caller |
| 5 | `ahci_port_wait` | AHCI busy poll | 2 | `--bus ahci` | timer HW | Med | Defer — AV orchestration |
| 6 | `ntfs_restore_usa_frs` | J size wrapper | 4 | `--disk ntfs` | none | Low | Reject — thin wrapper |
| 7 | `bdfe_to_fat_time` | BDFE → DOS time | 5 | `--disk exfat` | Good | Low | Defer — AO ban-list |
| 8 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Good | Low | Reject — export-only |
| 9 | `uni2ansi_char` | CP866 encode inverse | 10+ | multi-FS | Good | Low | Defer — AN ban-list |

### Why #1 wins

* **New semantic class:** first COFF symbol-table resolve loop (external `get_proc_ex` + internal section VA add) — complements Y/AT/BK without claiming PE loader ownership.
* Two live callers: `load_library` (`dll.inc`) + `ext_lib.inc` PE import helper.
* `get_proc_ex` stays FASM; injected callback keeps reloc-free Rust (no import-table walk in blob).
* Strong independent buffered oracle + 50k PRNG differential (`seed 0x43554255` / `'CUBU'`).
* Desktop partial soak via `.sys`/DLL load path; no live COFF mutation in smoke (REG-003 safe).

### Why alternatives lose

* `fsGetTime` / `fsReadCMOS`: CMOS port I/O cluster caution; weaker host oracle.
* `tcp_mss`: trivial TCP deepen; single caller; less novel than PE sym resolve.
* `ntfs_restore_usa_frs`: one `mov` + fallthrough to migrated Cut J — zero new semantics.
* AO calendar / AN inverse / export-only string: explicit ban-list or anti-cluster.
* `ahci_port_wait`: hardware timer poll; AV sibling orchestration.

### ABI target

Legacy ABI:

```text
stdcall fix_coff_symbols(sec, symbols, sym_count, strings, imports)
in:  sec → COFF section headers
     symbols → first COFF_SYM (walker)
     sym_count → symbol count
     strings → COFF string table base
     imports → import table ptr (may be 0)
out: EAX = 1 success / 0 if any external unresolved
clobbers: EAX, EBX, EDI (uses ebx esi proc)
preserves: ECX, EDX, ESI, EBP per stdcall + proc uses
stack: ret 20
```

Rust ABI:

```text
stdcall rust_fix_coff_symbols(sec, symbols, sym_count, strings, imports, get_proc_ex)
trampoline injects get_proc_ex; ret 24
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554255` (`'CUBU'`).
* ABI smoke: direct `rust_*` + public `fix_coff_symbols`; marker `FCFS`.
* QEMU OFF/ON desktop smoke; A/B framebuffer compare.
* Real subsystem soak: desktop partial (PE/DLL load) — no dedicated harness.
* Do not mutate live DLL state in smoke (REG-003).
