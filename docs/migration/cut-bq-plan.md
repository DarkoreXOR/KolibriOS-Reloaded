# Cut BQ Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bq-implementation.md`](cut-bq-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BQ** migrates CP866 string → UTF-8 buffer streaming —
> `cp866toUTF8_string` in `kernel/fs/parse_fn.inc`.  
> Cuts A–BP remain complete. Do not start Cut BR.

---

## Fresh post-BP migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **71 / 135** before this cut. `cp866toUTF8_string` remained
`[ ]` (deferred unicode wrapper); no pre-existing `BQ` docs or gate entry.

Mechanical check: 71 `[[rust.migrations]]` entries, 71 `[x]` + 64 `[ ]` = 135.

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
| **Unicode strings** | **No** | Cut Q/BP/AN char+string leaves ≠ Rust-owned unicode subsystem |
| HID / ISO9660 / Stage-3 / paging / PCI | **No** | prior rejections unchanged |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- |
| 1 | `cp866toUTF8_string` | CP866 string → UTF-8 buffer loop | 3 | `--disk iso9660` partial | Low | **SELECT** |
| 2 | `fix_coff_symbols` | PE import symbol patch | 2 | desktop partial | Med | Defer — PE deepen |
| 3 | `ext_write_time` | EXT write-time pack | 5 | no `--disk ext` | Med | Defer — AL compose |
| 4 | `ext_read_all_times` | 3× AL compose | 2 | no `--disk ext` | Low | Defer — AL compose |
| 5 | `fsGetTime` | CMOS→BDFE | 6+ | partial / CMOS hard | Med | Defer — calendar caution |
| 6 | `ahci_port_wait` | AHCI poll | 2 | `--bus ahci` | Med | Defer — AV deepen |
| 7 | `tcp_mss` | MSS clamp | 1 | partial net | Low | Defer — TCP deepen |

### Why #1 wins

* New semantic class: **CP866 string-scale** streaming encode (Cut BP is UTF-16
  string; Cut Q is char-level; this is the AN+Q per-byte compose loop).
* Three live in-kernel callers (`fs_lfn.inc` ×2, `iso9660.inc` ×1).
* Clean register ABI with SF (overflow) + ZF (NUL) dual flag channel — same
  packed return model as Cut BP.
* Strong differential oracle composing independent `cp866_decode` + `utf16_to_8`
  FASM-flow (not Rust-vs-Rust).
* Reloc-free via inlined AN+Q helpers in single FFI section (no cross-blob calls).
* Low blast radius: mutates caller-owned src/dst buffers only.
* Partial real soak: ISO9660 volume-name ASCII→UTF8 path (`--disk iso9660`).

### Why alternatives lose

* `fix_coff_symbols`: PE deepen; mutates COFF table; `get_proc_ex` dependency.
* `ext_write_time` / `ext_read_all_times`: no `--disk ext` soak; deepen AL cluster.
* `fsGetTime`: CMOS/RTC side effects; weaker independent oracle.
* `ahci_port_wait` / `tcp_mss`: cluster deepen; weaker coverage broadening.

### ABI target

Legacy ABI:

```text
register call cp866toUTF8_string
in:  ESI -> CP866 string (zero-terminated allowed)
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
stdcall rust_cp866_to_utf8_string(src, dest, ecx, src_out, dest_out, ecx_out)
-> packed (SF<<31)|(ZF<<30)|code_unit; ret 24
trampoline: preserve EBX/EDX/EBP; reconstruct SF via test packed; EAX=code unit
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43555051` (`'CUPQ'`).
* ABI smoke: public `cp866toUTF8_string` only; marker `UTBQ`; isolated synthetic buffers.
* QEMU OFF/ON desktop smoke; attach-only exFAT + iso9660 A/B where available.
* Real subsystem soak: ISO9660 volume-name path **PARTIAL**; LFN mount path **NOT AVAILABLE**.
