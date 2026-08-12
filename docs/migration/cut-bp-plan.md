# Cut BP Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bp-implementation.md`](cut-bp-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BP** migrates UTF-16 string → UTF-8 buffer streaming —
> `UTF16to8_string` in `kernel/fs/parse_fn.inc`.  
> Cuts A–BO remain complete. Do not start Cut BQ.

---

## Fresh post-BO migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **70 / 135** before this cut. `UTF16to8_string` remained
`[ ]` (deferred Q string wrapper); no pre-existing `BP` docs or gate entry.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BO closed last-extent leaf; mount/inode orchestration stays FASM |
| NTFS / MCB / FRS | **No** | MCB/time/bootsec leaves only |
| Networking / TCP / IPv4 / sockets | **No** | Stage-5 leaves only |
| PE / COFF loader | **No** | Y/AT/BK leaves only |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT | **No** | short-name/time/charset leaves only |
| **Unicode strings** | **No** | Cut Q char leaf + Cut BP string leaf ≠ Rust-owned unicode subsystem |
| HID / ISO9660 / Stage-3 / paging / PCI | **No** | prior rejections unchanged |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- |
| 1 | `UTF16to8_string` | UTF-16 string → UTF-8 buffer loop | 5 | `--disk exfat` attach | Low | **SELECT** |
| 2 | `cp866toUTF8_string` | CP866 string wrapper | 3 | partial | Low | Defer — wrapper / ban theme |
| 3 | `fix_coff_symbols` | PE import symbol patch | 2 | desktop partial | Med | Defer — PE deepen |
| 4 | `ext_write_time` | EXT write-time pack | 5 | no `--disk ext` | Med | Defer — AL compose |
| 5 | `ahci_port_wait` | AHCI poll | 2 | `--bus ahci` | Med | Defer — AV deepen |
| 6 | `tcp_mss` | MSS clamp | 1 | partial net | Low | Defer — TCP deepen |

### Why #1 wins

* New semantic class: **string-scale** UTF-16→UTF-8 streaming (Cut Q is char-level).
* Five live in-kernel callers across FAT/NTFS/exFAT/LFN (`fat.inc`, `ntfs.inc`,
  `exfat.inc` ×2, `fs_lfn.inc`).
* Clean register ABI with SF (overflow) + ZF (NUL) dual flag channel.
* Strong differential oracle composing independent FASM `lodsw` + Cut Q encode flow.
* Reloc-free via inlined encode loop in single FFI section (no cross-blob calls).
* Low blast radius: mutates caller-owned src/dst buffers only.
* Real attach-only exFAT secondary disk soak available (exFAT volume-label path).

### Why alternatives lose

* `cp866toUTF8_string`: fewer callers; deferred wrapper theme; AN+Q compose only.
* `fix_coff_symbols`: PE deepen; mutates COFF table; `get_proc_ex` dependency.
* `ext_write_time` / `ahci_port_wait` / `tcp_mss`: cluster deepen; weaker broaden story.

### ABI target

Legacy ABI:

```text
register call UTF16to8_string
in:  ESI -> UTF-16 string (zero-terminated allowed)
     EDI -> UTF-8 output buffer
     ECX -> signed byte budget (decreasing)
out: SF=1 on buffer exhaustion (UTF16to8 js path)
     ZF=1 on NUL code unit (test eax,eax after pop restore)
     ESI/EDI/ECX updated
preserves: EBX, EDX, EBP (via trampoline)
clobbers: EAX, ESI, EDI, ECX, flags
stack: plain ret
DF: unchanged
```

Rust ABI:

```text
stdcall rust_utf16_to_8_string(src, dest, ecx, src_out, dest_out, ecx_out)
-> packed (SF<<31)|(ZF<<30)|code_unit; ret 24
trampoline: preserve EBX/EDX/EBP; reconstruct SF via test packed; EAX=code unit
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43555042` (`'CUPB'`).
* ABI smoke: public `UTF16to8_string` only; marker `UTBP`; isolated synthetic buffers.
* QEMU OFF/ON desktop smoke; attach-only exFAT A/B.
