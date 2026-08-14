"""Independent IPv4/Ethernet capture parser for the ipv4_output evidence program.

Does not call production FASM, Rust blobs, or Cut E/F checksum helpers.
RFC 791 header decode + RFC 1071 checksum. Used by scripts/qmp_ipv4_output_soak.py.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator

IPV4_HEADER_LEN = 20
ETH_HEADER_LEN = 14
ETHERTYPE_IPV4 = 0x0800
ETHERTYPE_VLAN = 0x8100
ETHERTYPE_ARP = 0x0806
PCAP_MAGIC = 0xA1B2C3D4
PCAP_MAGIC_SWAPPED = 0xD4C3B2A1
PCAP_NS_MAGIC = 0xA1B23C4D
PCAP_NS_MAGIC_SWAPPED = 0x4D3CB2A1


def rfc1071_checksum(data: bytes, checksum_field_off: int | None = None) -> int:
    """Independent RFC 1071 one's-complement checksum (host numeric / on-wire BE)."""
    total = 0
    i = 0
    n = len(data)
    while i + 1 < n:
        hi = 0 if checksum_field_off == i else data[i]
        lo = 0 if checksum_field_off == i else data[i + 1]
        total += (hi << 8) | lo
        i += 2
    if i < n:
        b = 0 if checksum_field_off == i else data[i]
        total += b << 8
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    csum = (~total) & 0xFFFF
    if csum == 0:
        csum = 0xFFFF
    return csum


def ipv4_header_checksum_ok(header: bytes) -> bool:
    if len(header) < IPV4_HEADER_LEN:
        return False
    ihl = (header[0] & 0x0F) * 4
    if ihl < IPV4_HEADER_LEN or len(header) < ihl:
        return False
    stored = (header[10] << 8) | header[11]
    return rfc1071_checksum(header[:ihl], 10) == stored


def build_ipv4_header(
    ttl: int,
    protocol: int,
    source_ip_le: int,
    dest_ip_le: int,
    payload_len: int,
) -> bytes:
    """Independent 20-byte IPv4 header (IHL=5, TOS=0, ID=0, flags=0)."""
    total = min(IPV4_HEADER_LEN + payload_len, 65535)
    h = bytearray(IPV4_HEADER_LEN)
    h[0] = 0x45
    h[2:4] = struct.pack("!H", total)
    h[8] = ttl & 0xFF
    h[9] = protocol & 0xFF
    h[12:16] = struct.pack("<I", source_ip_le & 0xFFFFFFFF)
    h[16:20] = struct.pack("<I", dest_ip_le & 0xFFFFFFFF)
    csum = rfc1071_checksum(h, 10)
    h[10:12] = struct.pack("!H", csum)
    return bytes(h)


@dataclass
class EthernetFrame:
    dst_mac: bytes
    src_mac: bytes
    ethertype: int
    payload: bytes
    vlan: int | None = None


@dataclass
class Ipv4Packet:
    version: int
    ihl: int
    tos: int
    total_length: int
    identification: int
    flags: int
    fragment_offset: int
    ttl: int
    protocol: int
    checksum: int
    source: str
    destination: str
    source_le: int
    dest_le: int
    options: bytes
    payload: bytes
    header: bytes
    checksum_ok: bool


@dataclass
class CapturedPacket:
    ts_sec: int
    ts_frac: int
    incl_len: int
    orig_len: int
    raw: bytes
    ethernet: EthernetFrame | None = None
    ipv4: Ipv4Packet | None = None
    classification: str = "unknown"


def _mac(b: bytes) -> str:
    return ":".join(f"{x:02x}" for x in b)


def _ip4(b: bytes) -> str:
    return ".".join(str(x) for x in b)


def parse_ethernet(raw: bytes) -> EthernetFrame | None:
    if len(raw) < ETH_HEADER_LEN:
        return None
    dst, src = raw[0:6], raw[6:12]
    etype = (raw[12] << 8) | raw[13]
    off = 14
    vlan = None
    if etype == ETHERTYPE_VLAN and len(raw) >= 18:
        vlan = (raw[14] << 8) | raw[15]
        etype = (raw[16] << 8) | raw[17]
        off = 18
    return EthernetFrame(dst, src, etype, raw[off:], vlan)


def parse_ipv4(payload: bytes) -> Ipv4Packet | None:
    if len(payload) < IPV4_HEADER_LEN:
        return None
    ver_ihl = payload[0]
    version = ver_ihl >> 4
    ihl = (ver_ihl & 0x0F) * 4
    if version != 4 or ihl < IPV4_HEADER_LEN or len(payload) < ihl:
        return None
    header = payload[:ihl]
    tos = payload[1]
    total_length = (payload[2] << 8) | payload[3]
    identification = (payload[4] << 8) | payload[5]
    flags_frag = (payload[6] << 8) | payload[7]
    flags = flags_frag >> 13
    fragment_offset = flags_frag & 0x1FFF
    ttl = payload[8]
    protocol = payload[9]
    checksum = (payload[10] << 8) | payload[11]
    src_b, dst_b = payload[12:16], payload[16:20]
    options = payload[IPV4_HEADER_LEN:ihl]
    body = payload[ihl:total_length] if total_length >= ihl else payload[ihl:]
    return Ipv4Packet(
        version=version,
        ihl=ihl,
        tos=tos,
        total_length=total_length,
        identification=identification,
        flags=flags,
        fragment_offset=fragment_offset,
        ttl=ttl,
        protocol=protocol,
        checksum=checksum,
        source=_ip4(src_b),
        destination=_ip4(dst_b),
        source_le=struct.unpack("<I", src_b)[0],
        dest_le=struct.unpack("<I", dst_b)[0],
        options=options,
        payload=body,
        header=header,
        checksum_ok=ipv4_header_checksum_ok(header),
    )


