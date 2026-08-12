# Cut CD Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-cd-implementation.md`](cut-cd-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CD** migrates video blit source/dest rectangle compose —
> `blit_clip` in `kernel/video/blitter.inc`.  
> Cuts A–CC remain complete and must not be redone. **Cut CC is closed — do not
> modify.** Do not start Cut CE in this task.

---

## Fresh post-CC migration audit

### Inventory reconciliation

| Check | Result |
|-------|--------|
| `[x]` checklist items | **84** (post-CC baseline) |
| `[[rust.migrations]]` entries | **84** (Cut A = 4 symbols) / **85** count in `build.toml` includes Phase-C? — production gates **84** enabled |
| `[ ]` pending | **51** |
| Total scoped | **135** |
| Cut CC (`process_partition_table_entry`) | **closed** — untouched |
| REG-007 / REG-008 / REG-009 | Fixed; lessons applied (stdcall cleanup, register contracts, smoke mocks) |
| All prior gates | **84/84 enabled** |

Baseline before this cut: **84 / 135**. Target after: **85 / 135**.

### Path A verdict

**Path A: REJECTED**

| Cluster | Verdict | Why not Path A |
|---------|---------|----------------|
| Partition Z+AD+CC (+ `disk_scan_*`) | No | Validate/dispatch leaves; scan/I/O still FASM |
| AHCI AV+BM+CB | No | Slot/sig/poll leaves ≠ controller ownership |
| FS calendar / FAT / NTFS / EXT / XFS | No | Leaf pack/unpack; plugins FASM |
| PE Y+AT+BK+BU | No | Symbol/reloc leaves; loader FASM |
| Net AC/AS/AU/AY/M/V/BD | No | Route/timer/flag leaves ≠ stack ownership |
| HID L+BE | No | Accel/hotkey leaves |
| **Video H (+ `blit_clip`)** | **No** | Geometry compose only; LFB/`blit_32` hot path still FASM |
| Stage-4 AQ+BL | No | Translate footholds only |
| IRQ / sched / process / GUI Stage-7 | No | Late / boundaries |

### Ranked candidates (51 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`blit_clip`** | Dual `block_clip` compose + remap | 1 (`blit_32` / fn73) | desktop blit | **Excellent** | Med | **SELECT** |
| 2 | `enable_irq` | PIC/APIC IRQ unmask | 6 | desktop IRQ | Poor (I/O) | Med–High | Defer — interrupt path |
| 3 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** | Defer — ~32KB `unpack.p` + ~500-line decoder |
| 4 | `window._.set_window_clientbox` | GUI clientbox policy | 3 | desktop GUI | Good | Med | Defer — Stage-7 soft |
| 5 | `set_mouse_data` | HID aggregator | 0 in-kernel (PE export) | weak | Med | Med | Defer — side-effects / export-only fan-in |
| 6 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med | Defer — interrupt I/O |
| 7 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS plugin island |
| 8 | `disk_scan_partitions` | MBR/EBR/GPT orchestration | 1 | boot | poor as leaf | **High** | Reject — orchestration after CC |

### Why #1 wins

* **Intentional Cut H follow-on** deferred since H — dual clip + src/dst remap is a distinct compose class, not a trivial wrapper.
* **Live production path** via `blit_32` → syscall 73; every desktop that blits exercises it.
* **Composes Cut H** via pure `block_clip` (inlined into reloc-free blob — no cross-blob reloc).
* **Strong oracle** — synthetic `BLITTER` → CF + field vector; PRNG feasible.
* **Manageable size** (~80 nonblank lines); no I/O, alloc, IRQ, or mutex.
* **Limited blast radius** — one public symbol; legacy body retained behind gate.

### Why alternatives lose

* `enable_irq` / `irq_eoi`: new interrupt class but weak host oracle + I/O risk (Cut CC ABI lessons amplify cost).
* `unpack`: strongest raw FASM reduction but disproportionate decoder/global/mutex risk.
* `set_window_clientbox`: solid GUI leaf but Stage-7 soft deferral; weaker novelty vs H-compose.
* `set_mouse_data`: PE-export aggregator; REG-003-class global side-effects.
* Thin rejects (`tcp_mss`, `ntfs_restore_usa_frs`, `mutex_init`, `sysfn_getfreemem`, `get_phys_addr`, export-only strings): fail substance bar.

### Legacy ABI

```text
blit_clip() → void; plain ret
in:  ECX → BLITTER*
     BLITTER.{dc,sc,dst_x,dst_y,src_x,src_y,w,h} populated
out: CF = 0 → draw (mutates w,h,src_x,src_y,dst_x,dst_y)
     CF = 1 → don't draw (BLITTER fields unchanged)
preserves: EBX, ESI, EDI (push/pop)
clobbers: EAX, ECX, EDX, flags; 40 B stack temps
callees: block_clip ×2 (ESI=clip RECT*, EDI=mutable RECT*)
```

### Legacy CF quirk (mandatory)

Assembled FASM body ends `.done` with `add esp, 40` **before** `ret`.  
`ADD` writes CF, so the legacy reject path (`jc .done` with CF=1) typically returns with **CF cleared**. Documented contract is CF draw/reject; observable legacy CF after return is unreliable.

**Cut CD trampoline restores the documented CF contract** (`clc`/`stc` after pops). Mutation-on-success / no-mutate-on-reject matches FASM exactly. Full-reject blits may skip draw under Rust where legacy accidentally continued — safer and aligned with comments + `blit_32`'s `jc` check. Host oracle + ABI smoke assert documented CF; QEMU A/B expects desktop equivalence for normal (overlapping) blits.

### Rust ABI

```text
stdcall rust_blit_clip(blitter: *mut u8) -> u32; ret 4
EAX = 0 draw / 1 reject
Mutates BLITTER geometry fields only on draw path.
Pure helper calls kolibri_utils::block_clip (inline; no reloc to rust_block_clip).
```

### Trampoline / stack ownership

```text
push edi/esi/ebx
FASM stdcall rust_blit_clip, ecx   ; callee cleans 4 (ret 4) — NO add esp
test eax,eax → pop ebx/esi/edi → clc or stc; ret
```

REG-009: never double-clean stdcall args.

### Production gate

`USE_RUST_BLIT_CLIP = 1` in `kernel/video/blitter.inc` (independent of `USE_RUST_BLOCK_CLIP`).

### Oracle / tests / QEMU

| Item | Plan |
|------|------|
| Oracle | Independent FASM-flow mirror `fasm_oracle_blit_clip` |
| PRNG | seed `0x424C4954` (`'BLIT'`), ≥50_000 cases |
| Host | focused geometry tests + full `kolibri_utils` suite |
| ABI smoke | synthetic BLITTER: contain / clamp / reject-src / reject-dst; EBX/ESI/EDI/EBP canaries; CF via `jc`/`jnc` |
| QEMU | OFF then ON desktop; repeated ON; desktop blit soak (syscall 73 path) |

### Rollback

`USE_RUST_BLIT_CLIP = 0` → original FASM body.
