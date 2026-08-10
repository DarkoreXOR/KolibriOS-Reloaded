# Cut X Implementation — `set_io_access_rights`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-x-plan.md`](cut-x-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `set_io_access_rights` |
| Source | [`kernel/kernel.asm`](../../kernel/kernel.asm) |
| Callers | 2 (`r_f_port_area` enable loop; disable loop) |
| Rust symbol | `rust_set_io_access_rights` |
| Pure helper | `kolibri_utils::set_io_access_rights` |
| Subsystem | CPU / TSS I/O permission bitmap |

---

## Candidate comparison (post-W audit)

| Candidate | Outcome |
|-----------|---------|
| `set_io_access_rights` | **Selected** — first CPU/TSS I/O-bitmap BTR/BTS privilege state |
| `memmove` | Deferred #2 — memory class preferred, Stage-4 fanout |
| `get_coff_sym` | Deferred #3 — PE symbol scan; thinner than TSS class |
| `coff_get_align` / `pci_make_config_cmd` | Rejected — trivial scalar |
| `xfs_hashname` | Rejected — anti-cluster after R/W |
| `blit_clip` / `fat_time_to_bdfe` | Rejected — banned classes |
| `mutex_init` / `strtoint_dec` | Stage-4 / dead |
| `net_ptr_to_num4` / `is_string_userspace` | Deferred |

---

## Why selected

Cut X’s research question: does Strategy A + C remain viable for a **CPU/TSS privilege-state leaf** that mutates the I/O permission bitmap via BTR/BTS, with a register-only ABI and an injected global pointer — distinct from XFS R/W, TCP timers, calendar, GUI, and FAT naming?

| Preference | Result |
|------------|--------|
| Materially new vs A–W | Yes — first TSS I/O-bitmap privilege mutation |
| Anti-cluster after R/W | Yes — not XFS; different subsystem |
| Strategy A feasible | Pure bit ops; trampoline injects `tss._io_map_0` |
| Clear ABI | EAX=port; EBP=0/≠0; preserves EAX/EDI; plain ret |
| Testability | Exhaustive 65536-port domains; range loops; PRNG |
| Limited blast radius | 2 co-located syscall-46 callers; independent switch |

---

## Original implementation

FASM leaf in `kernel.asm` (retained under `USE_RUST_SET_IO_ACCESS_RIGHTS=0`):

* `EAX` = port bit index; `EBP` = 0 enable / ≠0 disable  
* `EDI = tss._io_map_0`  
* enable → `btr [edi], eax` (clear deny bit)  
* disable → `bts [edi], eax` (set deny bit)  
* `push edi eax` / `pop eax edi`; plain `ret`

Locked layout:

| Field | Value |
|-------|-------|
| `tss._io_map_0` | 4096 bytes |
| `tss._io_map_1` | 4096 bytes (contiguous) |
| Full map | 8192 bytes / 65536 bits |
| Enable | bit = 0 |
| Disable | bit = 1 |

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/io_access.rs`](../../rust_kernel/kolibri_utils/src/io_access.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_set_io_access_rights` |
| Build | [`rust_kernel/kolibri_utils/build-set-io-access-rights.ps1`](../../rust_kernel/kolibri_utils/build-set-io-access-rights.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_set_io_access_rights.bin` |
| Embed | [`kernel/rust/set_io_access_rights.inc`](../../kernel/rust/set_io_access_rights.inc) |

`#![no_std]` freestanding; byte/bit update only (no tables / `.rodata`).

### Blob lock

| Field | Value |
|-------|-------|
| Size | **61** bytes |
| Relocations | **0** |
| SHA-256 | `4A9F9DE117FD92011D2116290FD7C415280563822A6F92908587796B433CF16C` |
| Epilogue | `ret 12` (`c2 0c 00`) |
| Return | void |

### Trampoline

Hand-written register→stdcall trampoline in `kernel.asm`:

```text
push edi / eax / ecx / edx / ebp
push tss._io_map_0 / push ebp / push eax
call rust_set_io_access_rights   ; ret 12
pop ebp / edx / ecx / eax / edi
ret
```

Smoke runs after `tss._io_map_0` is mapped and filled (post-`ltr`), not in the early Phase-C smoke cluster. Smoke must **not** use EBP as a frame pointer (EBP is the legacy ABI input).

---

## Build / package sequence

```powershell
powershell -File rust_kernel/kolibri_utils/build-set-io-access-rights.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\dev_build\cut-w-final.img ..\..\dev_build\cut-x-on.img
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-x-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **240/240** (includes Cut X I/O-access suite) |
| Enable/disable / nonzero-EBP / idempotent / neighbor bits | **PASS** |
| Exhaustive all-ports enable-from-ones + disable-from-zeros | **PASS** |
| `r_f_port_area`-style range loops | **PASS** |
| Deterministic PRNG (50 000 mixed ops, seed `0x43555458`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow BTR/BTS oracle vs Rust | **PASS** |
| Boundary coverage | Ports 0/7/8/65535; byte boundaries; ebp∈{0,1,2,0xFFFFFFFF}; range enable/disable |

---

## ABI smoke

| Item | Result |
|------|--------|
| `set_io_access_rights_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C58` hang) |
| Vectors | Enable/disable 0xF010; nonzero-EBP disable 0xF011; range 0xF020..0xF027; EAX/ECX/EDX/ESI/EDI/EBP preserve; restore |
| Marker | `rust_set_io_access_rights_smoke_result = 'IOAX'` on success |

---

## QEMU validation

Kernels built with Cuts A–W production gates intact (`USE_RUST_XFS_GET_ADDR_BY_HASH=1`, etc.).

Images: CoW from `dev_build/cut-w-final.img`, replace `KERNEL.MNT`.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| OFF | `USE_RUST_SET_IO_ACCESS_RIGHTS=0` | **OK** (QMP `running` + screendump `dev_build/cut-x-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_SET_IO_ACCESS_RIGHTS=1` | **OK** (screendump `dev_build/cut-x-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C58`; boot continued to desktop).

**Real subsystem soak:** stock image boot does **not** deliberately force syscall 46 (`r_f_port_area`) port reserve/free. Leaf validated by differential + ABI smoke on live `tss._io_map_0`; generic boot/desktop/network attach is kernel integration regression. Target-specific live port-area traffic was **not** forced.

Production default after completion: **`USE_RUST_SET_IO_ACCESS_RIGHTS = 1`**.

Production image: `dev_build/cut-x-final.img`.

---

## Kernel sizes

| Artifact | Size |
|----------|------|
| `kernel-cut-x-off.mnt` | 238440 |
| `kernel-cut-x-on.mnt` | 238440 |

---

## Rollback

```text
USE_RUST_SET_IO_ACCESS_RIGHTS = 0
```

Legacy FASM body remains under the `else` branch.

---

## Known limitations

* Stock QEMU image does not exercise syscall 46 port reserve/free in the desktop soak.
* CF from BTR/BTS is not reconstructed (callers do not observe it).
* Privilege impact if wrong: incorrect I/O permission bits → port `#GP` for apps using reserved ports.

---

## Out of scope

* Migrating `memmove` / `get_coff_sym` / `r_f_port_area`  
* Cut Y  
