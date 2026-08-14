# Cut CX Plan — `ipv4_output`

**Date:** 2026-08-14  
**Status:** **COMPLETE** — Path B production migration of `ipv4_output` only  
**Inventory:** **106 / 138**  
**Production gates:** **107** `[[rust.migrations]]` enabled (`USE_RUST_IPV4_OUTPUT = 1`)  
**Implementation:** [`cut-cx-implementation.md`](cut-cx-implementation.md)  
**Evidence:** [`stage4-ipv4-output-oracle.md`](stage4-ipv4-output-oracle.md) — **IPV4_OUTPUT EVIDENCE READY**  
**Frontier:** [`post-cw-next-frontier.md`](post-cw-next-frontier.md)  
**Headroom:** [`reg012-headroom-audit.md`](reg012-headroom-audit.md) — slack **2365 B**  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md)

> **Nomenclature:** **Cut CX** migrates **only** `ipv4_output` in
> `kernel/network/IPv4.inc` (IPv4 header composition + output orchestration).  
> **Path B.** Do **not** start implementation from this document until
> authorized. Do **not** add the production gate in the planning task.  
> Do **not** migrate `ipv4_output_raw`, ARP, Ethernet, routing tables,
> netdev, TCP/UDP/ICMP, or PTE.

---

## 0. Verdict

| Item | Value |
|------|--------|
| Target | `ipv4_output` |
| Source | `kernel/network/IPv4.inc` (banner ~660; body @ 680–770) |
| Subsystem | IPv4 packet composition / output orchestration |
| Path | **B** |
| Path A? | **REJECTED** — does not own ARP cache, route tables, netdev, or `net_buff` |
| Proposed gate | `USE_RUST_IPV4_OUTPUT` (**not added yet**) |
| Proposed smoke marker | `IPV4` (`0x49505634`) |
| Decision | **CUT CX READY FOR IMPLEMENTATION** |
| Production changes in this task | **NONE** |

Selection rationale: evidence program is complete (independent RFC 791/1071 oracle, 50k host cases, live QEMU user-net/`filter-dump` ×3, RESET=0). REG-012 slack (2365 B) is enough for a ~250–550 B class blob. The leaf is a bounded orchestrator: header fill + checksum after FASM route/ARP/`eth_output`. Callers stay FASM. Sibling `ipv4_output_raw` is a different ABI and is out of scope.

---

## 1. Fresh source audit (2026-08-14)

Re-read against live `kernel/network/IPv4.inc`. Evidence summary still matches production FASM. **Do not use the inverted file banner as the ABI.**

### 1.1 Banner vs live callers

Banner @ 664–667 claims `AL = protocol`, `AH = TTL`. Every live caller does the opposite:

| Caller | Packing |
|--------|---------|
| `udp_output` | `mov al, [socket.ttl]` / `mov ah, IP_PROTO_UDP` |
| `icmp_output_raw` | `al = ttl`, `ah = IP_PROTO_ICMP` |
| `tcp_output` | `al = ttl`, `ah = IP_PROTO_TCP` |
| `tcp_respond` | same |
| `tcp_respond_segment` | `mov ax, IP_PROTO_TCP shl 8 + 128` → **AH=6, AL=128** |

Body stores `pop word [TimeToLive]`: little-endian `AX` at TTL/Protocol, so **AL → TTL, AH → Protocol**. Callers are the contract.

### 1.2 Body (verified)

```680:770:kernel/network/IPv4.inc
ipv4_output:
        cmp     ecx, 65500
        ja      .too_large
        push    ecx ax edi          ; dest, AX, length (see stack map §2.3)
        mov     eax, edi
        call    ipv4_route
        test    eax, eax
        jz      .no_route
        push    edx
        test    edi, edi
        jz      .loopback
        call    arp_ip_to_mac
        test    eax, 0xffff0000
        jnz     .arp_error
        push    ebx
        push    ax
        inc     [IPv4_packets_tx + edi]
        ; eth_output(proto=IPv4, device, size=payload+20, dest_mac=esp)
        call    eth_output
        jz      .eth_error
        add     esp, 6
  .continue:
        ; write 20-byte header at EDI; pop src/dest/AX/ecx; ipv4_checksum; edi+=20
        ret
```

