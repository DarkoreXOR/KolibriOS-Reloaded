# Cut CV Plan — `ext_SetFileInfo`

**Date:** 2026-08-14  
**Status:** **COMPLETE** — see [`cut-cv-implementation.md`](cut-cv-implementation.md)  
**Inventory at completion:** **104 / 138**  
**Production gate:** `USE_RUST_EXT_SET_FILE_INFO = 1`  
**Evidence:** [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md) — **EXT SETFILEINFO EVIDENCE READY**  
**Frontier:** [`post-pte-next-frontier.md`](post-pte-next-frontier.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> **Nomenclature:** **Cut CV** migrates **only** `ext_SetFileInfo` in
> `kernel/fs/ext.inc` (EXT plugin Path B leaf).  
> Do **not** implement this cut until explicitly authorized.  
> Do **not** reopen PTE ownership, Cut CU, or filesystem Path A.

---

## 0. Verdict

| Item | Value |
|------|--------|
| Target | `ext_SetFileInfo` |
| Source | `kernel/fs/ext.inc` |
| Path | **B** |
| Proposed gate | `USE_RUST_EXT_SET_FILE_INFO` |
| Proposed smoke marker | `ESFI` |
| Decision | **CUT CV COMPLETE** |
| Production changes in this task | **NONE** |

---

## 1. Fresh source audit (2026-08-14)

Re-read against live tree. Evidence summary still matches production FASM.

### 1.1 External contract (sysfn 70 / plugin vtable)

| Item | Fact |
|------|------|
| Syscall | `eax=70`, `ebx` → f.70 struct (`sys_file_system_lfn`) |
| Subfn | `6` = SetFileInfo |
| Safety | `file_system_is_operation_safe`: buffer length **32** for subfn 6 |
| Dispatch | dyndisk → partition → `ext_user_functions` index **6** |
| Plugin in | `ebx` → f.70; `ebp` → `EXTFS`; `esi` → UTF-8 path |
| Plugin out | `eax` (and historically `ebx`) for sysfn 70 |

Comment at `ext_user_functions` (lines 10–15):

```text
; ebx -> parameter structure of sysfunc 70
; ebp -> EXTFS structure
; esi -> path string in UTF-8
; out: eax, ebx = return values for sysfunc 70
```

### 1.2 Internal body (verified)

```3111:3149:kernel/fs/ext.inc
ext_SetFileInfo:
        call    extfsWritingInit
        call    findInode
        jc      @f
        lea     edi, [ebp+EXTFS.inodeBuffer]
        test    [edi+INODE.featureFlags], EXT4_IMMUTABLE_FL
        jnz     .error_denied
        push    esi
        mov     esi, [ebx+16]
        add     esi, 16
        call    fsCalculateTime
        add     eax, UNIXTIME_TO_KOS_OFFSET
        mov     [edi+INODE.aTime], eax
        add     esi, 8
        call    fsCalculateTime
        add     eax, UNIXTIME_TO_KOS_OFFSET
        mov     [edi+INODE.mTime], eax
        mov     ebx, edi
        pop     eax
        call    writeInode
        jc      @f
        xor     eax, eax
        jmp     @f
.error_denied:
        mov     eax, ERROR_ACCESS_DENIED
        stc
@@:
        push    eax
        jc      @f
        call    writeSuperblock
        mov     esi, [ebp+PARTITION.Disk]
        call    disk_sync
@@:
        call    ext_unlock
        pop     eax
        ret
```

### 1.3 Callers

**No direct `call ext_SetFileInfo` sites.** Sole production entry is the
`ext_user_functions` vtable slot (sysfn70.6 / dyndisk). One live semantic path.

### 1.4 What is **not** mutated

| Field | Behavior |
|-------|----------|
| BDFE attrs / flags / ctime slots in SetFileInfo buffer | **Ignored** |
| `INODE.cTime` | Updated **indirectly** by `writeInode` → `ext_write_time` (Cut BS) |
| `aTimeExtra` / `mTimeExtra` | **Not** written by this leaf |
| Size / mode / block map | Unchanged |

---

## 2. Path B justification

| Claim | Result |
|-------|--------|
| Single EXT plugin leaf? | **Yes** — one vtable function |
| Narrow metadata semantics? | **Yes** — atime/mtime only (+ writeInode ctime side effect) |
| Real production caller? | **Yes** — sysfn70.6 |
| Independent oracle? | **Yes** — guest ESFI.LOG + mini parser + debugfs |
| Deterministic CoW writeback? | **Yes** — evidence ×3 PASS |
| Rust-owned EXT FS state? | **No** — lock, findInode, writeInode, SB, disk stay FASM |
| Path A filesystem? | **Forbidden / incorrect** |

This is **not** EXT subsystem ownership. Prior Cut BS/AL/BR/CM remain
adjacent Path B leaves; CV does not merge them into Path A.

---

## 3. Cut boundary

### 3.1 In scope

- Public symbol `ext_SetFileInfo` only (gate ON body).
- Timestamp conversion using existing `fsCalculateTime` + `UNIXTIME_TO_KOS_OFFSET`.
- Ordering: mutate → `writeInode` → `writeSuperblock` → `disk_sync` → unlock.

### 3.2 Out of scope (remain FASM)

| Helper / area | Why stay FASM |
|---------------|---------------|
| `extfsWritingInit` / `ext_lock` / `ext_unlock` | Lock ownership + RO non-local return |
| `findInode` | Path walk / symlink / cache — large orch |
| `writeInode` / `writeInode_no_cTime` | Block I/O + checksum + cTime pack (BS) |
| `writeSuperblock` | SB checksum + sector write |
| `disk_sync` | Cache flush + driver flush |
| `fsCalculateTime` | Already Cut G (call public trampoline) |
| `ext_write_time` | Already Cut BS (via writeInode) |
| `GetFileInfo` / create / write / delete | Other plugin ops |
| Other FS plugins | NTFS/FAT/exFAT SetFileInfo |

### 3.3 Preferred call graph

```text
sysfn70.6 → dyndisk → ext_user_functions[6]
    ↓
ext_SetFileInfo  [FASM trampoline]
    ├─ extfsWritingInit     [FASM; may non-local return on RO]
    └─ rust_ext_set_file_info  [Rust, gate ON]
           ├─ findInode           [FASM callback; CF capture]
           ├─ immutable test      [Rust]
           ├─ fsCalculateTime ×2  [FASM public trampoline / Cut G]
           ├─ write aTime/mTime   [Rust]
           ├─ writeInode          [FASM callback; CF capture]
           ├─ writeSuperblock     [FASM callback]
           ├─ disk_sync           [FASM callback]
           └─ ext_unlock          [FASM callback]
```

**Hard rule:** lock acquire stays in FASM `extfsWritingInit` before Rust;
Rust **must** call `ext_unlock` on every return path (success and failure)
exactly as FASM does today.

---

## 4. Exact legacy ABI

### 4.A External syscall / plugin contract

| Item | Value |
|------|--------|
| Entry | Plugin ABI above |
| Buffer | `[ebx+16]` → 32-byte SetFileInfo buffer |
| Layout | `attrs(4) + flags(4) + ctime(8) + atime(8) + mtime(8)` |
| EXT uses | **only** atime @+16, mtime @+24 (BDFE datetime) |
| Path | `esi` UTF-8 (relative to volume after dyndisk) |
| Success | `eax=0` |
| Failure | nonzero `eax` (`ERROR_*`) |
| Stack | `call`/`ret` (not stdcall) for plugin entry |

### 4.B Internal sequencing ABI (must preserve)

| Step | Registers / state |
|------|-------------------|
| After `extfsWritingInit` | Locked; `eax`/`ebx` may be clobbered except RO early-exit |
| After `findInode` CF=0 | `esi` = **inode number**; `inodeBuffer` filled; `edi` → name |
| After `findInode` CF=1 | `eax` = `ERROR_FILE_NOT_FOUND` (or `ERROR_TOO_MANY_LINKS`); unlock; return `eax` |
| Immutable | `featureFlags & EXT4_IMMUTABLE_FL` (`= 10h`) → `ERROR_ACCESS_DENIED`, CF=1, **no** writeSuperblock/disk_sync |
| Save inode | FASM `push esi` then reuses `esi` as BDFE pointer; `pop eax` before `writeInode` |
| `fsCalculateTime` | `esi`→8-byte BDFE; `eax`=KOS secs since 2001-01-01; **ESI preserved**; EBX/ECX/EDX may clobber |
| Store | `aTime`/`mTime` = `eax + UNIXTIME_TO_KOS_OFFSET` (u32 wrap) |
| `writeInode` | `eax`=inode #, `ebx`→inode buffer; CF=1 → `ERROR_DEVICE`; **skips** SB/sync |
| Success path | `xor eax,eax` (clears CF); `writeSuperblock`; `esi=[ebp+PARTITION.Disk]`; `disk_sync`; unlock; `eax=0` |
| DF | Must leave DF clear (helpers may use `std`/`cld` internally — findInode symlink path) |
| Flags | Not a public contract beyond CF for internal helpers |

### 4.C `extfsWritingInit` RO hazard (REG-critical)

```text
if mount READ_ONLY:
  eax = ERROR_UNSUPPORTED_FS
  pop return-address into ebx
  ebx = 0
  ret   ; returns to *caller of ext_SetFileInfo* — skips body
else:
  wipe c_inode cache
  jmp mutex_lock (ext_lock)
```

Trampoline **must** `call extfsWritingInit` from FASM first so RO early-exit
semantics remain bit-identical. Do not reimplement this pop/ret in Rust.

---

## 5. Timestamp semantics

| Item | Exact behavior |
|------|----------------|
| BDFE layout | `sec,min,hour,pad, day,month, year_lo,year_hi` (8 bytes) |
| `fsCalculateTime` | KOS seconds since **2001-01-01** (Cut G; already Rust when gated) |
| Offset | `UNIXTIME_TO_KOS_OFFSET = (365*31+8)*86400 = 978307200` (`fs_lfn.inc`) |
| Inode store | `i_time = kos_secs + 978307200` (32-bit add; no ADC in this leaf) |
| Subseconds / extra | **Not** written |
| Timezone | None — civil BDFE treated as absolute calendar fields |
| Oracle fixture | atime BDFE `2012-07-04 14:22:11` → Unix **1341411731**; mtime `2018-11-23 09:05:30` → **1542963930** |
| ctime side effect | `writeInode` calls `ext_write_time` on `cTime` (+ optional extra) — **expected** |

Rust must **not** invent host `time_t` shortcuts; compose Cut G + constant add.

---

## 6. Failure semantics

| Case | EAX | SB/sync? | Unlock? | Inode atime/mtime written? |
|------|-----|----------|---------|----------------------------|
| RO mount | `ERROR_UNSUPPORTED_FS` | n/a (never locked) | n/a | No |
| Missing path | `ERROR_FILE_NOT_FOUND` | No | Yes | No |
| Too many symlinks | `ERROR_TOO_MANY_LINKS` | No | Yes | No |
| Immutable | `ERROR_ACCESS_DENIED` | No | Yes | No |
| `writeInode` I/O fail | `ERROR_DEVICE` | No | Yes | Buffer may be dirty in RAM; disk not updated |
| Success | `0` | Yes | Yes | Yes |

**Partial write:** FASM ignores `writeSuperblock` / `disk_sync` failures after a
successful `writeInode` (no CF check). Preserve that quirk unless a later
authorized cut changes it.

---

## 7. Shared state / locking

| Object | Role |
|--------|------|
| `EXTFS.Lock` | Mutex; acquired by `extfsWritingInit`/`ext_lock` |
| `EXTFS.inodeBuffer` | Working inode |
| `EXTFS.c_inode` | Directory cache wipe on write init (`-1`) |
| `EXTFS.superblock` | Updated checksum path in `writeSuperblock` |
| `DISK` caches | Flushed by `disk_sync` |
| Process/task | Not specially owned beyond path string in userspace |

**Policy:** FASM retains lock acquire; Rust must not take `mutex_lock` itself.
Rust may call FASM helpers while the EXT lock is held (matches today’s body).
`disk_sync` takes `DISK.CacheLock` internally — nested lock already exists in
production; do not change ordering.

---

## 8. Rust ABI design

### 8.1 Public FASM trampoline (proposed)

```text
USE_RUST_EXT_SET_FILE_INFO = 0   ; default OFF until gates green

if USE_RUST_EXT_SET_FILE_INFO
ext_SetFileInfo:
        call    extfsWritingInit          ; may non-local return
        ; Build stack ExtSetFileInfoCtx (see below)
        stdcall rust_ext_set_file_info, ctx_ptr
        ; EAX = status (CF not required at plugin boundary)
        ret
else
  ; original FASM body intact
end if
```

### 8.2 Context struct (stack-allocated; reloc-free)

```text
struct ExtSetFileInfoCtx
  f70           dd ?   ; ebx
  extfs         dd ?   ; ebp
  path          dd ?   ; esi at entry (UTF-8)
  find_inode    dd ?   ; callback
  write_inode   dd ?   ; callback
  write_sb      dd ?   ; callback
  disk_sync     dd ?   ; callback
  unlock        dd ?   ; callback
ends
```

Callbacks are FASM thunks that restore register ABI of each helper and return
status via EAX + CF (CF captured into a byte/flag for Rust — apply **REG-018**).

### 8.3 Rust entry

```text
stdcall rust_ext_set_file_info(ctx: *mut ExtSetFileInfoCtx) -> u32
; ret 4
; returns ERROR_* or 0
; must unlock on all paths after WritingInit returned into trampoline
```

### 8.4 Trampoline discipline (REG-009 / REG-010)

- No flags-as-public-contract at plugin boundary.
- Document every pushed dword before `stdcall`; verify return-address slot.
- Preserve DF clear on exit.
- Do not clobber EBP across callbacks that assume `ebp→EXTFS` (pin EBP in thunks — REG-017 lesson).

---

## 9. FASM helper dependency table

| Helper | In | Out | Notes |
|--------|----|-----|-------|
| `extfsWritingInit` | `ebp→EXTFS` | lock or non-local RO return | FASM-only before Rust |
| `findInode` | `esi→path`, `ebp` | CF; `esi`=inode #; buffer filled | CF capture required |
| `fsCalculateTime` | `esi→BDFE` | `eax`=KOS secs; ESI preserved | Cut G trampoline OK |
| `writeInode` | `eax`=ino, `ebx`→inode | CF; may set `ERROR_DEVICE` | Updates cTime via BS |
| `writeSuperblock` | `ebp` | (ignored status) | |
| `disk_sync` | `esi→DISK` | | Nested cache lock |
| `ext_unlock` | `ebp` | | Always on return |

Do **not** duplicate helper logic in Rust.

---

## 10. Relocation / blob strategy

| Approach | Choice |
|----------|--------|
| Globals / hard addresses | **Forbidden** |
| Context + callbacks | **Required** |
| Constants | Immediate `978307200`; `EXT4_IMMUTABLE_FL=0x10` |
| Cross-Rust calls | Prefer FASM `fsCalculateTime` trampoline over linking Cut G blob |
| Target | freestanding `i686`; **relocations = 0** |

### Size / memory estimate (planning)

| Item | Estimate |
|------|----------|
| Rust body | ~600–1400 B |
| FASM trampoline + thunks | ~150–300 B |
| Context | 36 B stack |
| Combined growth | **~1–2 KiB class** |

| Pack (unchanged) | Value |
|------------------|------:|
| `TMP_STACK_TOP` | `0x008E000` |
| `sys_proc` | `OS_BASE+0x008E000` |
| `SLOT_BASE` | `OS_BASE+0x0090000` |
| Current end `.bss` | `OS_BASE+0x8C7C3` |
| Headroom | ~2.1 KiB |

**Hard pre-activation stop:** if measured blob + `.bss` exceeds REG-012 pack /
headroom → **do not enable gate**; do not move `SLOT_BASE`.

---

## 11. Oracle (reuse + extend)

Primary three-layer evidence (already proven):

1. Guest `ESFI.LOG` + GetFileInfo BDFE  
2. Host Python EXT2 mini parser  
3. Docker/`debugfs` cross-check  

Harness: `scripts/qmp_ext_setfileinfo_soak.py`, `scripts/ext_setfileinfo_oracle.py`,
`tools/extsoak/extsoak.asm`.

### Required Cut CV cases

| ID | Case |
|----|------|
| A | atime+mtime update (fixture Unix 1341411731 / 1542963930) |
| B | same-value rewrite |
| C | second distinct timestamp |
| D | near-boundary valid BDFE (e.g. 2001-01-01 and late 32-bit-safe) |
| E | missing path → nonzero EAX |
| F | RO mount / immutable if fixture available |
| G | repeated writes |
| H | clean shutdown + CoW persistence |
| I | expected ctime change |
| J | size/mode/inode unchanged; ESFI.LOG side-effect classified |

### OFF vs ON

| Mode | Gate |
|------|------|
| OFF | FASM body |
| ON | Rust body |
| Compare | guest + mini + debugfs must match between OFF baseline and ON |

Schema fields (machine-readable): image, inode, path, initial/requested/guest/host/debugfs values, metadata diff, unexpected list, QEMU status, run_id, gate mode.

---

## 12. ABI smoke (in-kernel)

Marker: **`ESFI`** (unique hang/fail string).

Minimum vectors (synthetic buffers + injected fake callbacks where safer than live disk):

1. Success path: BDFE→expected Unix stores; writeInode called once; unlock called  
2. findInode CF fail → unlock + ERROR_FILE_NOT_FOUND  
3. Immutable → ERROR_ACCESS_DENIED; no writeInode  
4. writeInode CF fail → no disk_sync; unlock  
5. Register/stack canaries around trampoline  
6. DF clear on exit  

Smoke is **necessary but not sufficient** — full EXT CoW soak remains mandatory.

---

## 13. QEMU OFF / ON / A/B

| Step | Requirement |
|------|-------------|
| Disk | `--disk ext` (CoW of `images/ext-image.img`) |
| Target | `/hd0/1/ROOT.TXT` inode 12 |
| OFF | gate 0; establish baseline |
| ON | gate 1 |
| A/B | OFF vs ON metadata agree |
| ON×3 | RESET=0; ESFI.LOG + mini + debugfs PASS |
| Desktop non-black | Environment only (e.g. 779380) — **not** FS oracle |
| Shutdown | clean powerdown; parse final CoW |

exFAT is **not** required for EXT semantics.

---

## 14. Regression risks

| Risk | Mitigation |
|------|------------|
| Wrong plugin vs internal ABI | Document both; smoke + soak |
| BDFE offset error (+16/+24) | Differential + fixture hex |
| Epoch / wrap error | Reuse Cut G; fixed offset constant |
| Lost inode # (`esi` overwrite) | Save inode before BDFE walk |
| Lock leak | Unlock on all Rust paths; RO early-exit stays FASM |
| CF loss on helpers | REG-018 capture |
| EBP/ESI clobber in callbacks | REG-017 pin |
| Trampoline slot error | REG-009/010 |
| Relocations | link assert 0 |
| Memory pack overflow | measure before enable; REG-012 |
| Unexpected field mutation | case J metadata diff |
| Partial write quirk change | preserve ignore SB/sync errors |

Historical: REG-001 EDX, REG-009/010 trampoline, REG-012 pack, REG-017/018 callbacks.

---

## 15. Rollback

| Mode | Behavior |
|------|----------|
| OFF | Original FASM `ext_SetFileInfo` body **intact** |
| ON | Rust via trampoline |
| Gate | Single `USE_RUST_EXT_SET_FILE_INFO` |
| Helpers | Unchanged; other EXT gates (AL/BR/BS/CM) independent |

Disabling CV must not require disabling Cut G/BS/AL.

---

## 16. Production gate (proposed — do not add yet)

| Item | Value |
|------|--------|
| Name | `USE_RUST_EXT_SET_FILE_INFO` |
| Location | `kernel/fs/ext.inc` beside `ext_SetFileInfo` (+ `project/build.toml` entry at implementation) |
| Default during implementation | **0** until host+ABI+QEMU green |
| Enable condition | All §18 criteria PASS |

Naming matches existing `USE_RUST_EXT_WRITE_TIME` / `USE_RUST_EXT_READ_TIME`.

---

## 17. Memory / fixed addresses (hard)

Do **not** move:

- `TMP_STACK_TOP = 0x008E000`
- `sys_proc = OS_BASE+0x008E000`
- `SLOT_BASE = OS_BASE+0x0090000`

Pre-activation assertions:

1. Rust blob relocations = **0**  
2. Measured end `.bss` still under pack with ≥ safety margin  
3. `kernel.mnt` size recorded  
4. If overflow → **STOP** (plan remains valid; activation blocked)

---

## 18. Completion criteria (future implementation turn)

### Host

- [ ] Focused timestamp / differential tests (Cut G compose + offset)
- [ ] FASM vs Rust semantic differential on synthetic BDFE table
- [ ] Mini parser + debugfs on CoW results
- [ ] Full `cargo`/project suite green

### ABI

- [ ] `ESFI` smoke PASS (success + fail + canaries + DF)

### QEMU

- [ ] OFF baseline
- [ ] ON
- [ ] A/B parity
- [ ] ON×3, RESET=0
- [ ] `--disk ext` soak with ESFI.LOG extract

### Filesystem

- [ ] Guest Set/Get + sync persistence
- [ ] Inode 12 atime/mtime match
- [ ] Expected ctime; size/mode/inode unchanged
- [ ] Log artifact classified

### Memory / docs

- [ ] 0 relocations; `.bss` assertion; image size
- [ ] `cut-cv-implementation.md` + inventory `[x]` + gate enable **only after** green
- [ ] Link REG entries if any live fail

---

## 19. Documentation touchpoints

| Path | Action |
|------|--------|
| This file | **Created** (plan) |
| [`migration-plan.md`](migration-plan.md) | Record Cut CV **PLANNED** |
| [`migration-todo.md`](migration-todo.md) | Note planned, still `[ ]` |
| [`stage4-ext-setfileinfo-oracle.md`](stage4-ext-setfileinfo-oracle.md) | Cross-link (evidence complete) |
| [`post-pte-next-frontier.md`](post-pte-next-frontier.md) | Next = implement when authorized |
| Inventory count / gates / production code | **Unchanged** by this plan |

---

## 20. Explicit non-goals (this plan task)

- No Rust source for CV  
- No `USE_RUST_EXT_SET_FILE_INFO` in tree yet  
- No edits to `ext_SetFileInfo` body beyond future gated trampoline  
- No PTE / Cut CU follow-on  
- No NTFS SetFileInfo  

**Stop after plan. Do not implement Cut CV in the planning turn.**
