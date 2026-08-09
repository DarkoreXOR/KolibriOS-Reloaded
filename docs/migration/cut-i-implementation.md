# Cut I Implementation — `ntfs_decode_mcb_entry`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-i-plan.md`](cut-i-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ntfs_decode_mcb_entry` |
| Source | [`kernel/fs/ntfs.inc`](../../kernel/fs/ntfs.inc) |
| Callers | ≥7 sites in `ntfs.inc` (MFT scan, attr read, extend/shrink) |
| Rust symbol | `rust_ntfs_decode_mcb_entry` |
| Pure helper | `kolibri_utils::ntfs_decode_mcb_entry` / `McbDecodeResult` |
| Subsystem | Filesystem / NTFS data-run (MCB) codec |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `ntfs_decode_mcb_entry` | **Selected** — first NTFS VLE codec; dual buffers; ESI advance; inverted CF |
| `blit_clip` | Rejected — composition after H; CF/geometry already proven |
| `ntfs_restore_usa` | Rejected — same island; less packed/VLE novelty |
| `fat_next_short_name` | Rejected — DF novel but narrower |
| `fsTime2bdfe` | Rejected — calendar twin of G |
| `fat_time_to_bdfe` | Rejected — too small for Cut I bar |
| `antiAliasing` | Rejected — quirky EBP |
| `strtoint_dec` | Rejected — weaker complexity / conf-only |

---

## Why selected

Cut I’s research question: does Strategy A + C remain viable for a **variable-length packed codec** with **stream + stack dual buffers**, **ESI pointer advance**, **signed field extension**, and **CF polarity opposite Cut H**?

| Preference | Result |
|------------|--------|
| Outside proven families | Yes — NTFS MCB VLE, not string/checksum/calendar/geometry |
| New ABI class | Yes — stack 16 B out-buffer + ESI advance; CF=1 means *more* |
| Real kernel callers | ≥7 NTFS sites |
| Strategy A feasible | Zero tables; no `.rodata`; memset avoided via u64 stores |
| Clear ABI | ESI inout + stack buf → CF; plain `ret` |
| Testability | Strong oracle (CF + ESI delta + 16 buffer bytes) |

---

## Original implementation

FASM leaf in `ntfs.inc` (retained under `USE_RUST_NTFS_DECODE_MCB_ENTRY=0`):