`ipv4_fragment` (`proc` @ ~900) is **not** called. Identification is always 0 (`FIXME` in source). Flags/frag always 0. IHL always `0x45`. TOS always 0. No options.

### 1.3 What this cut is not

| Symbol | Why out |
|--------|---------|
| `ipv4_output_raw` | Different ABI (`EAX=socket`, copies `ESI`, fail `EAX=-1`); does not fill Version/IHL/TTL |
| `ipv4_route` | Already Cut AC; call the **public** FASM trampoline |
| `arp_ip_to_mac` | ARP cache + request + optional `delay_ms` (`ARP_BLOCK=1`) |
| `eth_output` | `net_buff_alloc` + Ethernet header + MTU |
| `loop_output` | Loopback `net_buff_alloc` |
| TCP/UDP/ICMP | Callers; they write payload after return and `transmit` |
| `page_tabs` / PTE | Unrelated; still blocked |

---

## 2. Exact legacy ABI

### 2.1 Inputs (live)

| Reg | Meaning |
|-----|---------|
| `AL` | TTL |
| `AH` | IP protocol (`1` ICMP, `6` TCP, `17` UDP, …) |
| `EBX` | `NET_DEVICE*` or **0** (auto-route) |
| `ECX` | payload length **not** including IPv4 header; `ja` if `> 65500` |
| `EDX` | requested source IP; **0** ⇒ route fills from `IPv4_address[dev]` |
| `EDI` | destination IP (header dest is **this** value, even if route rewrites next-hop) |

`tcp_respond_segment` does **not** load `EBX`. Whatever `EBX` is at entry is passed to `ipv4_route`. Do **not** “fix” this.

### 2.2 Outputs

**Success** (`EAX ≠ 0`, ZF=0 from `add edi, 20`):

| Reg | Meaning |
|-----|---------|
| `EAX` | `NET_BUFF*` (buffer start) |
| `EBX` | device pointer (`eth_output` keeps it; loopback sets `LOOPBACK_DEVICE`) |
| `ECX` | original payload length (popped) |
| `EDX` | Ethernet path: complete frame size from `eth_output` (`max(14+IPv4_total, 60)`). **Loopback path: not a frame size** — `loop_output` does not write `EDX`; leftover is routed source IP. Preserve. |
| `EDI` | start of IPv4 **payload** (header+20) |

**Failure** (`EAX = 0`, ZF=1 from `xor eax, eax`):

| Path | Stack unwind | Other regs | Counter |
|------|--------------|------------|---------|
| `.too_large` | none | entry regs except EAX=0 | no |
| `.no_route` | `add esp, 10` | route leftovers in EDX/EDI/EBX | no |
| `.arp_error` | pop src+dest+AX+len | EAX then zeroed | no |
| `.eth_error` | `add esp, 20` | EAX=0 | **already incremented** |

Callers all `jz .fail` / `jz .error` / `jz .ip_error`. ZF follows EAX. **Do not invent a CF contract.** Do not `setc` (REG-018).

### 2.3 Stack map (success Ethernet)

`push ecx` / `push ax` / `push edi` then `push edx` then MAC `push ebx` / `push ax`:

```text
[esp+0]  dest MAC  6 bytes
[esp+6]  source IP
[esp+10] original dest IP
[esp+14] AX (TTL/protocol)
[esp+16] payload length     ; [esp+6+8+2] in FASM
```

After `eth_output` success, `add esp, 6` drops MAC. `.continue` pops src, dest, AX, ecx.

### 2.4 Clobber / preserve

