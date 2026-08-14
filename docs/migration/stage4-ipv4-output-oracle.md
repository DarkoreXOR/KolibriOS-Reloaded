# Stage-4 `ipv4_output` packet oracle

**Date:** 2026-08-14  
**Status:** COMPLETE — research / tooling only  
**Inventory:** **105 / 138** (unchanged)  
**Production gates:** **106** (unchanged)  
**Production changes:** **NONE**  
**Cut CX:** not started — do not migrate `ipv4_output`

Parent: [`post-cw-next-frontier.md`](post-cw-next-frontier.md), [`reg012-headroom-audit.md`](reg012-headroom-audit.md)

---

## 0. Verdict

| Question | Answer |
|----------|--------|
| Can `ipv4_output` be isolated as a Path B leaf? | **Yes** — header fill after FASM route + ARP + `eth_output` |
| Independent packet-byte oracle? | **Yes** — RFC 791 header + RFC 1071 checksum (not Cut E/F, not FASM) |
| Host 50k PRNG? | **Yes** — seed `'IPV4'` (`0x4950_5634`), 12 tests PASS |
| Live QEMU capture? | **Yes** — `-netdev user` + `e1000` + `filter-dump` |
| FASM output captured? | **Yes** — guest UDP `IPV4SOAK{n}` + DHCP via `ipv4_output` |
| Repeated ×3, RESET=0? | **Yes** — identical guest UDP headers (`10.0.2.15→10.0.2.2`, TTL 128, ID 0, cksum `0x22b7`) |
| Custom UDP app payload? | **Yes** — firstapp `/sys/IPV4SOAK`; msg_board `START`/`PASS`; 5 UDP/run |
| Ready to migrate now? | Evidence is enough to *plan* a future Path B cut; **this turn does not implement it** |
| Decision | **IPV4_OUTPUT EVIDENCE READY** |

---

## 1. Source / ABI audit

**File:** `kernel/network/IPv4.inc` (`ipv4_output` @ ~680).

### Register ABI (live callers, not the file banner)

The banner claims `AL=protocol`, `AH=TTL`. **Every live caller does the opposite:**

```text
mov al, [socket.ttl]     ; TTL
mov ah, IP_PROTO_*       ; protocol
call ipv4_output
```

`pop word [TimeToLive]` stores little-endian `AX` at the TTL field: `AL→TTL`, `AH→Protocol`. Callers are the contract. The comment `ttl shl 8 + protocol` is misleading.

| In | Register | Meaning |
|----|----------|---------|
| protocol | `AH` | `IP_PROTO_ICMP=1`, `TCP=6`, `UDP=17` |
| TTL | `AL` | default 128 (`IP_SOCKET.ttl`) |
| device | `EBX` | `NET_DEVICE*` or 0 (auto-route) |
| payload length | `ECX` | not including IPv4 header; `>65500` → fail |
| source IP | `EDX` | 0 ⇒ route fills |
| dest IP | `EDI` | original dest; **header dest is this**, even if route rewrote next-hop |

| Out (success) | |
|---------------|--|
| `EAX` | `NET_BUFF*` (buffer start); **0 on error** (`jz` from callers) |
| `EBX` | device pointer |
| `ECX` | payload length (unchanged intent) |
| `EDX` | complete Ethernet frame size |
| `EDI` | start of IPv4 **payload** (after 20-byte header) |

Errors (`EAX=0`): too large, no route, ARP error bits (`test eax, 0xffff0000`, including `-2` request-sent), `eth_output` ZF.

**Not modeled as leaf behavior:** DF (CPU), locks (no mutex in the leaf), payload copy (callers `rep movsb` after return), fragmentation (`ipv4_fragment` is a **separate** proc, not called here), IP options (IHL always `0x45`), Identification (always **0**, FASM `FIXME`), DF/MF flags (always 0).

Kolibri IPv4 dwords are on-wire bytes as LE (`10.0.2.15` = `0x0F02000A`). EtherType `ETHER_PROTO_IPv4 = 0x0008` stores wire `08 00`.

### Callers

