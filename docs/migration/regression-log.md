# Live regression log

**Purpose:** Append every production / QEMU filesystem (or other live)
regression that survives desktop smoke, with root cause, fix, and how to
avoid repeating it.

**Not for:** per-cut host differentials or “desktop OK” checklists — those stay
in the cut’s `*-implementation.md`.

**Related:** Cut D EDX precedent in [`cut-d-implementation.md`](cut-d-implementation.md);
XFS A/B that cleared Cut AM in [`cut-am-implementation.md`](cut-am-implementation.md);
[`risk-register.md`](risk-register.md).

---

## How to append

When a live regression is found and fixed, **append a new `REG-NNN` entry** at
the bottom of [Entries](#entries) (do not rewrite history). Then:

1. Add one line to the [Index](#index).
2. Fold any new pattern into [Avoidance checklist](#avoidance-checklist) if it
   is not already there.
3. Optionally add a row to [`risk-register.md`](risk-register.md).

### Entry template

```markdown
### REG-NNN — short title (YYYY-MM-DD)

| Field | Value |
|-------|-------|
| Symptom | What the user / soak saw |
| Suspected | First blame (often the latest cut) |
| Cleared by A/B | What was ruled out |
| Root cause | Actual mechanism |
| Fix | Files + change |
| Verify | How confirmed |
| Avoid next time | Concrete check for future cuts |
```

---

## Avoidance checklist

Use this before enabling a migration gate and when debugging a live FS bug.

### Register / ABI (stdcall vs legacy FASM)

1. **EDX/ECX survive across “leaf” calls** — Rust `stdcall` clobbers EAX, ECX,
   EDX. Legacy FASM helpers often left ECX/EDX intact. Callers keep pointers in
   those registers across `call unicode.*` / similar.
   - Precedent: Cut D (`strncmp` → `get_service` EDX=`SRV*`).
   - Precedent: REG-001 (`copy_filename` → BDFE base in EDX).
   - Precedent: REG-013 (`ext_ReadFolder` BDFE cursor in EDX across name encode).
   - Precedent: REG-003 (smoke wiped `net_device_list[0]` / loopback — not a leaf bug).
2. Prefer fixing at the **wrapper that owns the live register** (`uses …`) or
   the trampoline that claims FASM-compatible preserve — document which.
3. ABI smoke must assert **caller-live** registers, not only the leaf’s
   documented outs.
4. **Never use a callee-saved register as a merge temp** — e.g. `pop ebx` to
   reconstruct upper EAX clobbers live EBX (`fat_Write` → `get_time_for_file`;
   REG-005). Match original FASM preserve set (EBX/ESI/EDI/EBP) when the leaf
   only touched AX.

### Blame the latest cut last

5. **A/B before rewriting the new cut:** gate OFF, prior `cut-*-final.img`,
   disable related Rust FS/unicode gates as a group. If the bug remains, the
   new leaf is cleared (REG-001 / Cut AM).
6. Ask whether the hot path **even calls** the new symbol (root XFS shortform
   readdir never hits `get_before_by_hashval`).

### Filesystem feature parity

6. When adding or soaking an FS, compare **empty-path / volume** handling to
   EXT/FAT/NTFS (`GetFileInfo` `.volume`, NUL-terminated `bdfe.name`).
7. **Never leave `bdfe.name` uninitialized** on paths that return success to
   Eolite — garbage looks like “whitespace then random artifacts”.
8. Test disks must have intentional labels if the UI is compared to the FAT
   floppy (`kolibri`); empty `sb_fname` is valid but must still NUL-terminate.

### Soak coverage

9. Desktop-only QEMU is **not** an XFS/NTFS/exFAT/EXT soak. Attach
   `python scripts/run_qemu.py --disk xfs` (etc.) and browse: dirs vs files,
   sizes, volume name, nested paths. Attach smoke alone misses empty-name
   readdir bugs (REG-013 / REG-001).
10. Prefer a prior known-good image (`dev_build/cut-*-final.img`) as A/B
    baseline, not only gate flips on the current tree.
11. **ABI smoke must not mutate live subsystem tables** that boot already
    initialized (e.g. `net_device_list` after `loop_init`). Save/restore or
    query existing entries; never plant-then-zero a live slot. Desktop smoke
    alone will not catch a destroyed loopback (REG-003).
12. **ABI smoke must not iterate live hardware-adjacent maps** (e.g. TSS IO-map
    enable/disable range loops) during early init. With AHCI or other PCI
    devices attached, the map state may be in a condition that causes a tight
    loop to never terminate. Use error-path-only calls (`start > end`, empty
    table) or fully synthetic state. Always ensure `.fail` paths are
    **non-fatal** (return to caller; don't `jmp @b`). Desktop-only QEMU will
    not catch AHCI-init-stage hangs — test with `--bus ahci --disk …`
    (REG-004).
13. **Injected stdcall callbacks in smoke must use correct `ret N`** — mock
    readers called from Rust must match `fs_read_cmos` smoke (`ret 4` for one
    arg; `ret 20` for five-arg `disk_add_partition`). Plain `ret` corrupts the
    stack each callback (REG-006, REG-007).
14. **Injected partition callbacks must restore `ESI`+`EBX`** before
    `disk_add_partition` — the callee reads **`[esi+DISK…]`** and **`[ebp-4]`→MBR
    buffer via saved **`EBX`**, not the stack `disk` arg alone (REG-008).
    Trampoline must save **`EBX`** (MBR buffer from `disk_scan_partitions`) and
    thunk must `mov esi,[disk]` before the call.
15. **Do not busy-wait on live `timer_ticks` before multitasking** — during
    early `high_code`, `timer_ticks` is frozen until `timer_ticks_enable` +
    `sti` (near end of boot log). Smoke or tests that call `ahci_port_wait`
    with `timeout > 0` and a busy synthetic TFD spin forever (REG-006).
    Use mock tick callbacks or `timeout = 0` for immediate timeout vectors.
16. **Never `call rust_*` + `add esp, N` for stdcall blobs** — Rust
    `extern "stdcall"` emits `ret N`. FASM must use the `stdcall` macro (or
    plain `call` with **no** caller cleanup). Double cleanup destroys
    `pushad`/return and yields a bootloader reset loop; headless `-no-reboot`
    looks like `query-status: shutdown`. Pixel non-black alone is not enough —
    watch QMP **`RESET`** events (REG-009).
17. **Prefer naked stdcall thunks over `proc`+early `ret N`** — early `ret N`
    before `endp` skips `leave` and leaves the frame pointer on the stack.
    Match Cut patterns that `jmp` into the real FASM callee when arg layout
    matches (REG-009).
18. **Naked trampolines that `sub esp` for a ctx must count the return
    address** when indexing original stdcall args —
    `ctx_size + saved_regs + 4` (REG-010). Mis-indexing feeds the retaddr
    as an argument; ABI smoke `jmp $` then looks like a black “running”
    desktop with `resets=0`.
19. **PE-exported leaves must preserve EBX/ESI/EDI/EBP** even when the
    FASM body only listed `uses ecx edx` — the original `proc` kept
    those regs intact via frame / non-use. Rust stdcall clobbers them;
    trampoline must push/pop the full set. Smoke must canary all six
    (REG-011 — frozen mouse after Cut CF).
20. **Never move `SLOT_BASE` upward without proving `SLOT_BASE +
    256*sizeof.APPDATA` still equals `VGABasePtr` (0xA0000).** Raising
    `SLOT_BASE` by 4 KiB to reclaim Stage-2 headroom overlaps the VGA
    window, boots a desktop wallpaper, and kills `@PANEL` / app launch
    (REG-012). Prefer shrinking the new leaf (smoke/blob) or rejecting
    the candidate over breaking the sys_proc↔SLOT↔VGA pack.
21. **FASM comments that say “bswap” may not be `bswap`.** Kolibri unpacker
    uses `shr ax,8` / `ror eax,16` / `xchg al,ah`, which **discards** the
    original high byte. `u32::swap_bytes()` / Intel `bswap` is a different
    function (REG-016). Emulate the instructions.
22. **Flag tests on `AL`/`AX` must not be widened to EAX.** Method/E8 bits
    live in the low byte; high bytes can be nonzero (`SDHCI.SYS` flags
    `0x1000081`) (REG-016).
23. **Match-literal `lea [base+ecx*4+0x100*4]` with `CH=match_bit`** addresses
    planes `256+symbol` **and** `512+symbol` inside `LZMA_LIT_SIZE=768`.
    Always using `probs[256+symbol]` is wrong (REG-015).
24. **`MENUET01` / COFF magic is not a full-file oracle.** Compare every dest
    byte. Unicorn-compare the OFF FASM body against the i686 blob on **all**
    boot KPCK (`LAUNCHER` can match while `@TASKBAR` E8 patches diverge).
25. **A multi-KiB `.text` blob is not free vs the TMP_STACK_TOP assert** —
    Kolibri is one linear org through `.bss`. Moving uninitialized LUTs after
    `sys_pgmap` (still first-4MiB PSE, B32-wiped) can reclaim space without
    touching `SLOT_BASE` (Cut CO / REG-012).
26. **Never use `in("esi")` (or `inout("esi")`) in i686 LLVM inline asm** —
    ESI is an LLVM internal. Pin the logical ESI value to EDX/ECX and
    `mov esi, …` in the template (REG-017).
27. **`setc r8` then `pop` of that register discards CF.** Capture CF with
    `sbb dest, dest` into a dedicated lateout **before** pops. Never `setc al`
    when EAX is a live callback result (REG-018).
28. **Host tests do not cover `cfg(target_os = "none")` invoke asm.** In-kernel
    smoke must exercise CF=0 callback paths, not only the `fn_ptr==0` skip.
29. **A `call` inside inline asm clobbers ECX/EDX (and ESI unless saved).**
    `in("ecx")` / `in("edx")` without `lateout` lets LLVM reuse those regs on
    the next loop iteration. Templates that `mov esi, …` must `push esi` /
    `pop esi` — ESI is an LLVM internal (cannot be a lateout operand)
    (REG-019). One-shot smoke (`next` always STC) does not catch this.
    Desktop non-black is not an Eolite/`load_file` oracle.

---

## Index

| ID | Date | Title | Status |
|----|------|-------|--------|
| [REG-001](#reg-001--xfs-directories-as-files-zero-sizes-2026-08-11) | 2026-08-11 | XFS directories as files / zero sizes | Fixed |
| [REG-002](#reg-002--xfs-volume-label-whitespace--garbage-2026-08-11) | 2026-08-11 | XFS volume label whitespace + garbage | Fixed |
| [REG-003](#reg-003--no-network-after-cut-ay-smoke-2026-08-11) | 2026-08-11 | No network after Cut AY smoke | Fixed |
| [REG-004](#reg-004--ahci-4-disk-init-screen-hang-no-desktop-2026-08-12) | 2026-08-12 | AHCI 4-disk init-screen hang (no desktop) | Fixed |
| [REG-005](#reg-005--intermittent-desktop-hang-cut-ca-fsreadcmos-ebx-2026-08-12) | 2026-08-12 | Intermittent desktop hang (Cut CA fsReadCMOS EBX) | Fixed |
| [REG-006](#reg-006--init-screen-hang-cut-cb-ahci-port-wait-smoke-2026-08-12) | 2026-08-12 | Init-screen hang (Cut CB ahci_port_wait smoke) | Fixed |
| [REG-007](#reg-007--post-bootloader-reset-cut-cc-smoke-2026-08-12) | 2026-08-12 | Post-bootloader reset (Cut CC smoke mock ret) | Fixed |
| [REG-008](#reg-008--post-bootloader-reset-cut-cc-disk-add-partition-esi-ebx-2026-08-12) | 2026-08-12 | Post-bootloader reset (Cut CC disk_add_partition ESI/EBX) | Fixed |
| [REG-009](#reg-009--bootloader-reset-loop-cut-cc-stdcall-double-cleanup-2026-08-12) | 2026-08-12 | Bootloader reset loop (Cut CC stdcall double cleanup) | Fixed |
| [REG-010](#reg-010--black-desktop-cut-cf-trampoline-arg-offset-2026-08-12) | 2026-08-12 | Black desktop (Cut CF trampoline arg offset) | Fixed |
| [REG-011](#reg-011--mouse-cursor-frozen-cut-cf-ebxesiediebp-2026-08-12) | 2026-08-12 | Mouse cursor frozen (Cut CF EBX/ESI/EDI/EBP) | Fixed |
| [REG-012](#reg-012--no-taskbar--apps-wont-launch-cut-cm-slot_base-vga-overlap-2026-08-13) | 2026-08-13 | No taskbar / apps won't launch (Cut CM SLOT_BASE↔VGA overlap) | Fixed |
| [REG-013](#reg-013--ext-eolite-empty-names--zero-sizes-2026-08-13) | 2026-08-13 | EXT Eolite empty names / zero sizes | Fixed |
| [REG-014](#reg-014--ext-eolite-names-ok-all-file-sizes-0-2026-08-13) | 2026-08-13 | EXT Eolite names OK, all file sizes 0 | Fixed |
| [REG-015](#reg-015--splash-only-desktop-cut-co-lzma-match-literal-planes-2026-08-13) | 2026-08-13 | Splash-only desktop (Cut CO LZMA match-literal planes) | Fixed |
| [REG-016](#reg-016--splash-only-desktop-cut-co-e8-not-bswap-2026-08-13) | 2026-08-13 | Splash-only desktop (Cut CO E8 not-bswap) | Fixed |
| [REG-017](#reg-017--black-desktop-cut-cq-get_name-ebp--utf-8-path-2026-08-13) | 2026-08-13 | Black desktop (Cut CQ get_name EBP = UTF-8 path) | Fixed |
| [REG-018](#reg-018--black-desktop-cut-cq-setc-pop-discarded-first-cf-2026-08-13) | 2026-08-13 | Black desktop (Cut CQ setc/pop discarded first CF) | Fixed |
| [REG-019](#reg-019--eolite-and-apps-hang-on-exfat-cut-cq-callback-clobbers-2026-08-13) | 2026-08-13 | Eolite / apps hang on exFAT (Cut CQ callback clobbers) | Fixed |

---

## Entries

### REG-001 — XFS directories as files / zero sizes (2026-08-11)

| Field | Value |
|-------|-------|
| Symptom | Eolite on `/hd0/1` (XFS): directories looked like files; all sizes **0**. Names often still present. |
| Suspected | Cut AM (`xfs._.get_before_by_hashval`) — timing coincided with the report. |
| Cleared by A/B | Full AM rollback; `cut-al-final.img`; disabling XFS Rust gates R/W/T/AK/AM together — **same bug**. Root shortform readdir never calls `get_before`. |
| Root cause | Cut A Rust unicode `stdcall` (`unicode.utf8.decode` / `cp866.encode` / `utf16.encode`) **clobbers EDX**. `xfs._.dir_sf_read` keeps the BDFE base in **EDX** across `xfs._.copy_filename`, then `mov edi, edx` and `xfs_get_inode_info` write attr/size into garbage → names may land, attrs/sizes do not. Related: `.` entry used `[ebp+XFS.cur_inode]` (block buffer) instead of `[ebp+XFS.cur_inode_save]` (live inode). |
| Fix | `kernel/fs/xfs.asm`: `proc xfs._.copy_filename uses eax edx`; `.` entry uses `cur_inode_save`. Notes in `kernel/unicode.inc` (EDX callee-clobbered; caller must preserve). |
| Verify | User soak: directories and correct sizes (e.g. `README.TXT` ≈ 127). |
| Avoid next time | Audit every `call unicode.*` / Rust stdcall site for live EDX/ECX; treat Cut D as the class precedent. ABI smoke for name copy should keep a BDFE canary in EDX. Do not assume the newest cut caused an FS browse bug without A/B. |

**Class:** register preserve / stdcall ABI drift (same family as Cut D).

---

### REG-002 — XFS volume label whitespace + garbage (2026-08-11)

| Field | Value |
|-------|-------|
| Symptom | After REG-001 fix: dirs/sizes OK, but disk/volume name showed **whitespace then memory junk**. Reference FAT volume shows `kolibri`. |
| Suspected | Label encoding / unicode path. |
| Cleared by A/B | `images/xfs-image.img` had empty `sb_fname` (12 zero bytes). Boot floppy FAT label still `KOLIBRI`. Symptom was XFS `GetFileInfo` empty path, not FAT. |
| Root cause | `xfs_GetFileInfo` had **no empty-path `.volume` handler** (EXT/NTFS/FAT do). Volume query filled size/attr via inode helpers or left fields unset and **never wrote/cleared `bdfe.name`** → Eolite displayed uninitialized memory. |
| Fix | `kernel/fs/xfs.inc` + `xfs.asm`: store `sb_fname` in `XFS.fname` at mount; empty-path `.volume` sets attr=8, partition byte size, encoding, and **NUL-terminated** name (encode path mirrors EXT). Test image / mkfs: `mkfs.xfs -L kolibri` in `tools/mkfs_utils/create_xfs_image.py` and `docker_populate_xfs.sh`. |
| Verify | User soak: volume shows `kolibri`, no junk. |
| Avoid next time | FS cut or soak checklist: empty path → volume BDFE with terminated name. Compare new FS `GetFileInfo` to EXT. Set intentional labels on regression disks when UI is compared to the boot floppy. |

**Class:** missing FS feature parity / uninitialized userspace-facing buffer.

---

### REG-003 — no network after Cut AY smoke (2026-08-11)

| Field | Value |
|-------|-------|
| Symptom | No network connection after Cut AY; Cut AX image still had network. |
| Suspected | Cut AY Rust `net_ptr_to_num4` trampoline / register preserve. |
| Cleared by A/B | Leaf algorithm + trampoline ABI smoke already passed host differentials and boot marker `NPT4`; failure was independent of gate ON vs leaf correctness. |
| Root cause | `net_ptr_to_num4_rust_smoke_test` runs **after** `stack_init` → `loop_init`, which registers loopback at `net_device_list[0]`. Smoke planted a fake pointer into slot 0 then wrote `0`, **orphaning loopback** while leaving `net_device_count` stale. Desktop non-black smoke still passed. |
| Fix | `kernel/rust/net_ptr_to_num4.inc`: stop mutating the live list; hit-test uses the existing loopback pointer at slot 0; miss/null still use non-member/null queries. |
| Verify | Rebuild ON; QEMU desktop PASS; user network path should match Cut AX (loopback present). |
| Avoid next time | Post-`stack_init` network smokes must save/restore or only read live `net_device_list` / similar tables. Never plant-then-zero boot-initialized slots. |

**Class:** ABI smoke destroys live boot state (not stdcall register drift).

---

### REG-004 — AHCI 4-disk init-screen hang (no desktop) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | `python scripts/run.py --disk exfat --disk ntfs --disk iso9660 --disk xfs --bus ahci` stopped at the **kernel initialization screen** (black screen with white log lines, last line "Reserving IRQs & ports") — desktop stage never reached. Reference `scripts/reference_qemu.py` with same args booted normally. |
| Suspected | AHCI-related migration cuts (AV, BG, BM, AQ, BA). |
| Cleared by A/B | Individually disabling all AHCI-related gates left the hang unchanged. Root shortform AHCI path never caused it. |
| Root cause | The **Cut AR ABI smoke** (`r_f_port_area_rust_smoke_test`) runs right after `reserve_irqs_ports`. Its **successful Rust reserve** path calls `enable_range` on ports `0xF100..0xF107`, iterating the TSS IO-map. With AHCI disks attached, the hardware/map state left by AHCI probing caused this loop to never return. The smoke also had a **fatal `jmp @b` hang marker** on failure, so any mismatch in that environment guaranteed a frozen init stage. Additionally, the original implementation used a stack-allocated synthetic reserved table whose memory was corrupted by `stdcall` argument pushes, and a `rep stosd` that wrote backwards if DF was set. Desktop-only smoke (no AHCI) never exposed this because those conditions were not triggered. |
| Fix | `kernel/rust/r_f_port_area.inc`: (1) moved synthetic reserved table to a global `iglobal` buffer (`r_f_port_area_smoke_reserved_ports`); (2) replaced the successful reserve/free smoke path with **error-path-only** calls (`start > end` → immediate return 1, no IO-map loop); (3) made the `.fail` path **return non-fatally** (sets `rust_r_f_port_area_smoke_result = 'FAIL'` and returns) instead of looping forever. Public trampoline canary retained via the same `start > end` error path to keep REG-001 (ECX/EDX) coverage. `docs/migration/cut-ar-implementation.md` updated accordingly. |
| Verify | QEMU 4-disk AHCI run reaches desktop (non-black ~779380 pixels); confirmed via `qmp_desktop_smoke.py --bus ahci --disk exfat --disk ntfs --disk iso9660 --disk xfs`. |
| Avoid next time | ABI smoke must not call any path that iterates live hardware-adjacent state (IO-map enable/disable loops, network device lists, etc.) during early init. Always use error-paths or synthetic-only state when the boot environment cannot guarantee a stable map. Non-fatal `.fail` paths prevent a smoke bug from permanently blocking desktop. |

**Class:** ABI smoke destroys / hangs live early-init state (same family as REG-003).

---

### REG-005 — Intermittent desktop hang (Cut CA fsReadCMOS EBX) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | ~70% of VM restarts: desktop appears but **no app icons**, mouse clicks dead; ~30% normal. User-reported after Cut CA. |
| Suspected | Cut CA `fsReadCMOS` Rust trampoline. |
| Cleared by A/B | Gate OFF (`USE_RUST_FS_READ_CMOS=0`) should restore stability (user can verify). Desktop QMP smoke passed both configs because it does not exercise `fat_Write` timestamp path. |
| Root cause | Cut CA trampoline used **`pop ebx`** to merge upper EAX after `stdcall rust_fs_read_cmos`, destroying **live EBX** on every CMOS read. Original FASM `fsReadCMOS` only touches AX. `fat_Write` (`kernel/fs/fat.inc`) sets `ebx` = write offset, then calls `get_time_for_file` / `get_date_for_file` (each 3× `fsReadCMOS`) before `cmp ecx, ebx` — corrupted offset → FS/GUI failure. Intermittent rate depends on boot-time FAT write activity. |
| Fix | `kernel/fs/fs_common.inc`: stack-only EAX merge; push/pop **EBX/ESI/EDI/EBP** around Rust call. `kernel/rust/fs_read_cmos.inc`: smoke adds EBX canary across `fsReadCMOS` (REG-004 pattern). |
| Verify | Rebuild ON; user soak 10+ restarts; QEMU smoke PASS. |
| Avoid next time | When original leaf preserves all GPRs except partial EAX, trampoline must not borrow callee-saved regs for temps. Smoke must include **caller-live EBX** pattern from real call sites (e.g. `fat_Write`). |

**Class:** stdcall/trampoline register drift (same family as REG-001).

---

### REG-006 — Init-screen hang (Cut CB ahci_port_wait smoke) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | Boot stops at the **kernel initialization logging screen** (after bootloader, before desktop). QEMU may appear hung for minutes or indefinitely. |
| Suspected | Cut CB `ahci_port_wait` Rust leaf or live AHCI poll. |
| Cleared by A/B | Live `ahci_port_wait` algorithm matches FASM; failure was **smoke-only** at `ahci_port_wait_rust_smoke_test` (runs after `ahci_init`, before APIC/memory boot_log lines complete). |
| Root cause | Two smoke bugs: (1) mock `cb_smoke_read_tfd*` callbacks used plain **`ret`** instead of **`ret 4`** for stdcall — stack corruption on every poll iteration. (2) public-trampoline timeout vector used **`timeout=1`** with synthetic busy TFD but **live `ahci_port_wait_read_ticks`** while **`timer_ticks` is frozen** until `timer_ticks_enable`/`sti` at end of boot — infinite spin in the poll loop. |
| Fix | `kernel/rust/ahci_port_wait.inc`: proper `proc … stdcall` mocks; timeout vector changed to **`timeout=0`** (immediate timeout without timer advance). |
| Verify | Rebuild ON; QEMU reaches desktop in normal time; user init screen progresses. |
| Avoid next time | Match `fs_read_cmos` mock pattern (`ret 4`). Never busy-wait on live `timer_ticks` in pre-multitasking smoke. Prefer mock tick callbacks for all `rust_*` direct tests. |

**Class:** ABI smoke / early-init timer assumption (REG-004 family).

---

### REG-007 — Post-bootloader reset (Cut CC smoke mock `ret`) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | VM **resets** immediately after the bootloader stage (before desktop). |
| Suspected | Cut CC `process_partition_table_entry` Rust leaf or live partition scan. |
| Cleared by A/B | Live leaf + trampoline ABI match FASM; failure was **smoke-only** at `process_partition_table_entry_rust_smoke_test` in `high_code` (after Cut Z/AD smokes). |
| Root cause | Mock `cc_smoke_disk_add_partition` (`stdcall`, five args) used plain **`ret`** instead of **`ret 20`** — stack corruption when the direct `rust_process_partition_table_entry` smoke vector invoked the callback. Secondary: extended-path smoke checked `cc_smoke_ext` instead of the caller stack slot written by the public trampoline (`pop eax` / `0x4321`). |
| Fix | `kernel/rust/process_partition_table_entry.inc`: mock uses **`ret 20`**; extended vector checks **`pop eax`** against `0x4321`. |
| Verify | Rebuild ON; QEMU reaches desktop; `rust_process_partition_table_entry_smoke_result == 'PART'`. |
| Avoid next time | Scale mock stdcall cleanup to arg count (`ret 4` × N dwords). Do not assert global `cc_smoke_ext` when the public trampoline writes the caller's `[esp+4]` extended slot. |

**Class:** ABI smoke stack corruption (REG-006 family).

---

### REG-008 — Post-bootloader reset (Cut CC `disk_add_partition` ESI/EBX) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | VM **resets** after bootloader when **`--bus ahci`** + multiple **`--disk`** images attached (partition scan during AHCI media init). Desktop-only floppy smoke still passed. |
| Suspected | Cut CC `process_partition_table_entry` Rust leaf or live partition scan. |
| Cleared by A/B | Gate OFF (`USE_RUST_PROCESS_PARTITION_TABLE_ENTRY=0`) stable; ON fails only when Rust path calls live `disk_add_partition`. |
| Root cause | Trampoline passed raw **`disk_add_partition`** to Rust. That routine uses **`ESI`→DISK** and entry **`EBX`→MBR buffer** (`disk_detect_partition` reads `[ebp-4]` from saved **`EBX`**), not the stdcall `disk` stack arg alone. Rust clobbers **`ESI`/`EBX`** before invoking the callback → garbage DISK pointer / buffer → triple fault during **`disk_scan_partitions`**. |
| Fix | `cc_trampoline_mbr_buf` saved in trampoline; **`cc_disk_add_partition_for_rust`** thunk sets **`ESI`**+**`EBX`** then **`stdcall disk_add_partition`**. |
| Verify | `python scripts/qmp_desktop_smoke.py --wait 90 --bus ahci --disk exfat --disk ntfs --disk iso9660 --disk xfs` PASS (779380 non-black). Image: `dev_build/test/kernel-20260812-150740.img`. |
| Avoid next time | Audit every Rust-injected callback against callee **register** deps, not only stack args. Match `fix_coff_symbols`/`get_proc_ex` thunk pattern. |

**Class:** live callback register contract / REG-001 family.

---

### REG-009 — Bootloader reset loop (Cut CC stdcall double cleanup) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | Bootloader → crash → bootloader loop with `python scripts/run.py --disk exfat --disk ntfs --disk iso9660 --disk xfs --bus ahci`. Headless QMP often reported **PASS** (non-black bootloader pixels) or **`shutdown`** (`-no-reboot`). |
| Suspected | Cut CC live `disk_add_partition` path (after REG-008). |
| Cleared by A/B | Disabling Cut CC ABI smoke alone restored desktop; gate OFF still failed while smoke used `call`+`add esp,28`. |
| Root cause | Trampoline/smoke did `call rust_process_partition_table_entry` + **`add esp, 28`** while the blob ends in **`ret 28`** (stdcall) → double cleanup. Secondary: FASM `proc stdcall` thunks with early **`ret 20`** skipped **`leave`**. |
| Fix | Trampoline uses FASM **`stdcall rust_…`**. Naked `cc_disk_add_partition_for_rust` tail-`jmp`s into `disk_add_partition` after setting **ESI/EBX**. Smoke mock is naked **`ret 20`**. QMP smoke counts **RESET** events. |
| Verify | Desktop + AHCI 4-disk: `resets=0`, `query-status: running`, non-black=779380. Image: `dev_build/test/kernel-20260812-152359.img`. |
| Avoid next time | Match Cut Z/AD: always `stdcall rust_*`. Treat non-black without RESET watch as insufficient for reboot-loop bugs. |

**Class:** stdcall vs cdecl trampoline mismatch / smoke ABI.

---

### REG-010 — Black desktop (Cut CF trampoline arg offset) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | QEMU `query-status: running`, `resets=0`, screendump **non-black=0** (never reached desktop). |
| Suspected | Cut CF `set_mouse_data` Rust leaf / HID globals. |
| Cleared by A/B | Gate ON with corrected trampoline reaches desktop (779380); first ON image hung in ABI smoke. |
| Root cause | Naked trampoline built a 44-byte stack `SetMouseDataCtx` after `push ecx`/`push edx`, then read stdcall args at **`[esp+52]`** — that slot is the **return address**, not `BtnState`. Correct base is **`ctx(44)+saved(8)+retaddr(4)=56`**. Smoke compared wrong results → `jmp $` hang before desktop. |
| Fix | `kernel/hid/mousedrv.inc`: arg loads at `[esp+56..72]`. |
| Verify | `python scripts/qmp_desktop_smoke.py --wait 90` PASS — non-black=779380, resets=0. |
| Avoid next time | When `sub esp, N` under a naked stdcall entry, always add **+4 for the return address** in the arg-offset formula; unit-check one known vector in smoke before relying on desktop pixels alone. |

**Class:** trampoline stack-frame arithmetic / smoke hang (related to REG-006/009).

---

### REG-011 — Mouse cursor frozen (Cut CF EBX/ESI/EDI/EBP) (2026-08-12)

| Field | Value |
|-------|-------|
| Symptom | Desktop boots, but **mouse cursor does not move** (HID input dead). |
| Suspected | Cut CF `set_mouse_data` Rust leaf / absolute-vs-relative compose. |
| Cleared by A/B | Gate OFF restores mouse; host oracle + desktop non-black smoke still passed ON. |
| Root cause | Naked Cut CF trampoline preserved only **ECX/EDX** (matching FASM `uses`), but used **EBX/ESI/EDI/EBP** as arg temps and let Rust clobber them. Original `proc set_mouse_data stdcall uses ecx edx` kept **EBP** via frame and never wrote **EBX/ESI/EDI**, so PE `SetMouseData` callers (PS/2/USB drivers) kept live state across the call. Destroying those regs broke the driver after return → no further cursor updates. |
| Fix | `kernel/hid/mousedrv.inc`: push/pop **EBX/ESI/EDI/EBP** (+ ECX/EDX); arg base **72**. `kernel/rust/set_mouse_data.inc`: smoke canaries for all six. |
| Verify | Rebuild ON; QEMU desktop PASS (779380); QMP mouse soak PASS. Image: `dev_build/test/kernel-20260812-165948.img`. User confirms cursor moves. |
| Avoid next time | For PE exports, treat **EBX/ESI/EDI/EBP** as preserved unless the legacy body truly clobbers them. ABI smoke must canary callee-saved regs, not only `uses` list. |

**Class:** PE-export register preserve / REG-001 family.

---

### REG-012 — No taskbar / apps won't launch (Cut CM SLOT_BASE↔VGA overlap) (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | Desktop wallpaper / icons appear, but **no bottom taskbar (`@PANEL`)** and **apps cannot be launched**. QEMU still `running`, `resets=0`; non-black dropped **779380 → 750708**. |
| Suspected | Cut CM `getInodeLocation` Rust math / trampoline. |
| Cleared by A/B | Gate OFF with the same moved `SLOT_BASE` still broken → not the Rust leaf. |
| Root cause | Cut CM raised `SLOT_BASE` `0x90000 → 0x91000` (and `sys_proc`/`TMP_STACK_TOP` to `0x8F000`) so Stage-2 could fit the 101 B blob + smoke. `APPDATA × 256 = 0x10000` must end at **`VGABasePtr = 0xA0000`**. New end `0xA1000` overlapped the VGA screen-access window → slot/process state corruption; panel and `fs_execute` paths fail while background still paints. |
| Fix | Revert pack: `SLOT_BASE=0x90000`, `sys_proc=0x8E000`, `TMP_STACK_TOP=0x8E000`. Fit CM by compacting ABI smoke (one direct `rust_*` vector; hang marker via iglobal default `DEAD0C6D`) so end `.bss` stays `0x8CFC3` under the `data32.inc` assert (`align 16` cliff). |
| Verify | Host `gilo_*` 11/11 + suite 745/745; QEMU OFF/ON/A/B/ON×3/`--disk ext` all **779380**, `resets=0`. Final image `dev_build/test/kernel-20260812-210330.img`. |
| Avoid next time | Before moving `SLOT_BASE`/`sys_proc`, assert `SLOT_BASE + 0x10000 == VGABasePtr`. Treat taskbar/@PANEL launch as a required soak when touching the process slot pack — desktop non-black alone is insufficient (wallpaper can still draw). |

**Class:** Stage-2 memmap / fixed-address pack (SLOT↔VGA).

---

### REG-013 — EXT Eolite empty names / zero sizes (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | Eolite on `/sd3/1` (EXT): entry **count OK** (e.g. Files: 9), but **empty names**, blank types, **0 B** sizes; dirs look like empty files. Other volumes OK. |
| Suspected | Cut CM `getInodeLocation` trampoline / out-slots (first EXT browse soak after CM tooling). |
| Cleared by A/B | Gate OFF still broken for names → **not** the CM leaf. Mount + root inode read still worked ON (count path OK). |
| Root cause | Same class as REG-001: Rust Cut A `unicode.utf8.decode` / `cp866.encode` / `utf16.encode` **clobber EDX**. `ext_ReadFolder` keeps the BDFE cursor in **EDX** and does `add edx, 40+264` (or `+520`) after the name loop. After `.` / `..`, EDX is garbage → later BDFEs never land in the result buffer (slots stay zeroed) while the folder **count** still increments. Latent since Cut A; first exposed when Cut CM added `--disk ext`. |
| Fix | `kernel/fs/ext.inc` `ext_ReadFolder`: `push edx` / `pop edx` around unicode calls on the CP866 and UTF-16 name paths (UTF-8 `rep movsb` path already safe). |
| Verify | Rebuild with CM ON (end `.bss` `0x8CFC3`); QEMU `--disk ext --bus ahci` desktop **779380**, `resets=0`. Image: `dev_build/test/kernel-20260812-213744.img`. Names restored; sizes were still 0 → [REG-014](#reg-014--ext-eolite-names-ok-all-file-sizes-0-2026-08-13). |
| Avoid next time | When adding a new FS soak, grep that FS’s readdir/name-copy for live **EDX/ECX** across `unicode.*`. Desktop attach smoke does **not** open Eolite — name/size browse is a separate checklist item (REG-001). |

**Class:** register preserve / stdcall ABI drift (REG-001 / Cut A unicode).

---

### REG-014 — EXT Eolite names OK, all file sizes 0 (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | After REG-013: EXT names/dirs visible, but **every file size is 0 B** (e.g. `README.TXT`). |
| Suspected | Follow-on from REG-013 soak; Cut BR `ext_read_all_times` vs `ext_ReadFolder` size `stosd`. |
| Cleared by A/B | N/A (REG-013 already cleared unicode EDX); size path is independent. |
| Root cause | Legacy `ext_read_all_times` advances **EDI by +24** (3× `fsTime2bdfe`). Cut BR Rust trampoline used `stdcall` only — callee restores EDI — so `ext_ReadFolder`’s post-call `stosd` size writes hit **BDFE+8** (timestamps) instead of **BDFE+32**. Size fields stay zero. Smoke compared `lea edi,[out+24]` to a constant (tautology) and never checked live EDI. Cut BR docs wrongly claimed callers ignore post-call EDI. |
| Fix | `kernel/fs/ext.inc`: `add edi, 24` after `rust_ext_read_all_times`. `kernel/rust/ext_read_all_times.inc`: smoke asserts `edi == out+24` without reloading. |
| Verify | Rebuild ON (end `.bss` `0x8CFC3`); QEMU `--disk ext --bus ahci` desktop **779380**, `resets=0`. Image: `dev_build/test/kernel-20260812-214237.img`. **User soak: EXT names + sizes OK.** |
| Avoid next time | When replacing a leaf that advances EDI/ESI via `stos`/`fsTime2bdfe`, trampoline must restore that side effect. ABI smoke must compare **live** EDI/ESI after the public symbol — never `lea` the expected value then `cmp` it to itself. |

**Class:** trampoline missing post-condition / pointer advance (related to REG-001 family).

---

### REG-015 — Splash-only desktop (Cut CO LZMA match-literal planes) (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | Cut CO gate ON: QEMU `running`, `resets=0`, **non-black≈6330–8020** (boot splash). OFF+LUT desktop **779380**. A/B FAIL. |
| Suspected | Rust KPCK/LZMA vs FASM `unpack`. |
| Cleared by A/B | OFF with the same pitch-LUT move still **779380** — LUT placement is not the desktop break. |
| Root cause | FASM `LzmaLiteralDecodeMatch` does `lea eax,[base+ecx*4+0x100*4]` with `CH=match_bit` (`setc ch`), so slots are `256+(match_bit<<8)+symbol` (planes **256** and **512** inside `LZMA_LIT_SIZE=768`). Production+oracle always used `probs[256+symbol]`. Host PRNG mostly `dest_len=0` / fail flags, so both sides agreed and missed FASM. |
| Fix | `rust_kernel/kolibri_utils/src/unpack.rs` `decode_literal_matched` / oracle `lit_match`. Fixture `testdata/launcher.kpck` (`upck_real_launcher_kpck`). |
| Verify | Host: LAUNCHER KPCK → `MENUET01`, `upck_*` PASS. Live desktop still failed until [REG-016](#reg-016--splash-only-desktop-cut-co-e8-not-bswap-2026-08-13) (`LAUNCHER` E8 count is 0). |
| Avoid next time | Translate `lea` with `CH` in the index as a 9-bit slot, not “+256”. Add a real compressed fixture, not only dest_len=0 PRNG. |

**Class:** FASM addressing / LZMA literal context (hidden by weak random corpus).

---

### REG-016 — Splash-only desktop (Cut CO E8 not-bswap) (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | After REG-015: still splash **~8020**, `resets=0`. Unicorn FASM=blob for `LAUNCHER`/`PS2MOUSE.SYS`; **41/238** KPCK files diverged (including `@TASKBAR`). |
| Suspected | Remaining LZMA mismatch; `fs_execute` / heap. |
| Cleared by A/B | Same as REG-015 (OFF+LUT desktop OK). |
| Root cause | FASM `.c1`/`.c2` comment says `"bswap eax" is not supported on i386` then uses `shr ax,8` / `ror eax,16` / `xchg al,ah`. That sequence is **not** Intel `bswap` (high byte discarded). Rust used `u32::swap_bytes()`. Files with E8/Jcc count 0 (`LAUNCHER`) matched; `@TASKBAR` flags `0x81` did not. Secondary: flags method/E8 tests were 32-bit (`flags & !0xC0 != 1`) vs FASM **AL** (`SDHCI.SYS` `0x1000081`). |
| Fix | `fasm_load_rel32` matching the three-instruction sequence; AL-only flag tests. Fixture `testdata/taskbar.kpck`. |
| Verify | Unicorn OFF FASM vs blob **238/238 EQ**. QEMU OFF `kernel-20260813-111308.img` **779380**; ON `kernel-20260813-121344.img` **779380**; A/B; ON×3; `resets=0`. Host `upck_*` 13/13, suite 764/764. |
| Avoid next time | Emulate the exact FASM byte-swap sequence. Do not widen AL flag tests. Compare **all** dest bytes of **all** boot KPCK, not header magic / not only `LAUNCHER`. |

**Class:** ISA-literal translation / E8 filter (hidden by count=0 fixtures).

---

### REG-017 — black desktop (Cut CQ get_name EBP = UTF-8 path) (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | Cut CQ gate ON: QEMU `running`, `resets=0`, **non-black=0** (black framebuffer). Smoke hang marker `DEAD0C71`. |
| Suspected | Trampoline stdcall / UTF-8 fill / live `exFAT_get_name`. |
| Cleared by A/B | Gate OFF desktop **779380**. Smoke skipped (production trampoline not on floppy desktop path) also **779380** — hang is smoke/`get_name` invoke, not blit/unpack. |
| Root cause | `invoke_kernel_get_name` did `mov esi, esi_in` **before** `mov ebp, fs`. LLVM kept `fs` in ESI, so EBP became the UTF-8 path pointer. FASM `exFAT_get_name` then used a garbage `exFAT*`. `in("esi")` is forbidden (LLVM internal). |
| Fix | Pin `f`/`fs`/`esi_in` to EBX/ECX/EDX; `mov ebp, ecx` then `mov esi, edx`; result ESI via a non-ESI lateout. |
| Verify | Combined with REG-018: ON `kernel-20260813-142651.img` **779380**, `resets=0`; OFF `kernel-20260813-143457.img` **779380**; A/B; ON×3; `--disk exfat`. Host `flfn_*` 16/16, suite 790/790. |
| Avoid next time | Never `in("esi")`. Always `mov ebp, fs` **before** clobbering the register that might still hold `fs`. |

**Class:** LLVM inline-asm register aliasing / callback ABI (get_name EBP).

---

### REG-018 — black desktop (Cut CQ setc/pop discarded first CF) (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | After REG-017 pin: `first=0` smoke vector PASS (desktop **779380**); `first` CLC vector **black non-black=0**, `resets=0`, `DEAD0C71`. UTF-8 cap / mini-fs pointers did not unhang. |
| Suspected | Unbounded UTF-8 write; NULL `LFN_reserve_place`/`path_in_utf8` stores. |
| Cleared by A/B | `first=0` (no callback) boots; hang is specifically post-CLC `first`. |
| Root cause | `invoke_kernel_dir_fn` emitted `setc cl` then `pop ecx`. CF=0 from a successful `first` was overwritten by the function-pointer low byte (nonzero). Rust took the CF=1 path and returned the pair pointer in EAX. Smoke `cmp eax,5` failed → `jmp @b`. Host tests never execute `cfg(target_os = "none")` invoke asm. |
| Fix | Capture CF with `sbb ebx, ebx` (dir) / `sbb eax, eax` (get_name) **before** pops. Do not `setc al` when EAX is the callback error code. |
| Verify | ON `kernel-20260813-142651.img` vector0+vector1 smoke, desktop **779380**, `resets=0`; OFF **779380**; A/B; ON×3; `--disk exfat`. Blob **1301 B / 0 reloc**. |
| Avoid next time | After `call` + `push`/`pop` of argument registers, capture CF into a dedicated lateout that is not subsequently popped. In-kernel smoke must include a CF=0 callback vector. |

**Class:** LLVM inline-asm CF capture / callback ABI.

---

### REG-019 — Eolite and apps hang on exFAT (Cut CQ callback clobbers) (2026-08-13)

| Field | Value |
|-------|-------|
| Symptom | Desktop and **WebView** OK; **Eolite / KFAR / other apps that open `/hd0/1` (testdisk exFAT)** hang or never finish. FAT LFN launch of Eolite itself (`/sys/FILE MANAGERS/EOLITE`) still works. |
| Suspected | Cut CQ `exFAT_find_lfn` production path (desktop smoke never opened Eolite or `load_file` on exFAT). |
| Cleared by A/B | Gate OFF: `load_file` of `/hd0/1/README.TXT`, `FILES WITH SPACES/HELLO WORLD.TXT`, `NESTED/A/FILE_A1.TXT` returns the fixture sizes (0x7F / 0x41 / 0x2E); desktop **779380**. Gate ON before the fix: same `load_file` never returned (partial desktop, boot log left on screen). |
| Root cause | `invoke_kernel_dir_fn` / `invoke_kernel_get_name` `call` clobber ECX/EDX. Missing `lateout` let LLVM keep FS/fn_ptr in those regs across the directory-walk loop. `get_name` also did `mov esi, edx` without saving ESI (LLVM internal). One-shot ABI smoke called `next` once (always STC) so it passed. Host `flfn_*` inject hooks, not the kernel invoke asm. |
| Fix | `exfat_find_lfn.rs`: `lateout("ecx")` / `lateout("edx")` on dir callbacks; `push`/`pop ebx/ebp/esi` around both `call`s; `lateout("ecx")` on get_name. Smoke vector 1: `next` CLC once then STC (two get_name/next pairs). |
| Verify | Host `flfn_*` 16/16 + suite **790/790**. ON `load_file` sizes match OFF; desktop **779380**, `resets=0`. Two-callback smoke PASS. Blob **1324 B / 0 reloc**. |
| Avoid next time | Any leaf that `call`s a kernel callback in a loop must lateout every cdecl/stdcall clobber and preserve ESI around `mov esi`. Smoke must iterate the callback at least twice. Attach testdisk and `load_file` a non-empty exFAT path — desktop attach-only is not enough (REG-001/013). |

**Class:** LLVM inline-asm clobber / callback loop (REG-017/018 family).

---

## Historical precedent (pre-log)

| Note | Where |
|------|--------|
| Cut D: Rust `strncmp` clobbered EDX; `get_service` lost `SRV*` → network | [`cut-d-implementation.md`](cut-d-implementation.md) |
| Cut A unicode: ECX preserve added for name loops; EDX left to callers | `kernel/unicode.inc`, REG-001 |
| Cut AM live-XFS A/B cleared the leaf; attrs/sizes were elsewhere | [`cut-am-implementation.md`](cut-am-implementation.md) |
