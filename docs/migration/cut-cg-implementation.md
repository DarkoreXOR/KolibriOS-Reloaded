# Cut CG Implementation — `get_proc_ex`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-cg-plan.md`](cut-cg-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CG** |
| FASM symbol | `get_proc_ex` |
| Source | [`kernel/core/dll.inc`](../../kernel/core/dll.inc) |
| Callers | FASM `fix_coff_symbols`; Cut BU injects as resolve callback |
| Rust symbol | `rust_get_proc_ex` |
| Pure helper | `kolibri_utils::get_proc_ex` / `get_proc_ex_with_base` |
| Subsystem | PE/COFF export resolve |
| Stage | Stage 8 PE loader foothold (after Y/AT/BK/BU) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — PE Y+AT+BK+BU+CG leaves do not establish Rust-owned PE loader.

Selected `get_proc_ex` over `enable_irq` (poor I/O oracle — **REJECT**), `irq_eoi` (same class), `unpack` (~32KB `unpack.p` + LZMA), `exFAT_find_lfn` (FS plugin island), and `blit_32` (LFB hot-path blast) after fresh post-CF audit. Historical PE ban stretch after Y+AT is stale with BU complete.

Memory: end `.bss` @ `OS_BASE+0x8C4C3`; assert needs `0x8D4C3 < TMP_STACK_TOP`. Existing **`TMP_STACK_TOP = 0x008D800`** unchanged (do not lower).

---

## Legacy ABI

```text
get_proc_ex stdcall(proc_name, imports) → EAX; ret 8
  imports == 0 → EAX = 0
  else walk NumberOfNames ([imports+24]) with bottom-checked loop
       (one probe even when NumberOfNames == 0 — legacy quirk)
  name = OS_BASE + [OS_BASE + AddressOfNames + esi*4]
  match: strncmp(name, proc_name, 256) == 0
  hit: EAX = OS_BASE + [OS_BASE + AddressOfFunctions + esi*4]
  quirk: name index indexes AddressOfFunctions directly (no NameOrdinals)
preserves: EBX, ESI (explicit uses); EDI/EBP untouched by body
clobbers: EAX; ECX/EDX via strncmp
flags / IF: unused / untouched
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_get_proc_ex` |
| Blob | **127** bytes, **0 relocations** |
| SHA-256 | `6f2127400180b01bc21deecb6cdaa787bc361e83a8efdb20062c83d85aa551f8` |
| Trampoline | `proc … uses ebx ecx edx esi edi ebp` → `stdcall rust_get_proc_ex`; outer `ret` cleans caller |
| Gate | `USE_RUST_GET_PROC_EX` (prod 1) |
| Rust ABI | `stdcall rust_get_proc_ex(proc_name, imports); ret 8` |
| Compare | Inlined 256-byte strncmp-class equality (no cross-blob reloc) |
| `OS_BASE` | Hardcoded `0x80000000` (reloc-free) |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow arena mirror (`get_proc_ex_oracle`) |
| PRNG seed | `0x47504558` (`'GPEX'`) |
| PRNG cases | 50,000 |
| Cut CG host tests | focused `gpex_*` **9/9 PASS** |
| Full host suite | **685/685 PASS** |
| ABI smoke | **PASS** — marker `'GPEX'` (gated `if USE_RUST_GET_PROC_EX`; hang=`DEAD0C60` on fail); synthetic export RVAs=`addr-OS_BASE` |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_GET_PROC_EX=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_GET_PROC_EX=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-172058.img`

---

## Subsystem soak

Desktop boot loads PE drivers / libraries through `load_library` → `fix_coff_symbols` → `get_proc_ex`. ON desktop reached with `query-status: running`, non-black=779380, resets=0, pixel-identical to OFF — exercises live export resolve. ABI smoke covers null imports / hit first / hit second / miss / register canaries before desktop.

---

## Regressions

None this cut.

Applied prior lessons: FASM `stdcall` only (no double cleanup); REG-001/011 preserve EBX/ESI/EDI/EBP; QMP `RESET` checked; smoke uses valid hex fail marker (`DEAD0C60` — `G` is not a hex digit).

---

## Rollback

```text
USE_RUST_GET_PROC_EX = 0
```

in `kernel/core/dll.inc` (or `enabled = false` for Cut CG in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/get_proc_ex.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_get_proc_ex` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/get_proc_ex.inc` | Blob embed + ABI smoke |
| `kernel/core/dll.inc` | Gate + trampoline + legacy body |
| `kernel/kernel32.inc` | Include |
| `kernel/kernel.asm` | Smoke call |
| `project/build.toml` | Blob + migration CG |
| `docs/migration/cut-cg-plan.md` | Plan |
| `docs/migration/migration-todo.md` | Inventory 88/135 |
| `docs/migration/migration-plan.md` | Cut CG entry |

---

## Stop

**Cut CG complete. Do not start Cut CH.**
