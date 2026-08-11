# Cut AY Implementation — `net_ptr_to_num4`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-ay-plan.md`](cut-ay-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `net_ptr_to_num4` |
| Source | [`kernel/network/stack.inc`](../../kernel/network/stack.inc) |
| Callers | ~12 (IPv4/ARP/ICMP/TCP/UDP packet paths + `net_ptr_to_num` wrapper) |
| Rust symbol | `rust_net_ptr_to_num4` |
| Pure helper | `kolibri_utils::net_ptr_to_num4` / `net_ptr_to_num4_from_slice` |
| Subsystem | Network NIC device-list index resolve |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AX audit: XFS/NTFS-MCB/AHCI/PE Path A still fail
the raised bar; address-math / calendar / USB leftovers stay ban-listed or weak.
Selected **`net_ptr_to_num4`** — new semantic class vs AS (socket ZF membership)
and AU (fragment keyed scan): fixed-width `net_device_list` pointer → index×4.

REG-001: trampoline preserves **EAX+EBX+ECX+EDX+ESI+EBP**; **EDI** is the result
(`mov edi, eax` after stdcall). TCP send keeps socket* in EAX across the call;
ipv4/arp keep packet headers in EDX.

Cut AC’s private route-path helper was renamed to `find_device_idx4` so the crate
root exports the Cut AY leaf without a name clash (AC blob stays self-contained;
no cross-section call into AY).

---

## Candidate comparison (post-AX audit)

| Candidate | Outcome |
|-----------|---------|
| `net_ptr_to_num4` | **Selected** — device ptr→index×4 |
| `get_proc_ex` | #2 — PE export resolve; PE ban stretch |
| `bdfe_to_fat_time` | Reject — easy calendar / AO pair |
| `usb_td_to_virt` | Defer — AQ compose / weak USB soak |
| `rebase_coff` | Defer — Y mutate anti-cluster |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_NET_PTR_TO_NUM4=0`:

```text
register call net_ptr_to_num4
in:  EBX = device ptr
out: EDI = index×4 (byte offset into net_device_list) or −1
preserves: EAX, EBX, ECX (push/pop), EDX, ESI, EBP
clobbers: flags; EDI is result
ret 0
```

Quirks retained:

* null `EBX` → `EDI = −1` without scanning
* full `NET_DEVICES_MAX` (16) scan including null “holes”
* first match wins if duplicates ever appear
* miss uses `or edi, -1` (all-ones)

---

## Rust ABI

```text
stdcall rust_net_ptr_to_num4(device, list_base, max) → EAX
  EAX = index×4 or 0xFFFFFFFF
  ret 12
```

Trampoline: injects `net_device_list` + `NET_DEVICES_MAX`; `mov edi, eax`;
restores EAX/EBX/ECX/EDX/ESI/EBP.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `net_ptr_to_num4.rs` + `ffi.rs` section `.text.rust_net_ptr_to_num4` |
| Extract | `extract_reloc_free_text.py` → `rust_net_ptr_to_num4.bin` |
| Embed | `kernel/rust/net_ptr_to_num4.inc` `file` directive |
| Trampoline | `stack.inc` under `USE_RUST_NET_PTR_TO_NUM4` |
| Gate | `USE_RUST_NET_PTR_TO_NUM4` (prod 1) |
| Smoke | `net_ptr_to_num4_rust_smoke_test` (after `stack_init`, after AU smoke) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_net_ptr_to_num4` |
| Blob/object size | 73 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `2BB2BC84EB8AB01EABD7D09CF71331A3D8E025C3556A47E398E9D68EA0F3CBF8` |
| Epilogue | `ret 12` (`c2 0c 00`) |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs slice/ptr helpers | **PASS** |
| Named vectors | null; empty; hit first/mid/last; holes; miss; full 16; first-match duplicate |
| PRNG | 50 000 vectors, seed `0x43555459` (`'CUTY'`) |
| Host tests | **480/480** cargo tests (incl. net_ptr_to_num4 suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `net_ptr_to_num4_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C59` hang) |
| Vectors | rust_* null/hit/miss; public trampoline miss+null; **live loopback hit** (no list mutate; REG-003) |
| Marker | `rust_net_ptr_to_num4_smoke_result = 'NPT4'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_NET_PTR_TO_NUM4=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_NET_PTR_TO_NUM4=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop (IDE) | 779380 | 779380 | **match** |

---

## Real subsystem soak

```text
Real subsystem soak: NOT AVAILABLE
```

Stock `scripts/run_qemu.py` / `qmp_desktop_smoke.py` do not attach a NIC
(`-netdev` / e1000 / etc.). Packet-hot callers of `net_ptr_to_num4` are therefore
not forced. Coverage is host differentials + boot ABI smoke (marker `NPT4`) on
synthetic `net_device_list` fixtures. Same class of limitation as Cuts AS/AU.

---

## Regressions

```text
REG-003 — FIXED during Cut AY
```

ABI smoke after `loop_init` planted over / zeroed `net_device_list[0]`, destroying
the live loopback entry. Not a Rust leaf / trampoline register bug. Fix: smoke
queries existing loopback; does not mutate the live device list. See
[`regression-log.md`](regression-log.md) REG-003.

---

## Production gate

| Item | Value |
|------|-------|
| Gate | `USE_RUST_NET_PTR_TO_NUM4 = 1` |
| `project/build.toml` | `[[rust.migrations]]` cut `AY`, `enabled = true` |
| Rollback | `USE_RUST_NET_PTR_TO_NUM4 = 0` or `enabled = false` |

---

## Image

`dev_build/cut-ay-final.img` (CoW copy of ON production kernel).

---

## Files changed

* `rust_kernel/kolibri_utils/src/net_ptr_to_num4.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/ipv4_route.rs` (rename private helper → `find_device_idx4`)
* `kernel/rust/net_ptr_to_num4.inc` (new)
* `kernel/network/stack.inc` (gate + trampoline)
* `kernel/kernel32.inc`
* `kernel/kernel.asm`
* `project/build.toml`
* `docs/migration/cut-ay-plan.md`
* `docs/migration/cut-ay-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`

---

## Known limitations

* No Path A claim for networking ownership
* AC `ipv4_route` still carries an inlined device-index helper (`find_device_idx4`) inside its own reloc-free blob — intentional to avoid cross-section calls; semantics match AY
* Packet-path soak **NOT AVAILABLE** without a QEMU NIC harness
* Thin leaf; blast radius is packet-hot (~12 callers) — trampoline EAX restore is mandatory (tcp_output)
* REG-003: post-`loop_init` smokes must not clobber `net_device_list`

**Stop; do not start Cut AZ.**
