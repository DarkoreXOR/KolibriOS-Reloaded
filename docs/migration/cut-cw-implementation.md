# Cut CW Implementation — `ntfs_SetFileInfo`

**Date:** 2026-08-14  
**Status:** **COMPLETE**  
**Inventory:** **104 → 105 / 138** (one Path B symbol)  
**Production gate:** `USE_RUST_NTFS_SET_FILE_INFO = 1`  
**Plan:** [`cut-cw-plan.md`](cut-cw-plan.md)  
**Evidence:** [`stage4-ntfs-setfileinfo-oracle.md`](stage4-ntfs-setfileinfo-oracle.md)

---

## 1. Boundary

| Item | Value |
|------|--------|
| Symbol | `ntfs_SetFileInfo` |
| Source | `kernel/fs/ntfs.inc` (vtable index 6; body ~4294) |
| Path | **B** (NTFS plugin leaf; not Path A ownership) |
| Syscall | sysfn 70 / subfn 6 |
| Gate | `USE_RUST_NTFS_SET_FILE_INFO` |
| Smoke | `NSFI` / fail hang `0xDEAD0C57` |

**Not migrated:** `writeRecord`, USA/fixup, `ntfs_find_lfn`, `ntfs_lock`,
`ntfsDone`, `fs_write64_sys`, `$STANDARD_INFORMATION`, `$FILE_NAME`,
`ntfs_Delete`, other NTFS operations, PTE.

This is **not** the EXT Cut CV ABI. Rust mutates parent-directory `$I30`
only.

---

## 2. Legacy ABI (preserved)

Plugin in: `ebx`→f.70, `ebp`→NTFS, `esi`→UTF-8 path. `call`/`ret`.  
SetFileInfo buffer: `[ebx+16]`, 32 bytes.  
Out: success `eax=0`, `ebx`=written record pointer; failure `eax` in
`{2,5,9}`, `ebx=0`.

Lookup failures **never enter Rust** (`ntfsFail` / `ntfsNotFound` /
`ntfsUnsupported` → `ntfsOut`).

Sequence (gate ON):

1. FASM `ntfs_lock`
2. FASM `ntfs_find_lfn` (CF / eax=0 / miss stay FASM)
3. `fragmentCount == 1` else `ntfsUnsupported` (EAX=2)
4. INDX vs `$INDEX_ROOT` pointer fixup
5. `cld`
6. 32-byte ctx on stack
7. `stdcall rust_ntfs_set_file_info(ctx)` — callee `ret 4` (REG-009)
8. `ebx = [ctx.record]`; `add esp,32`; `ret`

---

## 3. Rust ABI

```text
stdcall rust_ntfs_set_file_info(ctx: *mut NtfsSetFileInfoCtx) -> u32
; ret 4  (REG-009: never add esp for the arg)
```

Public result is EAX/`u32`. CF is not a contract (REG-018).

### `NtfsSetFileInfoCtx` (32 bytes, i686, no padding)

| Off | Field | Type | Purpose |
|-----|-------|------|---------|
| +0 | `f70` | ptr | f.70 block |
| +4 | `ntfs` | ptr | NTFS / PARTITION (`ebp`) |
| +8 | `entry` | ptr | resolved `$I30` index entry |
| +12 | `record` | ptr | `writeRecord` ebx (INDX buf or FRS) |
| +16 | `last_read` | u32 | `NTFS.LastRead` |
| +20 | `calc_time` | u32 | FASM `ntfsCalculateTime` |
| +24 | `write_record` | u32 | FASM `writeRecord` |
| +28 | `done` | u32 | FASM `ntfsDone` |

---

## 4. Callback contracts

| Helper | In | Out | Notes |
|--------|----|-----|-------|
| `ntfsCalculateTime` | ESI→BDFE | EDX:EAX FILETIME; ESI preserved | Cut AF; ×3 (ctime/atime/mtime) |
| `writeRecord` | EBX=record, EDX=LastRead, **EBP=PARTITION**, DF=0 | EAX ignored | USA + `fs_write64_sys` stay FASM |
| `ntfsDone` | EBP=NTFS | EAX=0 | `disk_sync` + `ntfs_unlock`; **exactly once** on the success path |

LLVM forbids `esi` as an inline-asm operand on this target. `writeRecord` /
`ntfsDone` fn pointers are pinned in **EDI**; `ntfsCalculateTime` fn in
**EBX**. Callee-saved regs are push/pop'd around each call (REG-017).

Write/`disk_sync` errors are **ignored**; this leaf still returns EAX=0
(legacy quirk, confirmed in FASM `jmp ntfsDone` with no CF test).

