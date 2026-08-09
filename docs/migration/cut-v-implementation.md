# Cut V Implementation — `tcp_set_persist`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-v-plan.md`](cut-v-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `tcp_set_persist` |
| Source | [`kernel/network/tcp_subr.inc`](../../kernel/network/tcp_subr.inc) |
| Callers | 2 (`tcp_timer.inc:153`, `tcp_output.inc:301`) |
| Rust symbol | `rust_tcp_set_persist` |
| Pure helper | `kolibri_utils::tcp_set_persist` |
| Subsystem | Network / TCP timers (persist arming) |

---

## Candidate comparison (post-U audit)

| Candidate | Outcome |
|-----------|---------|
| `tcp_set_persist` | **Selected** — persist-timer arming; consumes Cut M SRTT/RTTVAR |
| `coff_get_align` | Deferred #2 — PE foothold but trivial scalar body |
| `set_io_access_rights` | Deferred #3 — TSS I/O privilege risk |
| `net_ptr_to_num4` | Deferred — new but thin scan + 12-caller fanout |
| `xfs_hashname` | Deferred — thin hash leaf |
| `set_window_clientbox` / `fat_time_to_bdfe` / `pci_make_config_cmd` | Rejected — banned classes after R–U |
| `memmove` / `mutex_init` | Stage-4 fanout |
| `strtoint_dec` | Dead — `conf_lib.inc` still commented out |

---

## Why selected

Cut V’s research question: does Strategy A + C remain viable for a **TCP persist-timer policy leaf** (retransmit mutual exclusion; SRTT/RTTVAR→RTO; unsigned rangeset clamp; sticky persist flag; bounded `t_rxtshift++`) as a reloc-free blob with a FASM-flow differential oracle?

| Preference | Result |
|------------|--------|
| Materially new vs A–U | Yes — persist arming (not RTT twin of M; not FAT/GUI/calendar) |
| New control-flow class | Yes — timer mutual exclusion + clamp + shift + sticky flag |
| Strategy A feasible | Pure arithmetic + unaligned field I/O; no tables / `.rodata` |
| Clear ABI | `EAX`→socket; void; plain `ret`; EAX/EBX preserve |
| Testability | Gate/clamp/shift/saturate grids; 200k PRNG |
| Limited blast radius | 2 callers; independent switch |

---

## Original implementation

FASM leaf in `tcp_subr.inc` (retained under `USE_RUST_TCP_SET_PERSIST=0`):

* If `timer_flags & timer_flag_retransmission` → early exit  
* Else: `ebx = ((t_srtt>>2) + t_rttvar) >> 1`; `shl ebx, cl` with `cl = t_rxtshift`  
* `tcpt_rangeset timer_persist, ebx, 8, 94` (unsigned `jb`/`ja`)  
* `or timer_flags, timer_flag_persist`  
* If `t_rxtshift < 12`: `inc t_rxtshift`  
* Preserves `EBX` (push/pop); retains `EAX`

Locked field offsets (FASM struct audit / Cut M):

| Field | Offset |
|-------|--------|
| `t_rxtshift` | 118 |
| `t_srtt` | 210 |
| `t_rttvar` | 214 |
| `timer_flags` | 254 |
| `timer_persist` | 262 |

Constants: `TCP_time_pers_min=8`, `TCP_time_pers_max=94`, `TCP_max_rxtshift=12`, `timer_flag_retransmission=1`, `timer_flag_persist=8`.

---

## Rust implementation

| Artifact | Path |
|----------|------|
| Algorithm | [`rust_kernel/kolibri_utils/src/tcp.rs`](../../rust_kernel/kolibri_utils/src/tcp.rs) |
| FFI | [`rust_kernel/kolibri_utils/src/ffi.rs`](../../rust_kernel/kolibri_utils/src/ffi.rs) `rust_tcp_set_persist` |
| Build | [`rust_kernel/kolibri_utils/build-tcp-set-persist.ps1`](../../rust_kernel/kolibri_utils/build-tcp-set-persist.ps1) |
| Blob | `rust_kernel/kolibri_utils/out/rust_tcp_set_persist.bin` |

