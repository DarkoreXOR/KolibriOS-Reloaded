# Cut AA Implementation — `pid_to_slot`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-aa-plan.md`](cut-aa-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `pid_to_slot` |
| Source | [`kernel/core/taskman.inc`](../../kernel/core/taskman.inc) |
| Callers | 8 live (`sysfn_terminate2`, `sysfn_pid_to_slot`, `sysfn_min_rest_window`, IPC, debug×2, playnote, events) |
| Rust symbol | `rust_pid_to_slot` |
| Pure helper | `kolibri_utils::pid_to_slot` |
| Subsystem | Process / taskman (TID→slot) |

---

## Candidate comparison (post-Z audit)

| Candidate | Outcome |
|-----------|---------|
| `pid_to_slot` | **Selected** — first process-table TID walk (Stage 6 foothold) |
| `get_pg_addr` | Deferred #2 — Stage-4 VA→PA; 15 callers + `page_tabs` |
| `net_ptr_to_num4` | Deferred #3 — thin device-list scan; packet-hot fanout |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |
| `is_protective_mbr` / `get_coff_sym` | Rejected — anti-cluster after Z / Y |
| `irq_eoi` / `enable_irq` / `mutex_init` | Deferred/rejected — HW / trivial |

---

## Legacy ABI

FASM leaf in `taskman.inc` (retained under `USE_RUST_PID_TO_SLOT=0`):

```text
call / ret
in:  EAX = TID (pid)
out: EAX = slot index (1..thread_count) or 0
preserves: EBX, ECX (explicit push/pop); EDX/ESI/EDI/EBP de facto
clobbers: flags
skips slot 0; scans offsets 256 .. thread_count*256 inclusive (signed jle)
skips TSTATE_FREE (9); dword APPDATA.tid match; first match wins
```

---

## Rust ABI

```text
stdcall rust_pid_to_slot(pid, slot_base, thread_count) -> EAX
ret 12
```

Trampoline injects `SLOT_BASE` and `[thread_count]`; preserves EBX/ECX/EDX/ESI/EDI/EBP.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `pid_to_slot.rs` + `ffi.rs` section `.text.rust_pid_to_slot` |
| Extract | `extract_reloc_free_text.py` → `rust_pid_to_slot.bin` |
| Embed | `kernel/rust/pid_to_slot.inc` `file` directive |
| Trampoline | `taskman.inc` under `USE_RUST_PID_TO_SLOT` |
| Gate | `USE_RUST_PID_TO_SLOT` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_pid_to_slot` |
| Blob/object size | 81 bytes |
| Relocations | 0 |
| SHA-256 | `1B8BC96DDD670FD1B9855435272AC1A4703CDF39C0837B878BB38FC3559CAD70` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow signed-jle oracle vs Rust | **PASS** |
| Named vectors | skip slot 0; free-skip; first-match; inclusive bound; missing→0; all non-free states |
| Boundary | thread_count=0; signed jle with `thread_count<<8` negative; wrapping shl |
| PRNG | 50 000 vectors, seed `0x43555441` (`'CUTA'`) |
| Host tests | **274/274** cargo tests |

---

## ABI smoke

| Item | Result |
|------|--------|
| `pid_to_slot_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CAA` hang) |
| Vectors | synthetic table find/miss/free-skip/bound; live SLOT_BASE OS→2 / IDLE→1; missing→0; EBX/ECX/EDX/ESI/EDI/EBP preserve |
| Marker | `rust_pid_to_slot_smoke_result = 'PTSL'` on success |

---

## QEMU validation

Kernels built with Cuts A–Z production gates intact (`USE_RUST_IS_PARTITION_TABLE_ENTRY=1`, etc.).

Images: CoW from `tmp_images/cut-z-final.img`, replace `KERNEL.MNT`.

| Gate | Setting | Desktop | Network |
|------|---------|---------|---------|
| OFF | `USE_RUST_PID_TO_SLOT=0` | **OK** (QMP `running` + screendump `tmp_images/cut-aa-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_PID_TO_SLOT=1` | **OK** (screendump `tmp_images/cut-aa-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0CAA`; boot continued to desktop).

**Real subsystem soak:** boot smoke calls the public `pid_to_slot` symbol against live `SLOT_BASE` after OS/IDLE setup (`thread_count=2`), looking up real slot TIDs. OFF/ON both reach desktop with identical non-black sample counts.

Production default after completion: **`USE_RUST_PID_TO_SLOT = 1`**.

Production image: `tmp_images/cut-aa-final.img`.

---

## Rollback

```text
USE_RUST_PID_TO_SLOT = 0
```

Legacy FASM body remains in `taskman.inc` under the `else` branch.

---

## Known limitations

* Other flags after return are unspecified (callers only use EAX).
* Sibling `pid_to_appdata` (different loop / unsigned `jb`) is not migrated.
* Does not migrate `memmove` or Stage-4 memory helpers.

---

## Files changed

* `rust_kernel/kolibri_utils/src/pid_to_slot.rs` — algorithm + differential tests  
* `rust_kernel/kolibri_utils/src/lib.rs` — module export  
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_pid_to_slot`  
* `rust_kernel/kolibri_utils/build-pid-to-slot.ps1` — blob build  
* `rust_kernel/kolibri_utils/out/rust_pid_to_slot.bin` — reloc-free blob  
* `kernel/rust/pid_to_slot.inc` — embed + ABI smoke  
* `kernel/core/taskman.inc` — trampoline + `USE_RUST_PID_TO_SLOT`  
* `kernel/kernel32.inc` — include  
* `kernel/kernel.asm` — smoke call after SLOT_BASE setup  
* `tools/build/config.toml` — blob + migration registry  
* `docs/migration/cut-aa-plan.md` / `cut-aa-implementation.md` / `migration-plan.md`
