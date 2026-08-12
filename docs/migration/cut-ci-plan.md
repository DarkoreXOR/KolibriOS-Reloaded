# Cut CI Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-ci-implementation.md`](cut-ci-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CI** migrates USB TD phys→virt —
> `usb_td_to_virt` in `kernel/bus/usb/memory.inc`.  
> Cuts CC/CD/CE/CF/CG/CH remain complete and must not be modified. Do not start Cut CJ.

---

## Fresh post-CH repository audit

### Baseline verification (2026-08-12)

| Check | Result |
|-------|--------|
| Inventory | **89 / 135** (`migration-todo.md`; 89 `[x]` + 46 `[ ]`) |
| Production gates | **89** in `project/build.toml`, **all `enabled = true`** (90 blocks: 89 prod + 1 non-prod) |
| Unique `USE_RUST_*` gates | **89** |
| Cut CC | intact — `USE_RUST_PROCESS_PARTITION_TABLE_ENTRY = 1` |
| Cut CD | intact — `USE_RUST_BLIT_CLIP = 1` |
| Cut CE | intact — `USE_RUST_SET_WINDOW_CLIENTBOX = 1` |
| Cut CF | intact — `USE_RUST_SET_MOUSE_DATA = 1` |
| Cut CG | intact — `USE_RUST_GET_PROC_EX = 1` |
| Cut CH | intact — `USE_RUST_REBASE_COFF = 1`; blob 203 B / 0 reloc; SHA-256 `4aa5ae57…6c04` |
| `TMP_STACK_TOP` | **`0x008D800`** (`kernel/const.inc`; fixed-addresses + memory-model agree) |
| Early-stack assert | `data32.inc`: `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP`; end `.bss` @ `OS_BASE+0x8C783` → needs `0x8D783 < 0x8D800` (~125 B headroom; do not lower) |

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | EDX/ECX clobber across Rust stdcall | Preserve **EBX+EDX+ESI+EDI+EBP**; **ECX is an OUT** on hit (reconstruct, do not blindly restore) |
| REG-003 | Smoke mutates live globals | Synthetic page chain via `kernel_alloc`; never rewrite live TD slabs / `page_tabs` |
| REG-009 | stdcall double cleanup | Rust `ret 16`; trampoline is register-ABI outer — never `add esp` for Rust args |
| REG-010 | Trampoline arg offset | Account for every push before `stdcall rust_*` |
| REG-011 | PE path lost EBX/ESI/EDI/EBP | HC PE drivers may keep those live across `USBHCFunc` slot call |

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; `load_library` / page map stay FASM |
| AQ+BL + `usb_td_to_virt` / `get_phys_addr` | Translate footholds ≠ paging or USB HC ownership |
| Video H+CD + `blit_32` | Geometry only; LFB / win_map / cursor stay FASM |
| AH+AI + `exFAT_find_lfn` | Hash helpers ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| `unpack` | Single decoder island, not subsystem ownership |

Cut CI remains **Path B**.

---

## Special investigations (mandatory)

### `unpack` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/unpacker.inc` ~16–519 + runtime `unpack.p` (~31.2 KiB via `kernel_alloc`) |
| Boundary | `stdcall unpack(packed, unpacked); ret 8` — KPCK/LZMA + E8/E9 |
| Sub-cuts | Nested decoder locals share `unpack.*` globals — **no meaningful smaller public leaf** |
| Callers | 2 (`dll.inc`) under `unpack_mutex` |
| Oracle | Excellent (bitstream + golden unpack) |
| Size / memory | Full LZMA → multi-KiB blob; Stage-2 `.bss` headroom after CH is only ~125 B |
| Verdict | **DEFER** — excellent oracle, disproportionate size/state/blast for one safe cut |

### `blit_32` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/video/blitter.inc` ~257–585 |
| vs CD | CD owns geometry; `blit_32` is LFB **pixel** hot path that *calls* `blit_clip` |
| ABI | Syscall 73; ECX→blit struct; EBX flags; win_map; 32/24/16 bpp; soft/HW cursor |
| Oracle | Buffer-level oracle buildable but engineering-heavy; desktop A/B non-black is **insufficient** |
| Verdict | **DEFER** — natural CD follow-on, blast too high for one safe cut |

### `memmove` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel.asm` ~3419–3451 |
| ABI | `EAX=from`, `EBX=to`, `ECX=nbytes`; **forward-only** `rep movsd`/`movsb` (no C-style overlap reverse) |
| Callers | **~24** live (`kernel.asm`, HID, GUI, FAT, NTFS, exFAT, …) |
| Oracle | Good (bytes + overlap quirk documentation) |
| Blast | **Highest** among pending — system-wide |
| Verdict | **DEFER** — easy differential ≠ acceptable production blast at this stage |

### `usb_td_to_virt` — **SELECT**

