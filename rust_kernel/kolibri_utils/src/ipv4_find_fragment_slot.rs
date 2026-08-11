//! Cut AU: `ipv4_find_fragment_slot` — IPv4 reassembly fragment-slot lookup.
//!
//! Matches `kernel/network/IPv4.inc` FASM leaf semantics:
//! * Read `Identification` (u16), `SourceAddress`, `DestinationAddress` from
//!   the packet header pointed at by the caller
//! * Linear walk of `IPv4_fragments[0..count)` (stride 16)
//! * First slot whose `id`/`SrcIP`/`DstIP` all match → return slot address
//! * Exhausted walk → `0xFFFF_FFFF` (`-1`)
//!
//! Legacy quirk retained: empty slots (typically all-zero after init) match a
//! packet with Identification=0 and Src/Dst = 0.0.0.0 — FASM does not check TTL.
//!
//! `IPv4_fragments` base and count are passed explicitly so the Rust blob stays
//! reloc-free. No locks / `.rodata` / external calls.

/// `sizeof.IPv4_FRAGMENT_slot` (ttl/id/SrcIP/DstIP/ptr).
pub const FRAGMENT_SLOT_SIZE: usize = 16;

/// Production table length (`IPv4_MAX_FRAGMENTS`).
pub const IPV4_MAX_FRAGMENTS: u32 = 64;

/// Offsets within `IPv4_FRAGMENT_slot`.
pub const OFF_SLOT_TTL: usize = 0;
pub const OFF_SLOT_ID: usize = 2;
pub const OFF_SLOT_SRC_IP: usize = 4;
pub const OFF_SLOT_DST_IP: usize = 8;
pub const OFF_SLOT_PTR: usize = 12;

/// Offsets within `IPv4_header`.
pub const OFF_HDR_IDENTIFICATION: usize = 4;
pub const OFF_HDR_SRC_IP: usize = 12;
pub const OFF_HDR_DST_IP: usize = 16;

/// Minimal readable IPv4 header span for this leaf (through DestinationAddress).
pub const IPV4_HEADER_MIN_LEN: usize = 20;

/// Cut AU differential PRNG seed (`'CUTU'`).
pub const IPV4_FIND_FRAGMENT_SLOT_PRNG_SEED: u32 = 0x4355_5455;

/// FASM-faithful fragment-slot scan.
///
/// Returns the absolute address of the first matching slot, or `u32::MAX` (`-1`).
///
/// `load_slot(index)` returns `(id, src_ip, dst_ip)` for slot `index`.
#[inline(always)]
pub fn ipv4_find_fragment_slot_from_keys(
    id: u16,
    src_ip: u32,
    dst_ip: u32,
    count: u32,
    mut load_slot: impl FnMut(u32) -> (u16, u32, u32),
    slot_addr: impl Fn(u32) -> u32,
) -> u32 {
    // mov ecx, count / mov esi, base / … loop
    let mut i = 0u32;
    while i < count {
        let (slot_id, slot_src, slot_dst) = load_slot(i);
        // cmp [esi+id], ax / jne .try_next
        // cmp [esi+SrcIP], ebx / jne .try_next
        // cmp [esi+DstIP], edx / je .found_slot
        if slot_id == id && slot_src == src_ip && slot_dst == dst_ip {
            return slot_addr(i);
        }
        i = i.wrapping_add(1);
    }
    // or esi, -1
    u32::MAX
}

