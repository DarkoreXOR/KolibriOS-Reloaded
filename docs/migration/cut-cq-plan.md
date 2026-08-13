# Cut CQ Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-cq-implementation.md`](cut-cq-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut CQ** migrates exFAT UTF-8 path-component lookup —
> `exFAT_find_lfn` in `kernel/fs/exfat.inc`.  
> Cut CP remains complete and must not be modified. Do not start Cut CR.

---

## Fresh post-CP repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **97 / 135** (`migration-todo.md`; 97 `[x]` + 38 `[ ]`) |
| Production gates | **97** `[[rust.migrations]]` with `enabled = true` |
| Cut CP | **complete** — `blit_32` SHA `68e4f58b74effcbb35f3745baf24bb48c08057435ce614f3add9a816ab77ce72` (**2292 B / 0 reloc**) |
| `USE_RUST_BLIT_32` | **1** |
| REG-016 | **Fixed** — not reopened |
| Final CP image | `dev_build/test/kernel-20260813-130209.img` |
| Final CO image | `dev_build/test/kernel-20260813-121344.img` (779380 / `resets=0`) |
| `TMP_STACK_TOP` | **`0x008E000`** |
| `sys_proc` | **`0x008E000`** |
| `SLOT_BASE` | **`0x0090000`** (REG-012 pack) |
| End `.bss` (CP ON) | **`OS_BASE+0x8B503`** |
| Early-stack assert | `0x8C503 < 0x8E000` → **~7.2 KiB** headroom |

**Do not restore stale `0x008F000` / `SLOT_BASE=0x91000`.** Do not raise `TMP_STACK_TOP` speculatively. Do not reopen REG-016 without a new reproducible failure. Do not modify Cut CP.

### Path A decision: **REJECTED**

| Cluster | Why not Path A |
|---------|----------------|
| PE Y+AT+BK+BU+CG+CH | Leaf set exhausted; loader orchestration stays FASM |
| AQ+BL+CI + paging/alloc | Translate footholds ≠ paging/allocator ownership |
| Video H+CD+CP | Clip + syscall-73 blit leaves; LFB / win_map / cursor policy still FASM-owned |
| AH+AI+CL + `exFAT_find_lfn` | Hash/sector helpers + one lookup leaf ≠ plugin ownership |
| AS/AY + socket siblings | Mutex/list lifecycle still FASM |
| D+BB+BF+BH+CN + `strnlen` | Export/libc leaves ≠ string ownership |
| L+BE+CF HID | Policy leaves ≠ HID ownership |
| IRQ enable/eoi | Mask/EOI leaves; PIC init / ISR still FASM |
| AL+BR+BS+CM EXT | Time pack + one address leaf ≠ EXT FS ownership |
| CO `unpack` | Decoder island — **done**; still not subsystem ownership |
| GUI S+CE + `drawChar` | Geometry/clientbox footholds ≠ GUI server ownership |

Cut CQ remains **Path B**. Video Rust leaves (`blit_clip`, `blit_32`) do not establish a Rust-owned video subsystem.

---

## Complete candidate ranking (38 pending)