| Item | Finding |
|------|---------|
| Source | `kernel/bus/usb/memory.inc` ~20–42 (~22 lines) |
| Role | Walk TD page linked list; match TD phys to page; return linear TD |
| Callee | `get_pg_addr` (Cut AQ — already Rust) |
| Wiring | `usb_hc_func[7]` / PE export `USBHCFunc` (not direct FASM `call`) |
| Globals / callbacks | None in leaf |
| Oracle | **Excellent** — synthetic page chain + PA match/miss/offset (independent of USB HW) |
| Soak | QEMU has no dedicated `--usb` script flag; default PC UHCI often present; HC PE drivers may load. Document soak honestly (BH/CF class). Ban stretch after PE Stage-8 leaf exhaustion (parallel to CH’s Y-mutate stretch). |
| Size | Small reloc-free blob; stack/heap smoke (no large `.bss`) |
| Verdict | **SELECT** — coherent leaf, strong independent evidence, low fan-out, manageable blast |

### `exFAT_find_lfn` — **DEFER**

| Item | Finding |
|------|---------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Contract | ESI path UTF-8; EDI direntry; CF + EAX; EBP=`exFAT*`; stack callbacks |
| Callers | 1 |
| Oracle / soak | Partial (`--disk exfat` attach ≠ full LFN-walk differential) |
| Verdict | **DEFER** — FS plugin island |

### Overlooked leaves inspected

| Symbol | Verdict |
|--------|---------|
| `strchr` / `strnlen` | **REJECT** — PE export only; 0 in-kernel callers |
| `tcp_mss` | **REJECT** — thin clamp+store |
| `ntfs_restore_usa_frs` | **REJECT** — 3-line fallthrough to Cut J |
| `pid_to_appdata` | **REJECT** — dead (commented caller) |
| `get_phys_addr` | **REJECT** — thin AQ offset glue; 0 in-kernel callers |
| `mutex_init` | **REJECT** — thin + high fan-out |
| `net_ptr_to_num` / `sysfn_getfreemem` | **REJECT** — wrapper / façade |
| `enable_irq` / `irq_eoi` | **REJECT/DEFER** — I/O oracle class |

---

## Ranked candidates (46 remaining)

| Rank | Candidate | Semantic class | Callers | Soak | Oracle | Risk | Verdict |
| ---- | --------- | -------------- | ------- | ---- | ------ | ---- | ------- |
| **1** | **`usb_td_to_virt`** | USB TD phys→virt | HC table / PE | weak→partial USB | **Excellent** | Low–Med | **SELECT** |
| 2 | `unpack` | KPCK/LZMA + E8/E9 | 2 DLL | desktop DLL | Excellent | **High** | Defer — size/state |
| 3 | `memmove` | memory move | ~24 | everywhere | Good | High | Defer — blast |
| 4 | `blit_32` | LFB blit hot path | 1 (fn73) | desktop GUI | Hard | **High** | Defer — blast |
| 5 | `exFAT_find_lfn` | exFAT LFN walk | 1 | `--disk exfat` | partial | High | Defer — island |
| 6 | `mutex_init` | sync init | ~35 | everywhere | Perfect | Med | Reject — thin+fanout |
| 7 | `enable_irq` / `irq_eoi` | PIC/APIC | 4–6 | desktop IRQ | Poor (I/O) | Med–High | **REJECT** — oracle |
| 8 | thin / export-only / dead | varies | — | — | — | Low | Reject — substance bar |

### Why #1 wins

* Mandatory five: unpack/blit/memmove/exFAT fail size/LFB/blast/island; `usb_td_to_virt` clears coherent-boundary + independent-oracle preference.
* PE Stage-8 leaf set exhausted after CH — USB AQ-compose is the strongest remaining evidence-quality Path B leaf.
* Prior “USB weak soak” ban is **stale** after PE exhaustion (same stretch pattern as CH’s Y-mutate ban).
* Composes Cut AQ without claiming USB HC ownership (Path B).
* Clear register ABI; deterministic walk; strong synthetic oracle; small blob; rollback simple.
* Smoke uses heap-allocated pages (post-`init_kernel_heap`) — no `.bss` pressure on ~125 B headroom.

### Why alternatives lose

* `unpack`: strongest FASM reduction but ~32KB `unpack.p` + LZMA is not one safe cut.
* `blit_32`: LFB hot path with cursor/win_map/bpp blast after CD already took geometry.
* `memmove`: ~24-caller forward-only blast; host differential alone is insufficient.
* `exFAT_find_lfn`: plugin island with stack callbacks + CF/EBP.
* Thin / export-only / dead / I/O-oracle rejects fail the substance or evidence bar.

---

## Selected target

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: usb_td_to_virt
Source: kernel/bus/usb/memory.inc
Subsystem: USB HC TD phys→virt (Stage-4/USB foothold; AQ compose)
Stage: Stage 4 / USB (after AQ)
Why selected:
    Post-CH audit: Path A rejected; PE leaves exhausted; famous five lose on
    size/LFB/blast/island. USB weak-soak ban stretched after PE exhaustion.
    Strongest remaining evidence-quality Path B leaf is TD phys→virt walk.
Why this is a genuine migration boundary:
    Deterministic linked-page walk + get_pg_addr match; returns linear TD
    or 0. Distinct from get_phys_addr (offset+AQ) without claiming HC ownership.
