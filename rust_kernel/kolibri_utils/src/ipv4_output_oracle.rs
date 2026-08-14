//! Stage-4 host-only IPv4 output packet oracle (research — not a production cut).
//!
//! Independent RFC 791 / RFC 1071 model of the packet `ipv4_output` *emits*
//! after route + ARP + `eth_output` succeed. This is **not**:
//! * a translation of `kernel/network/IPv4.inc` stack dance
//! * a call into FASM `ipv4_output`
//! * a call into Cut E/F `checksum_1` / `checksum_2`
//! * a production `USE_RUST_*` blob
//!
//! Observed FASM *contract* (from callers + body, not the inverted comment):
//! * `AL` = TTL, `AH` = protocol (every live caller; the `IPv4.inc` banner is wrong)
//! * payload length `ECX`; error if `ECX > 65500`
//! * IHL always 5, TOS 0, Identification 0, flags/frag 0 (no options, no DF)
//! * header checksum over the 20-byte header with checksum field 0
//! * payload is **not** copied by `ipv4_output` (callers write after return)
//!
//! Seed: `'IPV4'` (`0x4950_5634`). See
//! `docs/migration/stage4-ipv4-output-oracle.md`.

#![cfg(test)]

/// PRNG seed for Stage-4 IPv4 output differential (`'IPV4'`).
pub const IPV4_OUTPUT_ORACLE_PRNG_SEED: u32 = 0x4950_5634;

/// FASM `IPv4.inc` `cmp ecx, 65500` / `ja .too_large`.
pub const IPV4_OUTPUT_MAX_PAYLOAD: u32 = 65500;

/// `sizeof.IPv4_header`.
pub const IPV4_HEADER_LEN: usize = 20;

/// Ethernet header length (dst 6 + src 6 + type 2).
pub const ETH_HEADER_LEN: usize = 14;

/// `ETHER_PROTO_IPv4` on the wire (`0x0800`).
pub const ETHERTYPE_IPV4: u16 = 0x0800;

/// Default `IP_SOCKET.ttl` in `socket.inc`.
pub const DEFAULT_TTL: u8 = 128;

pub const IP_PROTO_ICMP: u8 = 1;
pub const IP_PROTO_TCP: u8 = 6;
pub const IP_PROTO_UDP: u8 = 17;

/// Version 4, IHL 5 (no options) as stored in `VersionAndIHL`.
pub const VERSION_IHL_V4_20: u8 = 0x45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ipv4OutputError {
    TooLarge,
    NoRoute,
    ArpError,
    EthError,
}

/// Inputs that `ipv4_output` consumes (register ABI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ipv4OutputRequest {
    /// `AL` — TTL.
    pub ttl: u8,
    /// `AH` — IP protocol number.
    pub protocol: u8,
    /// `EBX` — device pointer, or 0 for auto-route. Oracle treats as opaque.
    pub device_ptr: u32,
    /// `ECX` — payload length (not including IPv4 header).
    pub payload_len: u32,
    /// `EDX` — requested source IP (0 ⇒ route fills it).
    pub source_ip: u32,
    /// `EDI` — destination IP.
    pub dest_ip: u32,
    /// Optional payload bytes (callers copy after `ipv4_output`; used by capture).
    pub payload: Vec<u8>,
}

/// Injected dependency results (route / ARP / ethernet are outside the leaf).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ipv4OutputDeps {
    pub route_ok: bool,
    /// Route-selected source (FASM `EDX` after `ipv4_route`).
    pub routed_source_ip: u32,
    /// Route-selected dest (may be gateway). Unused in the IPv4 header dest
    /// field — FASM pops the *original* dest into `DestinationAddress`.
    pub routed_next_hop: u32,
    pub arp_ok: bool,
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub eth_ok: bool,
}

/// Captured Ethernet + IPv4 bytes from a test-only output shim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedFrame {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
    pub ipv4: Vec<u8>,
    pub payload: Vec<u8>,
}

impl CapturedFrame {
    pub fn wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ETH_HEADER_LEN + self.ipv4.len());
        out.extend_from_slice(&self.dst_mac);
        out.extend_from_slice(&self.src_mac);
        out.extend_from_slice(&self.ethertype.to_be_bytes());
        out.extend_from_slice(&self.ipv4);
        out
    }
}