| Reg | Success | Failure |
|-----|---------|---------|
| EAX | buffer | 0 |
| EBX | device | leftover (not restored) |
| ECX | original length | leftover |
| EDX | see §2.2 | leftover |
| ESI | **clobbered** (`arp`/`eth` use it) | clobbered |
| EDI | payload start | leftover |
| EBP | preserved if callbacks preserve it (Cut AC `ipv4_route` saves EBP) | |
| DF | not set by the leaf; callers assume DF=0 (`rep movsb`). Implementation must `cld` before `ret`. | |
| ZF | 0 | 1 |
| CF | not a public contract | |

Convention: `call` / `ret` (not stdcall) at the public `ipv4_output` boundary. Inner Rust is stdcall (REG-009: callee cleans **only** the stdcall arg).

### 2.5 Caller liveness after return

| Caller | Uses after success | Failure |
|--------|--------------------|---------|
| `udp_output` | EAX buffer, EDI UDP header, ECX length, EBX device, then `transmit` | `jz .fail` → EAX=-1 |
| `icmp_output_raw` | EDI dest, ECX, EAX, EBX `transmit` | `jz .fail` |
| `tcp_output` | EDI TCP header dest, ECX, EAX pushed for send | `jz .ip_error` |
| `tcp_respond` | EDI `stosw` TCP header, EBX `transmit`, EAX pushed | `jz .error` |
| `tcp_respond_segment` | same pattern | `jz .error` |

None of them consume success-`EDX` as frame size. Still match FASM `EDX` on both paths.

---

## 3. Cut boundary

```text
caller (udp/icmp/tcp — stay FASM)
  ↓  register ABI §2
ipv4_output  [FASM trampoline, gate ON]
  ↓  stdcall rust_ipv4_output(ctx)
Rust
  ├── length check
  ├── call [ctx.route]        → public ipv4_route (Cut AC FASM trampoline)
  ├── if edi==0: call [ctx.loop_out]
  ├── else: call [ctx.arp]; inc packets_tx; call [ctx.eth_out]
  ├── write 20-byte header + RFC 1071 checksum
  └── return EAX + fill ctx outs
  ↓
caller (payload copy + NET_DEVICE.transmit — stay FASM)
```

**Call Cut AC via the public `ipv4_route` register ABI**, not `rust_ipv4_route`. Injecting `rust_ipv4_route` would require table bases (`IPv4_address/subnet/gateway`, `net_device_list`) and a Rust→Rust link (relocs / dual ownership). The FASM trampoline already injects those.

Keep FASM: ARP cache, `net_buff_alloc`, Ethernet header, loopback device, socket mutex, `transmit`.

---

## 4. Packet buffer ownership

`ipv4_output` does **not** allocate. `eth_output` / `loop_output` call `stdcall net_buff_alloc`.

### Ethernet (`eth_output`)

| Item | Value |
|------|--------|
| Alloc size | `payload_ip + sizeof.ETH_header + NET_BUFF.data` = `(ecx+20) + 14 + 24` |
| `NET_BUFF.type` | `NET_BUFF_ETH` (1) |
| `NET_BUFF.offset` | `NET_BUFF.data` (24) |
| `NET_BUFF.device` | `ebx` |
| Layout | `[NET_BUFF 24][ETH 14: dst MAC, src MAC, type 0x0008][IPv4 20][payload hole]` |
| `EAX` | `NET_BUFF*` |
| `EDI` | IPv4 header (= Ethernet payload) |
| `EDX` | `max(14+IPv4_total, ETH_FRAME_MINIMUM=60)` written to `NET_BUFF.length` |
| `ECX` | IPv4 total (header+payload) unchanged |

Dest MAC is 6 bytes at `[edx]` (stack). Src MAC copied from `ETH_DEVICE.mac`. Type = `ETHER_PROTO_IPv4` (`0x0008` in AX → on-wire `08 00`).

### Loopback (`loop_output`)

