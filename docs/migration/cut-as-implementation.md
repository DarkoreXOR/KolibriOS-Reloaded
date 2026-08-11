# Cut AS Implementation — `socket_check`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-as-plan.md`](cut-as-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `socket_check` |
| Source | [`kernel/network/socket.inc`](../../kernel/network/socket.inc) |
| Callers | 4 live (`socket_accept`, `socket_send_local_initialized`, `socket_notify`, `socket_free`); + `socket_ptr_to_num` wrapper |
| Rust symbol | `rust_socket_check` |
| Pure helper | `kolibri_utils::socket_check` / `socket_check_from_first` |
| Subsystem | Network / socket list membership (Stage-5 foothold) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AR audit: X+AR do not own TSS/IRQ seed; Stage-4
Path B exhausted after AQ; ban-list FS/Unicode/GUI peers remain disfavored.
Selected **`socket_check`** — strongest remaining leaf: Stage-5 lock-free
socket-list ZF membership with clear ABI and excellent differential domain.

REG-001: trampoline preserves **EBX+ECX+EDX+ESI+EDI+EBP** and restores **ZF**
via `test eax, eax` after stdcall (`pop` leaves EFLAGS).

---

## Candidate comparison (post-AR audit)

| Candidate | Outcome |
|-----------|---------|
| `socket_check` | **Selected** — Stage-5 list ZF membership |
| `get_coff_sym` | #2 — PE name→Value; Stage-8 foothold |
| `createMcbEntry` | #3 — NTFS encode; high FRS blast |
| `ipv4_find_fragment_slot` / `memmove` | Defer — weak soak / high fanout |
| `blit_clip` / `is_string_userspace` / `sysfn_get*` | Reject — composition / thin |
| X+AR / AQ+paging Path A | Reject — ownership incomplete |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_SOCKET_CHECK=0`:

```text
call / ret
in:  EAX = socket ptr (candidate)
out: EAX = candidate on hit / 0 on miss or null
     ZF set on miss/null
preserves: EBX (push/pop)
destroys: flags (via test)
no cli/sti
```

---

## Rust ABI

```text
stdcall rust_socket_check(candidate, net_sockets) -> EAX
  ret 8
```

Trampoline: injects `net_sockets` sentinel; preserves EBX/ECX/EDX/ESI/EDI/EBP;
`test eax, eax` restores ZF.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `socket_check.rs` + `ffi.rs` section `.text.rust_socket_check` |
| Extract | `extract_reloc_free_text.py` → `rust_socket_check.bin` |
| Embed | `kernel/rust/socket_check.inc` `file` directive |
| Trampoline | `socket.inc` under `USE_RUST_SOCKET_CHECK` |
| Gate | `USE_RUST_SOCKET_CHECK` (prod 1) |
| Smoke | `socket_check_rust_smoke_test` (after `stack_init`) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_socket_check` |
| Blob/object size | 34 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `5360C54BB3389617E37CA9F2E23992E6B66CBEC04187BCE23F617910FAF2C25E` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow HashMap oracle vs `socket_check_from_first` | **PASS** |
| Named vectors | null; empty; hit first/middle/last; miss; single-node |
| PRNG | 50 000 vectors, seed `0x43555453` (`'CUTS'`) |
| Host tests | **435/435** cargo tests (incl. socket_check suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `socket_check_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C53` hang) |
| Vectors | Direct `rust_*` null/hit A/B/C/miss; public trampoline empty-list ZF + null ZF + temporary live hit; EBX/ECX/EDX/ESI/EDI/EBP canaries |
| Marker | `rust_socket_check_smoke_result = 'SCHK'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_SOCKET_CHECK=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_SOCKET_CHECK=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop | 779380 | 779380 | **match** |

---

## Real subsystem soak

```text
Real subsystem soak: NOT AVAILABLE
```

Forced socket accept/notify/free under live traffic is not scripted; current
`qemu.args` has no e1000 NIC. Same class as prior Stage-5 network leaves
without a stock socket workload harness. Desktop boot + ABI smoke exercise
the public trampoline (including a temporary live `net_sockets` plant).

---

## Regressions

```text
NONE
```

(No live REG-NNN append for this cut.)

---

## Production gate

```text
USE_RUST_SOCKET_CHECK = 1
```

Rollback: `USE_RUST_SOCKET_CHECK = 0` (or `enabled = false` in
`project/build.toml` for cut AS).

Image: `dev_build/cut-as-final.img`

---

## Files changed

* `rust_kernel/kolibri_utils/src/socket_check.rs` — pure walk + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_socket_check` section
* `rust_kernel/kolibri_utils/src/lib.rs` — module export
* `rust_kernel/kolibri_utils/out/rust_socket_check.bin` — extracted blob
* `kernel/rust/socket_check.inc` — embed + ABI smoke
* `kernel/network/socket.inc` — trampoline + gate + FASM rollback body
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call after `stack_init`
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-as-plan.md` / `cut-as-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No forced socket-syscall soak / no e1000 in current qemu.args
* `net_sockets` list ownership, mutex, insert/remove remain FASM
* No Path A claim with `socket_check_port` / `socket_num_to_ptr`
* DEBUGF diagnostics on FASM null path are not reproduced by Rust (production
  `DEBUG_NETWORK` off)
