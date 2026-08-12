# Cut BN Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bn-implementation.md`](cut-bn-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BN** migrates classic XFS time conversion —
> `xfs._.conv_time_to_kos_epoch` in `kernel/fs/xfs.asm`.  
> Cuts A-BM remain complete and must not be redone. Do not start Cut BO.

---

## Post-BM migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **68 / 135** before this cut. `xfs._.conv_time_to_kos_epoch`
remained unmigrated; no pre-existing `BN` docs or gate entry existed.

### Path A verdict

**Path A: REJECTED**

No remaining cluster cleared the raised ownership bar:

* XFS remains a collection of related leaves, not a Rust-owned filesystem unit.
* AHCI remains leaf-only (cmdslot/sig/swap) without controller/DMA/IRQ ownership.
* PE/COFF, TCP, HID, Stage-3, paging/V86, FAT/exFAT, ISO, and NTFS remain
  partial footholds rather than coherent Rust-owned subsystems.

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `xfs._.conv_time_to_kos_epoch` | XFS classic seconds DQ -> BDFE | 3 | `--disk xfs` | Low | XFS time deepen | **SELECT** |
| 2 | `fsGetTime` | CMOS -> stacked BDFE | 6+ | Desktop only | Med | Calendar / Stage 2 deepen | Defer |
| 3 | `tcp_mss` | MSS clamp / socket field write | 1 | Partial net | Low | TCP deepen | Defer |
| 4 | `blit_clip` | double `block_clip` compose | 1 | Desktop only | Med | GUI / Stage 7 glue | Defer |
| 5 | `get_proc_ex` | import name lookup recurse | 1 | Desktop only | Med | PE ban stretch | Defer |

### Why #1 wins

* New semantic foothold inside XFS time handling without over-claiming ownership.
* Three live call-sites in `xfs_get_inode_info`, not an export-only or dead leaf.
* Clean legacy ABI: `movbe` high dword then `call fsTime2bdfe`; no hidden globals.
* Independent oracle is straightforward because the legacy leaf is an exact
  composition boundary.
* Real project harness exists for `--disk xfs`, matching earlier XFS cuts.
* Reloc-free Rust is feasible by reusing the already-proven Cut T calendar writer.

### Why the others lose

* `fsGetTime`: real CMOS / RTC side effects; weaker oracle; wider blast radius.
* `tcp_mss`: lower novelty, single caller, partial network soak only.
* `blit_clip`: Stage-7 GUI composition glue with desktop-only evidence.
* `get_proc_ex`: PE deepen remains explicitly deferred after Y/AT/BK.

### ABI target

Legacy ABI:

```text
call [ebp+XFS.conv_time_to_kos_epoch]
ECX -> on-disk DQ
  [ECX+0] hi_be = seconds since 2001-01-01
  [ECX+4] lo_be ignored
EDI -> 8-byte BDFE out
out: BDFE written, EDI += 8
clobbers: EAX/EBX/ECX/EDX, flags
preserves: ESI, EBP
plain ret
DF unchanged
```

Rust ABI:

```text
stdcall rust_xfs_conv_time_to_kos_epoch(secs, out)
secs = native-endian u32 seconds since 2001-01-01
out  = writable 8-byte BDFE block
ret 8
trampoline: movbe eax,[ecx+DQ.hi_be] ; call rust_* ; add edi,8
```

### Validation requirements

* Differential: exact 50,000 deterministic PRNG cases with fresh seed
  `0x4355544E` (`CUTN`).
* ABI smoke: public trampoline plus direct `rust_*`, unique marker `XFCT`.
* QEMU OFF and ON: QMP `running`, non-black framebuffer, PPM capture.
* Strongest relevant soak: `--disk xfs` A/B attach-only smoke, with claims kept precise.