/// Pointer-form walk used by the freestanding FFI.
///
/// # Safety
/// `packet` must be readable for at least [`IPV4_HEADER_MIN_LEN`] bytes;
/// `fragments` must address a readable array of `count` × 16-byte slots.
#[inline(always)]
pub unsafe fn ipv4_find_fragment_slot(
    packet: *const u8,
    fragments: *const u8,
    count: u32,
) -> u32 {
    let id = unsafe { read_u16_le(packet.add(OFF_HDR_IDENTIFICATION)) };
    let src_ip = unsafe { read_u32_le(packet.add(OFF_HDR_SRC_IP)) };
    let dst_ip = unsafe { read_u32_le(packet.add(OFF_HDR_DST_IP)) };
    let base = fragments as u32;
    ipv4_find_fragment_slot_from_keys(
        id,
        src_ip,
        dst_ip,
        count,
        |i| {
            let slot = unsafe { fragments.add((i as usize) * FRAGMENT_SLOT_SIZE) };
            (
                unsafe { read_u16_le(slot.add(OFF_SLOT_ID)) },
                unsafe { read_u32_le(slot.add(OFF_SLOT_SRC_IP)) },
                unsafe { read_u32_le(slot.add(OFF_SLOT_DST_IP)) },
            )
        },
        |i| base.wrapping_add(i.wrapping_mul(FRAGMENT_SLOT_SIZE as u32)),
    )
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`ipv4_find_fragment_slot`].
#[inline(always)]
pub unsafe fn ipv4_find_fragment_slot_ptr(
    packet: *const u8,
    fragments: *const u8,
    count: u32,
) -> u32 {
    unsafe { ipv4_find_fragment_slot(packet, fragments, count) }
}

