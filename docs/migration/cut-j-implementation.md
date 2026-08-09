# Cut J Implementation — `ntfs_restore_usa`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-j-plan.md`](cut-j-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ntfs_restore_usa` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | 8 sites (`ntfs_restore_usa` ×4 + `ntfs_restore_usa_frs` ×4 fall-through) |
| Rust symbol | `rust_ntfs_restore_usa` |
| Pure helper | `kolibri_utils::ntfs_restore_usa` / `UsaRestoreResult` |
| Subsystem | Filesystem / NTFS record integrity (USA) |

`ntfs_restore_usa_frs` is **not** migrated; it still loads `EAX` from `[EBP+NTFS.frs_size]` and falls through into `ntfs_restore_usa`.

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `ntfs_restore_usa` | **Selected** — USA integrity; strided multi-region same-buffer mutate; CF fail |
| `fat_next_short_name` | Rejected — DF novel but narrower fanout / less multi-region proof |
| `memmove` | Rejected — memcpy-class helper-risk interesting but thin + high blast radius |
| `blit_clip` | Rejected — composition after H; CF/geometry already proven |
| `xfs._.extent_unpack` | Rejected — thinner; no multi-region loop |
| `fsTime2bdfe` | Rejected — calendar twin of G |
| `fat_time_to_bdfe` | Rejected — too small |
| `antiAliasing` / `strtoint_dec` | Rejected — quirky EBP / conf-only |

---

## Why selected

Cut J’s research question: does Strategy A + C remain viable for a **multi-region integrity leaf** that validates a USA signature stream and **strides word restores** across sector tails **inside the same record**, with CF fail polarity matching Cut H, exact partial mutation on mid-loop reject, and zero tables — without compiler `memset`/`memcpy`/GOT?

| Preference | Result |
|------------|--------|
| Outside proven families | Yes — USA integrity, not VLE/string/checksum/calendar/geometry |
| New memory surface | Yes — same-buffer USA ↔ sector-end words; `+0x200` stride |
| Real kernel callers | 8 NTFS FRS/index sites |
| Strategy A feasible | Zero tables; explicit `u16` R/W |
| Clear ABI | `EBX`+`EAX` → CF; `pushad` preserve; plain `ret` |
| Testability | Strong oracle (CF + full buffer) |

---

## Original implementation

FASM leaf in `ntfs.inc` (retained under `USE_RUST_NTFS_RESTORE_USA=0`):