/// Independent RFC 1071 one's-complement checksum over `data`.
///
/// `checksum_field_off` bytes are treated as zero (IPv4 header checksum at 10).
/// Result is the 16-bit value in **host** numeric form (same as on-wire BE).
/// A computed 0 is represented as `0xFFFF` (RFC 1071 transmitted checksum).
pub fn rfc1071_checksum(data: &[u8], checksum_field_off: Option<usize>) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let hi = if Some(i) == checksum_field_off {
            0
        } else {
            data[i]
        };
        let lo = if Some(i) == checksum_field_off {
            0
        } else {
            data[i + 1]
        };
        sum += u32::from(u16::from_be_bytes([hi, lo]));
        i += 2;
    }
    if i < data.len() {
        let b = if Some(i) == checksum_field_off {
            0
        } else {
            data[i]
        };
        sum += u32::from(b) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let mut csum = (!sum) as u16;
    if csum == 0 {
        csum = 0xffff;
    }
    csum
}

/// True when the header's checksum field is consistent with RFC 1071.
pub fn ipv4_header_checksum_ok(header: &[u8]) -> bool {
    if header.len() < IPV4_HEADER_LEN {
        return false;
    }
    let ihl = (header[0] & 0x0f) as usize * 4;
    if ihl < IPV4_HEADER_LEN || header.len() < ihl {
        return false;
    }
    rfc1071_checksum(&header[..ihl], Some(10)) == u16::from_be_bytes([header[10], header[11]])
}

/// Independent RFC 791 IPv4 header (20 bytes, no options) matching the
/// constants `ipv4_output` writes — not a FASM instruction transcription.
pub fn build_ipv4_header(
    ttl: u8,
    protocol: u8,
    source_ip: u32,
    dest_ip: u32,
    payload_len: u32,
) -> [u8; IPV4_HEADER_LEN] {
    let total = (IPV4_HEADER_LEN as u32).saturating_add(payload_len).min(u16::MAX as u32) as u16;
    let mut h = [0u8; IPV4_HEADER_LEN];
    h[0] = VERSION_IHL_V4_20;
    h[1] = 0; // TOS
    h[2..4].copy_from_slice(&total.to_be_bytes());
    h[4..6].copy_from_slice(&0u16.to_be_bytes()); // Identification (FASM FIXME: always 0)
    h[6..8].copy_from_slice(&0u16.to_be_bytes()); // flags + frag offset
    h[8] = ttl;
    h[9] = protocol;
    h[10] = 0;
    h[11] = 0;
    h[12..16].copy_from_slice(&source_ip.to_le_bytes()); // Kolibri stores IPs as on-wire bytes
    h[16..20].copy_from_slice(&dest_ip.to_le_bytes());
    // Kolibri dwords: `127 + 1 shl 24` → memory 7F 00 00 01. `to_le_bytes` of
    // that dword is exactly those on-wire bytes.
    let csum = rfc1071_checksum(&h, Some(10));
    h[10..12].copy_from_slice(&csum.to_be_bytes());
    h
}

/// Leaf contract: header construction after mocked route/ARP/eth succeed.
pub fn ipv4_output_contract(
    req: &Ipv4OutputRequest,
    deps: Ipv4OutputDeps,
) -> Result<CapturedFrame, Ipv4OutputError> {
    if req.payload_len > IPV4_OUTPUT_MAX_PAYLOAD {
        return Err(Ipv4OutputError::TooLarge);
    }
    if !deps.route_ok {
        return Err(Ipv4OutputError::NoRoute);
    }
    if !deps.arp_ok {
        return Err(Ipv4OutputError::ArpError);
    }
    if !deps.eth_ok {
        return Err(Ipv4OutputError::EthError);
    }
    let mut payload = req.payload.clone();
    payload.resize(req.payload_len as usize, 0);
    let ipv4 = build_ipv4_header(
        req.ttl,
        req.protocol,
        deps.routed_source_ip,
        req.dest_ip, // original dest, not necessarily next-hop
        req.payload_len,
    );
    let mut ipv4_and_payload = ipv4.to_vec();
    ipv4_and_payload.extend_from_slice(&payload);
    Ok(CapturedFrame {
        dst_mac: deps.dst_mac,
        src_mac: deps.src_mac,
        ethertype: ETHERTYPE_IPV4,
        ipv4: ipv4_and_payload,
        payload,
    })
}

/// Test-only Ethernet output shim: copies the independent frame into a buffer.
pub fn capture_shim(frame: &CapturedFrame, buf: &mut Vec<u8>) -> usize {
    *buf = frame.wire_bytes();
    buf.len()
}

