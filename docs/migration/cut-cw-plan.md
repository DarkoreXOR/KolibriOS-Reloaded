# Cut CW Plan — `ntfs_SetFileInfo`

**Date:** 2026-08-14  
**Status:** **COMPLETE** — see [`cut-cw-implementation.md`](cut-cw-implementation.md)  
**Inventory:** **105 / 138**  
**Production gates:** **106** enabled (`USE_RUST_NTFS_SET_FILE_INFO = 1`)  
**Production changes:** Path B leaf `ntfs_SetFileInfo` only  
**Evidence:** [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md) — **NTFS SETFILEINFO EVIDENCE READY — SECONDARY TOOLING GAP**  
**Frontier:** [`post-cv-next-frontier.md`](post-cv-next-frontier.md)  
**Boot-stall (image only):** [`stage4-ntfs-boot-stall.md`](stage4-ntfs-boot-stall.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> **Nomenclature:** **Cut CW** migrates **only** `ntfs_SetFileInfo` in
> `kernel/fs/ntfs.inc` (NTFS plugin Path B leaf).  
> **COMPLETE.** Do **not** start Cut CX.  
> Do **not** reopen Cut CV, Cut CU, or PTE ownership.  
> Do **not** create filesystem Path A ownership.

---

## 0. Verdict

| Item | Value |
|------|--------|
| Target | `ntfs_SetFileInfo` |
| Source | `kernel/fs/ntfs.inc` (vtable @ 18–31; body @ 4294–4339) |
| Subsystem | NTFS parent-directory `$I30` metadata write |
| Path | **B** |
| Proposed gate | `USE_RUST_NTFS_SET_FILE_INFO` |
| Proposed smoke marker | `NSFI` (`0x4E534649`) |
| Decision | **CUT CW COMPLETE** |
| Production changes in this task | Path B `ntfs_SetFileInfo` only |

Live soak (FASM baseline, 2026-08-14): runs `1786710503-1` / `1786710541-2` / `1786710578-3` + control `1786710615-control` **PASS**, RESET=0. debugfs/ntfscat absent — **not** a hard gate.

---

## 1. Fresh source audit (2026-08-14)

Re-read against live `kernel/fs/ntfs.inc`. Evidence summary still matches production FASM. **Do not copy EXT Cut CV ABI.**

### 1.1 External contract (sysfn 70 / plugin vtable)

Comment at `ntfs_user_functions` (lines 10–15):

```text
; ebx -> parameter structure of sysfunc 70
; ebp -> NTFS structure
; esi -> path string in UTF-8
; out: eax, ebx = return values for sysfunc 70
```

| Item | Fact |
|------|------|
| Syscall | `eax=70`, `ebx` → f.70 struct (`sys_file_system_lfn`) |
| Subfn | **6** = SetFileInfo |
| Safety | `file_system_is_operation_safe`: buffer length **32** for subfn 6 (`fs_lfn.inc` `.case6`) |
| Dispatch | dyndisk → `PARTITION.FSUserFunctions` → `ntfs_user_functions` index **6** (`disk.inc` `.callFS`) |
| Plugin in | `ebx` → f.70; `ebp` → `NTFS` (extends `PARTITION`); `esi` → UTF-8 path |
| Plugin out | `eax` = `ERROR_*`; `ebx` see §4.B |
| Stack | `call`/`ret` (not stdcall) for plugin entry |
| Direct callers | **None** — sole production entry is the vtable slot |

Vtable (`ntfs_user_functions`): GetFileInfo idx 5, SetFileInfo idx 6, Delete idx 8. CW touches **only** idx 6.

### 1.2 Internal body (verified)

```4294:4339:kernel/fs/ntfs.inc
ntfs_SetFileInfo:
        call    ntfs_lock
        call    ntfs_find_lfn
        jnc     @f
        test    eax, eax
        jz      ntfsFail
        jmp     ntfsNotFound

@@:
        cmp     [ebp+NTFS.fragmentCount], 1
        jnz     ntfsUnsupported     ; record fragmented
        mov     esi, [ebp+NTFS.cur_index_buf]
        cmp     dword [esi], 'INDX'
        jz      @f
        sub     eax, esi
        mov     esi, [ebp+NTFS.indexRoot]
        movzx   edx, byte [esi+attributeOffset]
        add     eax, esi
        add     eax, edx
@@:
        mov     esi, [ebx+16]
        mov     edi, eax
        mov     eax, [esi]
        and     eax, 27h
        and     byte [edi+fileFlags], -28h
        or      [edi+fileFlags], al
        add     esi, 8
        call    ntfsCalculateTime
        mov     [edi+fileCreated], eax
        mov     [edi+fileCreated+4], edx
        add     esi, 8
        call    ntfsCalculateTime
        mov     [edi+fileAccessed], eax
        mov     [edi+fileAccessed+4], edx
        add     esi, 8
        call    ntfsCalculateTime
        mov     [edi+fileModified], eax
        mov     [edi+fileModified+4], edx
        mov     ebx, [ebp+NTFS.cur_index_buf]
        cmp     dword [ebx], 'INDX'
        jz      @f
        mov     ebx, [ebp+NTFS.frs_buffer]
@@:
        mov     edx, [ebp+NTFS.LastRead]
        call    writeRecord
        jmp     ntfsDone
```

Shared error tails (4341–4374): `ntfsUnsupported` / `ntfsNotFound` / `ntfsFail` / … → `ntfsOut` → `ntfs_unlock` + `xor ebx,ebx` + `pop eax` + `ret`.

Success tail:

```2867:2872:kernel/fs/ntfs.inc
ntfsDone:
        mov     esi, [ebp+PARTITION.Disk]
        call    disk_sync
        call    ntfs_unlock
        xor     eax, eax
        ret
```

### 1.3 What is mutated vs not

| On-disk object | Behavior |
|----------------|----------|
| Parent `$I30` index entry `fileFlags` low byte bits 0/1/2/5 | **Written** (`attrs & 0x27`) |
| Parent `$I30` `fileCreated` (0x18) | **Written** from BDFE ctime @+8 |
| Parent `$I30` `fileAccessed` (0x30) | **Written** from BDFE atime @+16 |
| Parent `$I30` `fileModified` (0x20) | **Written** from BDFE mtime @+24 |
| Parent `$I30` `recordModified` (0x28) | **Unchanged** (unlike Create/Write) |
| Parent `$I30` `fileRealSize` / `fileAllocatedSize` / name | **Unchanged** |
| File MFT `$STANDARD_INFORMATION` | **Unchanged** (live soak) |
| File MFT `$FILE_NAME` | **Unchanged** (live soak) |
| MFT record identity | **Unchanged** (fixture record **19**) |

Live soak preserved ctime by round-tripping GetFileInfo into the SetFileInfo buffer. The **source still writes `fileCreated`**. Rust must do the same.

### 1.4 NTFS vs EXT (do not conflate)

| Dimension | NTFS (`ntfs_SetFileInfo`) | EXT (`ext_SetFileInfo`, Cut CV) |
|-----------|---------------------------|----------------------------------|
| Lookup | `ntfs_find_lfn` → `$I30` index pointer | `findInode` → inode # + `inodeBuffer` |
| Write target | Parent directory index copy | File inode `aTime`/`mTime` |
| ctime in 32-byte buffer | **Applied** to `fileCreated` | **Ignored**; `writeInode` packs cTime as side effect |
| Immutable / RO file | **No check** | `EXT4_IMMUTABLE_FL` → `ERROR_ACCESS_DENIED` |
| Partition RO early-exit | **None** in this leaf (`ntfs_lock` is mutex only) | `extfsWritingInit` non-local `ret` |
| Persist helper | `writeRecord` (USA + `fs_write64_sys`) | `writeInode` + `writeSuperblock` |
| Sync | `ntfsDone` → `disk_sync` | explicit `disk_sync` then `ext_unlock` |
| Success `ebx` | Record buffer pointer (post-`writeRecord`) | leftover / 0 on some fail paths |

---

## 2. Path B justification

| Claim | Result |
|-------|--------|
| Single NTFS plugin leaf? | **Yes** — vtable idx 6 only |
| Narrow metadata semantics? | **Yes** — `$I30` flags + three FILETIMEs |
| Real production caller? | **Yes** — sysfn70.6 |
| Independent oracle? | **Yes** — guest `NSFI.LOG` + custom `$I30`/USA/SI/FN parser |
| Deterministic CoW writeback? | **Yes** — evidence ×3 + control PASS |
| Rust-owned NTFS FS state? | **No** — lock, LFN, USA, `writeRecord`, bitmap, MFT alloc stay FASM |
| Path A filesystem? | **Forbidden / incorrect** |
| PTE / Cut CU / Cut CV? | Unrelated; do not reopen |

This is **not** NTFS subsystem ownership. Adjacent Path B leaves (I/J/AE/AF/AG/AX/BT) stay independent.

---

## 3. Cut boundary

### 3.1 In scope

- Public symbol `ntfs_SetFileInfo` only (gate ON body).
- `$I30` field writes: masked `fileFlags`, `fileCreated`, `fileAccessed`, `fileModified`.
- Timestamp conversion via existing `ntfsCalculateTime` (Cut AF trampoline; already Rust).
- Ordering: mutate → `writeRecord` → `ntfsDone`.

### 3.2 Out of scope (remain FASM)

| Helper / area | Why stay FASM |
|---------------|---------------|
| `ntfs_lock` / `ntfs_unlock` / `NTFS.Lock` | Mutex ownership |
| `ntfs_find_lfn` | Path walk / `$INDEX_ROOT` / `$INDEX_ALLOCATION` / USA restore on INDX read |
| INDX vs resident `$INDEX_ROOT` pointer fixup | Plumbing (12 instructions) |
| `fragmentCount != 1` | Post-lookup FS constraint; reuse `ntfsUnsupported` |
| `writeRecord` | USA increment + sector tail swap + `fs_write64_sys` |
| `ntfs_restore_usa` / Cut J | Read path only |
| `ntfsDone` / `disk_sync` | Cache + driver flush + unlock + `eax=0` |
| `ntfsCalculateTime` body | Already Cut AF |
| `ntfs_GetFileInfo` / Create / Write / Delete / SetFileEnd | Other plugin ops |
| MFT allocation / bitmap / `createMcbEntry` | Unrelated write infra |
| Other FS plugins | EXT/FAT/exFAT SetFileInfo |

### 3.3 Preferred call graph

```text
sysfn70.6 → dyndisk → ntfs_user_functions[6]
    ↓
ntfs_SetFileInfo  [FASM trampoline]
    ├─ ntfs_lock              [FASM]
    ├─ ntfs_find_lfn          [FASM; CF error → ntfsFail / ntfsNotFound]
    ├─ fragmentCount == 1     [FASM; else ntfsUnsupported]
    ├─ INDX vs FRS entry ptr  [FASM]
    └─ rust_ntfs_set_file_info(ctx)  [Rust, gate ON]
           ├─ mask fileFlags + 3× FILETIME stores     [Rust]
           ├─ ntfsCalculateTime ×3                    [FASM/AF callback]
           ├─ writeRecord                             [FASM callback; USA+disk]
           └─ ntfsDone                                [FASM callback; sync+unlock+eax=0]
```

**Hard rule:** lock acquire and path lookup stay in FASM **before** Rust. Rust never implements USA/fixup/`fs_write64_sys`. Rust **must** invoke `ntfsDone` (or an equivalent unlock+sync path) so the mutex is not leaked. Recommended design has **no Rust-side error return** after lookup (fragment check stays FASM); therefore Rust always ends in `ntfsDone`.

Rejected alternative: `find_lfn` as a Rust CF-capturing callback (EXT-style). Larger blob, REG-018 risk, no semantic gain.

---

## 4. Exact legacy ABI

### 4.A External syscall / plugin contract

| Item | Value |
|------|--------|
| Entry | Plugin ABI above |
| Buffer | `[ebx+16]` → 32-byte SetFileInfo block |
| Layout | `attrs(4) + flags(4) + ctime(8) + atime(8) + mtime(8)` |
| NTFS uses | attrs @+0 (masked); **ctime @+8, atime @+16, mtime @+24** (BDFE datetime) |
| NTFS ignores | flags dword @+4; BDFE directory bit `0x10` (stripped by `and 27h`) |
| Path | `esi` UTF-8 relative to volume (after dyndisk) |
| Empty path | **No** volume special-case (unlike `ntfs_GetFileInfo`) — goes through `find_lfn` |
| Success | `eax=0` |
| Failure | nonzero `eax` (`ERROR_*` below) |
| DF | Not a public plugin contract; helpers require DF=0 for `movsw` |

BDFE datetime (8 bytes), same as Cut G/AF:

```text
+0 sec, +1 min, +2 hour, +3 pad, +4 day, +5 month, +6..+7 year (u16 LE)
```

### 4.B Internal sequencing ABI (must preserve)

| Step | Registers / state |
|------|-------------------|
| After `ntfs_lock` | `NTFS.Lock` held; `ecx` = mutex; EAX/EDX may be clobbered |
| After `ntfs_find_lfn` CF=0 | `eax` → `$I30` index entry in `cur_index_buf`; `cur_iRecord` = target MFT; `LastRead` / `indexRoot` / `rootLastRead` filled; `esi` restored to original path |
| After `find_lfn` CF=1, `eax=0` | `ERROR_FS_FAIL` (9) via `ntfsFail` |
| After `find_lfn` CF=1, `eax≠0` | `ERROR_FILE_NOT_FOUND` (5) via `ntfsNotFound` |
| `fragmentCount != 1` | `ERROR_UNSUPPORTED_FS` (2) via `ntfsUnsupported` (lock still held → `ntfsOut` unlocks) |
| Entry pointer if `cur_index_buf` is **not** `'INDX'` | Convert `eax` from buf-relative to `indexRoot + attributeOffset + rel` (live FRS `$INDEX_ROOT` body) |
| Entry pointer if `'INDX'` | `eax` already in the INDX buffer |
| `ntfsCalculateTime` | `esi` → 8-byte BDFE; `edx:eax` = FILETIME; **ESI preserved** (stdcall callee-saved / AF trampoline); EAX/EBX/ECX/EDX may clobber |
| `writeRecord` | `ebx` → record (`INDX` buf or `frs_buffer`); `edx` = `LastRead` (partition sector); **`ebp` → PARTITION/NTFS** (required by `fs_write64_sys`); DF=0 |
| After `writeRecord` | `eax` = disk error (ignored); `ebx` still record pointer (`fs_write64_sys` pushes/pops ebx) |
| `ntfsDone` | `esi=[ebp+PARTITION.Disk]`; `disk_sync`; `ntfs_unlock`; `eax=0`; **does not zero ebx** → success syscall `ebx` = record pointer |
| Failure `ntfsOut` | unlock; **`ebx=0`**; `eax` = pushed `ERROR_*` |
| CF | Not a public plugin return; error paths use `eax` only |

### 4.C Error codes (`kernel/fs/fs_lfn.inc`)

| Symbol | Value | Used by this leaf? |
|--------|------:|--------------------|
| `ERROR_SUCCESS` | 0 | Yes (success) |
| `ERROR_UNSUPPORTED_FS` | 2 | Yes (`fragmentCount != 1`) |
| `ERROR_FILE_NOT_FOUND` | 5 | Yes (LFN miss) |
| `ERROR_DISK_FULL` | 8 | Label exists; **not** jumped from SetFileInfo |
| `ERROR_FS_FAIL` | 9 | Yes (LFN `eax=0`) |
| `ERROR_ACCESS_DENIED` | 10 | Label exists; **not** jumped from SetFileInfo |
| `ERROR_DEVICE` | 11 | Label exists; **not** jumped from SetFileInfo |

### 4.D Registers / stack / DF

| Item | Contract |
|------|----------|
| Plugin preserved | `ebp` must remain NTFS across the body (including callbacks into `writeRecord`/`ntfsDone`) |
| Clobbered | EAX, ECX, EDX, ESI, EDI, EBX (ebx meaning differs success vs fail) |
| Stack | Plugin `call`/`ret`; Rust inner `stdcall` **ret 4** (REG-009) |
| DF | `writeRecord` uses `movsw`/`stosw` forward — trampoline `cld` before Rust; Rust must not leave DF=1 |

---

## 5. Exact NTFS semantics

### 5.1 `$I30` index offsets (`ntfs.inc` 100–117)

| Field | Offset | SetFileInfo |
|-------|-------:|-------------|
| `fileRecordReference` | 0x00 | unchanged |
| `fileCreated` | 0x18 | **FILETIME from BDFE ctime** |
| `fileModified` | 0x20 | **FILETIME from BDFE mtime** |
| `recordModified` | 0x28 | unchanged |
| `fileAccessed` | 0x30 | **FILETIME from BDFE atime** |
| `fileAllocatedSize` | 0x38 | unchanged |
| `fileRealSize` | 0x40 | unchanged |
| `fileFlags` | 0x48 | low-byte bits `0x27` replaced |

### 5.2 Attribute mask

```text
guest_attrs = dword [bdfe+0]
keep = guest_attrs & 0x27          ; R=1 H=2 S=4 A=0x20
fileFlags.low &= 0xD8              ; -28h as byte = ~0x27
fileFlags.low |= keep
```

Bit 28 (`0x10000000`, NTFS directory) lives outside the low byte → **preserved**. BDFE directory `0x10` is **not** written (masked off). GetFileInfo maps bit 28 → BDFE `0x10`; SetFileInfo does not invert that mapping.

### 5.3 FILETIME conversion

`ntfsCalculateTime` (Cut AF): `fsCalculateTime` (KOS secs since 2001-01-01) then

```text
edx:eax = eax * 10_000_000 + 0xC8A5E280_01C07A85   ; add/adc bias
```

Host oracle **must** include the ADC carry into the high dword. Live expected:

| Field | BDFE | FILETIME |
|-------|------|----------|
| atime | `0b160e000407dc07` (2012-07-04 14:22:11) | `129858853310000000` |
| mtime | `1e050900170be207` (2018-11-23 09:05:30) | `131874375300000000` |

Granularity: 1-second BDFE → 10⁷ FILETIME ticks. Zero/boundary: inherit Cut AF (`2001-01-01` → bias). No extra NTFS 100 ns field.

### 5.4 `writeRecord` does **not** rewrite extra metadata fields

USA update sequence is incremented and sector tails swapped; payload bytes already stored by SetFileInfo are written as-is. No automatic `recordModified` / SI / FN update.

### 5.5 Live fixture (A/B baseline)

| Item | Value |
|------|--------|
| Generator | `tools/mkfs_utils/ntfs_minimal.py` |
| Reference | `dev_build/ntfssoak/ntfs-minimal-reference.img` |
| Size | 134217728 |
| SHA-256 | `7d061686219c97c2d838a55f5ddb1303a7ed240ac72be66f68bf641cb4c179b9` |
| Target | `/hd0/1/ROOT.TXT` MFT **19** |
| Parent | Root MFT **5** `$I30` (resident in soak; USA validated on record 5) |

---

## 6. USA / fixup contract

Rust **must not** reimplement record serialization.

| Step | Owner | Notes |
|------|--------|-------|
| INDX read restore | FASM `ntfs_find_lfn` → `ntfs_restore_usa` (Cut J) | Already done before Rust runs |
| Resident `$INDEX_ROOT` | Mutation in `frs_buffer` | USA is the FILE record USA |
| `writeRecord` in | `ebx` record, `edx` sector, `ebp` PARTITION, DF=0 | Increments USN at `updateSequenceOffset`; copies 2-byte tails; writes USN into sector ends |
| `writeRecord` out | `jmp fs_write64_sys` | `eax` error ignored by SetFileInfo |
| Rust sees | Pointers to **already-decoded** index entry + record buffer | Not a raw un-fixup'd FILE image to parse |

Error propagation: disk write / `disk_sync` failures are **ignored**; FASM still returns `eax=0`. Preserve this quirk (partial persistence possible).

---

## 7. Lock ownership

| Event | Owner |
|-------|--------|
| Acquire | FASM trampoline `ntfs_lock` (`lea ecx,[ebp+NTFS.Lock]` → `mutex_lock`) **before** `find_lfn` |
| Held during Rust | Yes — lookup succeeded, mutex still owned |
| Release success | FASM `ntfsDone` (`disk_sync` then `ntfs_unlock`) called from Rust |
| Release LFN/fragment fail | FASM `ntfsOut` — Rust is **not** entered |
| Every early-return before Rust | Existing `ntfsFail` / `ntfsNotFound` / `ntfsUnsupported` |

Unlike EXT, there is no RO non-local `ret` that skips the body. Do not invent one.

---

## 8. Callback ABI table

Only helpers Rust actually calls after trampoline-owned lookup.

### 8.1 `ntfsCalculateTime` (Cut AF trampoline)

| Item | Value |
|------|--------|
| Convention | `call` (not stdcall) from SetFileInfo today; AF body is `stdcall rust_ntfs_calculate_time, esi` then `ret` |
| In | `ESI` → 8-byte BDFE |
| Out | `EDX:EAX` FILETIME |
| Preserved | **ESI**, EBP, EDI (stdcall callee-saved for the inner blob); plugin EBP must survive |
| Clobbered | EAX, EBX, ECX, EDX |
| DF | Leave-alone (AF does not `std`) |
| Lock | Held; must not unlock |
| Side effects | None |

Rust may `mov esi, bdfe; call [ctx.calc_time]`. Do **not** call `rust_ntfs_calculate_time` by absolute address (relocs). Inject the **public** `ntfsCalculateTime` symbol.

### 8.2 `writeRecord`

| Item | Value |
|------|--------|
| Convention | `call`; **not** stdcall |
| In | `EBX` → FILE/INDX record; `EDX` = partition sector (`LastRead`); **`EBP` → NTFS/PARTITION** |
| Out | `EAX` = `fs_write64_sys` error (ignore); `EBX` preserved as record |
| Clobbered | EAX, ECX, EDX, ESI, EDI |
| DF | **Must be 0** (`movsw`) |
| Lock | Held |
| Side effects | USA mutation of the record buffer + cached disk write |

### 8.3 `ntfsDone`

| Item | Value |
|------|--------|
| Convention | `call` / `jmp` |
| In | `EBP` → NTFS |
| Out | `EAX=0`; lock released |
| Clobbered | ESI (set to `DISK*`); EAX; EBX preserved (still record) |
| Side effects | `disk_sync` (cache + `DISKFUNC.flush`); `ntfs_unlock` |

Do **not** add `find_lfn`, `ntfs_lock`, `ntfs_restore_usa`, or `fs_write64_sys` as extra callbacks.

---

## 9. Rust context — `NtfsSetFileInfoCtx`

Minimum reloc-free i686 layout. **32 bytes** (8 dwords). No copies of FILE/INDX records. No Rust globals besides smoke marker.

| Off | Field | Type | Purpose | Lifetime |
|----:|-------|------|---------|----------|
| 0 | `f70` | `*mut u8` | f.70 (`ebx`); metadata at `+16` | syscall stack |
| 4 | `ntfs` | `*mut u8` | `ebp` NTFS/PARTITION | partition object |
| 8 | `entry` | `*mut u8` | resolved `$I30` index entry | index/FRS buffer |
| 12 | `record` | `*mut u8` | `writeRecord` ebx (INDX or FRS) | same |
| 16 | `last_read` | `u32` | `writeRecord` edx | scalar |
| 20 | `calc_time` | `u32` | `&ntfsCalculateTime` | FASM/AF |
| 24 | `write_record` | `u32` | `&writeRecord` | FASM |
| 28 | `done` | `u32` | `&ntfsDone` | FASM |

```rust
#[repr(C)]
pub struct NtfsSetFileInfoCtx {
    pub f70: *mut u8,
    pub ntfs: *mut u8,
    pub entry: *mut u8,
    pub record: *mut u8,
    pub last_read: u32,
    pub calc_time: u32,
    pub write_record: u32,
    pub done: u32,
}
pub const NTFS_SET_FILE_INFO_CTX_SIZE: usize = 32;
```

Callback ABI: register-ABI via inline asm (same class as Cut CV `findInode`/`writeInode`). Pin `EBP` to `ctx.ntfs` across `writeRecord` and `ntfsDone` (REG-017). Capture no CF (none of these three expose a public CF contract to the plugin).

---

## 10. Trampoline design

Gate **not created in this turn**. Future shape (REG-009 / REG-010 / REG-017 / REG-018):

```text
; Stack after `sub esp, 32` and before stdcall:
;   [esp+0]  f70
;   [esp+4]  ntfs
;   [esp+8]  entry
;   [esp+12] record
;   [esp+16] last_read
;   [esp+20] ntfsCalculateTime
;   [esp+24] writeRecord
;   [esp+28] ntfsDone
;   [esp+32] return to plugin caller
;
; stdcall rust_ntfs_set_file_info(ctx) pops the 4-byte arg (REG-009).
; Do NOT `add esp, 4` after the call.
; ctx remains at esp; add esp, 32 after copying ebx.

ntfs_SetFileInfo:                    ; gate ON
        call    ntfs_lock
        call    ntfs_find_lfn
        jnc     .found
        test    eax, eax
        jz      ntfsFail
        jmp     ntfsNotFound
.found:
        cmp     [ebp+NTFS.fragmentCount], 1
        jnz     ntfsUnsupported
        mov     esi, [ebp+NTFS.cur_index_buf]
        cmp     dword [esi], 'INDX'
        jz      .entry_ready
        sub     eax, esi
        mov     esi, [ebp+NTFS.indexRoot]
        movzx   edx, byte [esi+attributeOffset]   ; resident attr body off
        add     eax, esi
        add     eax, edx
.entry_ready:
        mov     edx, [ebp+NTFS.cur_index_buf]
        cmp     dword [edx], 'INDX'
        jz      .rec_ready
        mov     edx, [ebp+NTFS.frs_buffer]
.rec_ready:
        cld
        sub     esp, 32
        mov     [esp+0], ebx
        mov     [esp+4], ebp
        mov     [esp+8], eax
        mov     [esp+12], edx
        mov     eax, [ebp+NTFS.LastRead]
        mov     [esp+16], eax
        mov     dword [esp+20], ntfsCalculateTime
        mov     dword [esp+24], writeRecord
        mov     dword [esp+28], ntfsDone
        mov     eax, esp
        stdcall rust_ntfs_set_file_info, eax
        mov     ebx, [esp+12]          ; match FASM success ebx = record
        add     esp, 32
        ret
```

OFF body: **retain the original FASM listing unchanged** in the `else` branch.

Rust return: `eax` from `ntfsDone` (0). Trampoline must not call `ntfsDone` again (double unlock).

Do **not** expose CF as a Rust public contract.

---

## 11. Failure semantics

| Case | Return | Lock | Disk state |
|------|--------|------|------------|
| LFN error (`eax=0`) | 9 `ERROR_FS_FAIL` | unlocked (`ntfsOut`) | unchanged |
| LFN miss (`eax≠0`) | 5 `ERROR_FILE_NOT_FOUND` | unlocked | unchanged |
| Fragmented index/FRS (`fragmentCount≠1`) | 2 `ERROR_UNSUPPORTED_FS` | unlocked | unchanged |
| Missing / invalid path | via LFN | unlocked | unchanged |
| Read-only file (`fileFlags` bit 0) | **still success** if lookup works — no deny | — | mutated |
| Immutable (EXT-style) | **N/A** — no check | — | — |
| Malformed FILE / USA on lookup | LFN `.err` → typically `ERROR_FS_FAIL` | unlocked | unchanged |
| `writeRecord` / `fs_write64_sys` fail | **0** (ignored) | unlocked via `ntfsDone` | **partial possible** |
| `disk_sync` fail | **0** (ignored) | unlocked | **partial possible** |
| Rollback | **None** | — | in-memory then write |

Rust must not add ACCESS_DENIED or write-error mapping that FASM lacks.

Guest soak missing-file edge: `ntfssoak` `edge_miss_eax=5`. Preserve.

---

## 12. Oracle plan (reuse completed harness)

Primary independent oracle: custom MFT/`$I30` parser (`scripts/ntfs_setfileinfo_oracle.py`). debugfs/ntfscat is **optional** and currently unavailable.

### 12.1 Compare (FASM OFF vs Rust ON)

Guest: GetFileInfo → SetFileInfo → immediate GetFileInfo → final GetFileInfo → `NSFI.LOG` (magic `NSFI` v1, FLAG_LOG_OK).

Host: target FILE record 19; USA on parent record 5; `$STANDARD_INFORMATION` unchanged; `$FILE_NAME` unchanged; `$I30` atime/mtime; `fileCreated`/flags/size; metadata diff `unexpected_changes=[]`.

### 12.2 Matrix

| ID | Case |
|----|------|
| A | atime/mtime mutation (current soak) |
| B | idempotent re-set of the same values |
| C | second distinct value pair |
| D | boundary / near-boundary BDFE (Cut AF epochs) |
| E | missing file → `eax=5` |
| F | read-only file: SetFileInfo still succeeds (document; do not invent deny) |
| G | repeated write |
| H | writeback after shutdown (host `$I30` FILETIME) |
| I | unexpected-field guards (SI, FN, size, `recordModified`, ctime when preserved) |
| J | LFN fail / fragment path where safely testable |

Command:

```text
python scripts/qmp_ntfs_setfileinfo_soak.py --force-minimal --repeats 3 --with-control
```

Framebuffer non-black is **environment only**, not the NTFS oracle.

---

## 13. ABI smoke (`NSFI`)

In-kernel supplementary smoke (Cut CV `ESFI` pattern). Synthetic ctx; **no disk**.

Verify:

- Success: flags mask + three FILETIME stores at 0x18/0x30/0x20
- `calc_time` invoked **three** times with ESI = ctime/atime/mtime
- `writeRecord` once with ebx=record, edx=`last_read`, ebp=ntfs, DF=0
- `ntfsDone` once; no second unlock
- Failure not required inside Rust if lookup stays FASM — optional stub that skips `done` is **forbidden**
- Register canaries (EBX/ESI/EDI/EBP) around `stdcall`
- Stack balance / DF
- Context 32-byte layout
- Marker `NSFI`; fail `DEADxxxx` distinctive
- No unexpected stores to `recordModified` / size / name

Smoke is **necessary but not sufficient**. Live NTFS CoW soak remains mandatory.

---

## 14. Blob / relocation / memory

| Item | Plan |
|------|------|
| Symbol | `rust_ntfs_set_file_info` |
| Section | `.text.rust_ntfs_set_file_info` |
| Blob | `rust_ntfs_set_file_info.bin` |
| Relocs | **0** required |
| Context | **32 B** (this design) |
| Prior estimate | 600–900 B assumed `find_lfn`-in-Rust — **superseded** |
| Revised estimate | **250–500 B** class (CV was 398 B with more callbacks) — **do not claim fit until measured** |
| `.bss` | smoke marker + small scratch only |
| Headroom | ~4.7 KiB (`end .bss` ≈ `OS_BASE+0x8CD03` vs `TMP_STACK_TOP=0x008E000`) |
| Pack | **REG-012** — do not move `TMP_STACK_TOP` / `sys_proc` / `SLOT_BASE` |
| Overflow | **block activation**; do not relocate the memory pack |

Measure blob bytes, trampoline bytes, `kernel.mnt` size, end `.bss` before enabling the gate.

---

## 15. QEMU OFF / ON / A/B

| Step | Requirement |
|------|-------------|
| Image | Corrected `ntfs_minimal.py` fixture (SHA above); **not** `images/ntfs-image.img` |
| Target | `/hd0/1/ROOT.TXT` MFT 19 |
| OFF | gate 0; current FASM body; establish baseline |
| ON | gate 1; Rust leaf |
| A/B | OFF vs ON: guest BDFE + host `$I30`/USA/SI/FN agree |
| ON×3 | fresh CoW each run; RESET=0; `NSFI.LOG` + host parser PASS |
| Control | GetFileInfo-only still `$I30` unchanged |
| Firstapp | required |
| NTFSOAK | required |
| Shutdown | harness stop after LOG; parse final CoW |
| Desktop non-black | environment only |

---

## 16. Regression risks

| Risk | Mitigation |
|------|------------|
| FILETIME missing ADC carry | Reuse Cut AF; host oracle already ADC-faithful |
| Writing `$STANDARD_INFORMATION` / `$FILE_NAME` | Out of scope; host sidecar must stay byte-identical |
| Wrong MFT / wrong `$I30` entry | Trampoline uses `find_lfn` eax; soak asserts record 19 |
| USA corruption | `writeRecord` stays FASM; host `validate_root_usa` |
| `EBP` lost in `writeRecord` (`fs_write64_sys` needs PARTITION) | REG-017 pin `ebp=ntfs` |
| `ESI` lost between three `ntfsCalculateTime` calls | AF preserves ESI; Rust may also reload from `f70+16` |
| Callback CF mishandling | No CF contract; do not `setc`/`pop` (REG-018) |
| Trampoline slot / stdcall double-clean | REG-009/010 |
| Lock leak | Lookup errors never enter Rust; success always `ntfsDone` |
| Double `ntfsDone` | Trampoline must not unlock after Rust |
| Success `ebx` mismatch | Copy `ctx.record` after stdcall |
| DF=1 into `movsw` | trampoline `cld` |
| ctime regression | Write `fileCreated` from buffer; soak preserves it |
| Unexpected `recordModified` | Do not store 0x28 |
| Flags `0x10` directory bit | Mask `0x27` only |
| Relocations | link assert 0 |
| Memory pack overflow | measure; REG-012 stop |
| Mapping write errors to nonzero `eax` | Preserve ignore-error quirk |
| Inventing ACCESS_DENIED | FASM has no such path here |

Historical: REG-001 EDX, REG-009/010 trampoline, REG-012 pack, REG-017/018 callbacks.

---

## 17. Rollback

| Mode | Behavior |
|------|----------|
| OFF | Original FASM `ntfs_SetFileInfo` body **intact** |
| ON | Rust via trampoline |
| Gate | Single `USE_RUST_NTFS_SET_FILE_INFO` |
| Other NTFS | Untouched (Get/Create/Write/Delete/SetFileEnd, USA, MCB, mount) |
| Other cuts | CV/CU/AF/J independent |

Disabling CW must not require disabling Cut AF/AE/J/BT/CV.

---

## 18. Cut CV / CU / post-CV interaction

| Item | Relation |
|------|----------|
| Cut CU / Slice E | Indirect only (any alloc during `find_lfn` realloc stays FASM) |
| PTE / `map_page` | **None** — still blocked; unrelated |
| Cut CV | Precedent (plugin SetFileInfo Path B); **different ABI/semantics**; do not reopen |
| Path A FS | **No** — single NTFS plugin leaf |
| Batching | **Forbidden** — no Delete / SetFileEnd / other plugins |

---

## 19. Completion criteria (future implementation turn)

### Source

- [x] ABI re-audit vs this plan
- [x] Callback list still `{ntfsCalculateTime, writeRecord, ntfsDone}`

### Rust

- [x] Leaf implementation; `$I30` semantics; ignore-write-error quirk
- [x] Lock: acquire FASM; release via `ntfsDone` only

### Blob

- [x] 0 relocations; measured size + SHA-256 (180 B)
- [x] Context 32 B; trampoline size recorded
- [x] `.bss` / `kernel.mnt` / REG-012 assertions

### Oracle

- [x] Guest `NSFI.LOG`
- [x] Custom MFT/`$I30` parser + USA
- [x] SI/FN unchanged; metadata diff PASS
- [x] debugfs **not required**

### ABI

- [x] `NSFI` smoke PASS

### QEMU

- [x] OFF baseline
- [x] ON
- [x] A/B
- [x] ON×3, RESET=0
- [x] firstapp + NTFSOAK + writeback

### Filesystem

- [x] `$I30` atime/mtime match expected FILETIME
- [x] `$STANDARD_INFORMATION` / `$FILE_NAME` guards
- [x] ctime written from buffer (preserve in soak)
- [x] size/flags-directory-bit unchanged

### Rollback

- [x] gate OFF verified (original FASM)

### Documentation

- [x] `cut-cw-implementation.md`
- [x] final image path + SHA
- [x] inventory increment **exactly once** (`ntfs_SetFileInfo` `[x]`) → **105 / 138**
- [x] production gate enable only after green

---

## 20. Documentation touchpoints (this plan turn)

| Path | Action |
|------|--------|
| This file | **Created** (plan) |
| [`migration-plan.md`](migration-plan.md) | Record Cut CW **COMPLETE** |
| [`migration-todo.md`](migration-todo.md) | Note planned, still `[ ]` |
| [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md) | Cross-link |
| [`post-cv-next-frontier.md`](post-cv-next-frontier.md) | Next = implement when authorized |
| Inventory count / gates / production code | **Unchanged** |

---

## 21. Explicit non-goals (this plan task)

- No Rust source for CW  
- No `USE_RUST_NTFS_SET_FILE_INFO` in tree yet  
- No edits to `ntfs_SetFileInfo` / `writeRecord` / NTFS lock  
- No Cut CX  
- No inventory increment  

**CUT CW — COMPLETE.** Implementation: [`cut-cw-implementation.md`](cut-cw-implementation.md). Do **not** start Cut CX.
