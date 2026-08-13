# Stage-4 NTFS `SetFileInfo` write-path oracle

**Date:** 2026-08-14  
**Status:** COMPLETE — host/test tooling + live soak; **Cut CW production ON**  
**Inventory:** **105 / 138**  
**Production gates:** **106** enabled  
**Cut CW:** [`cut-cw-implementation.md`](cut-cw-implementation.md)  

Parent: [`post-cv-next-frontier.md`](post-cv-next-frontier.md), boot stall [`stage4-ntfs-boot-stall.md`](stage4-ntfs-boot-stall.md), EXT precedent [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md)

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| NTFS path audited? | **Yes** — `$I30` index copy, not file MFT `$STANDARD_INFORMATION` |
| Guest ABI documented? | **Yes** — sysfn 70 subfn 5/6 |
| Host independent parser? | **Yes** — `$I30` + USA + `$STANDARD_INFORMATION` / `$FILE_NAME` sidecar |
| Guest driver? | **Yes** — `NTFSOAK1` + `NSFI.LOG` |
| QMP soak harness? | **Yes** — `scripts/qmp_ntfs_setfileinfo_soak.py` |
| Corrected image boots + firstapp? | **Yes** |
| Live QEMU ×3 NTFS SetFileInfo? | **PASS** — RESET=0, host `$I30` matches, SI/FN unchanged |
| Control (GetFileInfo only)? | **PASS** — `$I30` unchanged |
| debugfs / ntfsprogs? | **Unavailable** on this Windows host |
| Decision | **NTFS SETFILEINFO EVIDENCE READY — SECONDARY TOOLING GAP** |

**STAGE-4 NTFS SETFILEINFO SOAK — COMPLETE — STOP**

Do **not** start Cut CX. Production `ntfs_SetFileInfo` is Cut CW (`USE_RUST_NTFS_SET_FILE_INFO = 1`).

---

## 1. NTFS path audit (FASM)

| Item | Location / fact |
|------|-----------------|
| Vtable | `ntfs_user_functions` — GetFileInfo idx 5, SetFileInfo idx 6 |
| Entry | `ntfs_SetFileInfo` @ `kernel/fs/ntfs.inc` ~4294 |
| Syscall | **70** — `ebx` → f.70 struct |
| Lookup | `ntfs_find_lfn` → parent directory `$I30` index entry pointer in `EAX` |
| Writable fields | Index **`fileFlags`**, **`fileCreated`**, **`fileAccessed`**, **`fileModified`** |
| **Not** updated | File MFT `$STANDARD_INFORMATION`; `$FILE_NAME` attribute |
| Persist | `writeRecord` on parent index buffer → `ntfsDone` → **`disk_sync`** |

**Unlike EXT:** SetFileInfo must **preserve** attrs (+0) and ctime (+8) from prior GetFileInfo.

Index offsets: `fileCreated=0x18`, `fileModified=0x20`, `fileAccessed=0x30`, `fileFlags=0x48`.

---

## 2. Selected metadata mutation

| Field | Value |
|-------|-------|
| On-disk target | Root `$I30` index entry for **`ROOT.TXT`** |
| Guest path | `/hd0/1/ROOT.TXT` |
| MFT record (corrected fixture) | **19** (`EMPTY`/`NSFI.LOG`/`README`/`ROOT` at 16+) |
| Mutation | BDFE **atime** (+16) + **mtime** (+24) |
| Requested atime | 2012-07-04 14:22:11 → BDFE `0b160e000407dc07` |
| Requested mtime | 2018-11-23 09:05:30 → BDFE `1e050900170be207` |
| Host FILETIME (ADC-faithful) | atime `129858853310000000`, mtime `131874375300000000` |

Host expected FILETIME must include the `ntfsCalculateTime` **ADC carry** into the high dword (Cut AF). Omitting carry misses by exactly `2^32`.

---

## 3. Guest ABI

```text
eax=70, ebx=&f70, int 0x40
subfn 5 GetFileInfo (40-byte BDFE out)
subfn 6 SetFileInfo (32-byte in: attrs, flags, ctime, atime, mtime)
sync: inside ntfs_SetFileInfo (disk_sync via ntfsDone)
```

---

## 4. Corrected disposable image

Generator: `tools/mkfs_utils/ntfs_minimal.py` (not `images/ntfs-image.img`).

| Item | Value |
|------|--------|
| Reference | `dev_build/ntfssoak/ntfs-minimal-reference.img` |
| Size | 134 217 728 (128 MiB) |
| SHA-256 | `7d061686219c97c2d838a55f5ddb1303a7ed240ac72be66f68bf641cb4c179b9` |
| Layout | MBR + type-0x07 partition @ LBA **2048** |
| Geometry | 512 B/sector, 8 sectors/cluster, MFT LCN 4, mirror LCN 32, FRS 1024 |
| FILE header | USA @ `0x30`, first attr @ `0x38`, live `0xFFFFFFFF` terminator |
| Base record field | **0** (nonzero `baseRecordReuse` made Kolibri treat every record as auxiliary) |
| `$I30` stride | padded `indexAllocatedSize` (Kolibri `add esi, [indexAllocatedSize]`) |
| MFT refs | `(seq << 48) \| record` |
| User records | start at **16** (CreateFile denies `iRecord < 16`) |
| Pre-seeded log | `NSFI.LOG` 256-byte resident stub |

