# Stage-4 EXT `SetFileInfo` write-path oracle

**Date:** 2026-08-14  
**Status:** COMPLETE — host/test tooling only  
**Inventory:** **103 / 138** (unchanged)  
**Production gates:** **104** (unchanged)  
**Production changes:** **NONE**  
**Cut CV:** not started  

Parent frontier: [`post-pte-next-frontier.md`](post-pte-next-frontier.md)

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Can EXT `SetFileInfo` be validated independently? | **Yes** (atime/mtime leaf) |
| Guest GetFileInfo confirms mutation? | **Yes** (ESFI.LOG + IMM/FIN markers) |
| Sync persists to CoW image? | **Yes** (`ext_SetFileInfo` → `disk_sync`; host inode match) |
| Durable guest log? | **Yes** — `/hd0/1/ESFI.LOG` on EXT CoW (v2, 164 bytes) |
| Host independent parser? | **Yes** — Python EXT2 mini walker |
| debugfs cross-check? | **Yes** — Docker e2fsprogs (`atime`/`mtime`/`ctime` hex agree) |
| Repeated QEMU (×3)? | **PASS**, `RESET=0` |
| Ready to migrate now? | **No cut yet** — evidence strong enough to *plan* a future Path B cut |
| Decision | **EXT SETFILEINFO EVIDENCE READY** |

Hardening (2026-08-14 follow-up): closed prior tooling gaps (floppy `ESFI.LOG` miss; Docker daemon was stopped). Daemon started from existing Docker Desktop install; no production FS changes.

---

## 1. EXT path audit

| Item | Location / fact |
|------|-----------------|
| Vtable | `ext_user_functions` in `kernel/fs/ext.inc` — GetFileInfo idx 5, SetFileInfo idx 6 |
| Syscall | **70** (`sys_file_system_lfn`) — `ebx` → f.70 struct |
| Safety | subfn 5 → 40-byte BDFE; subfn 6 → 32-byte buffer |
| `ext_SetFileInfo` | Locks; `findInode`; rejects `EXT4_IMMUTABLE_FL`; reads BDFE **atime @ buffer+16**, **mtime @ buffer+24** only |
| Encoding | `fsCalculateTime` (KOS seconds since 2001-01-01) + `UNIXTIME_TO_KOS_OFFSET` (978307200) → Unix inode `aTime`/`mTime` |
| Persist | `writeInode` → `writeSuperblock` → **`disk_sync`** (built-in) → unlock |
| Not mutated | attrs, ctime BDFE slot, size, mode, extents/blocks |
| `ext_GetFileInfo` | 40-byte BDFE; times via `ext_read_all_times` at +8; for 128-byte inodes slots are ctime/atime/mtime |

**Ownership / blast (future):** plugin-island leaf with shared EXT inode buffer + partition lock + disk sync. Not a clean isolated Path B without FS-plugin context; not a coherent shared Rust FS Path A yet. Closest future shape: **plugin Path B leaf** after gate design — still not authorized.

---

## 2. Selected metadata field

| Field | Value |
|-------|-------|
| Mutation | `INODE.aTime` + `INODE.mTime` |
| Target file | `/hd0/1/ROOT.TXT` (inode **12** on fixture) |
| Requested atime BDFE | 2012-07-04 14:22:11 → hex `0b160e000407dc07` |
| Requested mtime BDFE | 2018-11-23 09:05:30 → hex `1e050900170be207` |
| Expected Unix | atime **1341411731**, mtime **1542963930** |
| Units | EXT classic 32-bit Unix seconds (LE); no subsecond extras on `-I 128` image |

---

## 3. Guest ABI (sysfn70)

```text
struct f70 {
  u32 subfn;      // 5=GetFileInfo, 6=SetFileInfo
  u32 reserved0;
  u32 reserved1;
  u32 reserved2;  // unused for 5/6
  u32 buffer;     // ptr: 40-byte out (5) or 32-byte in (6)
  char path[];    // or empty + ptr at +21
};
```

- Entry: `eax=70`, `ebx=&f70`, `int 0x40`  
- Return: `eax=0` success; nonzero error (`ERROR_FILE_NOT_FOUND`, `ERROR_ACCESS_DENIED`, …)  
- SetFileInfo input layout: attrs(4)+flags(4)+ctime(8)+**atime(8)**+**mtime(8)**  
- Sync path: inside `ext_SetFileInfo` (`disk_sync`); no separate guest sync API required for this leaf  

---

## 4. Deterministic image + CoW

| Artifact | Path |
|----------|------|
| Reference EXT | `images/ext-image.img` (immutable; `python scripts/mkfs.py ext`) |
| Reference SHA256 | `878052dfc0f04416c68de7511f51962b836f9e4106a4c4509195b32a30b9a3b5` |
| CoW EXT | `dev_build/extsoak/ext-cow.img` (fresh copy each run) |
| Boot CoW | `prepare_image` + firstapp → `/sys/EXTSOAK1` |
| Recipe | `dev_build/extsoak/recipe.json` |

---

## 5. Guest driver

- Source: `tools/extsoak/extsoak.asm`  
- Build: `python scripts/build_extsoak.py` → `dev_build/extsoak/EXTSOAK1` (patches `run_id` over `0xDEADBEEF`)  
- Prepare: `python scripts/prepare_extsoak_image.py`  
- Flow: GetFileInfo → SetFileInfo → GetFileInfo → delay → GetFileInfo → edges → restore primary → board markers → **write `/hd0/1/ESFI.LOG`** → LAUNCHER  
- Markers: `EXTSOAK START|SET|PASS|FAIL|LOG|LOGFAIL`, `EXTSOAK IMM|FIN <32hex>`  