* `ESI` → packed MCB entry (advanced on return)  
* 16-byte buffer at `[ESP+4]` on entry (`[ESP]` after return)  
* Header nibble decode; length zero-pad; cluster sign-extend (`cmc`/`sbb`)  
* **CF=1** more / **CF=0** end  
* Preserves `EAX`/`ECX`/`EDI` (push/pop); does not touch `EBX`/`EBP`/`EDX`  
* Partial mutate on early reject (length high-bit / oversized nibbles)

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/ntfs_mcb.rs`](../../rust_kernel/kolibri_utils/src/ntfs_mcb.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_ntfs_decode_mcb_entry` |
| Build | [`rust_kernel/kolibri_utils/build-ntfs-decode-mcb.ps1`](../../rust_kernel/kolibri_utils/build-ntfs-decode-mcb.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_ntfs_decode_mcb_entry.bin` |

`#![no_std]` freestanding; byte-walk matching `lodsb`/`rep movsb`; builds padded fields as `u64` in registers (avoids `memset` which produced GOT/reloc blockers on first extract attempt); returns `0`/`1` for trampoline CF mapping.

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | Header zero; nibble >8; length high-bit reject; cluster sign |
| Loops | Bounded copy 0..8 per field |
| Arithmetic | Nibble extract; LE assemble; sign-extend shift |
| State | Stream read + 16 B stack buffer write (partial on reject) |
| vs Cut H | New family + **inverted CF** + **ESI advance** + packed VLE |

---

## ABI

### FASM `ntfs_decode_mcb_entry` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register/stack leaf, plain `ret` |
| Input | `ESI` → entry; 16 B buffer below return address |
| Output | `ESI` advanced; buffer `{run_size[8], cluster_delta[8]}`; **CF=1** more / **CF=0** end |
| Clobbers | flags (and temporarily working regs inside body) |
| Preserved | `EAX`, `ECX`, `EDI` (explicit); `EBX`/`EBP`/`EDX` untouched in FASM |

### Rust `rust_ntfs_decode_mcb_entry`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(esi_inout: *mut *mut u8, buffer: *mut u8)` |
| Return | `u32` in `EAX`: `0` end / `1` more |
| Epilogue | `ret 8` |

### Trampoline

```asm
ntfs_decode_mcb_entry:
        push    eax ecx edx edi
        push    esi
        mov     eax, esp
        lea     edx, [esp+24]
        stdcall rust_ntfs_decode_mcb_entry, eax, edx
        pop     esi
        pop     edi edx ecx
        test    eax, eax
        pop     eax
        jz      .rust_end
        stc
        ret
.rust_end:
        clc
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | ≥7 `call ntfs_decode_mcb_entry` in `ntfs.inc` |
| Upstream | NTFS MFT / attribute extent walk |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state | none |
| Static data / `.rodata` | none |
| External calls | none (first extract hit `memset`+GOT — fixed by u64 stores) |
| Compiler helpers | none observed in final blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_ntfs_decode_mcb_entry` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 8` present (`C2 08 00`; shared epilogue note OK) |
| Blob size | **971** bytes |
| Blob SHA-256 | `DA888977328E5EFBF9DBBD58353F10EE55CE6B9B9D1A579101E7AB09555739C9` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 8`.

**Note:** Blob is larger than the FASM leaf because LLVM unrolls the 0..8 bounded copies. Reloc-free and deterministic; size is not a gate failure.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-ntfs-decode-mcb.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\cut-i-on.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\cut-i-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-i-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-i-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4498,server,nowait
```

Rollback OFF: set `USE_RUST_NTFS_DECODE_MCB_ENTRY = 0`, reassemble, CoW/replace, QEMU (port 4499 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **93/93** (was 79; +14 ntfs_mcb) |
| Named edge cases (end, positive/negative, high-bit reject, nibble >8, zero-length quirks, max 8+8) | **PASS** |
| Exhaustive headers 0..255 × 8 payload patterns | **PASS** |
| Deterministic PRNG (200 000, seed `0xC07B10D`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle | **PASS** |
| Compare CF (`more`), consumed/ESI delta, all 16 buffer bytes | **PASS** |
| Partial mutate on length high-bit reject | **PASS** |
| Cluster sign-extend / cluster_len=0 prior-byte quirk | **PASS** |

---

## ABI / trampoline / registers

In-kernel smoke `ntfs_decode_mcb_rust_smoke_test` calls real `ntfs_decode_mcb_entry` with:

| Check | Result |
|-------|--------|
| End marker CF clear + buffer untouched | **PASS** |
| Simple positive CF set + run/delta | **PASS** |
| Negative delta sign-extend | **PASS** |
| Length high-bit partial mutate | **PASS** |
| Chain then end | **PASS** |
| EAX/EBX/ECX/EDX/EDI/EBP preserved; ESI advanced | **PASS** |
| Public symbol smoke | **PASS** |

---

## Kernel smoke

Called from `high_code` after Cut H smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_NTFS_DECODE_MCB_ENTRY=1`) | **PASS** — `running`; screendump `tmp_images/cut-i-on.ppm` (1024×768) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `tmp_images/cut-i-off.ppm` |

---