| Item | Value |
|------|--------|
| Alloc size | `IPv4_total + NET_BUFF.data` |
| `NET_BUFF.type` | `AF_INET4` (2) — FASM `mov edi, AF_INET4` |
| Layout | `[NET_BUFF 24][IPv4 20][payload hole]` — **no Ethernet header** |
| `EDI` | `eax + NET_BUFF.data` (IPv4 header) |
| `EDX` | **not written** |
| Failure | EAX=0; **`ipv4_output` does not check** — `jmp .continue` anyway. Preserve. |

Caller writes payload at returned `EDI`. Header writes must not assume payload is present. No extra alignment beyond `net_buff_alloc`.

---

## 5. Header semantics (exact bytes)

20-byte `IPv4_header` at `EDI` after buffer alloc. Kolibri IPv4 addresses are **on-wire octets stored as little-endian dwords** (`10.0.2.15` = `0x0F02000A` → bytes `0A 00 02 0F`). Confirmed by live capture.

| Offset | Field | Value | Order |
|--------|-------|-------|--------|
| 0 | Version/IHL | `0x45` | byte |
| 1 | TOS | `0` | byte |
| 2–3 | TotalLength | `htons(20 + payload_len)` | BE (`xchg cl, ch` of CX = payload+20) |
| 4–5 | Identification | `0` | BE word 0 |
| 6–7 | Flags + frag | `0` | no DF/MF |
| 8 | TTL | `AL` | byte |
| 9 | Protocol | `AH` | byte |
| 10–11 | Checksum | RFC 1071 | BE |
| 12–15 | Source | routed source (popped `edx` after route; loopback may overwrite stack slot with route `eax`) | LE dword = on-wire |
| 16–19 | Destination | **original** `EDI` dest, not gateway next-hop | LE dword |

Loopback quirk: `mov dword [esp], eax` replaces the pushed **source** with route’s dest/next-hop (`eax`) before `loop_output`. Header source on 127.0.0.1 is therefore dest IP. Preserve.

Do **not** call `ipv4_fragment`. Do **not** add options (IHL stays 5).

Live UDP evidence (FASM baseline): `0x45`, TOS 0, total 38, ID 0, flags 0, TTL 128, proto 17, src `10.0.2.15`, dst `10.0.2.2`, checksum `0x22b7`.

---

## 6. Checksum

FASM inlines `ipv4_checksum edi` (macro @ 140–199): 16-bit pairwise add of header bytes with checksum field skipped, `not bx`, zero → `dec bx` (`0xFFFF`), `xchg bl,bh`, then `neg`/`add` into `[ptr+10]` (creation with field 0 stores the value).

**Implementation:** independent RFC 1071 over the 20-byte header with checksum bytes treated as 0; computed 0 stored as `0xFFFF`; store as on-wire BE. **Do not** call the FASM macro and **do not** use Cut E/F `checksum_1`/`checksum_2`.

Oracle: `rfc1071_checksum` in `ipv4_output_oracle.rs` and `scripts/net_capture.py`. Live `0x22b7` already matches. Host 50k + TTL-never-zero stay the differential.

---

## 7. Callback ABIs (do not invent)

All four are **register** `call`/`ret`, not stdcall. Rust reaches them only through **injected pointers** to the public FASM labels (reloc-free).

### 7.1 `ipv4_route` (Cut AC public trampoline)

| | |
|--|--|
| IN | `EAX` = dest IP, `EBX` = device ptr or 0, `EDX` = requested source (0 = fill) |
| OUT | `EAX` = dest/next-hop or **0** on fail; `EDX` = source IP; `EDI` = device index **×4** |
| Destroyed | `EBX`, `ECX` (documented). AC trampoline **preserves ESI/EBP**. |
| Side effects | none on tables (read-only lookup) |
| Rust | `call [ctx.route]` with that register ABI. **Do not** `stdcall rust_ipv4_route`. |

### 7.2 `arp_ip_to_mac`

