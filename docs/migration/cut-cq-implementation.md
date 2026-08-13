# Cut CQ Implementation — `exFAT_find_lfn`

**Date:** 2026-08-13  
**Status:** complete (audited)  
**Plan:** [`cut-cq-plan.md`](cut-cq-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regressions this cut:** [`regression-log.md`](regression-log.md) REG-017, REG-018, REG-019.

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CQ** |
| FASM symbol | `exFAT_find_lfn` |
| Source | [`kernel/fs/exfat.inc`](../../kernel/fs/exfat.inc) |
| Callers | **1** live (`exFAT_hd_find_lfn`) |
| Rust symbol | `rust_exfat_find_lfn` |
| Pure helper | `kolibri_utils::exfat_find_lfn` / `ExFatFindLfnCtx` |
| Subsystem | fs/exFAT UTF-8 path-component lookup (Stage-2 leaf; AH+AI+CL compose) |
| Stage | Stage 2 / Stage 5 FS plugin foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — post-CP vacuum; no remaining pending group is a Rust-owned subsystem. Video H+CD+CP are clip/blit leaves; LFB / win_map / cursor policy stay FASM. PE/paging/exFAT-plugin/sockets/IRQ/GUI Stage 7 remain FASM-owned.

Selected `exFAT_find_lfn` over `drawChar` (Stage 7; hidden `dtext` stack ABI), `mem_test` (E820 skip / CR0+cache; no hardware-independent oracle), `strnlen` (PE-export-only thin), `tcp_mss` (thin clamp+store), IRQ leaves (no mask/EOI oracle), and remaining video (not in the 38; would duplicate CP’s pixel class). Plugin architecture is not a rejection: the leaf has independent lookup semantics. **Not Path A** — LFN entry assembly (0x85/0xC0/0xC1) stays in unmigrated FASM `exFAT_get_name`.

**Memory:** Blob + smoke iglobals raise linear `.bss`. **`TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` unchanged** (`0x8E000` / `0x8E000` / `0x90000`). ON end `.bss` **`OS_BASE+0x8BB03`**; assert `0x8CB03 < 0x8E000` PASS. OFF (FASM body) end `.bss` slightly larger. 524 B LFN is **stack**, not `.bss`.

---

## Legacy ABI

```text
exFAT_find_lfn  (plain `call` / `ret`, not stdcall)
  in:  ESI → UTF-8 path
       EBP → exFAT*
       [esp+4]  = next  (exFAT_notroot_next)
       [esp+8]  = first (exFAT_notroot_first)
       [esp+12] = cluster/sector pair (EAX for first/next)
  out: CF=0, EAX=0, ESI→next path component, EDI→direntry
       CF=1, EAX=error (5 = ERROR_FILE_NOT_FOUND, or callback EAX)
  preserves: EBX, EBP
  clobbers: EAX, ECX, EDX, ESI, EDI, flags
  DF: unchanged (no cld in the leaf; get_name `movsd` assumes DF=0)
  stack: 262*2 = 524 B LFN buffer; caller owns callback slots (ret 0)
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_exfat_find_lfn` |
| Blob | **1324** bytes, **0 relocations** |
| SHA-256 | `e146d2a5fbe9f30552ae8f3678c6bd12917c54976907083aef4e2491c11f217d` |
| Epilogue | `ret 4` (extractor `--expect-ret-imm 4`) |
| Trampoline | snapshot next/first/pair + ESI/EBP **before** stdcall (REG-010); `mov eax, esp` / `stdcall rust_exfat_find_lfn, eax`; **no** `add esp` for Rust args (REG-009); `add esp, 52` is **local ctx only**; `add esp, 8` drops saved EDI/ESI after mapping `esi_out`/`edi_out`; no `cld` |
| Gate | `USE_RUST_EXFAT_FIND_LFN` (prod **1**) |
| Rust ABI | `stdcall rust_exfat_find_lfn(ctx); ret 4` |
| Ctx | 52-byte `ExFatFindLfnCtx` (i686, 13 dwords): `fs`, field pointers, `first`/`next`/`get_name`, `pair*`, `esi_out`, `edi_out` |

Compose inlines Cut AB `utf8to16`, Cut C `utf16toUpper` (`sub eax,32/80`), Cut AI NameHash (no reloc to those blobs). `exFAT_get_name` and directory `first`/`next` are injected function pointers.

UTF-8 fill is capped at `LFN_UTF16_UNITS` (262) — the FASM `sub esp, 262*2` reservation. Longer unterminated paths would smash the FASM stack too.

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent RFC-style UTF-8 path split + independent upper + independent NameHash + independent 0x85/0xC0/0xC1 get_name fixture (not a copy of production). Production uses Cut AB `utf8to16` + `sub eax` upper + Cut AI hash + FASM `exFAT_get_name` |
| PRNG seed | `0x464C464E` (`'FLFN'`) |
| PRNG cases | **50,000** (synthetic directory buffers) |
| Edge cases | valid LFN chains, fragmented C1, NameHash mismatch, malformed/incomplete secondary, deleted (bit7 clear), type-0 end, Unicode/Cyrillic, max-length, mixed valid/invalid, `/` continuation, empty/not-found |
| Cut CQ host tests | focused `flfn_*` **16/16 PASS** |
| Full host suite | **790/790 PASS** |
| ABI smoke | **PASS** — public `rust_exfat_find_lfn` with **synthetic** mini-fs (not live mounts; REG-003). Vector 0: `first=0` → EAX=5. Vector 1: `first` CLC + stub `get_name` STC + `next` CLC then `get_name` STC + `next` STC → EAX=5 (REG-019 two-callback walk). Marker `'FLFN'` / fail `DEAD0C71`. EBX canary `0xB10000C1`. |

Host tests use directory/get_name **hooks**, not `invoke_kernel_*` (those exist only on `i686-none`). Desktop non-black counts are **not** the lookup oracle.

---

## QEMU validation

| Config | Gate | Image | non-black | resets | Result |
|--------|------|-------|-----------|--------|--------|
| OFF | FASM `exFAT_find_lfn` | `kernel-20260813-154207.img` | 779380 | 0 | desktop-reached PASS |
| ON | `USE_RUST_EXFAT_FIND_LFN=1` | `kernel-20260813-154326.img` | 779380 | 0 | desktop-reached PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 / 779380 / 779380 | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
(splash-class ≤20000 non-black is FAIL; desktop floor 100000. RESET is FAIL.)  
Final image: `dev_build/test/kernel-20260813-154326.img`

---

## Subsystem soak

**Kernel-live.** `python scripts/qmp_desktop_smoke.py --wait 90 --disk exfat`  
Result: `query-status: running`, non-black=**779380**, `resets=0`. Attaches `images/exfat-image.img` (LFN names including `FILES WITH SPACES` / `HELLO WORLD.TXT`, nested `NESTED/A`, `NESTED/B`). Production `exFAT_hd_find_lfn` → `exFAT_find_lfn` runs during exFAT partition create / directory walk. Primary correctness remains host `flfn_*` (independent lookup + get_name fixture), not framebuffer totals.

---

## Regressions

### REG-017 — get_name EBP became the UTF-8 path

Live ON smoke hang (`DEAD0C71` / black framebuffer). LLVM kept `fs` in ESI; `mov esi, esi_in` then `mov ebp, fs` set EBP to the path. Fix: pin `f`/`fs`/`esi_in` to EBX/ECX/EDX; `mov ebp, ecx` **before** `mov esi, edx`. Never `in("esi")` (LLVM internal).

### REG-018 — `setc cl` then `pop ecx` discarded first() CF

Vector 0 (`first=0`) passed; vector 1 (`first` CLC) hung. Blob used `setc cl` then `pop ecx`, so CF=0 was overwritten by the function-pointer low byte. Rust treated success as failure, returned the pair pointer in EAX, smoke `cmp eax,5` failed → `jmp @b`. Fix: capture CF with `sbb ebx, ebx` / `sbb eax, eax` **before** any pop; never `setc al` when EAX is the callback error code.

### REG-019 — callback clobbers hung live exFAT lookup

Desktop + WebView OK; Eolite/KFAR hung on testdisk `/hd0/1`. Gate OFF `load_file` of README / space / nested paths succeeded; ON hung in `exFAT_find_lfn`. `call` clobbered ECX/EDX; missing `lateout` plus unsaved ESI (`mov esi, edx` in get_name) let LLVM reuse those regs on the second directory entry. One-shot `next` STC smoke passed. Fix: `lateout` ECX/EDX; `push`/`pop ebx/ebp/esi` around both callbacks. Smoke vector 1 now CLC-then-STC on `next`.

REG-016 was **not** reopened. Applied prior lessons: REG-009 (no extra `add esp` after `ret 4`); REG-010 (snapshot callbacks before stdcall); REG-003 (synthetic fs only).

---

## Production gate

`USE_RUST_EXFAT_FIND_LFN = 1` in `kernel/fs/exfat.inc` (via `project/build.toml` migration registry).

---

## Rollback

```text
USE_RUST_EXFAT_FIND_LFN = 0
```

in `kernel/fs/exfat.inc` (or `enabled = false` for Cut CQ in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/exfat_find_lfn.rs` | production lookup + oracle + `flfn_*` |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_exfat_find_lfn` stdcall export |
| `rust_kernel/kolibri_utils/src/lib.rs` | `mod exfat_find_lfn` |
| `kernel/fs/exfat.inc` | gate + trampoline + FASM `else` body |
| `kernel/rust/exfat_find_lfn.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | gated smoke call |
| `project/build.toml` | blob + Cut CQ migration `enabled = true` |
| `docs/migration/cut-cq-plan.md` | plan |
| `docs/migration/cut-cq-implementation.md` | this report |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | Cut CQ entry |
| `docs/migration/regression-log.md` | REG-017, REG-018, REG-019 |

Memory docs (`fixed-addresses.md`, `memory-model.md`) **unchanged** — pack addresses did not move.

---

## Known limitations

* Kernel `invoke_kernel_dir_fn` / `invoke_kernel_get_name` exist only on `i686-none`; host `flfn_*` inject hooks.
* In-kernel smoke uses stub `first`/`next`/`get_name`, not live directory buffers (REG-003). Production get_name is FASM.
* UTF-8 fill cap at 262 units is the FASM stack reservation; it is not a new semantic for legal exFAT names (max 255).

---

**Stop after Cut CQ. Do not start Cut CR.**
