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
- **Status (2026-08-10):** **COMPLETE** — Phase C + Cuts A–AE production-validated (desktop + network NIC). Bisect log: [`black-screen-investigation.md`](black-screen-investigation.md). Diagnostic smokes D–AE re-enabled and validated (Stage 3 smoke pass); Cut M smoke expectation fixed for unsigned `ADD`+`JA`.
- **Done:** Phase C probe + CRC32 + UTF-16 + CP866 + UTF-8. Baseline: [`cut-a-final-architecture.md`](cut-a-final-architecture.md).
- **Cut B (pure util, not allocator):** `cp866toUpper` — **done** 2026-08-09 ([`cut-b-plan.md`](cut-b-plan.md), [`cut-b-implementation.md`](cut-b-implementation.md)).
- **Cut C:** `utf16toUpper` — **done** ([`cut-c-implementation.md`](cut-c-implementation.md)).
- **Cut D:** `strncmp` — **COMPLETE — FIXED** ([`cut-d-implementation.md`](cut-d-implementation.md)). Production regression: Rust body clobbered EDX; `get_service` required EDX=`SRV*` across the call → network loss. Fix: EDX-preserving FASM trampoline. Open (unchanged): FASM `cld` vs Rust DF leave-alone.
- **Cut E:** `checksum_1` — **done** 2026-08-09 ([`cut-e-plan.md`](cut-e-plan.md), [`cut-e-implementation.md`](cut-e-implementation.md)).
- **Cut F:** `checksum_2` — **done** 2026-08-09 ([`cut-f-plan.md`](cut-f-plan.md), [`cut-f-implementation.md`](cut-f-implementation.md)).
- **Cut G:** `fsCalculateTime` — **done** 2026-08-09 ([`cut-g-plan.md`](cut-g-plan.md), [`cut-g-implementation.md`](cut-g-implementation.md)).
- **Cut H:** `block_clip` — **done** 2026-08-09 ([`cut-h-plan.md`](cut-h-plan.md), [`cut-h-implementation.md`](cut-h-implementation.md)).
- **Cut I:** `ntfs_decode_mcb_entry` — **done** 2026-08-09 ([`cut-i-plan.md`](cut-i-plan.md), [`cut-i-implementation.md`](cut-i-implementation.md)).
- **Cut J:** `ntfs_restore_usa` — **done** 2026-08-09 ([`cut-j-plan.md`](cut-j-plan.md), [`cut-j-implementation.md`](cut-j-implementation.md)).
- **Cut K:** `fat_next_short_name` — **done** 2026-08-09 ([`cut-k-plan.md`](cut-k-plan.md), [`cut-k-implementation.md`](cut-k-implementation.md)).
- **Cut L:** `mouse_acceleration` — **done** 2026-08-09 ([`cut-l-plan.md`](cut-l-plan.md), [`cut-l-implementation.md`](cut-l-implementation.md)).
- **Cut M:** `tcp_xmit_timer` — **done** 2026-08-09 ([`cut-m-plan.md`](cut-m-plan.md), [`cut-m-implementation.md`](cut-m-implementation.md)).
- **Cut N:** `antiAliasing` — **done** 2026-08-09 ([`cut-n-plan.md`](cut-n-plan.md), [`cut-n-implementation.md`](cut-n-implementation.md)).
- **Cut O:** `test_app_header` — **done** 2026-08-09 ([`cut-o-plan.md`](cut-o-plan.md), [`cut-o-implementation.md`](cut-o-implementation.md)).
- **Cut P:** `is_region_userspace` — **done** 2026-08-09 ([`cut-p-plan.md`](cut-p-plan.md), [`cut-p-implementation.md`](cut-p-implementation.md)). ZF-out syscall gate; EAX/ECX/EDX preserve; overflow-to-zero quirk retained.
- **Cut Q:** `UTF16to8` — **done** 2026-08-09 ([`cut-q-plan.md`](cut-q-plan.md), [`cut-q-implementation.md`](cut-q-implementation.md)). SF-out streaming encode; ECX burn-down / INT_MIN / surrogates preserved.
- **Cut R:** `xfs._.extent_unpack` — **done** 2026-08-09 ([`cut-r-plan.md`](cut-r-plan.md), [`cut-r-implementation.md`](cut-r-implementation.md)). Omit-FP stdcall; EBP→XFS preserved; MOVBE BE bitfield unpack.
- **Cut S:** `window._.check_window_position` — **done** 2026-08-09 ([`cut-s-plan.md`](cut-s-plan.md), [`cut-s-implementation.md`](cut-s-implementation.md)). GUI screen-fit; EDI→WDATA.box; display dims via trampoline.
- **Cut T:** `fsTime2bdfe` — **done** 2026-08-09 ([`cut-t-plan.md`](cut-t-plan.md), [`cut-t-implementation.md`](cut-t-implementation.md)). EDI+=8 calendar inverse; completes G pair.
- **Cut U:** `fat_gen_short_name` — **done** 2026-08-10 ([`cut-u-plan.md`](cut-u-plan.md), [`cut-u-implementation.md`](cut-u-implementation.md)). UTF-8→8.3 state machine; composes B+K; reloc-free.
- **Cut V:** `tcp_set_persist` — **done** 2026-08-10 ([`cut-v-plan.md`](cut-v-plan.md), [`cut-v-implementation.md`](cut-v-implementation.md)). Persist-timer arming from SRTT/RTTVAR; reloc-free.
- **Cut W:** `xfs._.get_addr_by_hash` — **done** 2026-08-10 ([`cut-w-plan.md`](cut-w-plan.md), [`cut-w-implementation.md`](cut-w-implementation.md)). XFS dir leaf binary search; EAX+ZF dual return; reloc-free.
- **Cut X:** `set_io_access_rights` — **done** 2026-08-10 ([`cut-x-plan.md`](cut-x-plan.md), [`cut-x-implementation.md`](cut-x-implementation.md)). TSS I/O permission bitmap BTR/BTS; reloc-free.
- **Cut Y:** `fix_coff_relocs` — **done** 2026-08-10 ([`cut-y-plan.md`](cut-y-plan.md), [`cut-y-implementation.md`](cut-y-implementation.md)). PE/COFF DIR32/REL32 reloc buffer patch; reloc-free.
- **Cut Z:** `is_partition_table_entry` — **done** 2026-08-10 ([`cut-z-plan.md`](cut-z-plan.md), [`cut-z-implementation.md`](cut-z-implementation.md)). MBR/EBR partition-table entry validate (Bootable + 64-bit half-capacity); CF-out; reloc-free.
- **Cut AA:** `pid_to_slot` — **done** 2026-08-10 ([`cut-aa-plan.md`](cut-aa-plan.md), [`cut-aa-implementation.md`](cut-aa-implementation.md)). Process TID→slot linear walk over `SLOT_BASE`; signed `jle` bound; reloc-free via trampoline-injected globals.
- **Cut AB:** `utf8to16` — **done** 2026-08-10 ([`cut-ab-plan.md`](cut-ab-plan.md), [`cut-ab-implementation.md`](cut-ab-implementation.md)). ESI-advancing UTF-8→UTF-16 streaming decode (Q inverse; Cut-A leftover); reloc-free.
- **Cut AC:** `ipv4_route` — **done** 2026-08-10 ([`cut-ac-plan.md`](cut-ac-plan.md), [`cut-ac-implementation.md`](cut-ac-implementation.md)). IPv4 on-link/gateway/broadcast egress selection; reloc-free via trampoline-injected tables.
- **Cut AD:** `is_protective_mbr` — **done** 2026-08-10 ([`cut-ad-plan.md`](cut-ad-plan.md), [`cut-ad-implementation.md`](cut-ad-implementation.md)). GPT protective-MBR ZF recognition; Path B after cluster audit; reloc-free.
- **Cut AE:** `ntfs_datetime_to_bdfe` — **done** 2026-08-10 ([`cut-ae-plan.md`](cut-ae-plan.md), [`cut-ae-implementation.md`](cut-ae-implementation.md)). NTFS FILETIME (1601×10⁷) → BDFE; composes Cut T; Path B after post-AD cluster audit; reloc-free. **Stop; do not start Cut AF.**

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