| | |
|--|--|
| IN | `EAX` = IPv4 (next-hop from route), `EDI` = device×4 |
| OUT success | `EAX` = MAC bytes 0–1 (zero-extended), `EBX` = MAC bytes 2–5, `EDI` unchanged |
| OUT fail | `EAX` with high 16 bits set (`test eax, 0xffff0000`). Production: `-1` (`0xFFFFFFFF`) on full table / no valid mapping. Banner mentions `-2` request-sent; current body with `ARP_BLOCK=1` waits via `delay_ms` then rechecks, or returns `-1`. Treat any high-bit EAX as fail (matches FASM `jnz .arp_error`). Broadcast: `EAX=0x0000FFFF`, `EBX=0xFFFFFFFF`. |
| Clobber | `ESI`, `ECX`; may send ARP request; may `delay_ms` |
| DF | not a contract |

### 7.3 `eth_output`

| | |
|--|--|
| IN | `AX` = `ETHER_PROTO_IPv4` (`0x0008`), `EBX` = device, `ECX` = IPv4 total (payload+20), `EDX` = ptr to 6-byte dest MAC |
| OUT success | `EAX` = `NET_BUFF*`, `EBX` = device, `ECX` = IPv4 total, `EDI` = IPv4 header, `EDX` = frame size, **ZF=0** |
| OUT fail | `EAX=0`, **ZF=1** (`jz .eth_error`). MTU / OOM. |
| ZF | **must capture immediately** (REG-018 class). Do not expose CF. |

### 7.4 `loop_output`

| | |
|--|--|
| IN | `ECX` = IPv4 total, `EDI` = `AF_INET4` (2) |
| OUT success | `EAX` = `NET_BUFF*`, `EBX` = `LOOPBACK_DEVICE`, `ECX` unchanged, `EDI` = IPv4 header |
| OUT fail | `EAX=0`. **Legacy does not branch on this.** |
| EDX | unchanged |

---

## 8. Side effects — `IPv4_packets_tx`

| Item | Fact |
|------|------|
| Storage | `uglobal` `IPv4_packets_tx rd NET_DEVICES_MAX` (`NET_DEVICES_MAX=16`) |
| Index | `edi` = device×4; `inc dword [base + edi]` |
| Width | 32-bit wrap |
| Ethernet | increment **after** ARP success, **before** `eth_output` |
| `eth_error` | increment **already done** — keep |
| Loopback | increment **before** `loop_output` (`edi` is 0) |
| too_large / no_route / arp_error | **no** increment |

Inject `IPv4_packets_tx` base in ctx. Do not normalize “don’t count failed eth”.

---

## 9. Error semantics (public EAX=0 / ZF=1)

| ID | Condition | Callbacks run | Buffer | Counter | Notes |
|----|-----------|---------------|--------|---------|-------|
| A | `ECX > 65500` (`ja`) | none | none | no | unsigned compare |
| B | route `EAX=0` | route only | none | no | unwind 10 bytes |
| C | ARP high bits | route+ARP | none | no | |
| D | `eth_output` ZF | route+ARP+inc+eth | alloc may have failed inside eth | **yes** | |
| E | loopback `loop_output` fail | route+inc+loop | none if OOM | **yes** | still `.continue` (legacy) |

No Rust `Result` at the public boundary.

---

## 10. Context (`Ipv4OutputCtx`)

Stack-allocated by the FASM trampoline. i686, 4-byte aligned, **no heap**.

```text
; size 56 (14 dwords). Offsets for the implementation record.
struct Ipv4OutputCtx
  ttl_proto     dw ?   ; +0  AX as stored (AL=TTL, AH=proto)
                dw ?   ; +2  pad
  device_in     dd ?   ; +4  EBX
  payload_len   dd ?   ; +8  ECX
  source_in     dd ?   ; +12 EDX
  dest_in       dd ?   ; +16 EDI
  route         dd ?   ; +20 ipv4_route
  arp           dd ?   ; +24 arp_ip_to_mac
  eth_out       dd ?   ; +28 eth_output
  loop_out      dd ?   ; +32 loop_output
  packets_tx    dd ?   ; +36 IPv4_packets_tx
  net_devices   dd ?   ; +40 net_device_list
  out_device    dd ?   ; +44 EBX out
  frame_size    dd ?   ; +48 EDX out
  payload_ptr   dd ?   ; +52 EDI out
ends
; EAX out is the stdcall return value.
```