---

## 5. Semantics

| Object | Behavior |
|--------|----------|
| Parent `$I30` `fileFlags` low byte | bits `0x27` (R\|H\|S\|A); BDFE `0x10` masked off; directory bit 28 preserved |
| `$I30` `fileCreated` (0x18) | written from BDFE +8 |
| `$I30` `fileAccessed` (0x30) | written from BDFE +16 |
| `$I30` `fileModified` (0x20) | written from BDFE +24 |
| `$I30` `recordModified` (0x28) | **unchanged** |
| size / name | **unchanged** |
| File MFT `$STANDARD_INFORMATION` | **unchanged** |
| File MFT `$FILE_NAME` | **unchanged** |

Known soak FILETIME (Cut AF ADC-faithful): atime `129858853310000000`,
mtime `131874375300000000`. Target `/hd0/1/ROOT.TXT`, MFT **19**, parent
record **5**.

Failure (FASM, pre-Rust): LFN miss → EAX=5; LFN `eax=0` → EAX=9;
`fragmentCount != 1` → EAX=2. No ACCESS_DENIED in this leaf.

---

## 6. Blob

| Item | Value |
|------|--------|
| File | `rust_kernel/kolibri_utils/out/rust_ntfs_set_file_info.bin` |
| Size | **180 B** |
| Relocations | **0** |
| SHA-256 | `91d143e331dedf992439d1115b7029bdb8a1cd66897c377938839994b77945b8` |
| Tail | `c20400` (`ret 4`) |
| Context | 32 B stack (no heap) |

---

## 7. Memory (REG-012)

| Symbol | Value |
|--------|--------|
| TMP_STACK_TOP / sys_proc | `0x008E000` (unchanged) |
| SLOT_BASE | `0x0090000` (unchanged) |
| end of `.bss` | `OS_BASE+0x8CFC3` |
| Assert `$-OS_BASE+PAGE_SIZE < TMP_STACK_TOP` | `0x8DFC3 < 0x8E000` PASS |
| `kernel.mnt` | 304872 bytes |
| Headroom to `0x8E000` | ~4.1 KiB raw; assert slack 61 B |

Smoke fixtures live on the **stack** so the pack is not expanded. No
TMP_STACK_TOP / sys_proc / SLOT_BASE move.

---

## 8. Tests

| Layer | Result |
|-------|--------|
| Host focused (`ntfs_set_file_info`) | PASS (6) |
| Host suite (`kolibri_utils`) | PASS (856) |
| ABI smoke `NSFI` | PASS (boot reached firstapp; hang marker not taken). Checks: 3× calc_time, 1× writeRecord (ebx/edx/ebp), 1× ntfsDone, flags `0x10000021`, FILETIME stores, `recordModified`/size untouched, stdcall canaries. Idempotent second kernel call omitted for REG-012 size; host tests cover it. |
| NTFS soak OFF | PASS (`cw-off`, RESET=0) |
| NTFS soak ON ×3 + control | PASS, RESET=0 (runs `1786712818-1` / `1786712858-2` / `1786712912-3` + `1786712952-control`) |
| Guest GetFileInfo IMM/FIN + `NSFI.LOG` | PASS |
| Host `$I30` parser + USA + SI/FN | PASS |
| A/B | Guest BDFE, `$I30` atime/mtime FILETIME, flags, size, miss EAX=5, USA, SI/FN **match**. `$I30` ctime values differ only because `--force-minimal` restamps the fixture generator clock; soak round-trips GetFileInfo ctime so each run preserves *that image's* created time. |
| Rollback | Gate `=0` restores intact FASM body; OFF soak PASS |

Guest ON flags_decode: Set/Get PASS, atime/mtime match, idempotent write,
second distinct value, missing path EAX=5, durable log.

---

## 9. Rollback

```text
USE_RUST_NTFS_SET_FILE_INFO = 0
```

Original FASM body remains under `else`. No other NTFS migrations affected.

---

## 10. Files touched

- `rust_kernel/kolibri_utils/src/ntfs_set_file_info.rs`
- `rust_kernel/kolibri_utils/src/ffi.rs`, `lib.rs`
- `kernel/fs/ntfs.inc` (gate + trampoline + intact OFF body)
- `kernel/rust/ntfs_set_file_info.inc`
- `kernel/kernel32.inc`, `kernel/kernel.asm`
- `project/build.toml` (`[[rust.blobs]]` + `[[rust.migrations]]` cut CW)

Final image: `dev_build/test/kernel-20260814-131226.img`  
Soak-validated ON image: `dev_build/test/kernel-20260814-130700.img`