* `EBX` → record; `EAX` = size bytes  
* `sectors = size >> 9`; require `updateSequenceSize == sectors+1`  
* USA at `record + updateSequenceOffset`; first word = USN  
* Loop: require end-word == USN; restore next USA word; stride to next sector end  
* **CF=0** OK / **CF=1** fail  
* `pushad`/`popad`  
* Mid-loop USN mismatch leaves earlier sectors already restored (partial mutate)

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/ntfs_usa.rs`](../../rust_kernel/kolibri_utils/src/ntfs_usa.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_ntfs_restore_usa` |
| Build | [`rust_kernel/kolibri_utils/build-ntfs-restore-usa.ps1`](../../rust_kernel/kolibri_utils/build-ntfs-restore-usa.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_ntfs_restore_usa.bin` |

`#![no_std]` freestanding; explicit two-byte `u16` load/store (Cut I lesson — no `memset`/`memcpy`); returns `0`/`1` for trampoline CF mapping (Cut H polarity).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | USA size mismatch; USN mismatch; empty sector count early OK path in Rust |
| Loops | Bounded by `size >> 9` (tested 1..8) |
| Memory | Same allocation: USA array + sector end-words |
| vs Cut I | New family; CF polarity matches H (not I); no ESI/stack-out; strided in-place words |

---

## ABI

### FASM `ntfs_restore_usa` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EBX` → record; `EAX` = size |
| Output | sector ends restored on success; **CF=0** OK / **CF=1** fail |
| Preserved | all GPRs via `pushad`/`popad` |

### Rust `rust_ntfs_restore_usa`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(record: *mut u8, size: u32)` |
| Return | `u32` in `EAX`: `0` OK / `1` fail |
| Epilogue | `ret 8` |

### Trampoline

```asm
ntfs_restore_usa:
        pushad
        stdcall rust_ntfs_restore_usa, ebx, eax
        test    eax, eax
        popad
        jz      .rust_usa_ok
        stc
        ret
.rust_usa_ok:
        clc
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | `ntfs_restore_usa` ×4; `ntfs_restore_usa_frs` ×4 |
| Upstream | NTFS FRS / index post-read USA restore |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state | none |
| Static data / `.rodata` | none |
| External calls | none |
| Compiler helpers | none observed in final blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Compiler artifact audit

Mandatory after Cut I (`memset`+GOT rejection).

| Check | Result |
|-------|--------|
| Section | `.text.rust_ntfs_restore_usa` |
| Relocations targeting section | **0** (extractor hard-fail otherwise) |
| Symbol at section offset 0 | **yes** |
| Trailing / present `ret 8` (`C2 08 00`) | **yes** (trailing) |
| `CALL rel32` (`E8`) to helpers | **none** (byte `E8` at 0x67 is `jne` rel8 displacement, not CALL) |
| `CALL r/m` / PLT / GOT | **none** |
| `memset`/`memcpy`/`memmove`/`memcmp` symbols | **none** |
| External symbols | **none** |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_ntfs_restore_usa` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 8` trailing |
| Blob size | **113** bytes |
| Blob SHA-256 | `851FC92BA7CC0306F51547D6260F480BF419A5D8BE5D023488FE04165010C417` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 8`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-ntfs-restore-usa.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-j-on.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-j-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-j-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-j-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4500,server,nowait
```

Rollback OFF: set `USE_RUST_NTFS_RESTORE_USA = 0`, reassemble, CoW/replace, QEMU (port 4501 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **104/104** (was 93; +11 ntfs_usa) |
| Named: 1/2/4-sector OK; USA size mismatch; USN fail first/middle; size&lt;512 early fail; sentinels | **PASS** |
| Grid sectors 1..8 × USA offsets | **PASS** |
| Deterministic PRNG (200 000, seed `0xC07B10E`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle in `ntfs_usa.rs` tests | **PASS** |
| Compare OK/fail + entire record buffer | **PASS** |
| Partial mutate on mid-loop USN reject | **PASS** |

---

## ABI / trampoline / registers

In-kernel smoke `ntfs_restore_usa_rust_smoke_test` calls real `ntfs_restore_usa` with:

| Check | Result |
|-------|--------|
| 2-sector OK → CF clear + both end-words restored | **PASS** |
| USA size mismatch → CF set + no mutate | **PASS** |
| 2nd-sector USN mismatch → CF set + partial restore | **PASS** |
| 1-sector OK | **PASS** |
| EAX/EBX/ECX/EDX/ESI/EDI/EBP preserved on success path | **PASS** |
| Public symbol smoke | **PASS** |

---

## Kernel smoke

Called from `high_code` after Cut I smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_NTFS_RESTORE_USA=1`) | **PASS** — `running`; screendump `tmp_images/cut-j-on.ppm` (1024×768) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `tmp_images/cut-j-off.ppm` |

---

## Regression lock (Cuts A–I)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (104/104) |
| Cut I blob hash `rust_ntfs_decode_mcb_entry.bin` | **PASS** — `DA888977…39C9` unchanged |
| Cut H blob hash `rust_block_clip.bin` | **PASS** — `C79E5D83…E5D6` unchanged |
| Cut G blob hash `rust_fs_calculate_time.bin` | **PASS** — `B7B1AB42…1777` unchanged |
| Cut F blob hash `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Cut E blob hash `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut A CRC blob `rust_crc_32.bin` | **PASS** — `E8C42ED0…A6F4` unchanged (size 226) |
| Full extract of prior blobs after Cut J build | **PASS** |

---

## Determinism

Two independent freestanding rebuilds (second after wiping `i686-kolibri-none`):