fn xor32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_deps(src: u32) -> Ipv4OutputDeps {
        Ipv4OutputDeps {
            route_ok: true,
            routed_source_ip: src,
            routed_next_hop: 0,
            arp_ok: true,
            dst_mac: [0x02, 0, 0, 0, 0, 0x02],
            src_mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            eth_ok: true,
        }
    }

    fn req(proto: u8, payload: &[u8], src: u32, dst: u32, ttl: u8) -> Ipv4OutputRequest {
        Ipv4OutputRequest {
            ttl,
            protocol: proto,
            device_ptr: 0,
            payload_len: payload.len() as u32,
            source_ip: src,
            dest_ip: dst,
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn rfc1071_zero_payload_udp_like() {
        let h = build_ipv4_header(64, IP_PROTO_UDP, 0x0a00_020f, 0x0a00_0202, 8);
        assert_eq!(h[0], 0x45);
        assert_eq!(h[9], IP_PROTO_UDP);
        assert!(ipv4_header_checksum_ok(&h));
        assert_eq!(u16::from_be_bytes([h[2], h[3]]), 28);
        assert_eq!(&h[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn checksum_known_edge_all_zero_addrs() {
        let h = build_ipv4_header(1, 0, 0, 0, 0);
        assert!(ipv4_header_checksum_ok(&h));
        let c = u16::from_be_bytes([h[10], h[11]]);
        assert_ne!(c, 0);
    }

    #[test]
    fn checksum_never_stores_zero() {
        // Sweep TTLs; stored checksum must be non-zero (0x0000 → 0xFFFF).
        for ttl in 0..=255u8 {
            let h = build_ipv4_header(ttl, IP_PROTO_ICMP, 0, 0, 0);
            assert_ne!([h[10], h[11]], [0, 0], "ttl={ttl}");
            assert!(ipv4_header_checksum_ok(&h));
        }
    }

    #[test]
    fn vector_udp_like_payload() {
        let payload = b"IPV4SOAK";
        let src = 0x0f02_000a; // 10.0.2.15 as Kolibri dword
        let dst = 0x0202_000a; // 10.0.2.2
        let frame = ipv4_output_contract(&req(IP_PROTO_UDP, payload, src, dst, DEFAULT_TTL), ok_deps(src))
            .unwrap();
        assert_eq!(frame.ethertype, ETHERTYPE_IPV4);
        assert_eq!(frame.ipv4[9], IP_PROTO_UDP);
        assert_eq!(&frame.payload, payload);
        assert!(ipv4_header_checksum_ok(&frame.ipv4[..IPV4_HEADER_LEN]));
        assert_eq!(frame.ipv4.len(), IPV4_HEADER_LEN + payload.len());
    }

    #[test]
    fn vector_tcp_protocol_field_only() {
        let frame = ipv4_output_contract(
            &req(IP_PROTO_TCP, b"ABCD", 0x0100007f, 0x0200007f, 64),
            ok_deps(0x0100007f),
        )
        .unwrap();
        assert_eq!(frame.ipv4[9], IP_PROTO_TCP);
        assert!(ipv4_header_checksum_ok(&frame.ipv4[..IPV4_HEADER_LEN]));
    }

    #[test]
    fn vector_icmp_protocol_field() {
        let frame = ipv4_output_contract(
            &req(IP_PROTO_ICMP, &[8, 0, 0, 0], 0x0100007f, 0x0100007f, 128),
            ok_deps(0x0100007f),
        )
        .unwrap();
        assert_eq!(frame.ipv4[9], IP_PROTO_ICMP);
    }

    #[test]
    fn vector_minimum_and_larger_payload() {
        let src = 0x0a000001u32;
        let min = ipv4_output_contract(&req(IP_PROTO_UDP, &[], src, 1, 1), ok_deps(src)).unwrap();
        assert_eq!(u16::from_be_bytes([min.ipv4[2], min.ipv4[3]]), 20);
        let big = vec![0x5au8; 1400];
        let large = ipv4_output_contract(&req(IP_PROTO_UDP, &big, src, 2, 64), ok_deps(src)).unwrap();
        assert_eq!(large.payload.len(), 1400);
        assert_eq!(
            u16::from_be_bytes([large.ipv4[2], large.ipv4[3]]),
            1420
        );
    }

    #[test]
    fn vector_ttl_and_id_variations() {
        for ttl in [1u8, 64, 128, 255] {
            let h = build_ipv4_header(ttl, 17, 1, 2, 4);
            assert_eq!(h[8], ttl);
            assert_eq!(&h[4..6], &[0, 0]); // identification always 0
            assert_eq!(&h[6..8], &[0, 0]); // no DF/MF (unsupported by leaf)
        }
    }

    #[test]
    fn too_large_and_dep_failures() {
        let src = 1u32;
        let mut r = req(IP_PROTO_UDP, &[], src, 2, 64);
        r.payload_len = 65501;
        assert_eq!(
            ipv4_output_contract(&r, ok_deps(src)).unwrap_err(),
            Ipv4OutputError::TooLarge
        );
        r.payload_len = 8;
        r.payload = vec![0; 8];
        let mut d = ok_deps(src);
        d.route_ok = false;
        assert_eq!(
            ipv4_output_contract(&r, d).unwrap_err(),
            Ipv4OutputError::NoRoute
        );
        d = ok_deps(src);
        d.arp_ok = false;
        assert_eq!(
            ipv4_output_contract(&r, d).unwrap_err(),
            Ipv4OutputError::ArpError
        );
        d = ok_deps(src);
        d.eth_ok = false;
        assert_eq!(
            ipv4_output_contract(&r, d).unwrap_err(),
            Ipv4OutputError::EthError
        );
    }

    #[test]
    fn capture_shim_matches_independent_model() {
        let src = 0x0f02_000a;
        let frame = ipv4_output_contract(
            &req(IP_PROTO_UDP, b"PING", src, 0x0202_000a, 128),
            ok_deps(src),
        )
        .unwrap();
        let mut buf = Vec::new();
        let n = capture_shim(&frame, &mut buf);
        assert_eq!(n, 14 + 20 + 4);
        assert_eq!(buf, frame.wire_bytes());
        assert_eq!(&buf[12..14], &ETHERTYPE_IPV4.to_be_bytes());
        assert!(ipv4_header_checksum_ok(&buf[14..34]));
    }

    #[test]
    fn dest_header_uses_original_dest_not_gateway() {
        // FASM pops the original EDI dest, even when ipv4_route rewrote next-hop.
        let mut d = ok_deps(0x0a000001);
        d.routed_next_hop = 0x0a0000fe;
        let frame = ipv4_output_contract(
            &req(IP_PROTO_UDP, b"x", 0x0a000001, 0x08080808, 64),
            d,
        )
        .unwrap();
        assert_eq!(&frame.ipv4[16..20], &0x08080808u32.to_le_bytes());
        assert_eq!(&frame.ipv4[12..16], &0x0a000001u32.to_le_bytes());
    }

    #[test]
    fn prng_50k_header_checksum_and_contract() {
        let mut state = IPV4_OUTPUT_ORACLE_PRNG_SEED;
        let mut next = || xor32(&mut state);
        for i in 0..50_000u32 {
            let ttl = (next() & 0xff) as u8;
            let proto = match next() % 5 {
                0 => IP_PROTO_ICMP,
                1 => IP_PROTO_TCP,
                2 => IP_PROTO_UDP,
                3 => 89, // OSPF-like unused proto field
                _ => (next() & 0xff) as u8,
            };
            let payload_len = next() % 64;
            let src = next();
            let dst = next() | 1; // avoid all-zero dest as a distinct case
            if next() % 17 == 0 {
                let mut r = req(proto, &[], src, dst, ttl);
                r.payload_len = IPV4_OUTPUT_MAX_PAYLOAD + 1 + (next() % 8);
                assert_eq!(
                    ipv4_output_contract(&r, ok_deps(src)).unwrap_err(),
                    Ipv4OutputError::TooLarge
                );
                continue;
            }
            let payload: Vec<u8> = (0..payload_len).map(|_| (next() & 0xff) as u8).collect();
            let mut deps = ok_deps(src);
            match next() % 20 {
                0 => deps.route_ok = false,
                1 => deps.arp_ok = false,
                2 => deps.eth_ok = false,
                _ => {}
            }
            let r = req(proto, &payload, src, dst, ttl);
            match ipv4_output_contract(&r, deps) {
                Err(Ipv4OutputError::NoRoute) => assert!(!deps.route_ok),
                Err(Ipv4OutputError::ArpError) => assert!(!deps.arp_ok),
                Err(Ipv4OutputError::EthError) => assert!(!deps.eth_ok),
                Err(other) => panic!("case {i}: unexpected {other:?}"),
                Ok(frame) => {
                    assert!(deps.route_ok && deps.arp_ok && deps.eth_ok);
                    assert!(ipv4_header_checksum_ok(&frame.ipv4[..IPV4_HEADER_LEN]));
                    assert_eq!(frame.ipv4[8], ttl);
                    assert_eq!(frame.ipv4[9], proto);
                    let mut buf = Vec::new();
                    capture_shim(&frame, &mut buf);
                    assert_eq!(buf, frame.wire_bytes());
                    assert_eq!(
                        u16::from_be_bytes([frame.ipv4[2], frame.ipv4[3]]) as usize,
                        IPV4_HEADER_LEN + payload.len()
                    );
                }
            }
        }
    }
}
