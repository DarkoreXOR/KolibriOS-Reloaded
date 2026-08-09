# Cut AC Plan

**Date:** 2026-08-10  
**Status:** complete — see [`cut-ac-implementation.md`](cut-ac-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut AC** migrates the IPv4 on-link / gateway / broadcast
> router — `ipv4_route` in `IPv4.inc`.  
> Cuts A–AB remain complete and must not be redone. Do not start Cut AD.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `ipv4_route` |
| **Source** | [`kernel/network/IPv4.inc:943–1067`](../../kernel/network/IPv4.inc) |
| **Subsystem** | Network / IPv4 routing |
| **Purpose** | Select egress device + rewrite dest to gateway when off-link; fill source IP |

---

## Post-AB candidate audit (live tree)

### Coverage already proven (A–AB)

| Class | Cuts |
|-------|------|
| Scalar / CRC / Unicode encode+decode / casefold / string | A–D |
| Net checksum / TCP RTT + persist timer | E, F, M, V |
| Calendar BDFE↔secs (pair) | G, T |
| Video RECT clip + CF | H |
| NTFS MCB/USA / FAT 8.3 next+gen / XFS BE unpack + hash search | I–K, R, U, W |
| HID mouse accel | L |
| Font AA (quirky EBP) | N |
| Process MENUET header | O |
| ZF-out userspace region gate | P |
| EDI-advancing UTF-16→UTF-8 + SF | Q |
| Omit-FP stdcall + EBP-as-object + MOVBE/SHRD | R |
| GUI screen-fit + EDI→WDATA + display globals | S |
| UTF-8→FAT 8.3 SM + pushad/popad | U |
| CPU/TSS I/O-bitmap BTR/BTS | X |
| PE/COFF section walk + DIR32/REL32 buffer patch | Y |
| MBR/EBR partition validate + CF + 64-bit capacity | Z |
| Process-table TID walk + signed `jle` | AA |
| ESI-advancing UTF-8→UTF-16 streaming decode | AB |

### Deferred re-audit (live callers)

| Candidate | Callers | Verdict | Why |
|-----------|--------:|---------|-----|
| `memmove` | 24 | **defer Stage-4** | Forward-only `rep movsd`/`movsb`; EAX/EBX/ECX preserve; not C `memmove`; cross-subsystem fanout |
| `get_pg_addr` | 15 | **defer** | Stage-4 VA→PA; `page_tabs` / `OS_BASE` coupling |
| `v86_get_lin_addr` | 15 | **defer** | Stage-4 PTE walk |
| `net_ptr_to_num4` | 12 | **defer** | Thin device-list scan; packet-hot; inlined into AC device path only |
| `is_protective_mbr` | 1 | **defer (#2)** | GPT protective-MBR ZF validate; pairs Z without cloning — strong alt |
| `ntfs_test_bootsec` | 2 | **defer (#3)** | Strong FS bootsec+CF; mild validate-shape overlap with Z |
| `socket_check` | 5 | **defer** | Socket-list membership; useful but less Stage-5 leverage than routing |
| `uni2ansi_char` | 11 | **reject AC** | Unicode clustering post-AB |
| `irq_eoi` / `enable_irq` | 4 / 6 | **defer** | HW PIC/APIC; weak synthetic oracle |
| `mutex_init` | ~30 | **reject** | Trivial 3-store |

### `memmove` special evaluation

| Property | Finding |
|----------|---------|
| Implementation | Forward-only `rep movsd`/`movsb` — **not** bidirectional C `memmove` |
| Overlap | Correct for left-shift (`src = dest+N`); wrong for dest>src |
| ECX | Byte count; signed `test`/`jle` early-out |
| ESI/EDI | Scratch; restored |
| EAX/EBX | Preserved (push/pop of ECX/ESI/EDI only around body) |
| Stack ABI | `call`/`ret`; no stack args |
| Callers | 24 across kernel/FS/GUI/HID |
| Cut AC | **DEFER Stage-4** — preferred memory class, wrong blast radius; do not “fix” overlap |

### `ipv4_route` re-audit

| Property | Finding |
|----------|---------|
| Live callers | **4** — `ipv4_output`, `ipv4_output_raw`, `udp.inc` connect, `tcp_usreq.inc` connect |
| Hotness | Per outbound IPv4 send + UDP/TCP connect route resolve |
| ABI | EAX dest / EBX device-or-0 / EDX source → EAX dest-or-0, EDX source, EDI idx×4; EBX/ECX destroyed |
| Flags | Unspecified (clobbered) |
| Globals | `IPv4_address`, `IPv4_subnet`, `IPv4_gateway`, `net_device_list` (+ `NET_DEVICE.link_state`) |
| Mutation | Read-only tables; no structure write |
| Reloc risk | Absolute globals → trampoline-injected bases (Cut AA pattern) |
| Diff / smoke | Excellent synthetic tables; easy multi-reg outs |
| QEMU | Desktop + e1000 connect path can exercise; claim soak only with evidence |
| Note | `ipv4_output_raw` passes socket ptr in EAX without loading RemoteIP — live quirk; migrate documented ABI |

### Ranked top three

| Candidate | Subsystem | Callers | New class | Diff | ABI smoke | QEMU | Blast | Verdict |
|-----------|-----------|--------:|-----------|------|-----------|------|-------|---------|
| `ipv4_route` | IPv4 | 4 | **On-link/GW/bcast routing** | Excellent | Easy multi-out | Net | Low–med | **SELECT** |
| `is_protective_mbr` | Disk/GPT | 1 | GPT protective-MBR ZF | Excellent | Easy ZF | Disk scan | Low | Defer (#2) |
| `ntfs_test_bootsec` | NTFS mount | 2 | FS bootsec validate+CF | Excellent | Easy CF | Mount | Low | Defer (#3) |

```text
Selected target:
    ipv4_route

Source:
    kernel/network/IPv4.inc

Subsystem:
    Network / IPv4 routing

Why selected:
    First IPv4 routing-class cut (Stage-5 foothold); not checksum/timer clustering;
    4 live callers; deterministic table walk; excellent synthetic differential;
    reloc-free via trampoline-injected IPv4_* + net_device_list bases;
    expands verified envelope into packet egress path selection.

Candidate #2 and rejection reason:
    is_protective_mbr — strong GPT ZF companion to Cut Z, but AC prefers a new
    network routing class over another disk validate leaf.

Candidate #3 and rejection reason:
    ntfs_test_bootsec — excellent contained CF bootsec leaf; deferred to avoid
    validate-shape clustering after Z and to take the Stage-5 routing foothold.

Legacy ABI:
    call / ret
    in:  EAX = dest IP; EBX = device ptr or 0; EDX = source IP
    out: EAX = dest IP (may become gateway) or 0 on fail
         EDX = source IP (filled from IPv4_address[edi] when auto-route and EDX was 0;
               overwritten from IPv4_address[edi] on explicit-device path)
         EDI = device number × 4 (or -1 on device-ptr fail)
    destroyed: EBX, ECX
    preserves: ESI, EBP (untouched)
    flags: unspecified
    quirks: inlines net_ptr_to_num4 on EBX≠0; broadcast device pick by link_state;
            on-link requires (addr & subnet) ≠ 0; gateway scan skips loopback (edi=4..)

Critical invariants:
    Exact FASM branch order (broadcast / on-link / gateway / loopback / device)
    EAX=0 fail vs on-link 0.0.0.0 indistinguishable (legacy)
    Do not mutate IPv4_* or net_device_list
    DEBUGF is VERBOSE=0 / ERROR-only on fail — not part of functional ABI

Rust strategy:
    Freestanding ipv4_route(dest, device, source, table bases) → result triple
    Inline net_ptr_to_num4 device-list scan (no separate cut)
    No allocator / no panic / reloc-free section

Trampoline strategy:
    stdcall rust_ipv4_route with injected IPv4_address/subnet/gateway +
    net_device_list and out-pointers for EDX/EDI; preserve ESI/EBP

Differential strategy:
    Independent FASM-flow oracle over synthetic 16-slot tables + device stubs
    Named vectors for every major branch + PRNG corpus

ABI smoke strategy:
    In-kernel synthetic tables + public trampoline canaries (ESI/EBP preserve;
    EAX/EDX/EDI outs; fail path edi=-1)

QEMU strategy:
    OFF/ON from cut-ab-final.img; general desktop regression;
    network soak only if ipv4_route is evidenced on path

Production gate:
    USE_RUST_IPV4_ROUTE (dev 0 → prod 1 after gates)

Rollback gate:
    USE_RUST_IPV4_ROUTE=0
```

---

## Out of scope

* Migrating `memmove` / `get_pg_addr` / `net_ptr_to_num4` / `is_protective_mbr` /
  `ntfs_test_bootsec` / `socket_check`
* Beginning Cut AD
* Changing forward-only `memmove` overlap semantics
* “Fixing” `ipv4_output_raw` caller quirk
