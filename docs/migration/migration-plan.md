# Migration Plan

## Strategy

Staged coexistence: keep FASM bootable; replace behind **dependency cuts** ([`boundaries.md`](boundaries.md)). Not a big-bang rewrite.

## Stages

### Stage 0 — Baseline

- **Prereq:** restore real `init.inc` — **done** 2026-08-09 ([`fasm-baseline-restoration.md`](fasm-baseline-restoration.md); hash `F7391BA4…`).
- **Do:** build `kernel.mnt`; QEMU boot to desktop — **done** (uncompressed kernel on CoW image after freeing `DOCPACK`).
- **Tests:** boot, launch `/sys` apps — boot/desktop smoke done; broader app matrix still open.
- **Done:** reproducible assemble + boot path documented.
- **Rollback:** N/A (restore from upstream mirror `docs/_upstream/init.inc` if needed).

### Stage 1 — Documentation freeze + harness

- Capture syscall traces; start differential runner.
- **Status:** partial (docs + boot smoke exist; full CI harness still open).

### Stage 2 — Rust utils in-process

- Cut A from boundaries.
- **Risk:** low.
- **Done:** Phase C probe + CRC32 + UTF-16 + CP866 + UTF-8. Baseline: [`cut-a-final-architecture.md`](cut-a-final-architecture.md).
- **Cut B (pure util, not allocator):** `cp866toUpper` — **done** 2026-08-09 ([`cut-b-plan.md`](cut-b-plan.md), [`cut-b-implementation.md`](cut-b-implementation.md)).
- **Cut C:** `utf16toUpper` — **done** ([`cut-c-implementation.md`](cut-c-implementation.md)).
- **Cut D:** `strncmp` — **done** ([`cut-d-implementation.md`](cut-d-implementation.md)).
- **Cut E:** `checksum_1` — **done** 2026-08-09 ([`cut-e-plan.md`](cut-e-plan.md), [`cut-e-implementation.md`](cut-e-implementation.md)).
- **Cut F:** `checksum_2` — **done** 2026-08-09 ([`cut-f-plan.md`](cut-f-plan.md), [`cut-f-implementation.md`](cut-f-implementation.md)).
- **Cut G:** `fsCalculateTime` — **done** 2026-08-09 ([`cut-g-plan.md`](cut-g-plan.md), [`cut-g-implementation.md`](cut-g-implementation.md)).
- **Cut H:** `block_clip` — **done** 2026-08-09 ([`cut-h-plan.md`](cut-h-plan.md), [`cut-h-implementation.md`](cut-h-implementation.md)). **Stop; do not start Cut I.**

### Stage 3 — Compat syscall façade (selected)

- Easy query syscalls in Rust.
- **Status:** not started.

### Stage 4 — Memory exports

- Rust page/heap behind same symbols.
- **Status:** not started.

### Stage 5 — FS plugin / net protocol islands

- One filesystem; optional TCP path.
- **Status:** not started.

### Stage 6 — Scheduler policy + process create

- High risk; extensive soak tests.
- **Status:** not started.

### Stage 7 — GUI server

- Last major app-facing move.
- **Status:** not started.

### Stage 8 — PE driver loader

- Export directory ownership in Rust.
- **Status:** not started.

### Stage 9 — Boot ownership

- Rust entry; FASM residual only permanent asm.
- **Status:** not started.

## Definition of done (global)

Differential tests green; stock apps + drivers on QEMU/hardware sample; performance within agreed tolerance; docs updated.

## Audit amendments

See [`../compatibility/KNOWN_COMPATIBILITY_SURFACES.md`](../compatibility/KNOWN_COMPATIBILITY_SURFACES.md).

New required test gates before claiming app compatibility:

- Fn61 + GS read/write smoke (LFB mode)
- SYSENTER path if any libc stub present
- `MENUET01` and `MENUET02` (version 2) launch
- Fn9 0x4C layout golden buffer
- Driver: `DiskAdd` with reduced `strucsize`; IRQ handler return EAX; `GetService`+IOCTL round-trip
- Negative: CPL3 load from `SLOT_BASE` must `#PF` (documents protection invariant)

Downgraded effort: building permanent user-visible mirrors of `SLOT_BASE`/`window_data` is **not** required for app compat (paging already isolates).