Create + soak:

```text
python scripts/qmp_ntfs_setfileinfo_soak.py --force-minimal --repeats 3 --with-control
```

---

## 5. Guest driver

| Item | Path |
|------|------|
| Source | `tools/ntfssoak/ntfssoak.asm` |
| Build | `python scripts/build_ntfssoak.py` |
| Flow | GetFileInfo → preserve attrs+ctime → SetFileInfo → Get ×2 → edges → restore primary times → `NSFI.LOG` |
| Control | `control_placeholder` patched to 1: GetFileInfo-only |
| Markers | `NTFSOAK START\|SET\|CTRL\|PASS\|FAIL\|LOG\|IMM\|FIN` |
| Log | `/hd0/1/NSFI.LOG`, magic `NSFI`, v1 |

---

## 6. Host oracle

| Item | Path |
|------|------|
| Parser | `scripts/ntfs_setfileinfo_oracle.py` |
| `$I30` | Parent record **5** → `ROOT.TXT` atime/mtime FILETIME |
| Sidecar | Target FILE `$STANDARD_INFORMATION` + `$FILE_NAME` must be **unchanged** |
| USA | `validate_root_usa()` on record 5 |
| Preflight | USA `0x30`, first attr `0x38`, terminator present |

Does **not** copy Kolibri `writeRecord`.

---

## 7. Live QMP soak (2026-08-14)

Boot image: `dev_build/test/kernel-20260814-122019.img`  
Command: `python scripts/qmp_ntfs_setfileinfo_soak.py --force-minimal --no-prepare --repeats 3 --with-control --wait 90`  
Bus: IDE `-hda` → `/hd0/1`

| Run | Firstapp | NTFSOAK | Set/Get | Log | Host `$I30` | SI/FN | USA | RESET | Shutdown |
|-----|----------|---------|---------|-----|-------------|-------|-----|-------|----------|
| 1 `1786710503-1` | Yes | START/SET/PASS/LOG | PASS | `NSFI.LOG` MFT 17 | PASS | unchanged | valid | 0 | clean harness stop |
| 2 `1786710541-2` | Yes | same | PASS | present | PASS | unchanged | valid | 0 | same |
| 3 `1786710578-3` | Yes | same | PASS | present | PASS | unchanged | valid | 0 | same |
| Control `1786710615-control` | Yes | START/CTRL/PASS/LOG (no SET) | Get-only | present | **unchanged** | unchanged | valid | 0 | same |

Guest BDFE (all mutate runs): IMM/FIN `0b160e000407dc071e050900170be207`.

Host after mutate (run 1):

| Field | Before | After | Expected |
|-------|--------|-------|----------|
| `$I30` accessed | 134311840220000000 | **129858853310000000** | 129858853310000000 |
| `$I30` modified | 134311840220000000 | **131874375300000000** | 131874375300000000 |
| `$I30` created | 134311840220000000 | unchanged | — |
| `$I30` flags | 32 | 32 | — |
| File `$STANDARD_INFORMATION` | attrs 32 + original times | **byte-identical** | unchanged |
| File `$FILE_NAME` times | original | **byte-identical** | unchanged |

Writeback: guest GetFileInfo matched requested BDFE; after shutdown the host `$I30` FILETIME matched Cut-AF-faithful expected values. CoW SHA changed vs the immutable reference.

Secondary oracle: **debugfs / ntfscat not installed**.

Artifacts: `dev_build/ntfssoak/summary.json`, `run-*/result.json`, `run-*/ntfs-cow.img`, `run-*/NSFI.LOG`, `run-*/metadata-diff.json`.

---

## 8. Future Path B readiness

| Criterion | Status |
|-----------|--------|
| ABI / narrow boundary | **Clear** — plugin leaf; preserve attrs+ctime; index-only write |
| Host `$I30` oracle | **Ready** (live) |
| USA / fixup | **Ready** (live; record 5 USA valid after write) |
| Guest durable log | **Ready** (live `NSFI.LOG`) |
| Writeback persistence | **Proven** ×3 |
| Repeatability | **×3 + control PASS**, RESET=0 |
| Blast radius | **Medium** — index `writeRecord` + lock; not `$STANDARD_INFORMATION` |
| Path B | **Clear** — no Path A |
| Remaining | Optional debugfs; Cut CW **plan** not yet authored |

**Do not start Cut CW** until a cut plan exists. Evidence bar for a future Path B cut is otherwise met.

---

## 9. Recommended next task

**One task:** implement Cut CW **only when authorized**. Plan: [`cut-cw-plan.md`](cut-cw-plan.md). Do **not** implement in the planning turn.

---

## 10. Tooling index

| Component | Path |
|-----------|------|
| Guest driver | `tools/ntfssoak/ntfssoak.asm` |
| Build | `scripts/build_ntfssoak.py` |
| Prepare | `scripts/prepare_ntfssoak_image.py --force-minimal` |
| Oracle | `scripts/ntfs_setfileinfo_oracle.py` |
| Soak | `scripts/qmp_ntfs_setfileinfo_soak.py` |
| Minimal mkfs | `tools/mkfs_utils/ntfs_minimal.py` |

**STAGE-4 NTFS SETFILEINFO ORACLE — COMPLETE — STOP**