Size **56**. Lifetime: trampoline stack frame only. Do not duplicate `NET_DEVICE` / ARP table / route tables.

Optional smoke-only fields must stay off this struct (separate stack smoke ctx) so production size stays stable.

---

## 11. Trampoline (design only)

```text
USE_RUST_IPV4_OUTPUT = 0   ; default OFF until implementation gates green

if USE_RUST_IPV4_OUTPUT
align 4
ipv4_output:
        ; 1. Snapshot live inputs BEFORE any extra pushes (REG-010).
        ;    AX/EBX/ECX/EDX/EDI are the contract.
        push    ebp
        mov     ebp, esp
        sub     esp, 56                 ; Ipv4OutputCtx, align 4
        ; 2. Fill ctx from registers + injected public labels.
        mov     word [ebp-56], ax       ; ttl_proto
        mov     [ebp-52], ebx
        mov     [ebp-48], ecx
        mov     [ebp-44], edx
        mov     [ebp-40], edi
        mov     dword [ebp-36], ipv4_route
        mov     dword [ebp-32], arp_ip_to_mac
        mov     dword [ebp-28], eth_output
        mov     dword [ebp-24], loop_output
        mov     dword [ebp-20], IPv4_packets_tx
        mov     dword [ebp-16], net_device_list
        lea     eax, [ebp-56]
        stdcall rust_ipv4_output, eax   ; ret 4 only (REG-009)
        ; 3. Restore legacy outs. EAX already buffer/0.
        mov     ebx, [ebp-12]           ; out_device  — verify offsets in impl
        mov     ecx, [ebp-48]           ; original payload_len
        mov     edx, [ebp-8]            ; frame_size
        mov     edi, [ebp-4]            ; payload_ptr
        test    eax, eax                ; ZF = failure (callers jz)
        cld
        mov     esp, ebp
        pop     ebp
        ret
else
  ; original FASM body intact
end if
```

Offset comments above are illustrative; the implementation record must list the **assembled** layout. Do not index stdcall args through a mis-counted `esp` (REG-010). Do not `stdcall` then `add esp, 4` (REG-009). Pin nothing as UTF-8/EBP object here (REG-017 N/A except: do not let callbacks see a wrong `EBP` if a future thunk needs it — AC already saves EBP). Capture eth ZF inside Rust inline-asm **immediately** after `call` (REG-018); do not `setc`/`pop` a saved CF.

Public symbol stays `call`/`ret`. Inner Rust: `extern "stdcall" fn rust_ipv4_output(ctx: *mut Ipv4OutputCtx) -> u32` in `.text.rust_ipv4_output`, `ret 4`.

---

## 12. Relocations

Blob **must** have **0** relocations (extractor + `readelf`/`objdump` check used by other cuts).

| Need | How |
|------|-----|
| Callbacks | ctx function pointers filled by FASM |
| `IPv4_packets_tx` | ctx base pointer |
| `net_device_list` | ctx base; `ebx = [list + edi]` |
| Header constants | immediates (`0x45`, `0x0008`, `65500`, `20`) |
| Packet buffer | returned pointer from `eth_output` / `loop_output` |

No Rust global network state. No `rust_ipv4_route` symbol reference from this blob.

---

## 13. Blob / memory budget

