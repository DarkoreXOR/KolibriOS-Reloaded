# Cut CJ Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-cj-implementation.md`](cut-cj-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CJ** migrates forward memory move —
> `memmove` in `kernel/kernel.asm`.  
> Cuts CC/CD/CE/CF/CG/CH/CI remain complete and must not be modified. Do not start Cut CK.

---

## Fresh post-CI repository audit

### Baseline verification (2026-08-12)

| Check | Result |
|-------|--------|
| Inventory | **90 / 135** (`migration-todo.md`; 90 `[x]` + 45 `[ ]`) |
| Production gates | **90** `[[rust.migrations]]` with `enabled = true` (+ 1 non-prod Phase C-style block in the 91-count parse) |
| Unique `USE_RUST_*` gates | **90** |
| Cut CC | intact — `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY = 1` |
| Cut CD | intact — `USE_RUST_BLIT_CLIP = 1` |
| Cut CE | intact — `USE_RUST_SET_WINDOW_CLIENTBOX = 1` |
| Cut CF | intact — `USE_RUST_SET_MOUSE_DATA = 1` |
| Cut CG | intact — `USE_RUST_GET_PROC_EX = 1` |
| Cut CH | intact — `USE_RUST_REBASE_COFF = 1` |
| Cut CI | intact — `USE_RUST_USB_TD_TO_VIRT = 1`; blob **113 B / 0 reloc**; SHA-256 `132cfeb3b8e745dec17463e037cd48059f5f2b001f4b4cc68312d728823780e1` |
| `TMP_STACK_TOP` | **`0x008DC00`** (`kernel/const.inc`; `docs/compatibility/fixed-addresses.md` + `docs/architecture/memory-model.md` agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP`; CI end `.bss` @ `OS_BASE+0x8CA43` → needs `0x8DA43 < 0x8DC00` (~445 B headroom). **Do not lower.** |
| Docs vs tree | `migration-todo.md` / `migration-plan.md` / CI plan+impl agree with gates and blob |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **EAX/EBX/ECX/EDX/ESI/EDI/EBP** (legacy leaves all of them effectively intact) |
| REG-003 | Smoke mutates live globals | Synthetic buffers only — never touch `KEY_BUFF` / `msg_board_data` / live window boxes |
| REG-009 | stdcall double cleanup | Rust `ret 12`; trampoline is register-ABI outer — never `add esp` for Rust args |
| REG-010 | Trampoline arg offset | Account for every push before `stdcall rust_*` |
| REG-011 | PE/HID lost EBX/ESI/EDI/EBP | Keyboard/GUI/FS keep those live across `memmove` |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; `load_library` / page map stay FASM |
| AQ+BL+CI + `get_phys_addr` / `map_page` / `alloc_page` | Translate footholds ≠ paging or allocator ownership |
| Video H+CD + `blit_32` | Geometry only; LFB / win_map / cursor stay FASM |
| AH+AI + `exFAT_find_lfn` | Hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| D+BB+BF+BH + `strchr`/`strnlen` | Export/libc leaves ≠ string ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| `unpack` | Single decoder island, not subsystem ownership |
| `memmove` alone | Core util leaf — **not** memory-subsystem ownership |

Cut CJ remains **Path B**.

---

## Special investigations (mandatory)

### `unpack` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + heap `unpack.p` ≈ 31.2 KiB (`kernel.asm` alloc) |
| Boundary | `stdcall unpack(packed, unpacked); ret 8` — KPCK/LZMA + E8/E9; nested locals share `unpack.*` globals — **no smaller public leaf** |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Oracle | Excellent (bitstream + golden unpack) |
| Size / memory | Multi-KiB code blob + shared globals + mutex; heap state is clean but cut complexity is not |
| Verdict | **DEFER** — excellent oracle, disproportionate size/state for one safe cut |

### `blit_32` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/video/blitter.inc` ~257–585 |
| vs CD | CD owns geometry; `blit_32` is LFB **pixel** hot path that *calls* `blit_clip` |
| ABI | Syscall 73; ECX→blit struct; EBX flags; win_map; 32/24/16 bpp; soft/HW cursor |
| Oracle | Buffer-level oracle buildable but engineering-heavy; desktop non-black is **insufficient** |
| Verdict | **DEFER** — natural CD follow-on, LFB/cursor blast too high for one safe cut |

### `memmove` — **SELECT**

| Item | Finding |
|------|---------|
| Source | `kernel/kernel.asm` ~3423–3455 |
| Semantics | `EAX=from`, `EBX=to`, `ECX=nbytes`; **signed `ecx≤0` early out**; **forward-only** `rep movsd`/`movsb` (not C `memmove` reverse-overlap); **DF assumed clear** (no `cld`/`std`) |
| Callers | **~24** live across ~11 files (keymap, HID ring shift, GUI boxes/buttons, FAT/NTFS/exFAT sector copy, sys32/button list compact, msg board) — including intentional **forward-overlap** left-shifts (`KEY_BUFF+1→KEY_BUFF`, `msg_board_data+1→msg_board_data`, struct compact) |
| Oracle | **Excellent** — independent byte-level forward copy; cover non-overlap, forward-overlap, identical, zero/negative length, alignment, large sizes |
| Soak | Strongest remaining: every desktop boot + HID + GUI + FS paths hit it |
| Blast | Highest caller fan-out among pending — **but** blast is breadth of callers, not semantic complexity (~30 FASM lines; tiny blob) |
| Verdict | **SELECT** — see blast-radius justification below |

### `exFAT_find_lfn` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Contract | ESI path UTF-8; EDI direntry; CF + EAX; EBP=`exFAT*`; stack callbacks; calls unmigrated `exFAT_get_name` |
| Callers | 1 |
| Oracle / soak | Partial (`--disk exfat` attach ≠ full LFN-walk differential) |
| Verdict | **DEFER** — FS plugin island |

### `mutex_init` — **REJECT**

| Item | Finding |
|------|---------|
| Source | `kernel/core/sync.inc` ~17–21 |
| ABI | `__fastcall` ECX→MUTEX*; 3 stores |
| Callers | ~35 + PE `MutexInit` |
| Verdict | **REJECT** — thin + extreme fan-out; near-zero migration value |

### Overlooked leaves inspected

| Symbol | Verdict |
|--------|---------|
| `get_phys_addr` | **REJECT** — ~9-line AQ offset glue; 0 in-kernel callers; CI already rejected as thin |
| `strchr` / `strnlen` | **REJECT** — PE export only; 0 in-kernel callers |
| `tcp_mss` | **REJECT** — thin clamp+store |
| `ntfs_restore_usa_frs` | **REJECT** — 3-line fallthrough to Cut J |
| `pid_to_appdata` | **REJECT** — dead (commented caller) |
| `net_ptr_to_num` / `sysfn_*` façades | **REJECT** — wrapper / façade |
| `enable_irq` / `irq_eoi` | **REJECT/DEFER** — I/O oracle class |
| `alloc_page` / `map_page` / `disk_scan_*` / `*_SetFileInfo` / `tcp_output` / `drawChar` | **DEFER** — Stage 4–7 / orchestration / islands |

---

## Ranked candidates (45 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`memmove`** | forward memory move | ~24 / 11 files | **strong** desktop+HID+GUI+FS | **Excellent** | High fan-out / low algo risk | **SELECT** |
| 2 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** size/state | Defer — size/state |
| 3 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **Very high** | Defer — LFB blast |
| 4 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — island |
| 5 | `mutex_init` | sync init | ~35 | everywhere | Perfect | Med | Reject — thin+fanout |
| 6 | `get_phys_addr` | VA→PA+offset | PE only | weak USB | Excellent | Low | Reject — thin glue |
| 7 | `enable_irq` / `irq_eoi` | PIC/APIC | 4–6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 8 | export-only / dead / Stage late | varies | — | — | — | — | Reject / defer |

### Why #1 wins (blast-radius justification)

* Path A rejected; PE Stage-8 and USB TD walk exhausted at CH/CI.
* Mandatory five: unpack/blit/exFAT fail size/LFB/island; mutex fails substance; **memmove** is the only remaining high-evidence Path B with a **strong production soak**.
* Fan-out is high, but semantics are tiny and fully oracle-coverable (byte-identical forward copy). Risk class is ABI/register/DF — not algorithmic ambiguity (contrast `blit_32` / `unpack`).
* Prior deferrals correctly waited until lower-blast high-quality leaves were gone; that condition is now met.
* Intentional forward-overlap callers (keyboard ring, msg board, button/sys32 compact) make preserving **non-C** forward-only behavior mandatory — the oracle must encode that quirk.
* Clear register ABI; tiny reloc-free blob; rollback = gate off.

### Why alternatives lose

* `unpack`: strongest FASM reduction but ~32KB `unpack.p` + LZMA is not one safe cut.
* `blit_32`: LFB hot path with cursor/win_map/bpp blast after CD already took geometry.
* `exFAT_find_lfn`: plugin island with stack callbacks + unmigrated `exFAT_get_name`.
* `mutex_init` / `get_phys_addr` / export-only / I/O-oracle: fail substance or evidence bar.

---

## Selected target

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: memmove
Source: kernel/kernel.asm
Subsystem: core memory move (forward-only; Stage-2 util)
Stage: Stage 2 / core util (post-CI inventory)
Why selected:
    Post-CI audit: Path A rejected; unpack/blit/exFAT/mutex/get_phys fail
    size/LFB/island/thin bars. Strongest remaining evidence-quality Path B
    leaf with strong production soak is forward memmove. Blast is caller
    breadth, not semantic complexity — justified after leaf exhaustion.
Why this is a genuine migration boundary:
    Deterministic forward byte copy with signed ≤0 early-out; used as the
    kernel's shared buffer-shift/copy primitive. Distinct from allocator /
    paging ownership (Path B only).
Why Path A / Path B:
    Path B — one util leaf. Heap / page tables / alloc stay FASM.
Regression risks:
    REG-001/011 register preserve across Rust stdcall; forward-overlap
    left-shifts (KEY_BUFF, msg_board, button compact); signed ecx≤0;
    DF assumed clear; never double-clean stdcall; synthetic smoke only.
Rollback: USE_RUST_MEMMOVE = 0
```

---

## Legacy ABI (from FASM + call sites — not the nominal signature alone)

```text
memmove  (register ABI; not stdcall; not C memmove)
  in:  EAX = from (source)
       EBX = to   (destination)
       ECX = nbytes (signed: ecx≤0 → no-op return)
  out: EAX = from (unchanged)
       EBX = to   (unchanged)
       ECX = nbytes (restored from push)
       ESI/EDI restored
  body:
    test ecx, ecx / jle .ret
    push esi edi ecx
    edi=ebx; esi=eax
    if (ecx & ~3) != 0:
      push ecx; shr ecx,2; rep movsd; pop ecx; and ecx,3; jz .finish
    rep movsb
    .finish: pop ecx edi esi
  DF: assumed clear (no cld/std in leaf)
  flags: clobbered (not an observable contract for callers)
  preserves: EAX, EBX, ECX, EDX (untouched), ESI, EDI, EBP (untouched)
  overlap: ALWAYS forward (dest<src left-shift OK; dest>src overlap is
           historically unsafe — callers that overlap use dest < src)
  callbacks / globals: none
```

---

## Rust ABI / trampoline

```text
rust_memmove stdcall(from, to, nbytes); ret 12
  - freestanding; reloc-free section .text.rust_memmove
  - forward byte copy; nbytes as i32 ≤0 → no-op
  - no DF dependency in Rust (explicit indexed/ptr copy)

Trampoline (plain label):
  push edx esi edi ebp          ; REG-001/011 (plus leave EAX/EBX/ECX restore)
  push eax ebx ecx              ; save inputs / restore set
  stdcall rust_memmove, eax, ebx, ecx
  pop ecx ebx eax
  pop ebp edi esi edx
  ret
  ; NEVER add esp for Rust args (REG-009)
```

---

## Oracle / host tests

| Item | Plan |
|------|------|
| Oracle | Independent FASM-flow forward copy (`fasm_oracle_memmove`) — not a call to Rust |
| PRNG | seed `0x4D4D4F56` (`'MMOV'`), **50,000** cases |
| Cases | non-overlap; forward overlap (dest < src); identical src/dst; zero length; negative length (as u32 high bit); 1-byte; odd sizes; aligned/unaligned; large (≥256); boundary-adjacent within a buffer; dword-tail paths |
| Compare | full destination region bytes; source untouched except overlap zone; no register contract on host |
| Focused tests | `mmov_*` |
| Full suite | record exact pass count |

---

## ABI smoke

| Item | Plan |
|------|------|
| When | Early smoke cluster (with string smokes) — synthetic buffer only |
| Marker | success `'MMOV'` (`0x4D4D4F56`); hang `DEAD0C6A` |
| Cover | direct `rust_*` + public trampoline; non-overlap copy; forward-overlap shift; zero length; EAX/EBX/ECX/EDX/ESI/EDI/EBP canaries; buffer canaries past end |
| Never | mutate live `KEY_BUFF` / window boxes / FS buffers |

---

## QEMU validation

| Step | Gate | Expect |
|------|------|--------|
| OFF baseline | `USE_RUST_MEMMOVE=0` | desktop + `query-status: running` + resets=0 |
| ON | `=1` | same |
| A/B | OFF vs ON non-black | identical |
| ON ×3 | consecutive | each resets=0 |
| Subsystem soak | desktop + HID keyboard activity path + GUI window move path (exercise ring/box memmoves); optional `--disk exfat` or FAT browse if available | document actual paths run |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Non-black alone is never success — require no RESET, no hang, desktop reachability.

---

## Memory impact

Expect small reloc-free blob (tens–low hundreds of bytes). Smoke uses stack synthetic buffer (no large `.bss`).  
**Measured:** end `.bss` @ `OS_BASE+0x8CC83` → need `0x8DC83 < TMP_STACK_TOP`.  
`0x8DC00` failed by **0x83**. Raised `TMP_STACK_TOP` `0x008DC00` → `0x008DD00` (+256 B). Gap to `sys_proc` @ `0x8E000` remains (`0x300`). Do not lower.

---

## Docs to update on completion

* `cut-cj-plan.md` (this file → status complete)
* `cut-cj-implementation.md` (new)
* `migration-todo.md` → 91/135
* `migration-plan.md` → Cut CJ entry
* `fixed-addresses.md` / `memory-model.md` — **updated** for `TMP_STACK_TOP`
* `regression-log.md` only if a live regression occurs

**Cut CJ complete. Stop. Do not start Cut CK.**