| Build | Size | SHA-256 |
|-------|------|---------|
| #1 | 113 | `851FC92BA7CC0306F51547D6260F480BF419A5D8BE5D023488FE04165010C417` |
| #2 | 113 | `851FC92BA7CC0306F51547D6260F480BF419A5D8BE5D023488FE04165010C417` |

---

## Kernel sizes

| Config | `kernel.mnt` bytes |
|--------|-------------------|
| Cut J ON (Rust path) | **228024** |
| Cut J OFF (FASM body; blob still embedded) | **228056** |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut I baseline (227368) ≈ +656 bytes (113-byte blob + trampoline/smoke/data + alignment).

---

## Rollback

| Item | Value |
|------|-------|
| Switch | `USE_RUST_NTFS_RESTORE_USA` (`1` default / `0` original FASM) |
| Verified | Both ON and OFF assemble + QEMU desktop |

Independent of Cut A–I switches.  
Original FASM body remains in the `else` branch of `ntfs.inc`.

Wire-up:

* Trampoline / switch: `kernel/fs/ntfs.inc`  
* Embed + smoke: `kernel/rust/ntfs_restore_usa.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with synthetic USA records; live NTFS mount/I/O USA restore is not instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* FASM `LOOP` with `ECX=0` (`size < 512` and matching USA size 1) is pathological / effectively undefined; Rust takes an early OK path for `sectors==0` after a matching size check. Differential tests intentionally avoid that case.  
* Proof-of-life remains diagnostic only — not coupled to Cut J.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `ntfs_restore_usa` over FAT DF / memmove / blit / XFS / calendar alternates  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 8`, no external deps  
* Compiler artifact audit: no CALL helpers / GOT / memset-class  
* Rust unit + grid + PRNG vs separately coded FASM-oracle (CF + full buffer + partial mutate)  
* In-kernel trampoline smoke including CF, memory mutation, register preservation  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two full-target-wipe rebuilds  
* Independent rollback switch builds both ways  
* Cuts A–I blob hashes unchanged (documented B–I locks + CRC)  
* Full `kolibri_utils` suite green (104/104)  

### NOT PROVEN

* Byte-identical behavior of every live NTFS USA restore under real disk I/O  
* Host-assembled FASM binary vs Rust binary differential  
* Pathological `sectors==0` FASM `LOOP` wraparound behavior  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* Migrating `fat_next_short_name` / `memmove` / `blit_clip` / `ntfs_restore_usa_frs` as a separate symbol  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut K  

---

## Verification matrix

```text
Candidate audit                         PASS
Candidate comparison                    PASS
Cut J plan                              PASS
Rust implementation                     PASS
Compiler dependencies audited           PASS
Compiler artifact audit                 PASS
Link strategy validation                PASS
Artifact validation                     PASS
Rust tests                              PASS (104/104)
Differential/oracle                     PASS (grid + PRNG + named)
ABI/trampoline                          PASS (in-kernel smoke + CF)
Register preservation                   PASS (EAX/EBX/ECX/EDX/ESI/EDI/EBP)
Memory mutation                         PASS (full + partial)
Real caller smoke                       PASS (symbol); live NTFS NOT PROVEN
Relocation validation                   PASS (0)
External symbols                        NONE
Kernel smoke                            PASS
QEMU Rust ON                            PASS
QEMU Rust OFF                           PASS
Deterministic rebuild ×2                PASS
Rollback switch                         PASS
Cut A regression                        PASS (suite + crc blob)
Cut B regression                        PASS (cp866_upper hash)
Cut C regression                        PASS (utf16_upper hash)
Cut D regression                        PASS (strncmp hash)
Cut E regression                        PASS (checksum_1 hash)
Cut F regression                        PASS (checksum_2 hash)
Cut G regression                        PASS (fs_calculate_time hash)
Cut H regression                        PASS (block_clip hash)
Cut I regression                        PASS (ntfs_decode_mcb_entry hash)
Documentation                           COMPLETE
```

**Cut J COMPLETE — STOP. Do not start Cut K.**