`#![no_std]` freestanding; unaligned dword I/O; byte `t_rxtshift`; `wrapping_shl` matches x86 `shl r32, cl` (count & 31).

---

## Complexity

| Aspect | Detail |
|--------|--------|
| Branches | Retransmit gate; rangeset min/max; rxtshift saturate |
| Loops | none |
| Memory | Writes `timer_persist`, `timer_flags`; maybe `t_rxtshift` |
| vs Cut M | Consumes SRTT/RTTVAR to arm persist vs update estimators |
| vs Cut U | Timer policy leaf vs FAT naming state machine |

---

## ABI

### FASM `tcp_set_persist` (unchanged for callers)

| Item | Contract |
|------|----------|
| Convention | register leaf, plain `ret` |
| Input | `EAX` → `TCP_SOCKET*` |
| Output | void (mutates persist timer fields) |
| Preserved | `EAX`, `EBX` |

### Rust `rust_tcp_set_persist`

| Item | Contract |
|------|----------|
| Convention | `extern "stdcall"` |
| Args | `(socket: *mut u8)` |
| Return | void |
| Epilogue | `ret 4` |

### Trampoline

```asm
tcp_set_persist:
        push    ebx
        push    ecx
        push    edx
        push    eax
        stdcall rust_tcp_set_persist, eax
        pop     eax
        pop     edx
        pop     ecx
        pop     ebx
        ret
```

---

## Call graph

| Kind | Detail |
|------|--------|
| Direct | `tcp_timer.inc` (persist expiry → re-arm + `tcp_output`); `tcp_output.inc` (enter persist when window closed) |
| Upstream | TCP zero-window / persist-timer path |
| Related (not migrated) | retransmit timer policy, `tcp_output` itself |

---

## Dependency audit

| Kind | Value |
|------|-------|
| Global state in Rust | none |
| Static data / `.rodata` | none |
| External calls | none |
| Compiler helpers | none observed in final blob |
| Allocator / Scheduler / IRQ / Paging | none |

---

## Compiler artifact audit

| Check | Result (final) |
|-------|----------------|
| Section | `.text.rust_tcp_set_persist` |
| Relocations targeting section | **0** |
| Symbol at section offset 0 | **yes** |
| Trailing `ret 4` (`C2 04 00`) | **yes** |
| `CALL rel32` (`E8`) | **none** |
| External symbols | **none** |
| Offset immediates 118/210/214/254/262 | **present** |

---

## Artifact extraction

| Item | Result (**PROVEN**) |
|------|---------------------|
| Section | `.text.rust_tcp_set_persist` |
| Relocations | **0** |
| Symbol offset | **0** |
| Epilogue | `ret 4` trailing |
| Blob size | **88** bytes |
| Blob SHA-256 | `DBA4303B3EA5E68A5FF2FF85710B09E03BAD7F79D9D4CDB749142158F129A369` |

Extractor: [`extract_reloc_free_text.py`](../../rust_kernel/kolibri_utils/scripts/extract_reloc_free_text.py) with `--expect-ret-imm 4`.

Clean rebuild with `-SkipTest` reproduces the same SHA-256.

---

## Build commands

```powershell
powershell -File rust_kernel/kolibri_utils/build-tcp-set-persist.ps1

Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc

cd tools\kolibri_img
.\target\release\kolibri_img.exe cow ..\..\tmp_images\cut-u-final.img ..\..\tmp_images\cut-v-on.img
.\target\release\kolibri_img.exe replace ..\..\tmp_images\cut-v-on.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt

& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda ..\..\tmp_images\cut-v-on.img -boot a `
  -m 256 -vga std -display none `
  -netdev user,id=n0 -device e1000,netdev=n0 `
  -no-reboot -no-shutdown `
  -qmp tcp:127.0.0.1:4551,server,nowait
```

