# Stage-4 NTFS boot/attach stall

**Date:** 2026-08-14  
**Status:** COMPLETE — diagnostic only (no Cut CW / no production migration)  
**Inventory:** **104 / 138** (unchanged)  
**Production gates:** **105** enabled  
**Production kernel changes:** **NONE**  
**Cut CV:** **COMPLETE** — not reopened  
**Parent:** [`post-cv-next-frontier.md`](post-cv-next-frontier.md), [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md)

> Isolate why KolibriOS stalls before firstapp when a minimal NTFS disk is attached.
> Does **not** implement `ntfs_SetFileInfo`, add `USE_RUST_NTFS_SET_FILE_INFO`,
> or change the memory pack.

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Image size the cause? | **No** — 16 MiB and 128 MiB both stalled until the FILE-record layout was fixed |
| Disk attach / bus the cause? | **No** — zeros and empty-MBR 16 MiB disks reach firstapp; EXT reaches firstapp |
| BIOS / bootloader stall? | **No** |
| ATA/AHCI detection stall? | **No** — IDE IDENTIFY and AHCI `adding 'sd0'` complete |
| Partition scan reached? | **Yes** (eager, before firstapp) |
| NTFS bootsec accepted? | **Yes** — Cut AG `ntfs_test_bootsec` replica PASS on the fixture |
| Failing function? | FASM `ntfs_create_partition` `.scandata` (`kernel/fs/ntfs.inc`) |
| Completed Rust cut to blame? | **No** — not a Cut CU/CV/AG/J regression; fixture was invalid |
| Kernel markers added? | **No** — existing msg_board + host `.scandata` replica sufficed |
| Decision | **NTFS IMAGE COMPATIBILITY ISSUE PROVEN** |

**STAGE-4 NTFS BOOT STALL — COMPLETE — STOP**

---

## 1. Control matrix

Same boot image: `dev_build/test/kernel-20260814-113254.img`  
Harness: `python scripts/qmp_ntfs_boot_stall.py --verbose --wait 22`  
Oracle: msg_board tags `Searchap:` / `AUTORUN.DAT` / `L: /SYS/` (not non-black pixels).

### 1.1 Broken fixture (USA overlaying `0xFFFFFFFF` terminator)

| Case | Bus | Disk | Size | Firstapp | RESET | Shutdown | Timeout | Phase | Elapsed |
|------|-----|------|------|----------|-------|----------|---------|-------|---------|
| A no extra disk | IDE | — | — | **Yes** | 0 | 0 | no | FIRSTAPP_OR_DESKTOP | 6.11 s |
| B EXT | IDE `-hda` | `images/ext-image.img` | 64 MiB | **Yes** | 0 | 0 | no | FIRSTAPP_OR_DESKTOP | 6.12 s |
| C NTFS 16 MiB MBR | IDE `-hda` | `ntfs-16m-mbr.img` | 16 777 216 | **No** | 0 | 0 | **yes** | AFTER_IDE_IDENTIFY | 22.24 s |
| D NTFS empty root | IDE `-hda` | `ntfs-16m-emptyroot.img` | 16 777 216 | **No** | 0 | 0 | **yes** | AFTER_IDE_IDENTIFY | 22.23 s |
| E NTFS 16 MiB MBR | AHCI | same as C | 16 777 216 | **No** | 0 | 0 | **yes** | AFTER_IDE_IDENTIFY | 22.27 s |
| F NTFS 16 MiB raw | IDE `-hda` | `ntfs-16m-raw.img` | 16 777 216 | **No** | 0 | 0 | **yes** | AFTER_IDE_IDENTIFY | 22.25 s |
| G NTFS 128 MiB MBR | IDE `-hda` | `ntfs-128m-mbr.img` | 134 217 728 | **No** | 0 | 0 | **yes** | AFTER_IDE_IDENTIFY | 22.23 s |
| H zeros 16 MiB | IDE `-hda` | `zeros-16m.img` | 16 777 216 | **Yes** | 0 | 0 | no | FIRSTAPP_OR_DESKTOP | 6.16 s |
| I empty MBR 16 MiB | IDE `-hda` | `mbr-empty-16m.img` | 16 777 216 | **Yes** | 0 | 0 | no | FIRSTAPP_OR_DESKTOP | 6.12 s |

Broken C SHA-256: `a836ce2d016fa4bd50c9724e6f2014ae6e7970601eb26cedf014bf73c874161b`

**First stalling configuration:** C — any attached volume that Kolibri identifies as NTFS.

EXT boots; NTFS stalls. Same-size non-NTFS disks do not stall.

### 1.2 After disposable recipe fix (USA at `0x30`, first attr at `0x38`)

All nine cases: **FIRSTAPP_OR_DESKTOP**, RESET=0, timeout=no, elapsed ≈ 6.1 s.

