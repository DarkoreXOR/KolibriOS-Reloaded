# Cut M Implementation — `tcp_xmit_timer`

**Date:** 2026-08-09  
**Status:** complete (audited)  
**Plan:** [`cut-m-plan.md`](cut-m-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `tcp_xmit_timer` |
| Source | [`kernel/network/tcp_subr.inc`](../../kernel/network/tcp_subr.inc) |
| Callers | 5 (`tcp_input.inc`) |
| Rust symbol | `rust_tcp_xmit_timer` |
| Pure helper | `kolibri_utils::tcp_xmit_timer` |
| Subsystem | Network / TCP protocol (RTT estimator) |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `tcp_xmit_timer` | **Selected** — first TCP protocol estimator leaf beyond checksums |
| `memmove` | Rejected — memcpy helper-risk deferred; thin vs protocol novelty |
| `coff_get_align` | Rejected — PE leaf too small after A–L |
| `antiAliasing` | Rejected — GUI color deferred for TCP depth |
| `pci_make_config_cmd` | Rejected — too thin |
| `tcp_outflags` | Rejected — inline table / `.rodata` risk |
| `UTF16to8` | Rejected — FS path after G/I/J/K weight |
| `hotkey_test*` / `hotkey_do_test` | Rejected — HID after Cut L; IRQ/table risk |
| timers / clipboard / sound / sync | Rejected — not Strategy A+C leaves |

---

## Why selected

Cut M’s research question: does Strategy A + C remain viable for a **TCP protocol estimator leaf** (dual socket-field mutate; RFC793 fixed-point arithmetic; unsigned `add`/`ja` clamp; trampoline-injected stats counter) as a reloc-free blob with a byte-exact differential oracle?

| Preference | Result |
|------------|--------|
| Outside FS / NTFS / FAT / video geometry / HID | Yes — TCP protocol (beyond E/F checksums) |
| New ABI / memory property | Yes — dual-field in-place mutate; trampoline owns `TCPS_rttupdated`; ECX/EDX preserve for `tcp_input` |
| Strategy A feasible | Pure arithmetic + unaligned dword stores; no tables / `.rodata` |
| Clear ABI | `EAX`=rtt, `EBX`→socket; void; plain `ret` |
| Testability | Init/update/clamp grids; 200k PRNG |
| Limited blast radius | 5 callers; independent switch |

---

## Original implementation

FASM leaf in `tcp_subr.inc` (retained under `USE_RUST_TCP_XMIT_TIMER=0`):

* Always `inc [TCPS_rttupdated]`  
* If `t_rtt == 0`: `t_srtt = rtt<<3`, `t_rttvar = rtt<<1`  
* Else: `delta = rtt - (t_srtt>>3) - 1`; `add`/`ja` clamp `t_srtt`; CDQ abs(`delta`); subtract `(t_rttvar>>2)`; `add`/`ja` clamp `t_rttvar`  
* Preserves `EBX`/`ECX`/`EDX`

Locked field offsets (FASM struct audit):

| Field | Offset |
|-------|--------|
| `t_rtt` | 202 |
| `t_srtt` | 210 |
| `t_rttvar` | 214 |

Note: `SOCKET` size is 74 → these fields are **2-byte aligned**; Rust uses `read_unaligned` / `write_unaligned`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/tcp.rs`](../../rust_kernel/kolibri_utils/src/tcp.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_tcp_xmit_timer` |
| Build | [`rust_kernel/kolibri_utils/build-tcp-xmit-timer.ps1`](../../rust_kernel/kolibri_utils/build-tcp-xmit-timer.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_tcp_xmit_timer.bin` |

`#![no_std]` freestanding; unaligned dword I/O; no slice fills (avoids memset/GOT).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | Init vs update; two unsigned clamps |
| Loops | none |
| Memory | Writes `t_srtt` + `t_rttvar`; reads `t_rtt` gate |
| vs Cut L | Protocol dual-field mutate vs HID scalar curve |
| vs Cuts E/F | Estimator state transform vs checksum accumulate |

---

## ABI

### FASM `tcp_xmit_timer` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` = RTT sample; `EBX` → `TCP_SOCKET` |
| Output | void (mutates `t_srtt`, `t_rttvar`) |
| Side effect | `inc [TCPS_rttupdated]` |
| Preserved | `EBX`, `ECX`, `EDX` |

### Rust `rust_tcp_xmit_timer`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(rtt: u32, socket: *mut u8)` |
| Return | void |
| Epilogue | `ret 8` |

### Trampoline

```asm
tcp_xmit_timer:
        inc     [TCPS_rttupdated]
        push    ecx
        push    edx
        stdcall rust_tcp_xmit_timer, eax, ebx
        pop     edx
        pop     ecx
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | `tcp_input.inc` (5 sites: timestamp RTT + timed-ACK paths + SYN RTT) |
| Upstream | TCP segment receive / ACK processing |
| Related (not migrated) | `tcp_mss`, `tcp_outflags`, retransmit timer policy |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state in Rust | none (`TCPS_rttupdated` in trampoline) |
| Static data / `.rodata` | none |
| External calls | none |
| Compiler helpers | none observed in final blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Compiler artifact audit

| Check | Result (final) |
|-------|----------------|
| Section | `.text.rust_tcp_xmit_timer` |
| Relocations targeting section | **0** |
| Symbol at section offset 0 | **yes** |
| Trailing `ret 8` (`C2 08 00`) | **yes** |
| `CALL rel32` (`E8`) | **none** |
| Indirect `FF` call/jmp patterns | **none** in blob |
| `memset`/`memcpy`/`memmove`/`memcmp` / GOT | **none** |
| External symbols | **none** |
| Offset immediates 202/210/214 | **present** |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_tcp_xmit_timer` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 8` trailing |
| Blob size | **153** bytes |
| Blob SHA-256 | `D469B83C46B8AE558E9C6247B53933C8BDDD59C6CC474FC1EB0291C38F6CFC01` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 8`.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-tcp-xmit-timer.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\tools\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\cut-m-on.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\cut-m-on.img DOCPACK
.\target\release\kolibri_img.exe replace ..\..\dev_build\cut-m-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\dev_build\cut-m-on.img -boot a `
  -m 256 -vga std -display none `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4530,server,nowait
```

Rollback OFF: set `USE_RUST_TCP_XMIT_TIMER = 0`, assemble to `kernel-off.mnt`, CoW/replace, QEMU (port 4531 used in audit).

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **134/134** (was 126; +8 tcp) |
| Named init / update / clamp / `i32::MIN` abs | **PASS** |
| Neighbor-field non-mutation | **PASS** |
| Grid `t_rtt∈{0,1,2}` × `rtt 0..63` × sample srtt/rttvar | **PASS** |
| Deterministic PRNG (200 000, seed `0x7C900001`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Separately coded FASM-faithful host oracle in `tcp.rs` tests | **PASS** |
| Compare `t_srtt` + `t_rttvar` after call | **PASS** |
| `t_rtt` / `t_idle` / `t_rtseq` / `t_rttmin` unchanged | **PASS** |
| Unsigned wrap + zero → clamp-to-1 | **PASS** |

---

## ABI / trampoline / registers

In-kernel smoke `tcp_xmit_timer_rust_smoke_test` calls real `tcp_xmit_timer`:

| Check | Result |
|-------|--------|
| Init `rtt=5` → srtt=40, rttvar=10 | **PASS** |
| Update `rtt=5` with srtt=40/rttvar=10 → **1/1** (unsigned ADD+JA clamp; not signed 39/9) | **PASS** (smoke expectation corrected Stage 3) |
| Clamp `rtt=0` with srtt=1/rttvar=4 → srtt=1, rttvar=4 | **PASS** |
| `TCPS_rttupdated` increments by 1 | **PASS** |
| ECX/EDX/ESI/EDI preserved | **PASS** |
| Public symbol smoke | **PASS** |

---

## Kernel smoke

Called from `high_code` after Cut L smoke. Hang-on-fail; desktop reached (**PROVEN** via QEMU `query-status=running` + screendump).

---

## QEMU ON/OFF

| Config | Result |
|--------|--------|
| Rust ON (`USE_RUST_TCP_XMIT_TIMER=1`) | **PASS** — `running`; screendump `dev_build/cut-m-on.ppm` (2359312 bytes) |
| Rust OFF (`=0`) | **PASS** — `running`; screendump `dev_build/cut-m-off.ppm` (2359312 bytes) |

---

## Regression lock (Cuts A–L)

| Item | Result |
|------|--------|
| Full `kolibri_utils` suite | **PASS** (134/134) |
| Cut L blob `rust_mouse_acceleration.bin` | **PASS** — `D1E51E85…169A` unchanged |
| Cut K blob `rust_fat_next_short_name.bin` | **PASS** — `E9CFFE65…A52F` unchanged |
| Cut J blob `rust_ntfs_restore_usa.bin` | **PASS** — `851FC92B…C417` unchanged |
| Cut I blob `rust_ntfs_decode_mcb_entry.bin` | **PASS** — `DA888977…39C9` unchanged |
| Cut H blob `rust_block_clip.bin` | **PASS** — `C79E5D83…E5D6` unchanged |
| Cut G blob `rust_fs_calculate_time.bin` | **PASS** — `B7B1AB42…1777` unchanged |
| Cut F blob `rust_checksum_2.bin` | **PASS** — `20867904…0C0C` unchanged |
| Cut E blob `rust_checksum_1.bin` | **PASS** — `83D3FDDB…ED18` unchanged |
| Cut D blob `rust_strncmp.bin` | **PASS** — `F9158B38…2259` unchanged |
| Cut C blob `rust_utf16_to_upper.bin` | **PASS** — `B2D5C5E9…CCE1` unchanged |
| Cut B blob `rust_cp866_to_upper.bin` | **PASS** — `8F171F09…27E5` unchanged |
| Cut A UTF-8 / CP866 / UTF-16 / CRC blobs | **PASS** — unchanged vs prior locks |

---

## Determinism

| Build | Size | SHA-256 |
|-------|------|---------|
| #1 | 153 | `D469B83C46B8AE558E9C6247B53933C8BDDD59C6CC474FC1EB0291C38F6CFC01` |
| #2 (forced clean recompile + extract) | 153 | `D469B83C46B8AE558E9C6247B53933C8BDDD59C6CC474FC1EB0291C38F6CFC01` |

---

## Rollback

```
USE_RUST_TCP_XMIT_TIMER = 0   ; original FASM body
USE_RUST_TCP_XMIT_TIMER = 1   ; Rust trampoline (default)
```

Independent of Cuts A–L switches.

---

## Kernel sizes

| Artifact | Size | SHA-256 |
|----------|------|---------|
| `kernel.mnt` Rust ON | 229480 | `D18259A54574BF0393491DBFD5DB368DF90DC8B1665B991E2C17AEFD42A8D0ED` |
| `kernel-off.mnt` Rust OFF | 229576 | `9ED16316A4E56B108540BAFFFABC8C92EC6655260C40873891949978DCFFA121` |

(OFF is larger because the original FASM body exceeds the trampoline; the Rust blob remains embedded under both configs via `rust/tcp_xmit_timer.inc`.)

---

## Evidence summary

### PROVEN

* Candidate audit selecting TCP `tcp_xmit_timer` over memmove/PE/GUI/PCI/FS/HID alternates  
* Freestanding Rust + reloc-free 153-byte blob, 0 relocations, no helpers/GOT  
* Dual-field socket mutate + unsigned `add`/`ja` clamp oracle (grids + 200k PRNG seed `0x7C900001`)  
* Trampoline ABI: stats-counter injection, `ret 8`, ECX/EDX preservation in smoke  
* In-kernel public-symbol smoke (hang-on-fail)  
* QEMU ON/OFF → `running` + screendumps  
* Cuts A–L blob hashes unchanged  
* Determinism ×2  

### NOT PROVEN

* Live TCP handshake / ACK → `tcp_xmit_timer` under real network traffic in QEMU (no injected packets in this audit)  
* Host FASM binary vs Rust blob instruction-level equivalence (oracle is semantic)  
* Interaction with retransmit timer policy after estimator update  

### OUT OF SCOPE

* Migrating `tcp_mss`, `tcp_outflags`, or other TCP timer helpers  
* Changing `TCP_SOCKET` layout  
* `memmove` / PE loader / GUI color / Cut N  

---

## Remaining issues

none

---

## Stop

**Cut M COMPLETE — STOP.** Do not start Cut N.
