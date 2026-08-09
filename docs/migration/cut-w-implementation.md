# Cut W Implementation — `xfs._.get_addr_by_hash`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-w-plan.md`](cut-w-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs._.get_addr_by_hash` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 5 (`lookup_block`, `lookup_leaf`, `lookup_node`×1, `lookup_btree`×2) |
| Rust symbol | `rust_xfs_get_addr_by_hash` |
| Pure helper | `kolibri_utils::xfs_get_addr_by_hash` |
| Subsystem | FS / XFS directory hash binary search |

---

## Candidate comparison (post-V audit)

| Candidate | Outcome |
|-----------|---------|
| `xfs._.get_addr_by_hash` | **Selected** — binary search multi-state; EAX+ZF dual return |
| `memmove` | Deferred #2 — memory class preferred, Stage-4 fanout |
| `set_io_access_rights` | Deferred #3 — TSS preferred class, privilege risk |
| `coff_get_align` | Rejected for W — trivial Characteristics→mask |
| `net_ptr_to_num4` / `is_string_userspace` / `xfs_hashname` | Deferred |
| `blit_clip` / `fat_time_to_bdfe` / `pci_make_config_cmd` | Rejected — banned/thin classes |
| `mutex_init` / `strtoint_dec` | Stage-4 / dead |

---

## Why selected

Cut W’s research question: does Strategy A + C remain viable for an **algorithmic binary-search leaf** with an unusual **EAX payload + ZF found/miss** dual return, BE `movbe`-equivalent loads, and omit-frame-pointer stdcall — distinct from Cut R’s bitfield unpack and Cuts M/V timer policy?

| Preference | Result |
|------------|--------|
| Materially new vs A–V | Yes — multi-state search loop + dual EAX/ZF out |
| New control-flow class | Yes — below/above/equal/empty; ZF reconstruction via `cmp edx,1` |
| Strategy A feasible | Pure pointer walk + BE loads; no tables / `.rodata` |
| Clear ABI | EAX=hash in; stdcall base/len; EAX+ZF out; retn 8 |
| Testability | Exhaustive small domains; 50k PRNG sorted tables |
| Limited blast radius | 5 XFS lookup callers; independent switch |

---

## Original implementation

FASM leaf in `xfs.asm` (retained under `USE_RUST_XFS_GET_ADDR_BY_HASH=0`):

* `EAX` = hash; `_base` → leaf entries; `_len` = entry count  
* Loop: `mid = len>>1`; BE-load hash at `base+mid*8`; `cmp`  
* Below → `len = mid`; above → advance base by `(mid+1)*8`, `len -= mid+1`  
* Equal → BE-load address into EAX, ZF=1 from `cmp`  
* Empty → `EAX = 5`, `test esp,esp` → ZF=0  
* `uses ebx,esi`; `retn 8`

Locked layout:

| Field | Value |
|-------|-------|
| `hashval` | offset 0 (BE `dd`) |
| `address` | offset 4 (BE `dd`) |
| `sizeof.xfs_dir2_leaf_entry` | 8 |
| `ERROR_FILE_NOT_FOUND` | 5 |

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/xfs_hash_lookup.rs`](../../rust_kernel/kolibri_utils/src/xfs_hash_lookup.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_xfs_get_addr_by_hash` |
| Build | [`rust_kernel/kolibri_utils/build-xfs-get-addr-by-hash.ps1`](../../rust_kernel/kolibri_utils/build-xfs-get-addr-by-hash.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_xfs_get_addr_by_hash.bin` |
| Embed | [`kernel/rust/xfs_get_addr_by_hash.inc`](../../kernel/rust/xfs_get_addr_by_hash.inc) |

`#![no_std]` freestanding; raw pointer arithmetic only (no slice indexing — reloc-free).

### Blob lock

| Field | Value |
|-------|-------|
| Size | **96** bytes |
| Relocations | **0** |
| SHA-256 | `00A6F2309DEB7012763D885B6F5D5BB4908C8215CFE63F4C9F83C702BECF5B72` |
| Epilogue | `ret 12` |
| Return | `u64` in `EDX:EAX` = `(zf << 32) \| result` |

### Trampoline

Hand-written omit-FP trampoline in `xfs.asm`:

```text
push ebx/esi/ecx/edx
push len / push base / push eax(hash)
call rust_xfs_get_addr_by_hash   ; ret 12; EDX:EAX = zf:result
push eax
cmp  edx, 1                      ; ZF = found
pop  eax                         ; flag-neutral restore
pop  edx/ecx/esi/ebx
retn 8
```

---

## Build / package sequence

```powershell
powershell -File rust_kernel/kolibri_utils/build-xfs-get-addr-by-hash.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\tmp_images\cut-v-final.img ..\..\tmp_images\cut-w-on.img
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-w-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **230/230** (includes Cut W hash-lookup suite) |
| Empty / single / three-entry / power-of-two | **PASS** |
| Exhaustive sorted domains `n=0..7` | **PASS** |
| Duplicate-mid equal path | **PASS** |
| Deterministic PRNG (50 000 tables × present+miss, seed `0x43555457`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Boundary coverage | Empty miss; hit first/mid/last; miss below/gap/above; BE max hash/addr; duplicate mid |

---

## ABI smoke

| Item | Result |
|------|--------|
| `xfs_get_addr_by_hash_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C57` hang) |
| Vectors | Empty miss; hit mid/first/last; miss gap/below/above; single BE hit/miss; EBX/ESI preserve |
| Marker | `rust_xfs_get_addr_by_hash_smoke_result = 'XFSW'` on success |

---

## QEMU validation

Kernels built with Cuts A–V production gates intact (`USE_RUST_TCP_SET_PERSIST=1`, etc.).

Images: CoW from `tmp_images/cut-v-final.img`, replace `KERNEL.MNT`.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| OFF | `USE_RUST_XFS_GET_ADDR_BY_HASH=0` | **OK** (QMP `running` + screendump `tmp_images/cut-w-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_XFS_GET_ADDR_BY_HASH=1` | **OK** (screendump `tmp_images/cut-w-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C57`; boot continued to desktop).

**Real subsystem soak:** stock image has **no XFS volume**. Leaf validated by differential + ABI smoke; generic boot/desktop/network attach is kernel integration regression. Target-specific XFS directory lookup was **not** forced.

Production default after completion: **`USE_RUST_XFS_GET_ADDR_BY_HASH = 1`**.

Production image: `tmp_images/cut-w-final.img`.

---

## Kernel sizes

| Artifact | Size |
|----------|------|
| `kernel-cut-w-off.mnt` | 237896 |
| `kernel-cut-w-on.mnt` | 237864 |

---

## Rollback

```text
USE_RUST_XFS_GET_ADDR_BY_HASH = 0
```

---

## Files changed

* `rust_kernel/kolibri_utils/src/xfs_hash_lookup.rs` — new  
* `rust_kernel/kolibri_utils/src/lib.rs` — export Cut W  
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_xfs_get_addr_by_hash`  
* `rust_kernel/kolibri_utils/build-xfs-get-addr-by-hash.ps1` — new  
* `rust_kernel/kolibri_utils/out/rust_xfs_get_addr_by_hash.bin` — blob  
* `kernel/fs/xfs.asm` — trampoline + gate  
* `kernel/rust/xfs_get_addr_by_hash.inc` — embed + smoke  
* `kernel/kernel32.inc` — include  
* `kernel/kernel.asm` — smoke hook  
* `docs/migration/cut-w-plan.md`  
* `docs/migration/cut-w-implementation.md`  
* `docs/migration/migration-plan.md`  

---

## Known limitations

* Stock QEMU image does not mount an XFS filesystem; directory hash search is not live-exercised beyond ABI smoke tables.  
* Duplicate hash entries return the mid-equal path address (FASM quirk retained).  
* Trampoline preserves ECX/EDX beyond legacy `uses ebx,esi` (safe superset).  

---

## Stop

Cut W complete. **Do not start Cut X.**