| Item | Estimate | Hard rule |
|------|----------|-----------|
| Rust blob | **250–550 B** (header + checksum + 4 callbacks + error tails) | measure |
| FASM trampoline | **~120–180 B** | measure |
| Ctx | **56 B stack** | not `.data` |
| Smoke | stack synthetic buffers (CW/CV pattern); **no** new `.data` iglobals if avoidable |
| Relocs | **0** | stop if not |
| Pack | `TMP_STACK_TOP` `0x008E000`, `sys_proc` same, `SLOT_BASE` `0x0090000` | **do not move** |
| Slack | **2365 B** effective | blob+trampoline+align16 must fit with ~64 B cliff reserve |
| `kernel.mnt` | record before/after | |

If measured `.text` growth exceeds slack: shrink comments/debug, keep semantics, **do not** move the pack. Report blocker instead of packing into VGA/`SLOT_BASE`.

---

## 14. ABI smoke (`IPV4`)

In-kernel, gate ON, **no NIC required**. Marker `IPV4` / hang `0xDEAD0C78` (proposed; confirm unused at impl time).

Vectors (synthetic callbacks write a known 20+payload buffer):

1. UDP-like: AL=128, AH=17, dest/src like soak, payload 8 — header bytes vs oracle
2. TCP proto field only (AH=6), no TCP SM
3. ICMP proto (AH=1)
4. `ECX=65501` → EAX=0, ZF=1, counter unchanged
5. mocked no-route → EAX=0, no inc
6. mocked ARP fail → EAX=0, no inc
7. mocked eth ZF → EAX=0, **inc happened**
8. loopback `edi_route=0` — no ARP, inc on slot 0, header source quirk
9. Register canaries EBX/ESI/EBP around stdcall; DF=0; ECX restored
10. AL/AH decode: TTL≠protocol (catch banner inversion)

Do **not** replace the live QEMU soak with smoke.

---

## 15. Host oracle reuse

Reuse **only**:

- `rust_kernel/kolibri_utils/src/ipv4_output_oracle.rs` (seed `0x49505634`, 50k, RFC 1071)
- `scripts/net_capture.py`

Future differential:

| Left | Right |
|------|--------|
| Production FASM (OFF) | independent model |
| Production Rust (ON) | independent model |
| Live pcap | independent model + FASM baseline pcap |

Cases: existing 50k, length 65500/65501, checksum never-zero, live `IPV4SOAK` UDP (`0x22b7`), dest≠gateway, TOS/ID/flags stay 0.

Promote oracle helpers from `#[cfg(test)]` into the blob **only** as needed for `build_ipv4_header` / checksum — do not ship the 50k PRNG in the kernel.

---

## 16. QEMU OFF / ON / A-B / ON×3

Reuse `scripts/qmp_ipv4_output_soak.py`, `tools/ipv4_output_guest/ipv4soak.asm`, user-net + e1000 + `filter-dump`.

| Run | Gate | Must |
|-----|------|------|
| OFF | `USE_RUST_IPV4_OUTPUT=0` | FASM baseline; `IPV4SOAK0..4`; header oracle; RESET=0 |
| ON | `=1` | same packets / checksum / payload |
| A/B | OFF vs ON | exact guest IPv4 header bytes (ignore slirp ICMP) |
| ON×3 | `=1` | repeatability |

Oracle: firstapp markers, pcap parse, **not** desktop non-black. Guest MAC `52:54:00:12:34:56`. Exclude slirp ICMP port-unreachable from guest-origin scoring (already in harness).

---

## 17. Why ARP / Ethernet / routing stay FASM

| Component | Owner | Why CX must not take it |
|-----------|--------|-------------------------|
| Route tables | `IPv4_address/subnet/gateway` + Cut AC leaf | CX is a consumer; AC already owns the lookup |
| ARP cache | `ARP_table`, `arp_output_request`, `ARP_BLOCK` wait | cache + blocking wait + TX |
| `eth_output` | `net_buff_alloc`, MTU, MAC, min frame 60 | buffer allocator + L2 |
| `loop_output` | loopback device object | netdev |
| `NET_DEVICE.transmit` | drivers (I8254X) | after this leaf |
| Sockets | mutex, TTL, LocalIP | callers |

Rust boundary: IPv4 **header composition + checksum + call sequencing**. Not Path A network ownership.

