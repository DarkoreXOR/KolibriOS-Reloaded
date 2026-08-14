# Cut CX Implementation — `ipv4_output`

**Date:** 2026-08-14  
**Status:** **COMPLETE**  
**Inventory:** **105 → 106 / 138** (one Path B symbol)  
**Production gate:** `USE_RUST_IPV4_OUTPUT = 1`  
**Plan:** [`cut-cx-plan.md`](cut-cx-plan.md)  
**Evidence:** [`stage4-ipv4-output-oracle.md`](stage4-ipv4-output-oracle.md)

---

## 1. Boundary

| Item | Value |
|------|--------|
| Symbol | `ipv4_output` |
| Source | `kernel/network/IPv4.inc` |
| Path | **B** (header composition + output orchestration) |
| Gate | `USE_RUST_IPV4_OUTPUT` |
| Smoke | `IPV4` / fail hang `0xDEAD0C78` |

**Not migrated:** `ipv4_output_raw`, ARP, Ethernet, routing tables, TCP/UDP/ICMP,
socket state, netdev, `ipv4_fragment`, Cut E/F checksum helpers.

Call Cut AC via the **public** `ipv4_route` register ABI (not `rust_ipv4_route`).

---

## 2. Legacy ABI (preserved)

**Live inputs** (banner in source is inverted; callers win):

| Reg | Meaning |
|-----|---------|
| `AL` | TTL |
| `AH` | protocol |
| `EBX` | device or 0 (`tcp_respond_segment` may leave unset — preserve) |
| `ECX` | payload length (not including IPv4 header) |
| `EDX` | source IP (0 ⇒ route fills) |
| `EDI` | destination IP (**header dest stays this value**) |

**Success:** `EAX`=buffer, `EBX`=device, `ECX`=payload len, `EDX`=frame size
(or loopback leftover routed source), `EDI`=payload start, **ZF=0**, `cld`.

**Failure:** `EAX`=0, **ZF=1**. No CF contract. ESI clobbered.

---

## 3. Rust ABI / context

Public FASM: `call` / `ret`.

Inner: `stdcall rust_ipv4_output(ctx: *mut Ipv4OutputCtx) -> u32`  
section `.text.rust_ipv4_output`, **`ret 4` only (REG-009)**.

`Ipv4OutputCtx` — `#[repr(C)]`, **56 bytes**, stack-local:

| Off | Field |
|----:|-------|
| +0 | `ttl_proto` (u16) + pad |
| +4 | `device_in` |
| +8 | `payload_len` |
| +12 | `source_in` |
| +16 | `dest_in` |
| +20 | `route` → public `ipv4_route` |
| +24 | `arp` → `arp_ip_to_mac` |
| +28 | `eth_out` → `eth_output` |
| +32 | `loop_out` → `loop_output` |
| +36 | `packets_tx` |
| +40 | `net_devices` |
| +44 | `out_device` |
| +48 | `frame_size` |
| +52 | `payload_ptr` |

Trampoline ends: restore outs → `test eax,eax` / `jnz` → `test edi,edi`
(loopback-OOM continue) → `cld` → leave.

---

## 4. Semantics

- Header: `0x45`, TOS 0, ID 0, flags/frag 0, total `htons(20+payload)`, no options
- Dest IP = original `EDI`; source = routed source (loopback: next-hop quirk)
- Checksum: independent RFC 1071; field 0 during sum; 0 → `0xFFFF`; store BE
- Payload not copied
- `IPv4_packets_tx++` after ARP success / before `eth_output`; before `loop_output`;
  **not** undone on eth ZF fail
- Loopback `loop_output` EAX=0: still write header (legacy continue)

---

## 5. Blob / memory

| Item | Value |
|------|--------|
| Blob size | **850 B** |
| Relocations | **0** |
| SHA-256 | `00be8a07e9d19789dd5ad2197091487054ba85e337e0463879588a3f65b1a732` |
| Epilogue | `ret 4` (`5d c2 04 00`) |
| `kernel.mnt` | **306472** |
| `.bss` end | `OS_BASE+0x8CE03` |
| Assert slack | `0x8E000 - (0x8CE03+0x1000)` = **509 B** |
| Pack | `TMP_STACK_TOP` / `sys_proc` `0x008E000`, `SLOT_BASE` `0x0090000` — **unchanged** |

Blob + smoke are gated (`if USE_RUST_IPV4_OUTPUT`) so OFF builds do not pay
low-pack cost. Smoke fixtures are stack-local (REG-012).

---

## 6. Validation

| Gate | Result |
|------|--------|
| Host focused + 50k vs independent oracle | **PASS** |
| ABI smoke `IPV4` | **PASS** (boot continued; no `0xDEAD0C78`) |
| QEMU OFF | **PASS** (RESET=0, board PASS, checksum `0x22b7`) |
| QEMU ON | **PASS** |
| A/B guest UDP/DHCP IPv4 headers | **PASS** (exact-byte) |
| ON×3 | **PASS** (RESET=0) |
| Rollback OFF | **PASS** (gate 0 restores FASM; OFF soak green) |

**Implementation fix during ON soak:** `invoke_eth` must not reload the
out-pointer through `ESI` after `eth_output` (callee clobbers ESI). Push the
out pointer before `call` (REG-017 class). First ON×3 before the fix page-faulted;
after the fix, ON×3 PASS.

---

## 7. Artifacts

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/ipv4_output.rs` | production leaf |
| `kernel/rust/ipv4_output.inc` | blob embed + smoke |
| `kernel/network/IPv4.inc` | gated trampoline + FASM rollback |
| `dev_build/test/kernel-20260814-154644.img` | final ON image |
| `dev_build/memory/ipv4-output-soak.json` | ON×3 summary |
| `dev_build/memory/ipv4-output-off.pcap` | OFF capture |
| `dev_build/memory/ipv4-output-run{1,2,3}.pcap` | ON captures |

---

## 8. Rollback

`USE_RUST_IPV4_OUTPUT = 0` (or `enabled = false` in `project/build.toml` +
`apply_gates`) restores the original FASM body. No mixed state.
