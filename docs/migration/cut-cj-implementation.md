# Cut CJ Implementation — `memmove`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-cj-plan.md`](cut-cj-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CJ** |
| FASM symbol | `memmove` |
| Source | [`kernel/kernel.asm`](../../kernel/kernel.asm) |
| Callers | ~24 live across ~11 files (keymap, HID ring, GUI boxes/buttons, FAT/NTFS/exFAT, sys32, msg board) |
| Rust symbol | `rust_memmove` |
| Pure helper | `kolibri_utils::memmove` / `memmove_ptr` |
| Subsystem | core forward memory move (Stage-2 util) |
| Stage | Stage 2 / core util |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — no remaining pending symbol establishes Rust-owned subsystem state (PE/USB translate leaves exhausted at CH/CI; video/exFAT/IRQ/string-export clusters fail ownership or substance bars).

Selected `memmove` over `unpack` (~32KB `unpack.p` + LZMA), `blit_32` (LFB hot-path blast), `exFAT_find_lfn` (FS plugin island), `mutex_init` (thin+fanout), and `get_phys_addr` (thin AQ glue) after fresh post-CI audit. **Blast-radius justification:** fan-out is high, but semantics are tiny and fully oracle-coverable (byte-identical forward copy); risk class is ABI/register/DF, not algorithmic ambiguity. Prior deferrals correctly waited until lower-blast high-quality leaves were exhausted.

Memory: end `.bss` @ `OS_BASE+0x8CC83`; assert needs `0x8DC83 < TMP_STACK_TOP`. `0x8DC00` failed by **0x83**. Raised **`TMP_STACK_TOP` `0x008DC00` → `0x008DD00`** (+256 B; smallest clean step). Gap to `sys_proc` @ `0x8E000` remains (`0x300`).

---

## Legacy ABI

```text
memmove  (register ABI; not stdcall; not C memmove)
  in:  EAX = from; EBX = to; ECX = nbytes (signed ≤0 → no-op)
  out: EAX/EBX/ECX restored; ESI/EDI restored; EDX/EBP untouched
  body: forward rep movsd then movsb; DF assumed clear
  overlap: always forward (KEY_BUFF/msg_board left-shifts rely on this)
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_memmove` |
| Blob | **214** bytes, **0 relocations** |
| SHA-256 | `87fe76d1f58e59581fe1c81e594b9c09f429bca5c82375cbf2a671c1f755ace3` |
| Trampoline | plain label; push EDX/ESI/EDI/EBP + EAX/EBX/ECX; `stdcall rust_*`; restore; never `add esp` (REG-009) |
| Gate | `USE_RUST_MEMMOVE` (prod 1) |
| Rust ABI | `stdcall rust_memmove(from, to, nbytes); ret 12` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow dword-then-byte forward copy (`oracle_in_place`) |
| PRNG seed | `0x4D4D4F56` (`'MMOV'`) |
| PRNG cases | 50,000 |
| Cut CJ host tests | focused `mmov_*` **10/10 PASS** |
| Full host suite | **714/714 PASS** |
| ABI smoke | **PASS** — marker `'MMOV'` (hang=`DEAD0C6A`); stack synthetic buffer; non-overlap + forward-overlap + zero length + register canaries |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_MEMMOVE=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_MEMMOVE=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-235311.img`

---

## Subsystem soak

Desktop boots exercise keymap/`KEY_BUFF` ring shifts, GUI box/button compact, and msg-board paths. Additional **`--disk exfat`** attach soak: `query-status: running`, non-black=779380, resets=0. Primary correctness evidence remains the independent forward-copy oracle (incl. overlap quirk) + ABI smoke + A/B desktop.

---

## Regressions

None this cut.

Applied prior lessons: FASM register-ABI outer / Rust `stdcall` only (no double cleanup); REG-001/011 preserve EAX/EBX/ECX/EDX/ESI/EDI/EBP; REG-010 push order accounted; QMP `RESET` checked; synthetic stack buffer only (REG-003); hang marker `DEAD0C6A`.

---

## Rollback

```text
USE_RUST_MEMMOVE = 0
```

in `kernel/kernel.asm` (or `enabled = false` for Cut CJ in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/memmove.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_memmove` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/memmove.inc` | Blob embed + ABI smoke |
| `kernel/kernel.asm` | Gate + trampoline + legacy body + smoke call |
| `kernel/kernel32.inc` | Include |
| `kernel/const.inc` | `TMP_STACK_TOP` +0x100 |
| `project/build.toml` | Blob + migration CJ |
| `docs/compatibility/fixed-addresses.md` | TMP baseline |
| `docs/architecture/memory-model.md` | TMP baseline |
| `docs/migration/cut-cj-plan.md` | Plan |
| `docs/migration/migration-todo.md` | Inventory 91/135 |
| `docs/migration/migration-plan.md` | Cut CJ entry |

---

## Stop

**Cut CJ complete. Do not start Cut CK.**
