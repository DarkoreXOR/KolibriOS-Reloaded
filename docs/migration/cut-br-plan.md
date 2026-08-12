# Cut BR Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-br-implementation.md`](cut-br-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BR** migrates EXT inode triple-timestamp → BDFE fan-out —
> `ext_read_all_times` in `kernel/fs/ext.inc`.  
> Cuts A–BQ remain complete. Do not start Cut BS.

---

## Fresh post-BQ migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **72 / 135** before this cut. `ext_read_all_times` remained
`[ ]` (deferred AL compose); no pre-existing `BR` docs or gate entry.

Mechanical check: 72 `[[rust.migrations]]` entries, 72 `[x]` + 63 `[ ]` = 135.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BO closed last-extent leaf; mount/inode orchestration stays FASM |
| NTFS / MCB / FRS | **No** | MCB/time/bootsec leaves only |
| Networking / TCP / IPv4 / sockets | **No** | Stage-5 leaves only |
| PE / COFF loader | **No** | Y/AT/BK leaves only |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT / exFAT | **No** | short-name/time/charset leaves only |
| **EXT** | **No** | Cut AL + BR leaves ≠ Rust-owned EXT mount/inode subsystem |
| Unicode strings | **No** | BQ closed CP866 string class; no Path A claim |
| HID / ISO9660 / Stage-3 / paging / PCI | **No** | prior rejections unchanged |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `ext_read_all_times` | EXT 3× timestamp inode fan-out | 2 | no `--disk ext` | Low | EXT deepen | **SELECT** |
| 2 | `fix_coff_symbols` | PE import symbol patch | 2 | desktop partial | Med | PE deepen | Defer — `get_proc_ex` |
| 3 | `ext_write_time` | EXT write-time pack | 5 | no `--disk ext` | Med | EXT deepen | Defer — `fsGetTime` side effect |
| 4 | `fsReadCMOS` | CMOS BCD byte read | 6+ | CMOS hard | Med | calendar | Defer — HW I/O oracle |
| 5 | `fsGetTime` | CMOS→BDFE compose | 6+ | partial | Med | calendar caution | Defer — G cluster |
| 6 | `ahci_port_wait` | AHCI poll | 2 | `--bus ahci` | Med | AHCI deepen | Defer — AV sibling |
| 7 | `strchr` | reverse char search | 0 kernel | export-only | Low | string | Reject — export-only |

### Why #1 wins

* New semantic class: **inode-scale** triple timestamp orchestration (Cut AL is single-slot).
* Two live in-kernel callers (`ext_ReadFolder`, `ext_GetFileInfo`).
* Clean register ABI (`ESI` inode, `EDI` 3× BDFE out); read-only on inode.
* Strong differential oracle modeling FASM partial/fast paths + inlined AL+T compose.
* Reloc-free via inlined `ext_unix_to_secs` + `fs_time2bdfe_ptr` (no cross-blob calls).
* Low blast radius: writes caller-owned 24-byte BDFE block only.

### Why alternatives lose

* `fix_coff_symbols`: PE deepen; mutates COFF table; banned `get_proc_ex` dependency.
* `ext_write_time` / `fsGetTime`: CMOS/write side effects; calendar cluster caution.
* `fsReadCMOS`: port I/O; weaker independent host oracle.
* `strchr` / `strnlen`: export-only — zero in-kernel callers.
* `ahci_port_wait` / `tcp_mss`: cluster deepen without new semantic class.

### ABI target

Legacy ABI:

```text
register call ext_read_all_times
in:  ESI -> inode buffer
     EDI -> 3× BDFE output (advanced +24 by FASM via ext_read_time chain)
out: 24 bytes BDFE written (cr/c + a + m order)
preserves: ESI (inode pointer); callers save ECX on stack where needed
clobbers: EAX, EDX, ECX, EDI
stack: plain ret
DF: unchanged
```

Rust ABI:

```text
stdcall rust_ext_read_all_times(inode, out); ret 8
trampoline: preserve ESI; pass ESI/EDI; Rust writes 3× BDFE at out
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554252` (`'CUBR'`).
* ABI smoke: public `ext_read_all_times` + direct `rust_*`; marker `EXBR`; isolated synthetic inode/out buffers.
* QEMU OFF/ON desktop smoke; attach-only exFAT A/B.
