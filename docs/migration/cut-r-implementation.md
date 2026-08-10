# Cut R Implementation — `xfs._.extent_unpack`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-r-plan.md`](cut-r-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `xfs._.extent_unpack` |
| Source | [`kernel/fs/xfs.asm`](../../kernel/fs/xfs.asm) |
| Callers | 5 (`readdir_block`, `lookup_block`, `extent_list.seek`, `walk_extent_list`, `get_last_dirblock`) |
| Rust symbol | `rust_xfs_extent_unpack` |
| Pure helper | `kolibri_utils::xfs_extent_unpack` |
| Subsystem | FS / XFS |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `xfs._.extent_unpack` | **Selected** — EBP-as-object + MOVBE BE unpack |
| `window._.check_window_position` | Deferred — live path strong; ABI less novel |
| `fsTime2bdfe` / `blit_clip` / `memmove` | Deferred |

---

## Why selected

Cut R’s research question: does Strategy A + C remain viable for a **stdcall omit-frame-pointer leaf** that mutates `EBP+XFS.extent` while preserving EBP as the XFS partition pointer?

---

## Special ABI handling

Legacy callers keep `EBP → XFS` across nested `omit_frame_pointer_prologue` procs in `xfs.asm`.

The public trampoline is hand-written (no `push ebp` / `mov ebp, esp`):

```text
push eax/ebx/ecx/edx
mov  eax, [esp+20]            ; _extent_data
lea  ecx, [ebp+XFS.extent]    ; extent_out
push ecx / push eax
call rust_xfs_extent_unpack   ; ret 8
pop  edx/ecx/ebx/eax
retn 4
```

Rust API: `rust_xfs_extent_unpack(extent_data, extent_out)` — EBP never enters the Rust frame as a frame pointer.

---

## Original implementation

FASM leaf retained under `USE_RUST_XFS_EXTENT_UNPACK=0` (`movbe` / `shrd` / masks into `XFS.extent.*`).

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/xfs_extent.rs`](../../rust_kernel/kolibri_utils/src/xfs_extent.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_xfs_extent_unpack` |
| Build | [`rust_kernel/kolibri_utils/build-xfs-extent-unpack.ps1`](../../rust_kernel/kolibri_utils/build-xfs-extent-unpack.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_xfs_extent_unpack.bin` |
| Embed | [`kernel/rust/xfs_extent_unpack.inc`](../../kernel/rust/xfs_extent_unpack.inc) |

`#![no_std]` freestanding; explicit `u32` stores; no tables / GOT.

### Blob lock

| Field | Value |
|-------|-------|
| Size | **142** bytes |
| Relocations | **0** |
| SHA-256 | `665ACAFD18A875751202646786699F76BE3175A6312DE250377D59DA237ECED7` |
| Epilogue | `ret 8` |
| Endian ops | `bswap` (`0F CE` / `0F CA`) — LE host equivalent of FASM `movbe` loads |

### Layout lock (`xfs_bmbt_irec`)

| Field | Offset |
|-------|--------|
| `br_startoff` | 0 (lo/hi dwords) |
| `br_startblock` | 8 |
| `br_blockcount` | 16 |
| `br_state` | 20 |
| `sizeof` | 24 |

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **180/180** (incl. prior A–Q) |
| Zero / all-FF / state bit / mask+SHRD boundaries | **PASS** |
| Packed field round-trip samples | **PASS** |
| Padding untouched beyond 24 B | **PASS** |
| PRNG 200k vs independent FASM reference (`0x43555852`) | **PASS** |

---

## In-kernel smoke

`xfs_extent_unpack_rust_smoke_test` (wired after Cut Q smoke):

* Fake `sizeof.XFS` object filled `0xA5`; **EBP → fake XFS**
* Vectors: state=1 non-trivial; all-zero; all-FF max fields
* Asserts EBP unchanged; EAX/EBX/ECX/EDX(/ESI) preserved; fields exact; byte after irec still `0xA5`
* Fail hang: `EAX=0xDEAD0C52`, `EBX='XFSR'`, `ECX='FAIL'`

---

## QEMU validation

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| ON | `USE_RUST_XFS_EXTENT_UNPACK=1` | **OK** (QMP `running` + screendump `tmp_images/cut-r-on.ppm`, 12257 non-black samples) | **OK** (e1000 + user net) |
| OFF | `=0` (original FASM body) | **OK** (screendump `tmp_images/cut-r-off.ppm`, same non-black sample count) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C52`; boot continued to desktop).

### Real XFS live soak

**NOT AVAILABLE** — stock Cut Q lineage floppy has no XFS volume. QEMU validates integration/regression safety only, not live extent traversal.

Production default after completion: **`USE_RUST_XFS_EXTENT_UNPACK = 1`**.

Production image: `tmp_images/cut-r-final.img`.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel-cut-r-on.mnt` | 233640 | `13D6DAEEBC7B046DCD973B61FA19737462DA434AA57A92F11AB8F03BF88F0642` |
| `kernel-cut-r-off.mnt` | 233736 | `2DBCE833FC787A4F67DE2E8730413C294028E9D6EF00D1AF4EE7AAF56B034DE8` |

---

## Rollback

```text
USE_RUST_XFS_EXTENT_UNPACK = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/xfs_extent_unpack.inc`. Independent of Cuts A–Q. Cut D EDX trampoline untouched.

---

## Evidence summary

### PROVEN

* Omit-FP stdcall trampoline with EBP→XFS preserved  
* Bit-exact BE unpack vs independent host reference + 200k PRNG  
* Freestanding 142-byte blob, 0 relocs  
* In-kernel smoke hang-on-fail (EBP + registers + fields)  
* QEMU ON/OFF desktop regression (Cut Q image lineage)  

### NOT PROVEN

* Live XFS readdir/lookup/extent walk on a real XFS volume  

### OUT OF SCOPE

* Other XFS leaves  
* XFS image generation pipeline  
* Cut T  

---

## Files changed

* `rust_kernel/kolibri_utils/src/xfs_extent.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-xfs-extent-unpack.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_xfs_extent_unpack.bin` (new)
* `kernel/rust/xfs_extent_unpack.inc` (new)
* `kernel/fs/xfs.asm` (gate + trampoline; FASM body retained)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `orch/config.toml` (blob + missing Cut Q `utf16_to_8` entry)
* `docs/migration/cut-r-plan.md` / `cut-r-implementation.md` / `migration-plan.md`

---

## Cut R status

**COMPLETE.** Do not start Cut S until explicitly instructed.