## Regression lock (Cuts A–H)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (93/93) |
| Cut H blob hash `rust_block_clip.bin` | **PASS** — `C79E5D83…E5D6` unchanged |
| Cut G blob hash `rust_fs_calculate_time.bin` | **PASS** — `B7B1AB42…1777` unchanged |
| Cut F blob hash `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Cut E blob hash `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut D blob hash `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut C blob hash `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut B blob hash `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Full extract of prior blobs after Cut I build | **PASS** |

---

## Determinism

Two independent freestanding rebuilds after wiping `i686-kolibri-none/release`:

| Build | Size | SHA-256 |
|-------|------|---------|
| #1 | 971 | `DA888977328E5EFBF9DBBD58353F10EE55CE6B9B9D1A579101E7AB09555739C9` |
| #2 | 971 | `DA888977328E5EFBF9DBBD58353F10EE55CE6B9B9D1A579101E7AB09555739C9` |

---

## Kernel sizes

| Config | `kernel.mnt` bytes |
|--------|-------------------|
| Cut I ON (Rust path) | **227368** |
| Cut I OFF (FASM body; blob still embedded) | **227416** |

OFF is slightly larger because the original FASM body is assembled in addition to the still-embedded Rust blob. Size delta itself is not a failure.

ON − Cut H baseline (225960) ≈ +1408 bytes (971-byte blob + trampoline/smoke/data + alignment).

---

## Rollback

| Item | Value |
|------|-------|
| Switch | `USE_RUST_NTFS_DECODE_MCB_ENTRY` (`1` default / `0` original FASM) |
| Verified | Both ON and OFF assemble + QEMU desktop |

Independent of Cut A–H switches.  
Original FASM body remains in the `else` branch of `ntfs.inc`.

Wire-up:

* Trampoline / switch: `kernel/fs/ntfs.inc`  
* Embed + smoke: `kernel/rust/ntfs_decode_mcb_entry.inc`  
* Include: `kernel/kernel32.inc`  
* Smoke call: `kernel/kernel.asm` `high_code`

---

## Known limitations

* Smoke exercises the trampoline with fixed MCB vectors; live NTFS mount/extent walk under real disk I/O is not instrumented.  
* Rollback OFF still embeds the Rust blob — functional rollback of the call path only.  
* Host differential is algorithm-oracle level (not a host-assembled FASM binary).  
* Blob is larger than the FASM leaf due to LLVM unrolling (reloc-free still holds).  
* Proof-of-life remains diagnostic only — not coupled to Cut I.

---

## Evidence

### PROVEN

* Candidate audit + plan selecting `ntfs_decode_mcb_entry` over blit/FAT/calendar/GUI alternates  
* Reloc-free extract: exact section, 0 relocs, symbol @0, `ret 8`, no external deps  
* First extract diagnosed (`memset` + GOT) and fixed without weakening the extractor  
* Rust unit + exhaustive headers + PRNG vs separately coded FASM-oracle (CF + ESI + 16 bytes)  
* In-kernel trampoline smoke including inverted CF, ESI advance, stack buffer, register preservation  
* QEMU ON and OFF reach desktop (`running` + screendump)  
* Blob determinism across two full-target-wipe rebuilds  
* Independent rollback switch builds both ways  
* Cut B / C / D / E / F / G / H blob hashes unchanged  
* Full `kolibri_utils` suite green (93/93)  

### NOT PROVEN

* Byte-identical behavior of every live NTFS extent walk under real disk I/O  
* Host-assembled FASM binary vs Rust binary differential  

### OUT OF SCOPE

* Allocator / scheduler / IRQ / paging / drivers  
* Migrating `blit_clip` / `ntfs_restore_usa` / `fsTime2bdfe` / `fat_time_to_bdfe`  
* Introducing `rust-lld`  
* Removing FASM originals or earlier switches  
* Expanding proof-of-life  
* Cut J  

---

## Verification matrix

```text
Candidate audit                         PASS
Candidate comparison                    PASS
Cut I plan                              PASS
Rust implementation                     PASS
Compiler dependencies audited           PASS
Link strategy validation                PASS
Artifact validation                     PASS
Rust tests                              PASS (93/93)
Differential/oracle                     PASS (headers + PRNG + named)
ABI/trampoline                          PASS (in-kernel smoke + inverted CF)
Register preservation                   PASS (EAX/EBX/ECX/EDX/EDI/EBP + ESI advance)
Memory mutation                         PASS (16 B buffer + partial mutate)
Real caller smoke                       PASS (symbol); live NTFS NOT PROVEN
Relocation validation                   PASS (0)
Kernel smoke                            PASS
QEMU Rust ON                            PASS
QEMU Rust OFF                           PASS
Deterministic rebuild ×2                PASS
Rollback switch                         PASS
Cut A regression                        PASS (suite + blob extract)
Cut B regression                        PASS (cp866_upper hash)
Cut C regression                        PASS (utf16_upper hash)
Cut D regression                        PASS (strncmp hash)
Cut E regression                        PASS (checksum_1 hash)
Cut F regression                        PASS (checksum_2 hash)
Cut G regression                        PASS (fs_calculate_time hash)
Cut H regression                        PASS (block_clip hash)
Documentation                           COMPLETE
```

**Cut I COMPLETE — STOP. Do not start Cut J.**
