# Cut CG Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-cg-implementation.md`](cut-cg-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CG** migrates PE export-directory name→VA lookup —
> `get_proc_ex` in `kernel/core/dll.inc`.  
> Cuts CC/CD/CE/CF remain complete and must not be modified. Do not start Cut CH.

---

## Fresh post-CF repository audit

### Baseline verification (2026-08-12)

| Check | Result |
|-------|--------|
| Inventory | **87 / 135** (`migration-todo.md`; 87 `[x]` + 48 `[ ]`) |
| Production gates | **87** in `project/build.toml`, **all `enabled = true`** |
| Cut CC | intact — `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY = 1` |
| Cut CD | intact — `USE_RUST_BLIT_CLIP = 1` |
| Cut CE | intact — `USE_RUST_SET_WINDOW_CLIENTBOX = 1` |
| Cut CF | intact — `USE_RUST_SET_MOUSE_DATA = 1`; blob SHA-256 `a2a4…0532` |
| `TMP_STACK_TOP` | **`0x008D800`** (`kernel/const.inc`; fixed-addresses + memory-model agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP` (do not lower) |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **EBX+ESI+EDI+EBP** on trampoline (callers / BU inject) |
| REG-003 | Smoke mutates live globals | Synthetic export fixture only; no live PE wipe |
| REG-009 | stdcall double cleanup | `stdcall rust_*` only; Rust `ret 8`; outer `proc` `ret` cleans caller — never `add esp,8` |
| REG-010 | Trampoline arg offset + return address | No naked `sub esp` ctx here; still verify stack math if added |
| REG-011 | PE export path lost EBX/ESI/EDI/EBP | Preserve all four even though FASM listed only `uses ebx esi` |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU (+ `get_proc_ex`) | Resolve/reloc leaves; `load_library` / export wiring / `rebase_coff` stay FASM — no Rust-owned loader |
| Video H+CD | Geometry only; `blit_32` / LFB still FASM |
| HID L+BE+CF | Leaves; PE mouse drivers still FASM |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR / STI-CLI still FASM |
| `unpack` | Single DLL decoder island, not subsystem ownership |
| FS / net / AHCI / Stage-4 | Prior footholds ≠ shared-state ownership |

Cut CG remains **Path B**.

---

## Special investigations (mandatory)

### `enable_irq` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/apic.inc` ~391–432 |
| Hardware | PIC `in`/`out` `0x21`/`0xA1` (clear mask bit) **or** IOAPIC MMIO via `IOAPIC_read`/`IOAPIC_write` (clear mask bit 16) |
| STI/CLI | **Does not** touch `EFLAGS.IF` — mask-only |
| Controllers | Selected by `[irq_mode]` (`IRQ_APIC` vs PIC) |
| Callers | 6 live (boot timer/PIC2/FPU, keyboard, `attach_int_handler`, BIOS disk) |
| Host oracle | Mockable (CA/CB style) but **QEMU cannot independently prove mask-bit correctness** in this harness |
| Verdict | **REJECT** — fails evidence bar; do not weaken for caller count |

### `irq_eoi` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `apic.inc` ~371–386 (`__fastcall`, `CL` = irq) |
| Hardware | PIC: `out 0xA0`/`0x20` with `AL=0x20`; APIC: store 0 to `[LAPIC_BASE+APIC_EOI]` |
| Callers | 4 (`sched`, `irq.inc` ×2, `v86`) |
| Oracle | Same I/O class — EOI ordering not soak-observable |
| Verdict | **DEFER** with `enable_irq` |

### `unpack` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + `unpack.p` (~32 KiB) |
| Boundary | `stdcall unpack(packed, unpacked); ret 8` — KPCK/LZMA + E8/E9 |
| Sub-cuts | Nested decoder locals share `unpack.*` globals — **no meaningful smaller public leaf** |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Verdict | **DEFER** — excellent oracle, disproportionate size/state/blast |

### `exFAT_find_lfn` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Contract | ESI path UTF-8; EDI direntry; CF + EAX; EBP=`exFAT*`; stack callbacks |
| Callers | 1 |
| Oracle / soak | Partial (`--disk exfat`); heavy plugin state |
| Verdict | **DEFER** — FS plugin island |

### `blit_32` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/video/blitter.inc` ~257–585 |
| vs CD | CD owns geometry; `blit_32` is LFB **pixel** hot path that *calls* `blit_clip` |
| ABI | Syscall 73; ECX→blit struct; EBX flags; win_map ownership; 32/24/16 bpp; soft/HW cursor |
| Oracle | Buffer-level oracle buildable but engineering-heavy; desktop A/B non-black is insufficient |
| Verdict | **DEFER** — natural CD follow-on, blast too high for one safe cut |

### Newly ranked: `get_proc_ex` — **SELECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/dll.inc` ~660–685 |
| Role | PE export-directory name→VA; Cut BU injects it as external resolve callback |
| Ban stretch | Historical PE deferral after Y+AT — **stale** at 87/135 with Y+AT+BK+BU complete |
| Oracle | Excellent — synthetic export directory + independent strncmp-class compare |
| Size | ~25-line leaf; reloc-free via inlined 256-byte name compare + hardcoded `OS_BASE` |
| Verdict | **SELECT** |

---

## Ranked candidates (48 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`get_proc_ex`** | PE export name→VA | 1 FASM + BU inject | desktop `.sys`/DLL | **Excellent** | Low–Med | **SELECT** |
| 2 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** | Defer — size/state |
| 3 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **High** | Defer — blast |
| 4 | `enable_irq` | PIC/APIC IRQ unmask | 6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 5 | `irq_eoi` | PIC/APIC EOI | 4 | desktop IRQ | Poor | Med–High | Defer with enable_irq |
| 6 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — FS island |
| 7 | `tcp_mss` / `ntfs_restore_usa_frs` / thin wrappers | thin | varies | — | Good | Low | Reject — substance bar |
| 8 | `strchr` / `strnlen` | export-only | 0 kernel | — | Good | Low | Reject — export-only |

### Why #1 wins

* Mandatory five yield no SELECT under the evidence bar.
* Completes the PE resolve foothold after BU without claiming Path A.
* Clear stdcall ABI; deterministic export-directory walk; strong synthetic oracle.
* Manageable leaf; no I/O/IRQ/mutex/LFB; composes Cut D strncmp inline (reloc-free).
* Live production: `fix_coff_symbols` → `get_proc_ex` on COFF external resolve; desktop driver/DLL load soak.

### Why alternatives lose

* `enable_irq` / `irq_eoi`: interrupt I/O without a QEMU-visible mask/EOI oracle.
* `unpack`: strongest FASM reduction but ~32KB `unpack.p` + LZMA is not one safe cut.
* `exFAT_find_lfn`: plugin island with stack callbacks + CF/EBP.
* `blit_32`: LFB hot path with cursor/win_map/bpp blast after CD already took geometry.
* Thin / export-only rejects fail the substance bar.

---

## Selected target

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: get_proc_ex
Source: kernel/core/dll.inc
Subsystem: PE/COFF export resolve (Stage 8 foothold)
Stage: Stage 8 (after Y/AT/BK/BU)
Why selected:
    Post-CF audit: Path A rejected; enable_irq REJECT (oracle); unpack/blit/exFAT
    deferred on size/blast/island. PE ban stretch is stale after BU. Strongest
    remaining evidence-quality Path B leaf is export name→VA lookup.
Why this is a genuine migration boundary:
    Deterministic walk of PE export Name/Function RVA tables with strncmp(…,256);
    returns OS_BASE+func RVA or 0. Distinct from Cut AT (COFF_SYM) and Cut BU
    (symbol table loop) without claiming loader ownership.
Why Path A / Path B:
    Path B — one resolve leaf. load_library / rebase / export wiring remain FASM.
Regression risks:
    REG-001/011 register preserve; OS_BASE RVA math; name-index≠ordinal quirk;
    stdcall ret 8 ownership; synthetic smoke fixture RVAs.
Rollback: USE_RUST_GET_PROC_EX = 0
```

---

## Legacy ABI (from FASM + callers — not the nominal signature alone)

```text
proc get_proc_ex stdcall uses ebx esi, proc_name:dword, imports:dword
  imports == 0 → EAX = 0; ret 8
  else:
    ebx = imports
    esi = 0
    loop:
      name_rva_table = [ebx+32]          ; AddressOfNames
      name_rva = [OS_BASE + name_rva_table + esi*4]
      name_ptr = OS_BASE + name_rva
      stdcall strncmp, name_ptr, proc_name, 256
      if EAX == 0 → hit
      esi++
      if esi < [ebx+24] (NumberOfNames) → loop
    miss → EAX = 0
    hit:
      func_rva_table = [ebx+28]          ; AddressOfFunctions
      func_rva = [OS_BASE + func_rva_table + esi*4]
      EAX = OS_BASE + func_rva
  preserves: EBX, ESI (explicit uses); EDI/EBP untouched by body
  clobbers: EAX; ECX/EDX via strncmp
  flags: ZF from last strncmp/test (not an ABI contract for callers)
  stack: callee cleans 8 (ret 8)
  interrupt: untouched
  globals: none (reads export tables via OS_BASE)
  callbacks: strncmp (Cut D; inline in Rust blob)
  hidden quirk: uses name index as AddressOfFunctions index
                (does NOT consult AddressOfNameOrdinals) — preserve exactly
```

**Callers:**

* FASM `fix_coff_symbols` body: `stdcall get_proc_ex, edi, [imports]`
* Rust Cut BU: trampoline injects `get_proc_ex` as 6th arg callback
* Production soak: COFF external resolve during `.sys`/DLL load

**Callees:** `strncmp` only.

---

## Rust ABI / trampoline

```text
rust_get_proc_ex stdcall(proc_name, imports) → EAX; ret 8
  OS_BASE = 0x80000000 hardcoded (const.inc; reloc-free)
  strncmp(…, 256) inlined (no cross-blob call)

Trampoline (dll.inc under USE_RUST_GET_PROC_EX=1):
  proc get_proc_ex stdcall uses ebx ecx edx esi edi ebp, proc_name, imports
    stdcall rust_get_proc_ex, [proc_name], [imports]
    ret   ; FASM cleans caller 8; Rust already cleaned its 8
  endp
  Legacy FASM body retained under else.
```

---

## Oracle / host tests / smoke / QEMU

| Gate | Plan |
|------|------|
| Independent oracle | Separate FASM-flow mirror (`get_proc_ex_oracle`) — not a copy of the Rust body |
| PRNG | Seed `0x47504558` (`'GPEX'`); **50,000** cases |
| Focused host tests | Hit / miss / imports=0 / multi-name / empty NumberOfNames / long-name NUL stop |
| Full suite | Run and record exact count |
| ABI smoke | Marker `'GPEX'`; synthetic export dir with RVAs=`addr-OS_BASE`; canaries EBX/ESI/EDI/EBP; hang=`DEAD0CG0` |
| QEMU | OFF baseline → ON → A/B → ON ×3; RESET watch; desktop reachability |
| Subsystem soak | Desktop load path exercising COFF external resolve (drivers / libraries) |
| Memory | Verify Stage-2 placement vs `TMP_STACK_TOP=0x008D800`; raise only if proven |

---

## Memory impact

Expect a small reloc-free blob (~100–300 B class). Cumulative high-water after CF was `0x8D243` against `TMP_STACK_TOP=0x008D800`. Re-measure after extract; change layout only with proof.

---

## Docs to update on completion

* `cut-cg-plan.md` (this file → status complete)
* `cut-cg-implementation.md` (new)
* `migration-todo.md` (`get_proc_ex` → `[x]`; 88/135)
* `migration-plan.md` (Cut CG entry; Stage 5/8 note)
* `regression-log.md` only if a live regression occurs

**Stop after Cut CG. Do not start Cut CH.**
