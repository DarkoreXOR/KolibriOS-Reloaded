# Cut CV Implementation — `ext_SetFileInfo`

**Date:** 2026-08-14  
**Status:** **COMPLETE**  
**Inventory:** **103 → 104 / 138** (one Path B symbol)  
**Production gate:** `USE_RUST_EXT_SET_FILE_INFO = 1`  
**Plan:** [`cut-cv-plan.md`](cut-cv-plan.md)  
**Evidence:** [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md)

---

## 1. Boundary

| Item | Value |
|------|--------|
| Symbol | `ext_SetFileInfo` |
| Source | `kernel/fs/ext.inc` |
| Path | **B** (EXT plugin leaf; not Path A ownership) |
| Syscall | sysfn 70 / subfn 6 |
| Gate | `USE_RUST_EXT_SET_FILE_INFO` |
| Smoke | `ESFI` / fail `0xDEAD0C56` |

**Not migrated:** `extfsWritingInit`, `findInode`, `writeInode`, `writeSuperblock`,
`disk_sync`, `ext_unlock`, `fsCalculateTime`, other EXT/plugin leaves, PTE.

---

## 2. Legacy ABI (preserved)

Plugin in: `ebx`→f.70, `ebp`→EXTFS, `esi`→UTF-8 path.  
Buffer: `[ebx+16]` 32 bytes; EXT uses BDFE **atime@+16**, **mtime@+24**.  
Out: `eax` status.

Sequence (gate ON):

1. FASM `extfsWritingInit` (lock; RO non-local return unchanged)
2. `stdcall rust_ext_set_file_info(ctx)` 
3. Rust: `findInode` → immutable check → `fsCalculateTime`×2 + `UNIXTIME_TO_KOS_OFFSET`
   → store `INODE.aTime`/`mTime` → `writeInode` → (success) `writeSuperblock` +
   `disk_sync` → always `ext_unlock`

---

## 3. Rust ABI

```text
stdcall rust_ext_set_file_info(ctx: *mut ExtSetFileInfoCtx) -> u32
; ret 4  (REG-009: never add esp for the arg)
```

### `ExtSetFileInfoCtx` (44 bytes on i686)

| Off | Field | Type | Purpose |
|-----|-------|------|---------|
| +0 | `f70` | ptr | f.70 block |
| +4 | `extfs` | ptr | EXTFS / PARTITION |
| +8 | `path` | ptr | UTF-8 path |
| +12 | `inode_buf` | ptr | `&EXTFS.inodeBuffer` |
| +16 | `disk` | ptr | `PARTITION.Disk` value |
| +20 | `find_inode` | u32 | FASM `findInode` |
| +24 | `calc_time` | u32 | FASM `fsCalculateTime` |
| +28 | `write_inode` | u32 | FASM `writeInode` |
| +32 | `write_sb` | u32 | FASM `writeSuperblock` |
| +36 | `disk_sync` | u32 | FASM `disk_sync` |
| +40 | `unlock` | u32 | FASM `ext_unlock` |

---

## 4. Callback contracts

| Helper | In | Out | Notes |
|--------|----|-----|-------|
| `findInode` | `esi` path, `ebp` EXTFS | CF; `esi`=inode #; `edi` **clobbered** | Must push/pop EDI around call |
| `fsCalculateTime` | `esi`→BDFE | `eax`=KOS secs; ESI preserved by trampoline | Cut G |
| `writeInode` | `eax` ino, `ebx`→inode, `ebp` EXTFS | CF; `ERROR_DEVICE` | cTime via Cut BS |
| `writeSuperblock` | `ebp` EXTFS | (ignored) | |
| `disk_sync` | `esi`→DISK | | Nested cache lock |
| `ext_unlock` | `ebp` EXTFS | tail-`jmp` mutex_unlock | Always after lock |

**Inline-asm lesson (pre-enable):** do not leave EXTFS in EDI across `findInode`
(it clobbers EDI). Do not `lateout` a register that still holds a live EXTFS
pointer (REG-017/018). Always reload `ctx.extfs` after CF-bearing callbacks.

---

## 5. Timestamp semantics

`kos = fsCalculateTime(BDFE)`; `i_time = kos.wrapping_add(978307200)`.  
Oracle fixture: atime Unix **1341411731**, mtime **1542963930**.  
ctime updated only via existing `writeInode` → `ext_write_time`.

---

## 6. Failure semantics

| Case | EAX | SB/sync | Unlock |
|------|-----|---------|--------|
| RO (WritingInit non-local) | `ERROR_UNSUPPORTED_FS` | n/a | n/a |
| Missing | `ERROR_FILE_NOT_FOUND` | No | Yes |
| Immutable | `ERROR_ACCESS_DENIED` | No | Yes |
| writeInode fail | `ERROR_DEVICE` | No | Yes |
| Success | 0 | Yes | Yes |

---

## 7. Blob

| Metric | Value |
|--------|--------|
| File | `rust_kernel/kolibri_utils/out/rust_ext_set_file_info.bin` |
| Size | **398** bytes |
| Relocations | **0** |
| SHA-256 | `b10b847708cdfce77dfbb5589fbdfb32f30c7eab2bb09198778f1feb1df46d27` |
| Section | `.text.rust_ext_set_file_info` |
| `expect_ret_imm` | 4 |

---

## 8. Tests

| Layer | Result |
|-------|--------|
| Host focused (`ext_set_file_info`) | PASS (7) |
| ABI smoke `ESFI` | PASS (success / miss / immutable / write fail + canaries) |
| EXT soak OFF baseline | PASS |
| EXT soak ON ×3 | PASS, RESET=0 |
| Guest GetFileInfo + ESFI.LOG | PASS |
| Host EXT2 mini parser | PASS |
| debugfs (when available) | PASS (harness) |
| Rollback | Gate `=0` restores intact FASM body; OFF baseline run PASS |

---

## 9. Memory (post-CV)

| Symbol | Value |
|--------|--------|
| TMP_STACK_TOP / sys_proc | `0x008E000` (unchanged, REG-012) |
| SLOT_BASE | `0x0090000` (unchanged) |
| end of `.bss` | `OS_BASE+0x8CD03` (build log) |
| Headroom to `0x8E000` | ~7.6 KiB |
| Image | `dev_build/test/kernel-20260813-220901.img` (see `dev_build/last_image.txt`) |

No TMP_STACK_TOP / sys_proc / SLOT_BASE move.

---

## 10. Rollback

```text
USE_RUST_EXT_SET_FILE_INFO = 0
```

Original FASM body remains under `else`. No other EXT migrations affected.

---

## 11. Files touched

- `rust_kernel/kolibri_utils/src/ext_set_file_info.rs` (new)
- `rust_kernel/kolibri_utils/src/ffi.rs`, `lib.rs`
- `kernel/rust/ext_set_file_info.inc` (new)
- `kernel/fs/ext.inc` (gate + trampoline)
- `kernel/kernel32.inc`, `kernel/kernel.asm`
- `project/build.toml` (blob + migration CV)
- Docs: this file, plan/todo/migration-plan updates