| Case | Firstapp | Non-black (secondary) |
|------|----------|------------------------|
| A | Yes (`L: /SYS/@TASKBAR`) | 36692 (board primary; pixels not a PASS oracle) |
| B | Yes | 786389 |
| C | **Yes** | 782017 |
| D | **Yes** | 784477 |
| E | **Yes** | 442425 |
| F | **Yes** | 786389 |
| G | **Yes** | 725995 |
| H | Yes | 785039 |
| I | Yes | 786389 |

Fixed C SHA-256: `fbe035f566e7b6d7f61d99f8bb807cbf60a3f9b86b67878d5d231d77ce0ac0a5`

---

## 2. QEMU configuration

| Item | Value |
|------|--------|
| Boot | CoW kernel image from `dev_build/last_image.txt` |
| IDE | `-hda` → guest `/hd0/1` (`run_qemu.append_ide_image`) |
| AHCI | AHCI port 0 → guest `/sd0/1` (`run_qemu.append_ahci_image`) |
| QMP | TCP `127.0.0.1:4610+` |
| Wait | 22 s (stall cases timed out; pass cases exited on board tags at ~6 s) |
| Status | QEMU running until harness kill |
| RESET | 0 on every case |
| Shutdown | 0 on every case |

---

## 3. Marker chain

No production `DEBUGF` / `NTFSSTALL` instrumentation was added.

Existing board phases used:

```text
BIOS_OR_EARLY
  → AFTER_IDE_IDENTIFY     (K : Channel / Device / AHCI adding 'sd0')
    → FIRSTAPP_OR_DESKTOP  (Searchap: / AUTORUN.DAT / L: /SYS/)
```

Host replica of Kolibri `.scandata` (in `scripts/qmp_ntfs_boot_stall.py`) adds:

```text
NTFS_BOOTSEC_OK
  → MFT0_FILE + USA_VALID
    → ATTR_WALK_OK          (0xFFFFFFFF terminator)
      vs ATTR_WALK_HANG     (sizeWithHeader=0)
```

| Image | Last successful | First missing |
|-------|-----------------|---------------|
| Broken NTFS (C–G) | AFTER_IDE_IDENTIFY; bootsec OK; MFT0 `FILE`+USA valid; attrs `$STANDARD_INFORMATION` + `$DATA` + `$BITMAP` seen | `0xFFFFFFFF` terminator; FIRSTAPP_OR_DESKTOP |
| EXT / zeros / empty MBR / no disk | FIRSTAPP_OR_DESKTOP | — |
| Fixed NTFS | FIRSTAPP_OR_DESKTOP | — |

AHCI broken-C last board lines: `K: AHCI: found drive on port 0` … `adding 'sd0'` — disk add finished; hang is inside subsequent `disk_detect_partition` → `ntfs_create_partition`.

IDE broken-C last `K :` lines: Channel 2 Disk 1 / Device not found — IDENTIFY complete; next kernel work is IRQ/cache/`boot_detectpart` with no further `K :` prints.

---

## 4. Failure phase

```text
search_partitions / disk_add_partition
  → disk_detect_partition
    → ntfs_create_partition
      → ntfs_test_bootsec          PASS (Cut AG)
      → read $MFT record 0         PASS (magic FILE)
      → ntfs_restore_usa_frs       PASS
      → .scandata                  HANG
```

`.scandata` (`kernel/fs/ntfs.inc` ~400–409):

```text
cmp dword [eax], -1          ; 0xFFFFFFFF terminator
jz  .fail_free_frs
cmp dword [eax], 0x80        ; unnamed $DATA
...
add eax, [eax+sizeWithHeader]
jmp .scandata
```

When `sizeWithHeader == 0`, `eax` does not advance → infinite loop. Boot never reaches desktop/firstapp. No RESET.

This is **eager mount** during partition scan, not firstapp touching NTFS.

---

## 5. Disk / layout

| Item | Broken and fixed fixtures |
|------|---------------------------|
| Bus | IDE `-hda` (default soak) and AHCI (both stalled when NTFS was invalid) |
| Sector size | 512 |
| Cluster size | 4096 (8 sectors/cluster) |
| MBR | Default `part_lba=2048`, type `0x07`; raw `part_lba=0` also stalled when invalid |
| GPT | None |
| Volume boot | Offset 1 048 576 (MBR) or 0 (raw); OEM `NTFS    `; 0x55AA |
| Total sectors (16 MiB MBR) | 30720 (bootsec); Cut AG replica OK |
| MFT LCN | 4 |
| MFT mirror LCN | 32 |
| CPR / CPI | −10 / −12 (1024-byte FRS, 4096-byte index) |
| Filesystem identification | `ntfs_test_bootsec` **accepts** the volume |

Kolibri accepts both partitioned and whole-disk NTFS. Layout was **not** the stall variable.

---

## 6. NTFS structures (independent host parser)

Parser: `scripts/ntfs_setfileinfo_oracle.py` + `walk_attrs_kolibri` in the stall script.

### 6.1 Broken generator (`ntfs_minimal._build_file_record` before this task)

