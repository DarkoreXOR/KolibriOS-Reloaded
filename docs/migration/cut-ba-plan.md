# Cut BA Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-ba-implementation.md`](cut-ba-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BA** migrates PCI mechanism-1 config address encode —
> `pci_make_config_cmd` in `kernel/bus/pci/pci32.inc`.  
> Cuts A–AZ remain complete and must not be redone. Do not start Cut BB.

---

## Post-AZ migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Preserve **EBX+ECX+EDX**; callers keep ESI=size across the call |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| REG-003 | ABI smoke mutates live globals | Smoke uses **synthetic register inputs only** — no CF8/CFC MMIO, no live PCI poke |
| Cut AZ | Stage-3 size→ZF gate | Complete; do **not** deepen P/AZ syscall façade as Path A |

### Inventory prerequisite

One-time [`migration-todo.md`](migration-todo.md) created from `build.toml` + live
`cut-*-plan.md` symbols (baseline **55 / 135** before this cut).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** — leaves ≠ mount/I/O ownership |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY network Path A? | **No** |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV AHCI Path A? | **No** |
| P+AZ Stage-3 Path A? | **No** |
| PCI leaf as Path A? | **No** — one address-encode leaf ≠ PCI bus ownership |
| Strongest remaining leaf? | **Yes** — `pci_make_config_cmd` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `get_proc_ex` | PE ban stretch (AY/AZ #2 again) |
| `tcp_outflags` | Mild M/V TCP deepen; `.flaglist` reloc risk |
| `fat_name_is_legal` | Charset table + FAT deepen |
| `is_string_userspace` | Thin P sibling |
| `strtoint_dec` | Strong alternate; conf_lib only; less new foothold |
| `ahci_is_sig_known` / `swap_bytes_in_words` | AV deepen |
| `hotkey_do_test` | Indirect call table — reloc-hostile |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-AZ)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `pci_make_config_cmd` | bus/PCI | 2 | **PCI config address encode** | Excellent | Med (all mech-1 CF8) | boot PCI + `--bus ahci` | **SELECT** |
| `strtoint_dec` | core/conf | 2 | Decimal string→int | Excellent | Low | conf parse | #2 |
| `get_proc_ex` | PE / DLL | 1 | Import name→VA | Excellent | Med | `.sys` load | #3 (PE ban) |
| `tcp_outflags` | TCP | 1 | State→flags table | Excellent | Low | Partial net | #4 deepen |
| `fat_name_is_legal` | FAT | 1 | Charset validate | Good | Low | `--disk exfat` | #5 |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: pci_make_config_cmd
Source: kernel/bus/pci/pci32.inc
Subsystem: PCI config-space address encode (bus/PCI foothold)
Stage: Stage-2 leaf / bus foothold (PCI enumeration stays FASM)
Why selected:
    Post-AZ audit: Path A rejected everywhere. Strongest remaining leaf is
    pci_make_config_cmd — first PCI foothold, pure bit math, excellent
    differential domain, exercised on every mech-1 config access (boot +
    AHCI), without deepening overused FS/net/PE clusters or ban-lists.
Why this is a genuine migration boundary:
    Deterministic AH/BH/BL → enable+bus+devfn+reg dword. Distinct from
    AHCI cmdslot / partition validate without claiming PCI ownership.
Why Path A / Path B:
    Path B — one encode leaf. Config I/O, device scan, and drivers remain
    FASM.
Regression risks:
    REG-001: EBX/ECX/EDX preserve (ESI live across call in pci_read/write_reg).
    REG-003: smoke must not touch live PCI ports.
CPU/interrupt-state risks:
    None in leaf — pure compute; no cli/sti.
Shared-state risks:
    None — no globals.
Concurrency/locking risks:
    None.
Required differential tests:
    Independent FASM-flow oracle (shl/mov ax,bx/and/or); AH-only bus;
    BH/BL; high EAX clear; 50k PRNG seed 0x43554241 ('CUBA').
Required ABI tests:
    Marker PCIC; synthetic regs; EBX/ECX/EDX (+ESI/EDI/EBP) canaries;
    no CF8/CFC MMIO.
Required A/B tests:
    Gate OFF vs ON desktop; prefer --bus ahci (PCI+AHCI path).
Required real subsystem validation:
    Boot PCI config via pci_read_reg/pci_write_reg on AHCI boot.
Rejected alternatives:
    strtoint_dec; get_proc_ex; tcp_outflags; fat_name_is_legal;
    Path A clusters; AO/AN/address-math/socket/PE/USB ban-list.
Expected legacy ABI:
    call with AH=bus, BH=devfn, BL=reg; EAX=config dword; ret (not stdcall).
Expected Rust ABI:
    stdcall rust_pci_make_config_cmd(bus,devfn,reg) → EAX; ret 12;
    trampoline unpacks AH/BH/BL and restores EBX/ECX/EDX.
Differential-testing strategy:
    Independent oracle mirroring FASM instruction flow; 50k 'CUBA'.
ABI-risk assessment:
    Medium — all mech-1 PCI config; REG-001; tiny pure leaf mitigates.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **EBX+ECX+EDX** preserve;
`USE_RUST_PCI_MAKE_CONFIG_CMD` rollback.

---

## Out of scope

* Claiming Path A for PCI bus / config I/O
* Migrating `pci_read_reg` / `pci_write_reg` / mechanism 2
* Migrating `strtoint_dec` / `get_proc_ex` / `tcp_outflags`
* Beginning Cut BB
