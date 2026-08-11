# Cut BD Implementation — `tcp_outflags`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bd-plan.md`](cut-bd-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `tcp_outflags` |
| Source | [`kernel/network/tcp_subr.inc`](../../kernel/network/tcp_subr.inc) |
| Callers | 1 (`tcp_output.inc` — send-path flags) |
| Rust symbol | `rust_tcp_outflags` |
| Pure helper | `kolibri_utils::tcp::tcp_outflags` |
| Subsystem | TCP state → header flags |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BC audit: XFS/NTFS/network/AHCI/PE/FAT/string/Stage-3
Path A still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. Thin leftovers (`is_string_userspace`, `v86_get_lin_addr`,
`coff_get_align`, `swap_bytes_in_words`) ranked below. Selected
**`tcp_outflags`** — new TCP semantic (11-byte `TCPS_*` → `TH_*` table),
distinct from Cuts M/V timer arithmetic.

REG-001: trampoline preserves **EAX** (socket), **EBX**, **ECX**; places flags
in **EDX** (legacy OUT).

REG-003: ABI smoke uses **stack synthetic TCP_SOCKET only** — never mutates
live socket list / `net_device_list`.

Reloc-free note: `.flaglist` is inlined onto the stack (no PC-relative label,
no `.rodata`).

---

## Candidate comparison (post-BC audit)

| Candidate | Outcome |
|-----------|---------|
| `tcp_outflags` | **Selected** — state→flags table |
| `is_string_userspace` | #2 — thin P sibling |
| `v86_get_lin_addr` | #3 — Stage-4 address math |
| `coff_get_align` | #4 — thin PE glue |
| `swap_bytes_in_words` | #5 — AV deepen |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_TCP_OUTFLAGS=0`:

```text
call tcp_outflags
in:  EAX → TCP_SOCKET*
out: EDX = header flags (DL meaningful; upper bits zero via movzx)
preserves: EAX, EBX, ECX, ESI, EDI, EBP
clobbers: EDX, flags
ret (plain)
```

Quirks retained:

* `t_state` is a dword used as a byte index into an 11-entry table
* Defined for `TCPS_*` 0..=10 only
* FASM has no bounds check (out-of-range would read past `.flaglist` into
  following code) — Rust returns **0** for `state > 10` (documented limitation)

Locked offset: `TCP_SOCKET.t_state` = **114** (dword before `t_rxtshift` @ 118).

Table (byte values):

| State | Flags |
|------:|------:|
| CLOSED (0) | `TH_RST\|TH_ACK` = 0x14 |
| LISTEN (1) | 0 |
| SYN_SENT (2) | `TH_SYN` = 0x02 |
| SYN_RECEIVED (3) | `TH_SYN\|TH_ACK` = 0x12 |
| ESTABLISHED (4) | `TH_ACK` = 0x10 |
| CLOSE_WAIT (5) | `TH_ACK` = 0x10 |
| FIN_WAIT_1 (6) | `TH_FIN\|TH_ACK` = 0x11 |
| CLOSING (7) | `TH_FIN\|TH_ACK` = 0x11 |
| LAST_ACK (8) | `TH_FIN\|TH_ACK` = 0x11 |
| FIN_WAIT_2 (9) | `TH_ACK` = 0x10 |
| TIME_WAIT (10) | `TH_ACK` = 0x10 |

---

## Rust ABI

```text
stdcall rust_tcp_outflags(socket) → EAX = flags
  ret 4
```

Trampoline: `push ebx` / `push ecx` / `push eax` /
`stdcall rust_…, eax` / `mov edx, eax` / `pop eax` / `pop ecx` / `pop ebx`.

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_tcp_outflags` |
| Size | **56 bytes** |
| Relocations | **0** |
| SHA-256 | `BE750686B91ABE9AEA70995C2460153475B9F6D2F2F62A55E0F1BAEFBC47F9F7` |
| Epilogue | `ret 4` (`c2 04 00`) |

---

## Differential tests

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle (11-byte `.flaglist`) | **PASS** |
| All defined states 0..=10 | **PASS** |
| Named vectors (CLOSED/LISTEN/SYN_*/ESTABLISHED/FIN_WAIT_1/TIME_WAIT) | **PASS** |
| Out-of-range → 0 | **PASS** |
| Non-mutation of socket buffer | **PASS** |
| 50k PRNG seed `0x43554244` (`CUBD`) | **PASS** |

---

## ABI smoke

| Check | Result |
|-------|--------|
| Marker | `TCPF` (`rust_tcp_outflags_smoke_result`) |
| Direct `rust_tcp_outflags` vectors | **PASS** |
| Public trampoline EDX out + EAX/EBX/ECX/ESI/EDI canaries | **PASS** |
| Live socket list / `net_device_list` mutation | **none** (stack scratch only) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_TCP_OUTFLAGS=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_TCP_OUTFLAGS=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **PASS** — same 779380 non-black; 72 differing bytes (≈0.003%, clock/timing noise) |
| Desktop boot | **PASS** both OFF and ON |

---

## Real subsystem soak

| Check | Result |
|-------|--------|
| Desktop boot with network stack init | **PASS** (smoke after `stack_init` path; desktop non-black) |
| Full TCP handshake / active `tcp_output` peer soak | **PARTIAL / NOT AVAILABLE** as automated remote-peer harness — leaf is on the send path; stock desktop does not force a remote TCP conversation |

---

## Regressions

**NONE** discovered this cut. No new `REG-*` entry.

---

## Production gate

| Item | Value |
|------|-------|
| Gate | `USE_RUST_TCP_OUTFLAGS = 1` |
| Rollback | `USE_RUST_TCP_OUTFLAGS = 0` or `[[rust.migrations]]` `cut = "BD"` `enabled = false` |
| Image | `dev_build/cut-bd-final.img` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/tcp.rs` — helper + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` / `lib.rs` — `rust_tcp_outflags` export
* `kernel/network/tcp_subr.inc` — gate + trampoline
* `kernel/rust/tcp_outflags.inc` — blob embed + smoke
* `kernel/kernel32.inc` / `kernel/kernel.asm` — include + smoke call
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-bd-plan.md` / `cut-bd-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md` / `migration-todo.md`

---

## Known limitations

* Out-of-range `t_state > 10`: Rust returns 0; FASM would read past `.flaglist`
  into following instruction bytes — undefined and not reproduced.
* Full remote TCP handshake soak is not automated in this environment.