| Rank | Symbol | Oracle | Memory | Soak | Verdict |
|------|--------|--------|--------|------|---------|
| **1** | **`exFAT_find_lfn`** | **4/5** independent lookup + LFN-entry fixture (not `--disk` alone) | modest blob; 524 B **stack** LFN; ~7.2 KiB `.bss` headroom | `--disk exfat` live lookup | **SELECT** |
| 2 | `drawChar` | 4/5 exact pixels | KiB + smoothing | GUI text | Defer — Stage 7; hidden `dtext` stack-frame ABI (`[esp+20+deltaToScreen]`); `syscall_getpixel` |
| 3 | remaining video (`move_cursor_*`, `restore_*`, …) | varies | KiB | cursor/LFB | Reject — **not in the 38**; would duplicate CP pixel class without inventory slot |
| 4 | `strnlen` | 5/5 | ~20 B | PE export; `_strnlen` in taskman is a **private copy** | Reject — thin export-only after CN/BB |
| 5 | `tcp_mss` | 5/5 | ~20 B | 1 TCP caller | Reject — thin clamp+store / TCP deepen |
| 6 | `mutex_init` | 5/5 | ~20 B | ~33 callers | Reject — thin + fan-out |
| 7 | `enable_irq` / `irq_eoi` | 1/5 | ~80 B | IRQ path | Reject — no deterministic mask/EOI oracle |
| 8 | `mem_test` | 1/5 | ~60 B | boot-only; **skipped when E820 present** (QEMU always has E820) | Reject — CR0/cache/`wbinvd` probe; no hardware-independent oracle |
| 9 | `get_phys_addr` / `net_ptr_to_num` / `ntfs_restore_usa_frs` / `sysfn_*` / `pid_to_appdata` / `socket_check_owner` / `socket_ptr_to_num` | 5/5 | tiny | weak/dead | Reject — thin / wrapper / façade / dead |
| … | remaining Stage 4–7 orchestration | varies | High | deferred/ban/unsuitable | unchanged |

### Special scrutiny outcomes

| Target | Outcome |
|--------|---------|
| `exFAT_find_lfn` | **SELECT.** 1 live caller (`exFAT_hd_find_lfn`). UTF-8 path component → UTF-16-upper + Cut AI NameHash + directory walk via stack callbacks + name compare with `/` continuation. LFN **entry** state machine (0x85/0xC0/0xC1, hash skip, deleted, type-0) lives in unmigrated `exFAT_get_name` (also used by `exFAT_ReadFolder`) — injected, not merged. Independent host oracle covers lookup + a FASM-faithful get_name **fixture**. Live `--disk exfat` soaks the composition. Not Path A. Plugin architecture is not a rejection: the leaf has independent semantic substance. |
| `drawChar` | ~540 lines; smoothing; Cut N `antiAliasing`; `syscall_getpixel`; **reads `dtext` stack locals**. Stage 7. Exact-pixel oracle is possible but blast + hidden ABI outrank CQ. |
| Remaining video | Cursor restore/move / VGA — not in the scoped 38; hardware cursor + LFB coupling; would be a trivial video sequence after CP. |
| IRQ | PIC `in`/`out` + APIC MMIO. Desktop reachability is not a mask/EOI oracle. |
| `strnlen` | Public `proc strnlen` is PE-export only. `taskman.inc` `_strnlen` is a distinct private leaf. Thin `repne scasb`. |
| `mem_test` | Early-out when `BOOT_LO.memmap_block_cnt != 0`. Remainder: CR0_CD + `wbinvd` + 1 MiB store probe. QEMU E820 skips the body. |
| Thin/wrappers | `tcp_mss`, `mutex_init`, `get_phys_addr`, `net_ptr_to_num`, `ntfs_restore_usa_frs`, `sysfn_getfreemem`, `sysfn_mouse_acceleration`, `pid_to_appdata`, `socket_check_owner` rejected. |

### Selection rationale

Post-CP, syscall-73 blit is gone. Remaining named leaves are thin, IRQ-without-oracle, Stage 4–7 orchestration, Stage-7 `drawChar` (stack-coupled), or this exFAT lookup. **`exFAT_find_lfn`** is the AH+AI+CL follow-on with a **mandatory independent lookup oracle** (not desktop non-black), live `--disk exfat` production path, reloc-free ctx + callback injection, and stack LFN (not `.bss`). Evidence quality (path/hash/compare + LFN-entry fixture) outranks `drawChar` Stage-7 blast and thin-leaf count inflation.

---

## Selected target: `exFAT_find_lfn`

