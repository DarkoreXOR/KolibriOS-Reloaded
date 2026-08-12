# Cut BO Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bo-implementation.md`](cut-bo-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BO** migrates XFS final data-directory block selection —
> `xfs._.get_last_dirblock` in `kernel/fs/xfs.asm`.  
> Cuts A–BN remain complete and must not be redone. Do not start Cut BP.

---

## Fresh post-BN migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **69 / 135** before this cut. `xfs._.get_last_dirblock`
remained unmigrated; no pre-existing `BO` docs or gate entry existed.

### Path A verdict

**Path A: REJECTED**

No remaining cluster cleared the raised ownership bar:

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | R/W/AM/AP/AW/AK/BN/BO are complementary leaves; mount/inode/dir orchestration stays FASM |
| NTFS / MCB / FRS | **No** | MCB/time/bootsec leaves only; FRS/bitmap/write orchestration stays FASM |
| Networking / TCP / IPv4 / sockets | **No** | checksum/route/socket/timer leaves only; stateful protocol core stays FASM |
| PE / COFF loader | **No** | Y/AT/BK leaves only; loader/import orchestration stays FASM |
| AHCI | **No** | AV/BG/BM leaves only; wait/DMA/IRQ ownership stays FASM |
| FAT | **No** | short-name/time/charset leaves only |
| Strings | **No** | D/BB/BF/BH leaves only; export-only peers remain |
| HID | **No** | L/BE leaves only; `set_mouse_data` is side-effect heavy |
| ISO9660 | **No** | AJ/BI pair exhausted; mount/read traversal stays FASM |
| Stage-3 FS/syscall helpers | **No** | P/AZ/BJ gates only; façade not ownership |
| Paging / V86 | **No** | AQ/BL translate leaves only |
| PCI / bus | **No** | BA scalar helper only |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `xfs._.get_last_dirblock` | XFS last data dirblock arithmetic | 2 | `--disk xfs` attach-only | Low | XFS dir deepen | **SELECT** |
| 2 | `fix_coff_symbols` | PE import symbol patch loop | 2 | Desktop / `.sys` partial | Med | PE deepen | Defer |
| 3 | `ahci_port_wait` | AHCI busy/DRQ poll | 2 | `--bus ahci` | Med | AV deepen | Defer |
| 4 | `ext_write_time` | EXT write-time pack | 5 | No `--disk ext` | Med | AL compose | Defer |
| 5 | `ext_read_all_times` | 3× AL compose | 2 | No `--disk ext` | Low | AL compose | Defer |
| 6 | `tcp_mss` | MSS clamp | 1 | Partial net | Low | TCP deepen | Defer |

### Why #1 wins

* New semantic class after BN: directory block selection, not another time-conversion leaf.
* Two live in-kernel callers in `xfs.asm` (`xfs_readdir` extents path and `xfs._.get_inode_short`).
* Clean register ABI with strong differential oracle (extent unpack + dirblklog arithmetic).
* Real project harness exists for `--disk xfs` attach-only A/B.
* Low blast radius: pure arithmetic on caller-owned inode/XFS state; no globals mutated.
* Reloc-free Rust is feasible by inlining only the needed extent-field decode.

### Why the alternatives lose

* `fix_coff_symbols`: mutates live COFF symbol table; depends on `get_proc_ex`; PE deepen after Y/AT/BK.
* `ahci_port_wait`: hardware-adjacent polling against `timer_ticks`; controller orchestration not a pure leaf.
* `ext_write_time` / `ext_read_all_times`: no `--disk ext` soak; deepen AL rather than broaden coverage.
* `tcp_mss`: single caller, trivial clamp, weaker soak.

### ABI target

Legacy ABI:

```text
register call xfs._.get_last_dirblock
in:  EBX -> inode buffer
     EBP -> XFS
out: EDX:EAX = last data directory block
preserves: EBX, ECX
clobbers: EAX, EDX, flags
stack: plain ret
DF: unchanged
```

Behavior notes:

* Reads `di_nextents` as BE32 from `[ebx + XFS.nextents_offset]`.
* Selects the final `xfs_bmbt_rec` at `ebx + inode_core_size + nextents*16 - 16`.
* Unpacks only `br_startoff` and `br_blockcount` from that record.
* Returns `br_startoff + ((br_blockcount >> dirblklog) - 1)` with x86 `&31` shift masking and `dec` underflow preserved.

Rust ABI:

```text
stdcall rust_xfs_get_last_dirblock(inode, nextents_offset, inode_core_size, dirblklog, out_hi)
-> EAX = block_lo; *out_hi = block_hi; ret 20
trampoline: preserve EBX/ECX; materialize XFS fields from EBP; write EDX:EAX
```

### Validation requirements

* Differential: exact 50,000 deterministic PRNG cases with fresh seed `0x4355424F` (`'CUBO'`).
* ABI smoke: public trampoline plus direct `rust_*`, unique marker `XFBO`.
* QEMU OFF and ON: QMP `running`, non-black framebuffer, PPM capture.
* Strongest relevant soak: `--disk xfs` attach-only A/B smoke, with claims kept precise.
