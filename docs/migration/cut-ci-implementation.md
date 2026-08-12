# Cut CI Implementation — `usb_td_to_virt`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-ci-plan.md`](cut-ci-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CI** |
| FASM symbol | `usb_td_to_virt` |
| Source | [`kernel/bus/usb/memory.inc`](../../kernel/bus/usb/memory.inc) |
| Callers | `usb_hc_func[7]` / PE export `USBHCFunc` (*HCI drivers) |
| Rust symbol | `rust_usb_td_to_virt` |
| Pure helper | `kolibri_utils::usb_td_to_virt` / `usb_td_to_virt_ptr` |
| Subsystem | USB HC TD phys→virt (Stage-4/USB foothold; AQ compose) |
| Stage | Stage 4 / USB (after AQ) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — no remaining pending symbol establishes Rust-owned subsystem state (PE leaf set exhausted at CH; AQ+USB translate footholds ≠ HC ownership).

Selected `usb_td_to_virt` over `unpack` (~32KB `unpack.p` + LZMA), `blit_32` (LFB hot-path blast), `memmove` (~24-caller blast), and `exFAT_find_lfn` (FS plugin island) after fresh post-CH audit. Prior “USB weak soak” ban stretched after PE Stage-8 exhaustion (parallel to CH’s Y-mutate stretch).

Memory: end `.bss` @ `OS_BASE+0x8CA43`; assert needs `0x8DA43 < TMP_STACK_TOP`. Raised **`TMP_STACK_TOP` `0x008D800` → `0x008DC00`** (+1 KiB). Gap to `sys_proc` @ `0x8E000` remains (`0x400`).

---

## Legacy ABI

```text
usb_td_to_virt  (register ABI; not stdcall)
  in:  EAX = virt address of first TD page
       ECX = TD physical address
  out: EAX = linear TD address, or 0 if not found
       ECX = (TD phys & 0xFFF) on hit; UNCHANGED on miss/empty
  walk: get_pg_addr match window; next = [page+0xFFC]
preserves: EBX, EDX, ESI, EDI, EBP
clobbers: EAX; ECX on hit; flags
```

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_usb_td_to_virt` |
| Blob | **113** bytes, **0 relocations** |
| SHA-256 | `132cfeb3b8e745dec17463e037cd48059f5f2b001f4b4cc68312d728823780e1` |
| Trampoline | label preserves EBX/EDX/ESI/EDI/EBP; `stdcall rust_*` injects `page_tabs`+`OS_BASE`; reconstructs ECX on hit |
| Gate | `USE_RUST_USB_TD_TO_VIRT` (prod 1) |
| Rust ABI | `stdcall rust_usb_td_to_virt(first, td_phys, page_tabs, os_base); ret 16` |
| Compose | Inlines Cut AQ `get_pg_addr` (reloc-free) |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow walk (`fasm_oracle_usb_td_to_virt`) |
| PRNG seed | `0x55544456` (`'UTDV'`) |
| PRNG cases | 50,000 |
| Cut CI host tests | focused `utdv_*` **10/10 PASS** (incl. live low-VA pointer path on Windows) |
| Full host suite | **704/704 PASS** |
| ABI smoke | **PASS** — marker `'UTDV'` (gated `if USE_RUST_USB_TD_TO_VIRT`; hang=`DEAD0C62`); `kernel_alloc` synthetic 2-page chain after heap |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_USB_TD_TO_VIRT=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_USB_TD_TO_VIRT=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-181817.img`

---

## Subsystem soak

Desktop boot with production gate ON reached `query-status: running`, non-black=779380, resets=0, pixel-identical to OFF. ABI smoke exercises hit/miss/offset + register canaries on a `kernel_alloc` TD page chain before desktop. No dedicated `scripts/` USB attach flag; QEMU PC UHCI may be present and HCI PE drivers may load via `USBHCFunc` — soak documented as **partial** (BH/CF class honesty). Primary correctness evidence is the independent oracle + ABI smoke + A/B desktop.

---

## Regressions

None this cut.

Applied prior lessons: FASM register-ABI outer / Rust `stdcall` only (no double cleanup); REG-001/011 preserve EBX/EDX/ESI/EDI/EBP; ECX OUT reconstructed on hit; REG-010 push of td_phys accounted; QMP `RESET` checked; smoke uses valid hex fail marker (`DEAD0C62`); synthetic pages only (REG-003).

---

## Rollback

```text
USE_RUST_USB_TD_TO_VIRT = 0
```

in `kernel/bus/usb/memory.inc` (or `enabled = false` for Cut CI in `project/build.toml` then rebuild). Legacy FASM body retained under `else`.

---

## Files touched

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/usb_td_to_virt.rs` | Pure leaf + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_usb_td_to_virt` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | Re-exports |
| `kernel/rust/usb_td_to_virt.inc` | Blob embed + ABI smoke |
| `kernel/bus/usb/memory.inc` | Gate + trampoline + legacy body |
| `kernel/kernel32.inc` | Include |
| `kernel/kernel.asm` | Smoke call (post-heap) |
| `kernel/const.inc` | `TMP_STACK_TOP` +0x400 |
| `project/build.toml` | Blob + migration CI |
| `docs/compatibility/fixed-addresses.md` | TMP baseline |
| `docs/architecture/memory-model.md` | TMP baseline |
| `docs/migration/cut-ci-plan.md` | Plan |
| `docs/migration/migration-todo.md` | Inventory 90/135 |
| `docs/migration/migration-plan.md` | Cut CI entry |

---

## Stop

**Cut CI complete. Do not start Cut CJ.**