---

## Tests

| Suite | Result |
|-------|--------|
| `cargo test -p kolibri_utils` | **220/220** (includes Cut V persist suite) |
| Retransmit gate / named clamp / rxtshift saturate / sticky flag / shift-mask edges | **PASS** |
| Structured grid (flags × srtt × rttvar × rxtshift) | **PASS** |
| Deterministic PRNG (200 000, seed `0x7C900002`) | **PASS** |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Boundary coverage | Retransmit early-exit; min/max clamp; shift 0/4/11/12/31/32/255; sticky persist OR; neighbor non-mutation on gate |

---

## ABI smoke

| Item | Result |
|------|--------|
| `tcp_set_persist_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C56` hang) |
| Vectors | Retransmit gate; first arm (persist=10); min clamp 8; max clamp 94; rxtshift saturate 12; EAX/EBX/ECX/EDX/ESI/EDI preserve |
| Marker | `rust_tcp_set_persist_smoke_result = 'TCPV'` on success |

---

## QEMU validation

Kernels built with Cuts A–U production gates intact (`USE_RUST_FAT_GEN_SHORT_NAME=1`, etc.).

Images: CoW from `tmp_images/cut-u-final.img`, replace `KERNEL.MNT`.

| Build | Switch | Desktop | Internet |
|-------|--------|---------|----------|
| OFF | `USE_RUST_TCP_SET_PERSIST=0` | **OK** (QMP `running` + screendump `tmp_images/cut-v-off.ppm`, 2333217 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_TCP_SET_PERSIST=1` | **OK** (screendump `tmp_images/cut-v-on.ppm`, 2333217 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0C56`; boot continued to desktop).

**Real subsystem soak:** stock image boot does **not** deliberately force a zero-window persist-timer path. Leaf validated by differential + ABI smoke; generic boot/desktop/network attach is kernel integration regression. Target-specific live persist traffic was **not** forced.

Production default after completion: **`USE_RUST_TCP_SET_PERSIST = 1`**.

Production image: `tmp_images/cut-v-final.img`.

---

## Kernel sizes

| Artifact | Size |
|----------|------|
| `kernel-cut-v-off.mnt` | 237384 |
| `kernel-cut-v-on.mnt` | 237304 |

---

## Rollback

```text
USE_RUST_TCP_SET_PERSIST = 0
```

restores the original FASM body. Rust blob remains embedded via `rust/tcp_set_persist.inc`. Independent of Cuts A–U.

---

## Known limitations

* Stock QEMU boot does not deliberately exercise the persist-timer production callers (zero-window / persist expiry).
* Flags after return are unspecified (same as legacy FASM).
* `DEBUGF` verbose network logging is not reproduced in the Rust body (debug-only; not observable in production builds).

---

## Files changed

* `rust_kernel/kolibri_utils/src/tcp.rs` — `tcp_set_persist` + differential tests  
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_tcp_set_persist`  
* `rust_kernel/kolibri_utils/src/lib.rs` — exports  
* `rust_kernel/kolibri_utils/build-tcp-set-persist.ps1` — blob build  
* `rust_kernel/kolibri_utils/out/rust_tcp_set_persist.bin` — reloc-free blob  
* `kernel/rust/tcp_set_persist.inc` — embed + smoke  
* `kernel/network/tcp_subr.inc` — trampoline + `USE_RUST_TCP_SET_PERSIST`  
* `kernel/kernel32.inc` — include  
* `kernel/kernel.asm` — smoke call  
* `docs/migration/cut-v-plan.md`  
* `docs/migration/cut-v-implementation.md`  
* `docs/migration/migration-plan.md`  

---

## Completion rule

Cut V complete → **STOP**. Do not start Cut W.