Why Path A / Path B:
    Path B — one translate leaf. USB thread / HC PE drivers / pipe model remain FASM.
Regression risks:
    REG-001 ECX OUT on hit; page match unsigned window; next ptr at +0xFFC;
    trampoline injects page_tabs+OS_BASE; synthetic smoke only; weak USB soak honesty.
Rollback: USE_RUST_USB_TD_TO_VIRT = 0
```

---

## Legacy ABI (from FASM + call sites — not the nominal signature alone)

```text
usb_td_to_virt  (register ABI; not stdcall)
  in:  EAX = virt address of first TD page (page-aligned list head)
       ECX = TD physical address
  out: EAX = linear TD address, or 0 if not found
       ECX = (TD phys & 0xFFF) on hit; UNCHANGED on miss/empty
  walk:
    while EAX != 0:
      push EAX
      call get_pg_addr          ; EAX := phys page of that virt
      sub  EAX, ECX             ; page_phys - td_phys
      jz   .found
      cmp  EAX, -0x1000
      ja   .found               ; unsigned: td_phys in [page_phys, page_phys+0xFFF]
      pop  EAX
      mov  EAX, [EAX+0x1000-4]  ; next page virt
    .found:
      pop  EAX                  ; virt page
      and  ECX, 0xFFF
      add  EAX, ECX
  preserves: EBX, EDX, ESI, EDI, EBP (body + get_pg_addr leave them)
  clobbers: EAX (ret); ECX on hit only; flags
  DF / IF: untouched
  stack: no args; plain `ret`
  globals: none
  callbacks: none (calls get_pg_addr only)
  interrupt: may run from HC PE driver context — keep leaf non-blocking
```

---

## Rust ABI / trampoline

```text
rust_usb_td_to_virt stdcall(first, td_phys, page_tabs, os_base) -> EAX; ret 16
  inlines Cut AQ get_pg_addr(page_tabs, os_base) — reloc-free
  returns EAX only; trampoline reconstructs ECX:
    save td_phys; call rust_*; restore ecx=td_phys; if eax!=0: and ecx,0xFFF

Trampoline (public usb_td_to_virt):
  push ebx, edx, esi, edi, ebp
  push ecx                      ; save td_phys (REG-010: account for this)
  stdcall rust_usb_td_to_virt, eax, ecx, page_tabs, OS_BASE
  pop ecx
  test eax, eax
  jz @f
  and ecx, 0xFFF
@@:
  pop ebp, edi, esi, edx, ebx
  ret
```

Gate: `USE_RUST_USB_TD_TO_VIRT` in `kernel/bus/usb/memory.inc` (legacy body under `else`).

---

## Oracle / host tests

| Item | Plan |
|------|------|
| Oracle | Independent FASM-flow walk (`usb_td_to_virt_oracle`) — not a call to Rust |
| PRNG | seed `0x55544456` (`'UTDV'`), 50,000 cases |
| Cases | empty head; single-page hit/miss; multi-page hit on 2nd/3rd; offset 0 / mid / 0xFFF; boundary `page_phys+0x1000` miss; next-ptr terminate |
| Compare | EAX result; ECX semantics (hit offset vs miss preserve) |
| Focused tests | `utdv_*` |
| Full suite | record exact pass count |

---

## ABI smoke

| Item | Plan |
|------|------|
| When | After `init_kernel_heap` + RING0 stack (like Cut Y/X) — `kernel_alloc 0x2000` for two linked pages |
| Marker | success `'UTDV'` (`0x55544456`); hang `DEAD0C62` |
| Cover | direct `rust_*` + public trampoline; hit / miss / offset; EBX/EDX/ESI/EDI/EBP canaries; ECX OUT on hit |
| Never | mutate live USB TD slabs or `page_tabs` |

---

## QEMU validation

| Step | Gate | Expect |
|------|------|--------|
| OFF baseline | `USE_RUST_USB_TD_TO_VIRT=0` | desktop + `query-status: running` + resets=0 |
| ON | `=1` | same |
| A/B | OFF vs ON non-black | identical |
| ON ×3 | consecutive | each resets=0 |
| Subsystem soak | desktop + USB HC path if present | document partial if no HCI traffic; ABI smoke is primary proof |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Non-black alone is never success — require no RESET, no hang, desktop reachability.

---

## Memory impact

Blob **113 B** reloc-free. Smoke uses `kernel_alloc` + stack locals.  
**Measured:** end `.bss` @ `OS_BASE+0x8CA43` → need `0x8DA43 < TMP_STACK_TOP`.  
Raised `TMP_STACK_TOP` `0x008D800` → `0x008DC00` (+1 KiB). Gap to `sys_proc` @ `0x8E000` remains (`0x400`). Do not lower.

---

## Docs to update on completion

* `cut-ci-plan.md` (this file → status complete)
* `cut-ci-implementation.md` (new)
* `migration-todo.md` → 90/135
* `migration-plan.md` → Cut CI entry
* `fixed-addresses.md` / `memory-model.md` — **updated** for `TMP_STACK_TOP`
* `regression-log.md` only if a live regression occurs

**Cut CI complete. Stop. Do not start Cut CJ.**
