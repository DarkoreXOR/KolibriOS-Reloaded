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

9. Desktop-only QEMU is **not** an XFS/NTFS/exFAT soak. Attach
   `python scripts/run_qemu.py --disk xfs` (etc.) and browse: dirs vs files,
   sizes, volume name, nested paths.
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

## Historical precedent (pre-log)

| Note | Where |
|------|--------|
| Cut D: Rust `strncmp` clobbered EDX; `get_service` lost `SRV*` → network | [`cut-d-implementation.md`](cut-d-implementation.md) |
| Cut A unicode: ECX preserve added for name loops; EDX left to callers | `kernel/unicode.inc`, REG-001 |
| Cut AM live-XFS A/B cleared the leaf; attrs/sizes were elsewhere | [`cut-am-implementation.md`](cut-am-implementation.md) |
