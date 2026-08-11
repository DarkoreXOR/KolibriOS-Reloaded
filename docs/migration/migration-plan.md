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
- **Status (2026-08-11):** **COMPLETE** — Phase C + Cuts A–AV production-validated (desktop). Bisect log: [`black-screen-investigation.md`](black-screen-investigation.md). Diagnostic smokes D–AV re-enabled and validated (Stage 3 smoke pass); Cut M smoke expectation fixed for unsigned `ADD`+`JA`.
- **Done:** Phase C probe + CRC32 + UTF-16 + CP866 + UTF-8. Baseline: [`cut-a-final-architecture.md`](cut-a-final-architecture.md).
- **Cut B (pure util, not allocator):** `cp866toUpper` — **done** 2026-08-09 ([`cut-b-plan.md`](cut-b-plan.md), [`cut-b-implementation.md`](cut-b-implementation.md)).
- **Cut C:** `utf16toUpper` — **done** ([`cut-c-implementation.md`](cut-c-implementation.md)).
- **Cut D:** `strncmp` — **COMPLETE — FIXED** ([`cut-d-implementation.md`](cut-d-implementation.md)). Production regression: Rust body clobbered EDX; `get_service` required EDX=`SRV*` across the call → network loss. Fix: EDX-preserving FASM trampoline. Open (unchanged): FASM `cld` vs Rust DF leave-alone. **Live regressions (append-only):** [`regression-log.md`](regression-log.md) (REG-001 EDX/unicode×XFS readdir; REG-002 XFS volume `GetFileInfo`).
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
- **Cut AE:** `ntfs_datetime_to_bdfe` — **done** 2026-08-10 ([`cut-ae-plan.md`](cut-ae-plan.md), [`cut-ae-implementation.md`](cut-ae-implementation.md)). NTFS FILETIME (1601×10⁷) → BDFE; composes Cut T; Path B after post-AD cluster audit; reloc-free.
- **Cut AF:** `ntfsCalculateTime` — **done** 2026-08-10 ([`cut-af-plan.md`](cut-af-plan.md), [`cut-af-implementation.md`](cut-af-implementation.md)). NTFS BDFE → FILETIME (AE inverse; compose G); Path B after post-AE cluster audit; reloc-free.
- **Cut AG:** `ntfs_test_bootsec` — **done** 2026-08-10 ([`cut-ag-plan.md`](cut-ag-plan.md), [`cut-ag-implementation.md`](cut-ag-implementation.md)). NTFS bootsector multi-rule CF validate; Path B after post-AF raised-bar cluster audit (no Rust-owned subsystem yet); reloc-free.
- **Cut AH:** `calculate_SetChecksum_field` — **done** 2026-08-10 ([`cut-ah-plan.md`](cut-ah-plan.md), [`cut-ah-implementation.md`](cut-ah-implementation.md)). exFAT SetChecksum rolling hash (skip indices 2–3); Path B after post-AG raised-bar cluster audit (socket near-miss blockers unchanged); reloc-free.
- **Cut AI:** `exFAT_hash_calculate` (NameHash) — **done** 2026-08-10 ([`cut-ai-plan.md`](cut-ai-plan.md), [`cut-ai-implementation.md`](cut-ai-implementation.md)). exFAT NameHash via shared `exfat_rolling_checksum` (no skip); extracted from former inline in `exFAT_find_lfn`; Path B after post-AH audit (Path A SetChecksum+NameHash rejected — helper reuse ≠ Rust-owned subsystem); reloc-free.
- **Cut AJ:** `iso9660_compare_name` — **done** 2026-08-10 ([`cut-aj-plan.md`](cut-aj-plan.md), [`cut-aj-implementation.md`](cut-aj-implementation.md)). ISO9660 path-component UTF-8↔ASCII/UCS-2BE upper match (`;` version + `name_len`); Path B after post-AI raised-bar cluster audit (no Path A; ISO compare pair / sockets / exFAT-as-subsystem rejected); reloc-free.
- **Cut AK:** `xfs._.conv_bigtime_to_kos_epoch` — **done** 2026-08-10 ([`cut-ak-plan.md`](cut-ak-plan.md), [`cut-ak-implementation.md`](cut-ak-implementation.md)). XFS v5 bigtime (ns) → BDFE; Path B after post-AJ raised-bar cluster audit (`cd_compare_name` already AJ-routed; no Path A); reloc-free.
- **Cut AL:** `ext_read_time` — **done** 2026-08-11 ([`cut-al-plan.md`](cut-al-plan.md), [`cut-al-implementation.md`](cut-al-implementation.md)). EXT/ext4 Unix (+extra epoch bits) → BDFE; Path B after post-AK raised-bar cluster audit (first EXT foothold; no Path A); reloc-free.
- **Cut AM:** `xfs._.get_before_by_hashval` — **done** 2026-08-11 ([`cut-am-plan.md`](cut-am-plan.md), [`cut-am-implementation.md`](cut-am-implementation.md)). XFS DA interior-node first-match-by-hash (v4/v5; EBX=node quirk); Path B after post-AL raised-bar cluster audit (no Path A; complements Cut W without claiming XFS ownership); reloc-free.
- **Cut AN:** `ansi2uni_char` — **done** 2026-08-11 ([`cut-an-plan.md`](cut-an-plan.md), [`cut-an-implementation.md`](cut-an-implementation.md)). CP866 → Unicode decode (Cut A encode inverse); Path B after post-AM raised-bar + REG-001 trampoline discipline (ECX+EDX preserve); reloc-free.
- **Cut AO:** `fat_time_to_bdfe` — **done** 2026-08-11 ([`cut-ao-plan.md`](cut-ao-plan.md), [`cut-ao-implementation.md`](cut-ao-implementation.md)). DOS FAT packed-time → BDFE unpack (FAT+exFAT entry paths); Path B after post-AN raised-bar cluster audit (no Path A; `--disk exfat` A/B); reloc-free.
- **Cut AP:** `xfs_hashname` — **done** 2026-08-11 ([`cut-ap-plan.md`](cut-ap-plan.md), [`cut-ap-implementation.md`](cut-ap-implementation.md)). XFS dir name ROL7 hash (feeds W/AM lookup); Path B after post-AO raised-bar cluster audit (no Path A; `--disk xfs` A/B); reloc-free.
- **Cut AQ:** `get_pg_addr` — **done** 2026-08-11 ([`cut-aq-plan.md`](cut-aq-plan.md), [`cut-aq-implementation.md`](cut-aq-implementation.md)). Kernel VA→PA page translate (Stage-4 foothold); Path B after post-AP raised-bar cluster audit (no Path A; REG-001 ECX/EDX trampoline; `--disk xfs` DMA soak); reloc-free.
- **Cut AR:** `r_f_port_area` — **done** 2026-08-11 ([`cut-ar-plan.md`](cut-ar-plan.md), [`cut-ar-implementation.md`](cut-ar-implementation.md)). I/O port-area reserve/free (syscall 46; Cut X follow-on); Path B after post-AQ Stage-4 audit (paging Path A rejected; Stage-4 leaves exhausted); reloc-free.
- **Cut AS:** `socket_check` — **done** 2026-08-11 ([`cut-as-plan.md`](cut-as-plan.md), [`cut-as-implementation.md`](cut-as-implementation.md)). Socket-list lock-free ZF membership (Stage-5 foothold); Path B after post-AR raised-bar cluster audit (X+AR Path A rejected); reloc-free.
- **Cut AT:** `get_coff_sym` — **done** 2026-08-11 ([`cut-at-plan.md`](cut-at-plan.md), [`cut-at-implementation.md`](cut-at-implementation.md)). PE/COFF symbol name→Value scan (Stage-8 foothold); Path B after post-AS raised-bar cluster audit (socket Path A / Y+sym Path A rejected; preferred over `createMcbEntry`); reloc-free.
- **Cut AU:** `ipv4_find_fragment_slot` — **done** 2026-08-11 ([`cut-au-plan.md`](cut-au-plan.md), [`cut-au-implementation.md`](cut-au-implementation.md)). IPv4 reassembly fragment-slot keyed scan (Stage-5 foothold); Path B after post-AT raised-bar cluster audit (Y+AT+rebase Path A / createMcbEntry rejected); reloc-free.
- **Cut AV:** `ahci_find_cmdslot` — **done** 2026-08-11 ([`cut-av-plan.md`](cut-av-plan.md), [`cut-av-implementation.md`](cut-av-implementation.md)). AHCI free command-list slot bit scan (driver foothold); Path B after post-AU raised-bar cluster audit (network Path A / createMcbEntry / rebase_coff rejected); reloc-free. **Stop; do not start Cut AW.**

### Stage 3 — Compat syscall façade (selected)

- Easy query syscalls in Rust.
- **Status:** foothold via Cuts P/X/AR (userspace gate + I/O rights + port-area); broader `sysfn_*` query façade still open.

### Stage 4 — Memory exports

- Rust page/heap behind same symbols.
- **Status:** foothold via Cut AQ (`get_pg_addr`); allocator/map/fault remain FASM.

### Stage 5 — FS plugin / net protocol islands

- One filesystem; optional TCP path.
- **Status:** foothold via Cuts AC/M/V/AS/AU (IPv4 route + TCP timers + socket_check + fragment-slot lookup); AHCI cmdslot via Cut AV; broader protocol/driver islands still open.

### Stage 6 — Scheduler policy + process create

- High risk; extensive soak tests.
- **Status:** not started.

### Stage 7 — GUI server

- Last major app-facing move.
- **Status:** not started.

### Stage 8 — PE driver loader

- Export directory ownership in Rust.
- **Status:** foothold via Cuts Y/AT (`fix_coff_relocs` + `get_coff_sym`); loader orchestration remains FASM.

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