| Site | Path |
|------|------|
| `udp.inc` `udp_output` | sockets send UDP (`AL=ttl`, `AH=17`) |
| `icmp.inc` `icmp_output_raw` | ICMP |
| `tcp_output.inc` `tcp_output` | TCP data/control |
| `tcp_subr.inc` `tcp_respond` | ACK/RST/keepalive |
| `tcp_subr.inc` `tcp_respond_segment` | RST from a received segment (`AX = TCP<<8 \| 128`) |

`ipv4_output_raw` (`socket.inc` `socket_send_ip`) is a **sibling**: copies bytes from `ESI`, does **not** fill Version/IHL/TTL (commented out), then checksums + `NET_DEVICE.transmit`. Different ABI (`EAX=socket`, fail `EAX=-1`). **Out of this leaf.**

### Dependency boundary (keep FASM)

```text
ipv4_output
  ├─ ipv4_route          (Cut AC already Rust; injected tables)
  ├─ arp_ip_to_mac       (FASM; mock in host oracle)
  ├─ eth_output          (FASM alloc + Ethernet header; mock capture shim)
  │    └─ NET_DEVICE.transmit   (driver; QEMU e1000 / filter-dump)
  └─ loop_output         (edi==0 loopback; not on the NIC pcap path)
```

Side effect: `inc [IPv4_packets_tx + edi]`. No lock in the leaf.

Host oracle injects route/ARP/eth success/failure. Live soak uses real FASM ARP + `eth_output` + I8254X; the oracle compares **IPv4 header bytes** after those succeed.

---

## 2. Independent packet model

Host module: `rust_kernel/kolibri_utils/src/ipv4_output_oracle.rs` (`#[cfg(test)]` only).

Python twin: `scripts/net_capture.py` (`build_ipv4_header` / `rfc1071_checksum`).

20-byte header:

| Field | Value |
|-------|--------|
| Version/IHL | `0x45` |
| TOS | 0 |
| TotalLength | `htons(20 + payload_len)` |
| Identification | 0 |
| Flags/frag | 0 |
| TTL / Protocol | request `AL` / `AH` |
| Checksum | RFC 1071, field 0 during sum; 0 stored as `0xFFFF` |
| Src / Dst | Kolibri dword memory order (`to_le_bytes` = on-wire) |

Payload is **not** part of `ipv4_output`; callers write after return. Capture compares Ethernet + IPv4 + payload.

Test-only `capture_shim` copies Ethernet (dst/src MAC + EtherType `0x0800`) + IPv4 bytes. **Not** wired into the kernel.

---

## 3. Checksum oracle

RFC 1071 one's-complement over the header with checksum bytes treated as 0. Independent of `checksum_1` / `checksum_2` (those have a different zero/`not` order). The leaf uses the FASM `ipv4_checksum` macro, not Cuts E/F.

Verified: stored checksum never 0 (TTL sweep 0..255); 50k random headers validate; Python self-test PASS; live guest UDP checksum `0x22b7` matches the independent rebuild.

---

## 4. Host vectors

| ID | Coverage |
|----|----------|
| A | UDP-like proto 17 + payload |
| B | TCP proto 6 field only (no TCP SM) |
| C | ICMP proto 1 |
| D/E | empty and 1400-byte payload |
| F | distinct src/dst; dest header uses **original** dest, not gateway |
| G | TTL 1/64/128/255 |
| H | Identification always 0 |
| I | DF/MF **unsupported** (always 0) |
| J | options **unsupported** (IHL=5) |
| K | checksum edges + never-zero store |
| L | max payload 65500 vs 65501 too-large |
| M | too-large error |
| N | mocked no-route / ARP / eth fail |

PRNG: XorShift32, seed `0x4950_5634`, **50_000** cases.

---

## 5. QEMU network harness

Default `[qemu].args` still have no NIC (desktop-only). Disposable soak adds:

- `scripts/qmp_ipv4_output_soak.py`
- `scripts/net_capture.py`
- guest `tools/ipv4_output_guest/ipv4soak.asm` (MENUET01)

Config per run:

```text
-netdev user,id=n0,net=10.0.2.0/24,dhcpstart=10.0.2.15
-device e1000,netdev=n0,mac=52:54:00:12:34:56
-object filter-dump,id=dump0,netdev=n0,file=dev_build/memory/ipv4-output-runN.pcap
```

Guest launch (test image only, not production `firstapp`):