### 5.1 Hardened ESFI.LOG protocol

| Item | Value |
|------|--------|
| Path | `/hd0/1/ESFI.LOG` on the **EXT CoW** (not floppy `/sys`) |
| Why EXT | Survives in the disposable disk under test; extractable after shutdown |
| Format | Binary v2, 164 bytes, magic `ESFI` |
| Contents | flags, Get/Set eax, initial/req/imm/final BDFE, `run_id`, create/write eax, inode hint 12, path tag `ROOT`, ticks |
| Flush | EXT create + write path (plugin `disk_sync` on writes); final rewrite includes `FLAG_LOG_OK` |
| Non-contamination | Separate inode (observed **15**); target oracle remains inode **12** / `ROOT.TXT` only |
| Host extract | `extract_ext2_root_file()` from CoW → `run-*/ESFI.LOG` + `guest-report.json` |

---

## 5b. Failure classes (soak)

| Class | Meaning |
|-------|---------|
| `esfi-log-extraction` / `esfi-log-parse` / `esfi-log-run-id` / `esfi-log-persistence` | Guest log missing/bad |
| `guest-marker` / `guest-bdfe` | Board / BDFE mismatch |
| `host-inode` | Mini parser target mismatch |
| `debugfs-mismatch` / `debugfs-error` | Secondary cross-check failed |
| `DEBUGFS_CROSSCHECK_UNAVAILABLE` | Docker/native debugfs absent — recorded, not silent |
| `reset` / `timeout` / `tooling` | Environment |

Primary PASS requires ESFI.LOG + mini parser. debugfs PASS required when available.

---

## 6. Host expected-state model

```text
unix = fs_calculate_time(BDFE) + 978307200
```

Implemented in `scripts/ext_setfileinfo_oracle.py` (FASM-faithful month tables).  
Compare: guest IMM/FIN hex ↔ expected BDFE hex ↔ on-disk inode `atime`/`mtime`.

---

## 7. Host EXT parser + debugfs

| Parser | Role |
|--------|------|
| `parse_ext2_root_file_times` / `extract_ext2_root_file` | Independent SB@1024 walker |
| `debugfs` via Docker (or native) | Secondary cross-check; hex `atime:`/`mtime:`/`ctime:` |

`probe_debugfs_environment()` records Docker daemon / native availability.

**Do not** use production Rust EXT blobs as the oracle for this write path.

---

## 8. QEMU protocol

```text
python scripts/prepare_extsoak_image.py
python scripts/qmp_ext_setfileinfo_soak.py --repeats 3
```

Primary correctness: durable `ESFI.LOG` + host mini inode + debugfs when available — **not** non-black pixel count.

---

## 9. Live evidence

### 9.1 Initial oracle (pre-hardening)

Guest board hex + mini parser ×3 PASS; floppy ESFI.LOG missing; Docker stopped → debugfs skipped.

### 9.2 Hardened (2026-08-14)

| Run | PASS | RESET | ESFI.LOG run_id | debugfs | atime/mtime |
|-----|------|------:|-----------------|---------|-------------|
| harden1 | yes | 0 | 0x6A7E37CA | PASS | 1341411731 / 1542963930 |
| 1786656771-1 | yes | 0 | 0x6A7E3803 | PASS | same |
| 1786656814-2 | yes | 0 | 0x6A7E382E | PASS | same |
| 1786656861-3 | yes | 0 | 0x6A7E385D | PASS | same |

Summary: `dev_build/extsoak/summary.json` (`ok: true`, `reset_total: 0`).

Metadata diff: ROOT.TXT atime/mtime/ctime expected; size/mode/inode unchanged.  
Log side-effect: `ESFI.LOG` inode 15 classified `expected_test_log_artifact`.

Environment: Docker Desktop was **Stopped**; started existing install → daemon **running**; method `docker_debugfs` (alpine e2fsprogs). No native `debugfs` on Windows PATH.

Edge cases in guest: idempotent set, second distinct times (then restore), nonexistent path (nonzero eax).

---

## 10. Future migration readiness

| Criterion | Status |
|-----------|--------|
| Exact ABI | Documented |
| Independent metadata oracle | Mini parser |
| Durable ESFI.LOG | PASS on EXT CoW |
| Guest GetFileInfo | ESFI.LOG + board IMM/FIN |
| Sync persistence | Proven on CoW |
| debugfs cross-check | PASS (Docker) |
| Repeatability | ×3 PASS hardened |
| Safe rollback | Gate-off FASM body (future) |
| Blob/memory | Small leaf; ~2.1 KiB headroom OK **if** authorized later |
| Blast radius | Plugin lock + inode + SB + disk_sync — not Path A FS ownership |

Evidence is strong enough to author a **future Cut CV plan** for `ext_SetFileInfo` as a plugin Path B leaf.  
**Do not** implement the cut / gate in this task.

---

## 11. A/B scaffolding

Recipe records:

- OFF: legacy FASM `ext_SetFileInfo` (current baseline)  
- ON: future Rust — **not gated in this task**

---

## 12. Recommended next research/task (exactly one)

**Implement Cut CV** per [`cut-cv-plan.md`](cut-cv-plan.md) — **only when explicitly authorized**. Plan is complete; do not add the gate or Rust body in the planning turn.
