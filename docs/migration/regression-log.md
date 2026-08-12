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

### Blame the latest cut last

4. **A/B before rewriting the new cut:** gate OFF, prior `cut-*-final.img`,
   disable related Rust FS/unicode gates as a group. If the bug remains, the
   new leaf is cleared (REG-001 / Cut AM).
5. Ask whether the hot path **even calls** the new symbol (root XFS shortform
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

---

## Index

| ID | Date | Title | Status |
|----|------|-------|--------|
| [REG-001](#reg-001--xfs-directories-as-files-zero-sizes-2026-08-11) | 2026-08-11 | XFS directories as files / zero sizes | Fixed |
| [REG-002](#reg-002--xfs-volume-label-whitespace--garbage-2026-08-11) | 2026-08-11 | XFS volume label whitespace + garbage | Fixed |
| [REG-003](#reg-003--no-network-after-cut-ay-smoke-2026-08-11) | 2026-08-11 | No network after Cut AY smoke | Fixed |
| [REG-004](#reg-004--ahci-4-disk-init-screen-hang-no-desktop-2026-08-12) | 2026-08-12 | AHCI 4-disk init-screen hang (no desktop) | Fixed |

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

## Historical precedent (pre-log)

| Note | Where |
|------|--------|
| Cut D: Rust `strncmp` clobbered EDX; `get_service` lost `SRV*` → network | [`cut-d-implementation.md`](cut-d-implementation.md) |
| Cut A unicode: ECX preserve added for name loops; EDX left to callers | `kernel/unicode.inc`, REG-001 |
| Cut AM live-XFS A/B cleared the leaf; attrs/sizes were elsewhere | [`cut-am-implementation.md`](cut-am-implementation.md) |