---

## 18. Rollback

| | |
|--|--|
| Gate | `USE_RUST_IPV4_OUTPUT` only (add at implementation, default 0 until soak green, then 1) |
| OFF | original FASM body @ 680 (keep under `else`) |
| ON | trampoline → Rust blob |
| Must not affect | `ipv4_output_raw`, `tcp_output`, `udp_output`, `icmp_output_raw`, ARP, Ethernet, `USE_RUST_IPV4_ROUTE` |

One inventory increment **when the cut is complete**, not at plan time.

---

## 19. Regression risks

| Risk | Mitigation |
|------|------------|
| AL/AH inversion (banner) | Smoke vector TTL≠proto; callers already AL=TTL |
| Wrong `ECX` (header vs payload) | TotalLength = htons(20+payload); return ECX = original payload |
| Byte order of length/checksum/IPs | BE length/checksum; LE IP dwords; live `0x22b7` |
| ID/flags/options “improvements” | always 0 / IHL 5 |
| `ipv4_route` via `rust_ipv4_route` | forbidden; public FASM only |
| ARP `-1` vs `-2` | `test eax, 0xffff0000` |
| eth ZF missed | immediate jz / setz (REG-018) |
| Output reg corruption | ctx outs + canaries (REG-001 EDX class) |
| Counter before eth fail | increment anyway |
| Loopback `.continue` on OOM | do not add a check |
| Loopback `EDX` ≠ frame size | do not set it |
| `tcp_respond_segment` EBX passthrough | do not zero EBX |
| Stack / stdcall double-clean | REG-009/010 |
| DF | `cld` |
| Relocs | extractor 0 |
| Pack overflow | measure vs 2365 B; no pack move |
| Smoke `.data` iglobals | stack fixtures (REG-012) |

Do not “fix” legacy quirks.

---

## 20. Completion checklist (implementation turn)

Architecture:

- [ ] Public ABI still §2; boundary still only `ipv4_output`

Rust:

- [ ] Header bytes exact vs oracle
- [ ] RFC 1071 checksum exact
- [ ] Callbacks exact
- [ ] Errors A–E exact
- [ ] `IPv4_packets_tx` timing exact

Blob:

- [ ] 0 relocations
- [ ] exact size + SHA-256 recorded
- [ ] `end .bss` / `kernel.mnt` recorded; pack addresses unchanged

Oracle:

- [ ] 50k host cases still PASS
- [ ] live UDP `IPV4SOAK` header parity
- [ ] checksum `0x22b7` (or documented equivalent for same header)

ABI:

- [ ] `IPV4` smoke PASS (hang-on-fail)

QEMU:

- [ ] OFF PASS
- [ ] ON PASS
- [ ] A/B PASS
- [ ] ON×3 PASS
- [ ] RESET=0
- [ ] filter-dump guest UDP valid

Rollback:

- [ ] gate OFF restores FASM body

Documentation:

- [ ] `cut-cx-implementation.md`
- [ ] inventory **105 → 106 / 138** exactly once
- [ ] `[[rust.migrations]]` row + `USE_RUST_IPV4_OUTPUT = 1`
- [ ] final CoW image path

---

## 21. Implementation file map (when authorized)

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/ipv4_output.rs` | production leaf (new) |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_ipv4_output` stdcall export |
| `kernel/rust/ipv4_output.inc` | embed + smoke |
| `kernel/network/IPv4.inc` | gated trampoline; FASM `else` body kept |
| `project/build.toml` | one `[[rust.migrations]]` row |
| existing oracle / soak / guest | reuse |

Do **not** create Cut CY from this plan.

---

## 22. Explicit non-goals (this planning task)

- No edits to `ipv4_output` / `ipv4_output_raw` / ARP / Ethernet / routing
- No Rust production network blob
- No `USE_RUST_IPV4_OUTPUT`
- No inventory increment
- No PTE / scheduler / NTFS mount work