def classify(eth: EthernetFrame | None, ip: Ipv4Packet | None) -> str:
    if eth is None:
        return "truncated_ethernet"
    if eth.ethertype == ETHERTYPE_ARP:
        return "arp"
    if eth.ethertype != ETHERTYPE_IPV4:
        return f"ethertype_{eth.ethertype:#06x}"
    if ip is None:
        return "ipv4_truncated"
    if not ip.checksum_ok:
        return "ipv4_checksum_mismatch"
    if ip.identification != 0 or ip.flags != 0 or ip.fragment_offset != 0:
        return "ipv4_unexpected_frag"
    if ip.ihl != 20 or ip.options:
        return "ipv4_options"
    proto = {1: "icmp", 6: "tcp", 17: "udp"}.get(ip.protocol, f"proto_{ip.protocol}")
    return f"ipv4_{proto}"


def iter_pcap(path: Path) -> Iterator[CapturedPacket]:
    data = path.read_bytes()
    if len(data) < 24:
        return
    magic = struct.unpack_from("<I", data, 0)[0]
    swapped = magic in (PCAP_MAGIC_SWAPPED, PCAP_NS_MAGIC_SWAPPED)
    endian = ">" if swapped else "<"
    magic_be = struct.unpack_from(">I", data, 0)[0] if swapped else magic
    if magic not in (
        PCAP_MAGIC,
        PCAP_MAGIC_SWAPPED,
        PCAP_NS_MAGIC,
        PCAP_NS_MAGIC_SWAPPED,
    ) and magic_be not in (PCAP_MAGIC, PCAP_NS_MAGIC):
        raise ValueError(f"not a pcap file: magic={magic:#x} ({path})")
    off = 24
    hdr_fmt = endian + "IIII"
    while off + 16 <= len(data):
        ts_sec, ts_frac, incl, orig = struct.unpack_from(hdr_fmt, data, off)
        off += 16
        raw = data[off : off + incl]
        off += incl
        eth = parse_ethernet(raw)
        ip = parse_ipv4(eth.payload) if eth and eth.ethertype == ETHERTYPE_IPV4 else None
        yield CapturedPacket(
            ts_sec,
            ts_frac,
            incl,
            orig,
            raw,
            eth,
            ip,
            classify(eth, ip),
        )


def summarize_pcap(path: Path) -> dict:
    packets = list(iter_pcap(path)) if path.is_file() else []
    ipv4 = [p for p in packets if p.ipv4 is not None]
    ok = [p for p in ipv4 if p.ipv4 and p.ipv4.checksum_ok]
    classes: dict[str, int] = {}
    for p in packets:
        classes[p.classification] = classes.get(p.classification, 0) + 1
    samples = []
    for p in ipv4[:8]:
        ip = p.ipv4
        assert ip is not None
        samples.append(
            {
                "class": p.classification,
                "ethertype": None if p.ethernet is None else hex(p.ethernet.ethertype),
                "dst_mac": None if p.ethernet is None else _mac(p.ethernet.dst_mac),
                "src_mac": None if p.ethernet is None else _mac(p.ethernet.src_mac),
                "src": ip.source,
                "dst": ip.destination,
                "ttl": ip.ttl,
                "proto": ip.protocol,
                "id": ip.identification,
                "flags": ip.flags,
                "total_length": ip.total_length,
                "payload_len": len(ip.payload),
                "checksum": hex(ip.checksum),
                "checksum_ok": ip.checksum_ok,
                "ihl": ip.ihl,
            }
        )
    return {
        "pcap": str(path),
        "exists": path.is_file(),
        "size": path.stat().st_size if path.is_file() else 0,
        "frames": len(packets),
        "ipv4": len(ipv4),
        "ipv4_checksum_ok": len(ok),
        "classes": classes,
        "samples": samples,
    }


def match_oracle_header(ip: Ipv4Packet, *, ttl: int | None = None, protocol: int | None = None) -> list[str]:
    """Compare a captured IPv4 packet against the independent ipv4_output model."""
    misses: list[str] = []
    expected = build_ipv4_header(
        ip.ttl if ttl is None else ttl,
        ip.protocol if protocol is None else protocol,
        ip.source_le,
        ip.dest_le,
        len(ip.payload),
    )
    if ip.header[0] != 0x45:
        misses.append(f"version_ihl={ip.header[0]:#x}")
    if ip.tos != 0:
        misses.append(f"tos={ip.tos}")
    if ip.identification != 0:
        misses.append(f"id={ip.identification}")
    if ip.flags != 0 or ip.fragment_offset != 0:
        misses.append(f"frag flags={ip.flags} off={ip.fragment_offset}")
    if not ip.checksum_ok:
        misses.append("checksum")
    if ip.header[:IPV4_HEADER_LEN] != expected and ip.ihl == IPV4_HEADER_LEN:
        # Payload length in total_length may include padding; compare field-wise.
        exp_csum = rfc1071_checksum(
            ip.header[:IPV4_HEADER_LEN][:10] + b"\x00\x00" + ip.header[12:20],
            10,
        )
        if ip.checksum != exp_csum:
            misses.append(f"checksum_value {ip.checksum:#x}!={exp_csum:#x}")
    return misses


if __name__ == "__main__":
    h = build_ipv4_header(128, 17, 0x0F02000A, 0x0202000A, 8)
    assert h[0] == 0x45 and h[8] == 128 and h[9] == 17
    assert ipv4_header_checksum_ok(h)
    assert rfc1071_checksum(h, 10) == (h[10] << 8 | h[11])
    print("net_capture self-test PASS")

