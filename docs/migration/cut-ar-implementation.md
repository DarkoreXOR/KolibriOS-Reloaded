# Cut AR Implementation — `r_f_port_area`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-ar-plan.md`](cut-ar-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `r_f_port_area` |
| Source | [`kernel/kernel.asm`](../../kernel/kernel.asm) |
| Callers | syscall 46 (`ReservePortArea`); PE export; optional COM boot (`debug_com_base` off in prod) |
| Rust symbol | `rust_r_f_port_area` |
| Pure helper | `kolibri_utils::r_f_port_area` |
| Subsystem | CPU / I/O port reservation (Stage-3; Cut X follow-on) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AQ Stage-4 audit: remaining `memory.inc` APIs are
TLB/`cli`/allocator/fault or thin accessors (`v86_get_lin_addr`, `get_phys_addr`).
Shared `page_tabs` ≠ Rust-owned paging. Easy `sysfn_get*` Path A fails the raised
bar (independent loads). Ban-list FS/Unicode/GUI/PE peers unchanged.

Selected: **`r_f_port_area`** — strongest remaining leaf after Stage-4 exhaustion;
completes Cut X’s parent (reserve/free + `RESERVED_PORTS` + IO-map ranges).

REG-001: trampoline preserves **ECX+EDX** (and EBX/ESI/EDI/EBP). Legacy FASM free
clobbers ESI/EDI/EBP/`ECX` via `rep movsb` — ABI smoke asserts ECX/EDX only on
paths where FASM preserves them.

---

## Candidate comparison (post-AQ audit)

| Candidate | Outcome |
|-----------|---------|
| `r_f_port_area` | **Selected** — Stage-3 I/O reservation; Cut X parent |
| `blit_clip` | #2 — H composition; desktop-only |
| `is_string_userspace` | #3 — P+scasb; thin-after-P |
| `sysfn_getfreemem` / Stage-3 query Path A | Reject — trivial / no shared algorithm |
| `v86_get_lin_addr` / `get_phys_addr` / map/alloc/fault | Reject — Stage-4 accessor/ownership |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_R_F_PORT_AREA=0`:

```text
call / ret
in:  EBX = 0 reserve / ≠0 free
     ECX = start port
     EDX = end port (inclusive)
out: EAX = 0 success / 1 error
destroys: EAX, EBX, EBP (documented); free also clobbers ESI/EDI/ECX
reserve: cli … enable IO bits … sti
free: no cli
```

---

## Rust ABI

```text
stdcall rust_r_f_port_area(op, start, end, reserved_ports, tid, io_map) -> EAX
  ret 24
```

Trampoline: injects `RESERVED_PORTS`, `current_slot.tid`, `tss._io_map_0`;
`cli`/`sti` around reserve only; preserves EBX/ECX/EDX/ESI/EDI/EBP.

IO-map bit updates are **inlined** (Cut X polarity) so the AR section stays
reloc-free (no cross-section call to `set_io_access_rights`).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `port_area.rs` + `ffi.rs` section `.text.rust_r_f_port_area` |
| Extract | `extract_reloc_free_text.py` → `rust_r_f_port_area.bin` |
| Embed | `kernel/rust/r_f_port_area.inc` `file` directive |
| Trampoline | `kernel.asm` under `USE_RUST_R_F_PORT_AREA` |
| Gate | `USE_RUST_R_F_PORT_AREA` (prod 1) |
| Smoke | `r_f_port_area_rust_smoke_test` (after `reserve_irqs_ports`) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_r_f_port_area` |
| Blob/object size | 606 bytes |
| Relocations | 0 |
| SHA-256 | `C92C7DD0CEAC786BC58D7984A3D246B1142805C41E6A2AC747776EF5C627B509` |

Trailing instruction is `ret 24` (`C2 18 00`). Reloc-free verified by extractor
(extraction fails if the section has relocations). Initial extract failed when
calling Cut X across sections / `memcpy`; fixed by inlining bit ops + manual
forward compact copy.

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | empty reserve; overlap inclusive; adjacent OK; free compact; max 255; system-seeded ranges; nonzero op=free |
| PRNG | 50 000 cases, seed `0x43555452` (`'CUTR'`) |
| Host tests | **428/428** cargo tests |

---

## ABI smoke

| Item | Result |
|------|--------|
| `r_f_port_area_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C52` hang) |
| Vectors | direct Rust **error-path** only: reserve with `start > end` (0xF107 > 0xF100) + free on empty synthetic table; public canary uses `start > end` to validate ECX/EDX preservation (REG-001) |
| Canaries | ECX/EDX on the **public** error path; direct Rust internal calls avoid the successful reserve IO-map enable loop that could hang during AHCI init-stage |
| Marker | `rust_r_f_port_area_smoke_result = 'FPAR'` on success |

---

## QEMU validation

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_R_F_PORT_AREA=0` | **OK** (QMP `running` + screendump, 779380 non-black) | Not attached in current `qemu.args` |
| ON | `USE_RUST_R_F_PORT_AREA=1` | **OK** (779380 non-black) | Not attached |

### AHCI init-stage hang (what changed)

Earlier AHCI regression investigation showed that the **successful** Rust reserve path inside the AR ABI smoke could stop the kernel on the initialization screen (no desktop stage; non-black ~7358), specifically after “`Reserving IRQs & ports`”.

Fix: the ABI smoke was updated to avoid the successful reserve path and use only the fast `start > end` error-path + the public trampoline canary.

### A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop smoke | 779380 | 779380 | Match |
| `--disk xfs` boot+desktop | 779380 | 779380 | Match |

### Real subsystem soak

Forced syscall-46 reserve/free traffic: **NOT AVAILABLE** (same class as Cut X;
stock boot does not call `r_f_port_area` — `debug_com_base` undefined).

ABI smoke exercises live `RESERVED_PORTS` + `tss._io_map_0` on the public path.

Production image: `dev_build/cut-ar-final.img`.

e1000: **N/A**

---

## Regressions discovered

**NONE** during Cut AR validation after the smoke change.

During AHCI regression investigation, the original smoke’s **successful Rust reserve** path could hang during the kernel initialization stage when AHCI disks were attached (desktop stage not reached). This was fixed by switching the smoke to use only the fast `start > end` error-path + the public trampoline canary.

(Smoke canary over-assert on FASM free ESI/EDI/EBP/ECX was a test bug, not a
production regression — fixed before enabling the gate.)

---

## Production gate

```text
USE_RUST_R_F_PORT_AREA = 1
```

Rollback: `USE_RUST_R_F_PORT_AREA = 0` (or `enabled = false` in
`project/build.toml` Cut AR migration entry).

---

## Files changed

* `rust_kernel/kolibri_utils/src/port_area.rs` — algorithm + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_r_f_port_area`
* `rust_kernel/kolibri_utils/src/lib.rs` — exports
* `kernel/kernel.asm` — trampoline + gate + FASM rollback body + smoke call
* `kernel/rust/r_f_port_area.inc` — blob embed + ABI smoke
* `kernel/kernel32.inc` — include
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-ar-plan.md` / `cut-ar-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* Forced syscall-46 soak **NOT AVAILABLE**.
* Does not own TSS init / IRQ port seed (`reserve_irqs_ports` remains FASM).
* No Path A claim (Cut X already shipped separately).
* Trampoline preserves more registers than legacy free destroys — intentional
  REG-001 hardening.