| Structure | Observation |
|-----------|-------------|
| Boot record | Valid for Kolibri |
| MFT record 0 | USA **valid**; first attr at `0x30`; USA placed at end of attrs |
| `$DATA` / `$BITMAP` attrs | Present, sizes 72/68/68 |
| Terminator | Written as `0xFFFFFFFF`, then **overwritten by USA** when attr end was 8-byte aligned |
| MFT0 walk | `sizeWithHeader=0` at offset 256 (USA USN + zero tails) |
| Root (record 5) | Same hang class (`usa_offset` == end of last attr) |
| `$Bitmap` record 6 | Walk **OK** (attr end 292, USA at 296 — terminator survived) |
| MFT mirror | Present at LCN 32 |
| USA/fixups | Count 3; restore would succeed; leftover USA still occupies terminator slot |

### 6.2 Required Kolibri FILE layout

Windows-compatible:

| Field | Required value |
|-------|----------------|
| FILE header | 48 bytes |
| USA offset | `0x30` |
| USA count | sectors+1 (`3` for 1024-byte FRS) |
| First attribute offset (`0x14`) | `0x38` (after 6-byte USA, 8-aligned) |
| Attribute list | ends with DWORD `0xFFFFFFFF` **not** overlaid by USA |

### 6.3 Classification of the minimal image

| Option | Result |
|--------|--------|
| A valid NTFS, Kolibri-incompatible | Partial (USA-at-end *can* be valid **if** terminator remains) |
| B invalid NTFS | **Yes** — terminator destroyed |
| C valid and failing elsewhere | No (after recipe fix, firstapp runs) |
| D missing a Kolibri-required structure | **Yes** — live `0xFFFFFFFF` after last attribute |
| E never reaching NTFS init | **No** — bootsec + MFT read happen |

---

## 7. Root cause

`tools/mkfs_utils/ntfs_minimal.py` placed the Update Sequence Array at
`align8(end_of_attributes)` **without advancing past the terminator**. When that
offset equalled the terminator offset (typical: MFT0 attrs end at 256), the USA
USN (`1`) and stored tails overwrote `0xFFFFFFFF`.

After `ntfs_restore_usa_frs`, `.scandata` still walks into the USA. With stored
tails `[0, 0]`, `sizeWithHeader` at USA+4 is **0** → infinite loop.

Not caused by: image size, IDE vs AHCI, MBR vs raw, empty vs populated root,
firstapp, Cut CU allocator, Cut CV EXT SetFileInfo, Cut AG bootsec, Cut J USA
restore.

---

## 8. Corrected disposable recipe

Already applied in `tools/mkfs_utils/ntfs_minimal.py` `_build_file_record`:

1. USA at `0x30`, count = `MFT_RECORD_SIZE/512 + 1`.
2. First attribute at `0x38`.
3. Write attributes, then `0xFFFFFFFF`.
4. Apply sector-tail USA fixups **without** moving USA onto the terminator.

Host check: `mft0_walk.ok == true` (stall script logs this before QEMU).

Reproduce:

```text
python scripts/qmp_ntfs_boot_stall.py --verbose --wait 22
```

Expect all cases `ok: true`.

---

## 9. Completed-cut / regression check

| Cut | Role | Blamed? |
|-----|------|---------|
| AG `ntfs_test_bootsec` | Accepts bootsec (correct) | No — replica PASS |
| J `ntfs_restore_usa` | Restores tails (correct) | No |
| I / AX MCB | After `.scandata` | Not reached on broken image |
| CU allocator | Bitmap `alloc_pages` after `.scandata` | Not reached |
| CV `ext_SetFileInfo` | EXT only | No (EXT case boots) |

No FASM-OFF/ON bisect required. Production NTFS mount code was not changed.

---

## 10. Production vs disposable changes

| Change | Class |
|--------|--------|
| Kernel / gates / inventory / memory pack | **NONE** |
| `scripts/qmp_ntfs_boot_stall.py` | Diagnostic harness |
| `tools/mkfs_utils/ntfs_minimal.py` USA/first-attr layout | Disposable image generator (required recipe) |
| `part_lba` on `format_minimal_ntfs` | Disposable (MBR vs raw matrix) |

---

## 11. SetFileInfo status

Live NTFS SetFileInfo soak was blocked because firstapp never started.

**Boot/attach prerequisite is now satisfied** with the corrected `ntfs_minimal` recipe (firstapp + desktop board tags on 16 MiB and 128 MiB, IDE and AHCI).

SetFileInfo itself is **not** evidenced in this task. Do not start Cut CW.

**Next prerequisite:** run `python scripts/qmp_ntfs_setfileinfo_soak.py` ×3 against a soak image built with the corrected generator.

---

## 12. Artifacts

| Path | Role |
|------|------|
| `dev_build/ntfsstall/summary.json` | Matrix + hashes + board tails |
| `dev_build/ntfsstall/image-analysis.json` | Host bootsec/MFT/USA/walk |
| `dev_build/ntfsstall/ntfs-16m-mbr.img` | Fixed 16 MiB MBR fixture |
| `scripts/qmp_ntfs_boot_stall.py` | Reproduction |

Screendumps are disposable and may be deleted after the JSON is saved; non-black is secondary only.
