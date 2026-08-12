# Cut CH Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-ch-implementation.md`](cut-ch-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CH** migrates PE preferred-base DIR32 rebase —
> `rebase_coff` in `kernel/core/dll.inc`.  
> Cuts CC/CD/CE/CF/CG remain complete and must not be modified. Do not start Cut CI.

---

## Fresh post-CG repository audit

### Baseline verification (2026-08-12)

| Check | Result |
|-------|--------|
| Inventory | **88 / 135** (`migration-todo.md`; 88 `[x]` + 47 `[ ]`) |
| Production gates | **88** in `project/build.toml`, **all `enabled = true`** (89 blocks: 88 prod + 1 disabled non-prod) |
| Cut CC | intact — `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY = 1` |
| Cut CD | intact — `USE_RUST_BLIT_CLIP = 1` |
| Cut CE | intact — `USE_RUST_SET_WINDOW_CLIENTBOX = 1` |
| Cut CF | intact — `USE_RUST_SET_MOUSE_DATA = 1` |
| Cut CG | intact — `USE_RUST_GET_PROC_EX = 1`; blob 127 B / 0 reloc; SHA-256 `6f212740…551f8` |
| `TMP_STACK_TOP` | **`0x008D800`** (`kernel/const.inc`; fixed-addresses + memory-model agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP`; end `.bss` @ `OS_BASE+0x8C4C3` → needs `0x8D4C3 < 0x8D800` (do not lower) |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **EBX+ESI+EDI+EBP** on trampoline |
| REG-003 | Smoke mutates live globals | Synthetic COFF fixture only; no live DLL wipe |
| REG-009 | stdcall double cleanup | `stdcall rust_*` only; Rust `ret 12`; outer `proc` `ret` cleans caller — never `add esp,12` |
| REG-010 | Trampoline arg offset + return address | No naked `sub esp` ctx here; still verify stack math if added |
| REG-011 | PE path lost EBX/ESI/EDI/EBP | Preserve all four even though FASM listed only `uses ebx esi` |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG (+ `rebase_coff`) | Resolve/reloc/rebase leaves; `load_library` / page map / export wiring stay FASM — no Rust-owned loader |
| Video H+CD | Geometry only; `blit_32` / LFB / win_map / cursor stay FASM |
| HID L+BE+CF | Leaves; PE mouse drivers still FASM |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR / STI-CLI still FASM |
| `unpack` | Single DLL decoder island, not subsystem ownership |
| FS / net / AHCI / Stage-4 | Prior footholds ≠ shared-state ownership |

Cut CH remains **Path B**.

---

## Special investigations (mandatory)

### `unpack` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + runtime `unpack.p` (~32 KiB via `kernel_alloc`) |
| Boundary | `stdcall unpack(packed, unpacked); ret 8` — KPCK/LZMA + E8/E9 |
| Sub-cuts | Nested decoder locals share `unpack.*` globals — **no meaningful smaller public leaf** |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Oracle | Excellent (bitstream + golden unpack) |
| Size / memory | Full LZMA decoder → multi-KiB blob; Stage-2 headroom after CG is only ~829 B to `TMP_STACK_TOP` for `.bss` assert — code blob also grows image |
| Verdict | **DEFER** — excellent oracle, disproportionate size/state/blast for one safe cut |

### `blit_32` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/video/blitter.inc` ~257–585 |
| vs CD | CD owns geometry; `blit_32` is LFB **pixel** hot path that *calls* `blit_clip` |
| ABI | Syscall 73; ECX→blit struct; EBX flags; win_map; 32/24/16 bpp; soft/HW cursor |
| Oracle | Buffer-level oracle buildable but engineering-heavy; desktop A/B non-black is **insufficient** |
| Verdict | **DEFER** — natural CD follow-on, blast too high for one safe cut |

### `exFAT_find_lfn` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Contract | ESI path UTF-8; EDI direntry; CF + EAX; EBP=`exFAT*`; stack callbacks |
| Callers | 1 |
| Oracle / soak | Partial (`--disk exfat` attach ≠ full LFN-walk differential) |
| Verdict | **DEFER** — FS plugin island |

### `enable_irq` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/apic.inc` ~391–432 |
| Hardware | PIC `in`/`out` 0x21/0xA1 **or** IOAPIC MMIO (clear mask bit 16) |
| STI/CLI | **Does not** touch `EFLAGS.IF` — mask-only |
| Callers | 6 |
| Oracle | Mockable ports, but **QEMU cannot independently prove mask-bit correctness** |
| Verdict | **REJECT** — fails evidence bar; do not weaken for caller count |

### `irq_eoi` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `apic.inc` ~371–386 (`__fastcall`, `CL` = irq) |
| Hardware | PIC EOI outs **or** `[LAPIC_BASE+APIC_EOI]=0` |
| Callers | 4 |
| Verdict | **DEFER** with `enable_irq` — same I/O oracle class |

### Newly ranked: `rebase_coff` — **SELECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/dll.inc` ~876–911 |
| Role | Preferred-base DIR32 rebase when usermode load base ≠ preferred |
| vs Y | Y patches DIR32/REL32 with **symbol Value**; rebase is **Type 6 only**, addend = **delta**, `sym` **unused** |
| Ban stretch | “Y mutate anti-cluster” — **stale** after AT+BK+BU+CG (same rationale CG used for resolve ban) |
| Oracle | Excellent — synthetic COFF (Cut Y pattern), Type 6 / skip / multi-sec / delta |
| Size | ~36-line leaf; estimate ~150–250 B reloc-free |
| Verdict | **SELECT** — last PE checklist leaf with substance + oracle after CG deepen |

### Overlooked leaves inspected

| Symbol | Verdict |
|--------|---------|
| `strchr` / `strnlen` | **REJECT** — PE export only; 0 in-kernel callers |
| `tcp_mss` | **REJECT** — thin clamp+store |
| `ntfs_restore_usa_frs` | **REJECT** — 3-line fallthrough to Cut J |
| `pid_to_appdata` | **REJECT** — dead (commented caller) |
| `usb_td_to_virt` | **DEFER** — AQ compose + weak USB soak |
| `memmove` | **DEFER** — high blast / forward-only quirk / ~24 callers |
| `mutex_init` / `net_ptr_to_num` / `sysfn_getfreemem` | **REJECT** — thin / wrapper / façade |

---

## Ranked candidates (47 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`rebase_coff`** | PE preferred-base DIR32 rebase | 1 (`load_library`) | desktop DLL off-base | **Excellent** | Low–Med | **SELECT** |
| 2 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** | Defer — size/state |
| 3 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **High** | Defer — blast |
| 4 | `memmove` | memory move | ~24 | everywhere | Good | High | Defer — blast |
| 5 | `usb_td_to_virt` | USB TD→virt | HC path | weak USB | Good | Med | Defer — soak |
| 6 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — island |
| 7 | `enable_irq` | PIC/APIC unmask | 6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 8 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med–High | Defer with enable_irq |
| 9 | thin / export-only / dead | varies | — | — | — | Low | Reject — substance bar |

### Why #1 wins

* Mandatory five yield no SELECT under the evidence bar (oracle / size / blast / island).
* Clear PE deepen after CG: completes Stage-8 leaf set without claiming Path A.
* Distinct from Y (delta-add, Type 6 only, unused `sym`).
* Clear stdcall ABI; deterministic reloc walk; strong synthetic oracle (Y pattern).
* Manageable leaf; no I/O/IRQ/mutex/LFB; fits current `TMP_STACK_TOP` headroom.
* Live production: `load_library` when usermode base ≠ preferred.

### Why alternatives lose

* `enable_irq` / `irq_eoi`: interrupt I/O without a QEMU-visible mask/EOI oracle.
* `unpack`: strongest FASM reduction but ~32KB `unpack.p` + LZMA is not one safe cut.
* `exFAT_find_lfn`: plugin island with stack callbacks + CF/EBP.
* `blit_32`: LFB hot path with cursor/win_map/bpp blast after CD already took geometry.
* `memmove` / `usb_td_to_virt`: blast or soak weakness.
* Thin / export-only / dead rejects fail the substance bar.

---

## Selected target

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: rebase_coff
Source: kernel/core/dll.inc
Subsystem: PE/COFF preferred-base DIR32 rebase (Stage 8 foothold)
Stage: Stage 8 (after Y/AT/BK/BU/CG)
Why selected:
    Post-CG audit: Path A rejected; enable_irq REJECT (oracle); unpack/blit/exFAT
    deferred on size/blast/island. Y-mutate ban stretch is stale after AT–CG.
    Strongest remaining evidence-quality Path B leaf is preferred-base DIR32 rebase.
Why this is a genuine migration boundary:
    Deterministic walk of COFF section reloc tables; Type 6 only; patch
    [VA+secVA+delta] += delta. Distinct from Cut Y (sym.Value / REL32) without
    claiming loader ownership.
Why Path A / Path B:
    Path B — one rebase leaf. load_library / page map / export wiring remain FASM.
Regression risks:
    REG-001/011 register preserve; Type-6-only filter; unused sym; patch address
    = VA+secVA+delta (not VA+secVA alone); stdcall ret 12 ownership;
    synthetic smoke fixture; nSections=0 FASM do-while quirk (document like Y).
Rollback: USE_RUST_REBASE_COFF = 0
```

---

## Legacy ABI (from FASM + callers — not the nominal signature alone)

```text
proc rebase_coff stdcall uses ebx esi, coff:dword, sym:dword, delta:dword
  locals: n_sec dd
  eax = coff
  ebx = zero-extend nSections
  n_sec = ebx
  esi = coff + 20          ; first COFF_SECTION
  edx = delta              ; held for entire walk
  .fix_sec:               ; entered before testing n_sec (do-while quirk)
    edi = coff + [esi].PtrReloc
    ecx = zero-extend [esi].NumReloc
    if ecx == 0 → .next
    .reloc_loop:
      if [edi].Type != 6 → .next_reloc
      .dir_32:
        eax = [edi].VirtualAddress + [esi].VirtualAddress
        add [eax + edx], edx     ; patch at (VA+secVA+delta); addend = delta
      .next_reloc:
        edi += 10
        ecx--
        jnz .reloc_loop
    .next:
      esi += sizeof.COFF_SECTION
      n_sec--
      jnz .fix_sec
  ret                      ; FASM stdcall cleans 12
  preserves: EBX, ESI (explicit uses); EDI/EBP not in uses but callers may need them
  clobbers: EAX, ECX, EDX (EDX holds delta; final value = delta)
  flags: not an ABI contract for callers
  stack: callee cleans 12 (ret 12)
  interrupt: untouched
  globals: none
  callbacks: none
  hidden: `sym` argument is UNUSED (passed by load_library but never read)
```

**Callers:**

* `load_library` in `dll.inc` ~1312–1315:
  `sub ebx, [esi+DLLDESCR.defaultbase]` / `jz @f` /
  `stdcall rebase_coff, [coff_hdr], [symbols_ptr], ebx`
* Production soak: usermode DLL/image load when base ≠ preferred (partial; smoke covers synthetic)

**Callees:** none.

---

## Rust ABI / trampoline

```text
rust_rebase_coff stdcall(coff, sym, delta) → void; ret 12
  sym accepted for ABI parity; ignored (matches FASM)
  nSections==0: clean no-op (while-guard; document FASM do-while quirk like Cut Y)

Trampoline (dll.inc under USE_RUST_REBASE_COFF=1):
  proc rebase_coff stdcall uses ebx ecx edx esi edi ebp, coff, sym, delta
    stdcall rust_rebase_coff, [coff], [sym], [delta]
    ret   ; FASM cleans caller 12; Rust already cleaned its 12
  endp
  Legacy FASM body retained under else.
```

---

## Oracle / host tests / smoke / QEMU

| Gate | Plan |
|------|------|
| Independent oracle | Separate FASM-flow mirror (`rebase_coff_oracle`) — not a copy of the Rust body |
| PRNG | Seed `0x52424346` (`'RBCF'`); **50,000** cases |
| Focused host tests | Type 6 patch / Type≠6 skip / multi-sec / delta=0 / unused-sym / empty NumReloc |
| Full suite | Run and record exact count |
| ABI smoke | Marker `'RBCF'`; synthetic COFF; canaries EBX/ESI/EDI/EBP; hang=`DEAD0C61` |
| QEMU | OFF baseline → ON → A/B → ON ×3; RESET watch; desktop reachability |
| Subsystem soak | Desktop PE/DLL load path (`load_library`); smoke covers off-preferred rebase math |
| Memory | Verify Stage-2 placement vs `TMP_STACK_TOP=0x008D800`; raise only if proven |

---

## Memory impact

Expect a small reloc-free blob (~150–250 B class, comparable to Cut Y 237 B). Cumulative high-water after CG was `0x8D4C3` against `TMP_STACK_TOP=0x008D800` (~829 B headroom). Re-measure after extract; change layout only with proof. Do not lower `TMP_STACK_TOP`.

---

## Docs to update on completion

* `cut-ch-plan.md` (this file → status complete)
* `cut-ch-implementation.md` (new)
* `migration-todo.md` (`rebase_coff` → `[x]`; 89/135)
* `migration-plan.md` (Cut CH entry; Stage 5/8 note)
* `regression-log.md` only if a live regression occurs
* `fixed-addresses.md` / `memory-model.md` only if `TMP_STACK_TOP` changes

**Stop after Cut CH. Do not start Cut CI.**
