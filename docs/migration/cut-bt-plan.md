# Cut BT Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bt-implementation.md`](cut-bt-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BT** migrates NTFS CMOS-metadata FILETIME pack —
> `ntfsGetTime` in `kernel/fs/ntfs.inc`.  
> Cuts A–BS remain complete. Do not start Cut BU.

---

## Fresh post-BS migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **74 / 135** before this cut. `ntfsGetTime` remained
`[ ]` (deferred NTFS time deepen); no pre-existing `BT` docs or gate entry.

Mechanical check: 74 `[[rust.migrations]]` entries, 74 `[x]` + 61 `[ ]` = 135.
`strtoint_dec` remains dead/excluded (not counted).

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BN/BO/time/dir leaves only |
| NTFS / MCB / FRS | **No** | AL/BR/BS/BT pack leaves ≠ Rust-owned NTFS mount/write subsystem |
| Networking / TCP / IPv4 / sockets | **No** | Stage-5 leaves only |
| PE / COFF loader | **No** | Y/AT/BK leaves only |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT / exFAT | **No** | AO calendar siblings ban-listed |
| EXT | **No** | AL/BR/BS leaves only; no `--disk ext` |
| Unicode strings | **No** | BQ closed CP866 string class |
| HID / ISO9660 / Stage-3 / paging / PCI | **No** | prior rejections unchanged |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `ntfsGetTime` | KOS secs → NTFS FILETIME pack (CMOS metadata) | 4 | `--disk ntfs` | Low | NTFS time deepen | **SELECT** |
| 2 | `fix_coff_symbols` | PE import symbol patch loop | 2 | desktop partial | Med–High | PE deepen | Defer — `get_proc_ex` dep + ban stretch |
| 3 | `fsGetTime` | CMOS → KOS secs compose | 5+ | partial | Med | calendar caution | Defer — CMOS side effect / G cluster |
| 4 | `fsReadCMOS` | CMOS BCD byte read | 12 | CMOS hard | Med–High | calendar | Defer — port I/O oracle |
| 5 | `ahci_port_wait` | AHCI poll | 2 | `--bus ahci` | Med | AHCI deepen | Defer — AV sibling orchestration |
| 6 | `bdfe_to_fat_time` | BDFE → DOS time pack | 5 | `--disk exfat` | Low | AO ban-list | Defer — calendar anti-cluster |
| 7 | `strchr` / `strnlen` | C string helpers | 0 kernel | export-only | Low | string | Reject — export-only |
| 8 | `ntfs_restore_usa_frs` | J wrapper | 4 | `--disk ntfs` | Low | NTFS USA | Reject — Cut J explicitly excluded |
| 9 | `ext_SetFileInfo` | EXT metadata write orchestration | vtable | no `--disk ext` | High | FS write path | Defer — orchestration |

### Why #1 wins

* **New semantic class:** CMOS-backed metadata FILETIME pack (BS twin on NTFS write path; complements Cut AF `ntfsCalculateTime` BDFE path).
* Four live in-kernel `call ntfsGetTime` on NTFS create/resize/GetFileInfo metadata stamp paths.
* Clean split: `fsGetTime` (CMOS + Cut G compose) stays FASM; Rust owns deterministic ×10⁷ + bias pack only.
* Strong independent differential oracle mirroring FASM `mul`/`add`/`adc` flow (`filetime_from_secs_2001`).
* Reloc-free 23 B blob; composes existing AF bias constants.
* Real subsystem validation: `--disk ntfs` attach A/B available.

### Why alternatives lose

* `fix_coff_symbols`: PE deepen; mutates COFF; depends on unmigrated `get_proc_ex`.
* `fsGetTime` / `fsReadCMOS`: CMOS port I/O; calendar cluster caution; weaker host oracle.
* AO calendar siblings: explicit ban-list anti-cluster.
* `strchr` / export-only wrappers: zero in-kernel callers.
* `ntfs_restore_usa_frs`: zero new semantics vs Cut J.

### ABI target

Legacy ABI:

```text
regcall ntfsGetTime()
in:  implicit call fsGetTime → EAX = KOS secs since 2001-01-01
out: EDX:EAX = NTFS FILETIME (100ns since 1601-01-01)
clobbers: EAX, ECX, EDX, ESI (fsGetTime sets ESI to stack BDFE ptr)
preserves: EBX, EDI, EBP (callers use EDI after return)
stack: plain ret
DF: unchanged
```

Rust ABI:

```text
stdcall rust_ntfs_get_time_pack(kos_secs); ret 4 → EDX:EAX
trampoline: call fsGetTime → rust pack
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554254` (`'CUBT'`).
* ABI smoke: direct `rust_*` + fsGetTime-oracle + public `ntfsGetTime`; marker `NTGT`.
* QEMU OFF/ON desktop smoke; `--disk ntfs` attach A/B.
* Do not assert ESI preserve across `ntfsGetTime` (REG-001 / fsGetTime clobber).