1. `kolibri_img put IPV4SOAK`
2. Patch `KERNEL.MNT` `/sys/LAUNCHER\0` → `/sys/IPV4SOAK\0` (same length)
3. AUTORUN append `/sys/IPV4SOAK` as backup

The app waits for NIC count ≥ 2, otherwise `sysfn 70.7` starts `/sys/LAUNCHER`, then sets `10.0.2.15/24` gw `10.0.2.2` and UDP-sends `IPV4SOAK{n}` to `10.0.2.2:9`. Msg-board: `START` / `LAUNCH` / `NIC` / `IP` / `SEND` / `PASS`.

Live runs always took the `LAUNCH` path (I8254X is not up at firstapp). Capture: QEMU `filter-dump` classic pcap, DLT_EN10MB. No TAP, no admin.

---

## 6. Repeatability (FASM baseline)

Image: `dev_build/test/kernel-20260814-145116.img`  
JSON: `dev_build/memory/ipv4-output-soak.json`

| Run | status | RESET | frames | guest IPv4 | guest UDP stimulus | oracle | board |
|-----|--------|------:|-------:|-----------:|-------------------:|--------|-------|
| 1 | running | 0 | 20 | 8 | 5× `IPV4SOAK0..4` | 8/8 guest headers | START…PASS |
| 2 | running | 0 | 20 | 8 | 5× same | 8/8 | START…PASS |
| 3 | running | 0 | 20 | 8 | 5× same | 8/8 | START…PASS |

Guest stimulus IPv4 (all three runs):

| Field | Value |
|-------|--------|
| Ethernet src | `52:54:00:12:34:56` |
| Ethernet dst | `52:55:0a:00:02:02` (slirp gw) |
| EtherType | `0x0800` |
| src / dst | `10.0.2.15` → `10.0.2.2` |
| TTL | 128 |
| proto | 17 |
| ID / flags | 0 / 0 |
| IHL / TOS | 20 / 0 |
| total length | 38 (20 IP + 8 UDP + 10 payload) |
| checksum | `0x22b7`, RFC 1071 OK, independent rebuild match |

Also present (not the authored stimulus, still FASM `ipv4_output`): DHCP/UDP `0.0.0.0→255.255.255.255` TTL 128 ID 0.

QEMU slirp ICMP port-unreachable (TTL 255, TOS 192, ID≠0) quotes the UDP payload; **excluded** from guest-origin scoring.

---

## 7. Failure classes

| Class | This evidence |
|-------|----------------|
| QEMU net config | no — pcap 3976 B ×3 |
| Guest net init | no — `IPV4SOAK NIC` / `IP` |
| Packet not generated | no — 5 UDP/run |
| Not captured | no |
| ARP | present (needed for 10.0.2.2) |
| Checksum mismatch | no for guest IPv4 |
| Custom UDP magic | captured (`IPV4SOAK0..4`) |
| RESET / shutdown | 0 / 0 |

---

## 8. Future migration readiness

| Gate | Status |
|------|--------|
| Exact ABI | Documented (TTL in `AL`) |
| Independent packet oracle | Host 50k + Python parser |
| Deterministic FASM capture | UDP stimulus ×3 identical |
| QEMU network soak | user-net + e1000 + filter-dump |
| Host parser | `scripts/net_capture.py` |
| Callback boundary | route/ARP/`eth_output` stay FASM |
| Memory | slack **2365 B** after REG-012; still measure blob |
| Rollback | no gate yet; FASM body intact |

**Still not a cut:** no trampoline, no `USE_RUST_IPV4_OUTPUT`, no blob. `tcp_output` remains a separate island. `ipv4_output_raw` still quirky.

---

## 9. Artifacts

| Item | Path |
|------|------|
| Oracle (Rust) | `rust_kernel/kolibri_utils/src/ipv4_output_oracle.rs` |
| Parser | `scripts/net_capture.py` |
| QEMU harness | `scripts/qmp_ipv4_output_soak.py` |
| Guest stimulus | `tools/ipv4_output_guest/ipv4soak.asm` |
| Captures | `dev_build/memory/ipv4-output-run{1,2,3}.pcap` |
| Summary JSON | `dev_build/memory/ipv4-output-soak.json` |