| Field | Value |
|-------|-------|
| Source | `kernel/fs/exfat.inc` ~859–1003 |
| Subsystem | fs/exFAT path lookup (Stage-2 leaf; AH+AI+CL compose) |
| Stage | Stage 2 / Stage 5 FS plugin foothold |
| Path | **B** |
| Callers | **1** live (`exFAT_hd_find_lfn`) |
| Callees | `utf8to16` (AB, inlined); `utf16toUpper` (C, inlined); `exFAT_hash_calculate` (AI, inlined); `exFAT_get_name` (injected); stack `first`/`next` (injected) |
| Globals | `ebp` → `exFAT*` fields — **injected pointers** |
| Callbacks | `first` / `next` (plain `call`; EAX=pair, EBP=fs, EDI in/out, CF); `get_name` (EDI=entry, ESI=LFN cursor, EBP=fs, CF) |

### Legacy ABI

```text
exFAT_find_lfn  (plain `call` / `ret`, not stdcall)
  in:  ESI → UTF-8 path
       EBP → exFAT*
       [esp+4]  = next  (exFAT_notroot_next)
       [esp+8]  = first (exFAT_notroot_first)
       [esp+12] = cluster / sector pair (EAX for first/next)
  out: CF=0, EAX=0, ESI→next path component, EDI→direntry
       CF=1, EAX=error (5 = ERROR_FILE_NOT_FOUND, or callback EAX)
  preserves: EBX, EBP
  clobbers: EAX, ECX, EDX, ESI, EDI, flags
  DF: unchanged (no cld; get_name `movsd` assumes DF=0)
  stack: 262*2 = 524 B LFN buffer; caller owns callback slots (ret 0)
```

### Rust ABI

```text
stdcall rust_exfat_find_lfn(ctx); ret 4
  ctx = trampoline-built ExFatFindLfnCtx (52 bytes)
  EAX = 0 success / error code (trampoline test → clc/stc)
  ctx.esi_out / ctx.edi_out written before return
```

Trampoline: snapshot ESI/EBP and `[esp+4]`/`[esp+8]`/`lea [esp+12]` **before** stdcall (REG-010); preserve EBX/EBP; **no** `add esp` for Rust args (REG-009). `add esp, 52` is **local ctx only**. No `cld`.

### Oracle

| Item | Value |
|------|-------|
| Independent | Standard UTF-8 path split + independent upper + independent NameHash + independent 0x85/0xC0/0xC1 get_name fixture; production uses Cut AB `utf8to16` + `sub eax,32/80` upper + Cut AI hash |
| PRNG seed | `0x464C464E` (`'FLFN'`) |
| PRNG cases | 50,000 (synthetic directory buffers) |
| Edge cases | valid LFN chains, fragmented C1, NameHash mismatch, malformed/incomplete secondary, deleted (bit7 clear), type-0 end, Unicode, max-length (17×15), mixed valid/invalid, `/` continuation, empty/not-found |

### Validation plan

| Layer | Plan |
|-------|------|
| Host tests | `flfn_*` focused + 50k PRNG + canaries + full suite |
| ABI smoke | Public `exFAT_find_lfn` with **synthetic** `sizeof.exFAT` + buffer-walk first/next (not live mounts; REG-003). Real FASM `exFAT_get_name`. Marker `'FLFN'`. |
| QEMU | OFF / ON / A/B / ON×3 — desktop reached, **779380**, `resets=0` |
| Subsystem soak | `python scripts/qmp_desktop_smoke.py --wait 90 --disk exfat` |
| Rollback | `USE_RUST_EXFAT_FIND_LFN = 0` |

### Memory impact

Blob expected hundreds of bytes (UTF loop + compare + inlined hash/utf8). 524 B LFN is **stack**, not `.bss`. Smoke uses stack `sizeof.exFAT` like Cut CL (REG-003). Do **not** raise `TMP_STACK_TOP` unless measured end `.bss` requires it.

### Gate

`USE_RUST_EXFAT_FIND_LFN = 1` in `kernel/fs/exfat.inc` / `project/build.toml`.

---

**Stop after Cut CQ. Do not start Cut CR.**