#[inline(always)]
unsafe fn read_u16_le(p: *const u8) -> u16 {
    let b = unsafe { core::slice::from_raw_parts(p, 2) };
    u16::from_le_bytes([b[0], b[1]])
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not derived from Rust walk helpers).
    fn oracle(
        id: u16,
        src_ip: u32,
        dst_ip: u32,
        slots: &[(u16, u32, u32)],
        base: u32,
    ) -> u32 {
        let mut esi = base;
        let mut ecx = slots.len() as u32;
        let mut idx = 0usize;
        // FASM uses `loop` with initial ECX = count; zero count never enters.
        if ecx == 0 {
            return u32::MAX;
        }
        loop {
            let (sid, ssrc, sdst) = slots[idx];
            if sid == id && ssrc == src_ip && sdst == dst_ip {
                return esi;
            }
            esi = esi.wrapping_add(FRAGMENT_SLOT_SIZE as u32);
            idx += 1;
            ecx -= 1;
            if ecx == 0 {
                break;
            }
        }
        u32::MAX
    }

    fn run_vs_oracle(id: u16, src: u32, dst: u32, slots: &[(u16, u32, u32)], base: u32) {
        let got = ipv4_find_fragment_slot_from_keys(
            id,
            src,
            dst,
            slots.len() as u32,
            |i| slots[i as usize],
            |i| base.wrapping_add(i.wrapping_mul(FRAGMENT_SLOT_SIZE as u32)),
        );
        let exp = oracle(id, src, dst, slots, base);
        assert_eq!(
            got, exp,
            "mismatch id={id:#x} src={src:#x} dst={dst:#x} got={got:#x} exp={exp:#x}"
        );
    }

    #[test]
    fn empty_table_miss() {
        let slots: [(u16, u32, u32); 0] = [];
        run_vs_oracle(1, 2, 3, &slots, 0x1000);
        assert_eq!(
            ipv4_find_fragment_slot_from_keys(1, 2, 3, 0, |_| (0, 0, 0), |_| 0),
            u32::MAX
        );
    }

    #[test]
    fn miss_populated() {
        let slots = [(0x1111, 0x0a00_0001, 0x0a00_0002), (0x2222, 1, 2)];
        run_vs_oracle(0x3333, 1, 2, &slots, 0x2000);
    }

    #[test]
    fn hit_first_middle_last() {
        let slots = [
            (0x10, 0xA, 0xB),
            (0x20, 0xC, 0xD),
            (0x30, 0xE, 0xF),
        ];
        let base = 0x3000;
        run_vs_oracle(0x10, 0xA, 0xB, &slots, base);
        run_vs_oracle(0x20, 0xC, 0xD, &slots, base);
        run_vs_oracle(0x30, 0xE, 0xF, &slots, base);
        assert_eq!(
            ipv4_find_fragment_slot_from_keys(
                0x20,
                0xC,
                0xD,
                3,
                |i| slots[i as usize],
                |i| base + i * 16
            ),
            base + 16
        );
    }

    #[test]
    fn first_match_wins_on_duplicates() {
        let slots = [
            (0x55, 1, 2),
            (0x55, 1, 2),
            (0x55, 1, 2),
        ];
        run_vs_oracle(0x55, 1, 2, &slots, 0x4000);
        assert_eq!(
            ipv4_find_fragment_slot_from_keys(
                0x55,
                1,
                2,
                3,
                |i| slots[i as usize],
                |i| 0x4000 + i * 16
            ),
            0x4000
        );
    }

    #[test]
    fn partial_key_miss() {
        let slots = [(0x99, 0x1111_1111, 0x2222_2222)];
        // id match only
        run_vs_oracle(0x99, 0, 0, &slots, 0x5000);
        // src match only
        run_vs_oracle(0, 0x1111_1111, 0, &slots, 0x5000);
        // dst match only
        run_vs_oracle(0, 0, 0x2222_2222, &slots, 0x5000);
    }

    #[test]
    fn zeroed_empty_slot_matches_zero_packet_quirk() {
        // Legacy does not check ttl; zeroed table hits id=0/src=0/dst=0.
        let slots = [(0, 0, 0), (0x11, 1, 2)];
        run_vs_oracle(0, 0, 0, &slots, 0x6000);
        assert_eq!(
            ipv4_find_fragment_slot_from_keys(
                0,
                0,
                0,
                2,
                |i| slots[i as usize],
                |i| 0x6000 + i * 16
            ),
            0x6000
        );
    }

    #[test]
    fn pointer_form_matches_keys() {
        let mut table = [0u8; 48];
        // slot0: id=0xABCD src=0x01020304 dst=0x05060708
        table[2] = 0xCD;
        table[3] = 0xAB;
        table[4..8].copy_from_slice(&0x0102_0304u32.to_le_bytes());
        table[8..12].copy_from_slice(&0x0506_0708u32.to_le_bytes());
        // slot1: different
        table[16 + 2] = 0x11;
        table[16 + 3] = 0x22;
        table[16 + 4..16 + 8].copy_from_slice(&0xAAAAu32.to_le_bytes());
        table[16 + 8..16 + 12].copy_from_slice(&0xBBBBu32.to_le_bytes());

        let mut hdr = [0u8; 20];
        hdr[4] = 0xCD;
        hdr[5] = 0xAB;
        hdr[12..16].copy_from_slice(&0x0102_0304u32.to_le_bytes());
        hdr[16..20].copy_from_slice(&0x0506_0708u32.to_le_bytes());

        let base = table.as_ptr() as u32;
        let got = unsafe { ipv4_find_fragment_slot(hdr.as_ptr(), table.as_ptr(), 3) };
        assert_eq!(got, base);

        // miss
        hdr[4] = 0x00;
        hdr[5] = 0x00;
        let miss = unsafe { ipv4_find_fragment_slot(hdr.as_ptr(), table.as_ptr(), 3) };
        assert_eq!(miss, u32::MAX);
    }

    #[test]
    fn prng_50k_vs_oracle() {
        let mut state = IPV4_FIND_FRAGMENT_SLOT_PRNG_SEED;
        let mut next = || {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..50_000 {
            let n = (next() % 8) as usize; // 0..7 slots
            let mut slots = Vec::with_capacity(n);
            for _ in 0..n {
                let id = (next() & 0xFFFF) as u16;
                let src = next();
                let dst = next();
                slots.push((id, src, dst));
            }
            let q_id = if next() & 1 == 0 && n > 0 {
                slots[(next() as usize) % n].0
            } else {
                (next() & 0xFFFF) as u16
            };
            let q_src = if next() & 1 == 0 && n > 0 {
                slots[(next() as usize) % n].1
            } else {
                next()
            };
            let q_dst = if next() & 1 == 0 && n > 0 {
                slots[(next() as usize) % n].2
            } else {
                next()
            };
            let base = 0x7000_0000u32.wrapping_add(next() & 0xFFF0);
            run_vs_oracle(q_id, q_src, q_dst, &slots, base);
        }
    }
}
