# Cut BS Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bs-implementation.md`](cut-bs-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut BS** migrates EXT inode write-time pack —
> `ext_write_time` in `kernel/fs/ext.inc`.  
> Cuts A–BR remain complete. Do not start Cut BT.

---

## Fresh post-BR migration audit

### Inventory baseline

[`migration-todo.md`](migration-todo.md) reconciled against `project/build.toml`
and live FASM symbols: **73 / 135** before this cut. `ext_write_time` remained
`[ ]` (candidate AL write twin); no pre-existing `BS` docs or gate entry.

Mechanical check: 73 `[[rust.migrations]]` entries, 73 `[x]` + 62 `[ ]` = 135.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| XFS | **No** | BN/BO closed time/dir leaves; mount orchestration stays FASM |
| NTFS / MCB / FRS | **No** | MCB/time/bootsec leaves only |
| Networking / TCP / IPv4 / sockets | **No** | Stage-5 leaves only |
| PE / COFF loader | **No** | Y/AT/BK leaves only |
| AHCI | **No** | AV/BG/BM leaves only |
| FAT / exFAT | **No** | short-name/time/charset leaves only |
| **EXT** | **No** | AL/BR/BS pack+fan-out leaves ≠ Rust-owned EXT mount/write subsystem |
| Unicode strings | **No** | BQ closed CP866 string class |
| HID / ISO9660 / Stage-3 / paging / PCI | **No** | prior rejections unchanged |

### Ranked candidates

| Rank | Candidate | Semantic class | Callers | Soak | Risk | Cluster | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ---- | ------- | ------- |
| 1 | `ext_write_time` | EXT KOS→inode i_time pack (write path) | 5 | no `--disk ext` | Low | EXT write deepen | **SELECT** |
| 2 | `fix_coff_symbols` | PE import symbol patch loop | 2 | desktop partial | Med | PE deepen | Defer — `get_proc_ex` dep + ban stretch |
| 3 | `fsReadCMOS` | CMOS BCD byte read | 6+ | CMOS hard | Med | calendar | Defer — port I/O oracle |
| 4 | `fsGetTime` | CMOS→KOS secs compose | 6+ | partial | Med | calendar caution | Defer — G cluster / CMOS side effect |
| 5 | `ahci_port_wait` | AHCI poll | 2 | `--bus ahci` | Med | AHCI deepen | Defer — AV sibling orchestration |
| 6 | `strchr` | forward char search | 0 kernel | export-only | Low | string | Reject — export-only |
| 7 | `ext_SetFileInfo` | EXT metadata write orchestration | 1 | no `--disk ext` | High | FS write path | Defer — orchestration |

### Why #1 wins

* New semantic class on the **write path**: inverse pack of Cut AL epoch math (not another read fan-out).
* Five live in-kernel `stdcall` callers (`writeInode`, create/delete paths).
* Clean split: `fsGetTime` (CMOS) stays FASM; Rust owns deterministic pack only.
* Strong independent differential oracle mirroring FASM `add/adc/test/jns/inc` flow.
* Reloc-free single blob; no cross-Rust-section calls.
* Low blast radius: writes caller-owned inode field pointers only.

### Why alternatives lose

* `fix_coff_symbols`: PE deepen; mutates COFF table; depends on banned/near-banned `get_proc_ex`.
* `fsGetTime` / `fsReadCMOS`: CMOS port I/O; calendar cluster caution; weaker host oracle.
* `ext_SetFileInfo`: full FS write orchestration — not a leaf.
* `strchr` / `strnlen`: export-only — zero in-kernel callers.
* `ahci_port_wait`: hardware poll deepen without new semantic class.

### ABI target

Legacy ABI:

```text
stdcall ext_write_time(time_ptr, extra_time_ptr)
in:  implicit fsGetTime → EAX = KOS secs since 2001-01-01
     time_ptr → writable i_*time slot
     extra_time_ptr → writable extra slot or -1 (skip)
out: [time_ptr] = i_time; optional [extra_time_ptr] = extra&3
preserves: EBX, ESI, EDI (proc `uses`)
clobbers: EAX, ECX, EDX
stack: stdcall ret 8
DF: unchanged
```

Rust ABI:

```text
stdcall rust_ext_write_time_pack(kos_secs, time_ptr, extra_time_ptr); ret 12
trampoline: call fsGetTime → rust pack
```

### Validation requirements

* Differential: 50,000 deterministic PRNG cases, seed `0x43554253` (`'CUBS'`).
* ABI smoke: public `ext_write_time` + direct `rust_*`; marker `EXBS`; isolated synthetic field buffers.
* QEMU OFF/ON desktop smoke; attach-only exFAT A/B.
* EXT disk soak: **NOT AVAILABLE** (no `--disk ext` harness).
