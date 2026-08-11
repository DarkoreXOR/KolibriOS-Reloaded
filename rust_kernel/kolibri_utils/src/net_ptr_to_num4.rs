//! Cut AY: `net_ptr_to_num4` — NIC device-list pointer → index×4.
//!
//! Matches `kernel/network/stack.inc` FASM leaf semantics:
//! * Null device (`EBX == 0`) → `EDI = -1` immediately
//! * Else linear scan `list[0..max)` comparing dword slots to device
//! * Hit → `EDI = (slot_index * 4)` (= byte offset from list base)
//! * Miss → `EDI = -1`
//!
//! Legacy preserves EAX/EBX/ECX/EDX/ESI/EBP (ECX via push/pop); EDI is the
//! result. Holes (null slots after remove-without-compact) are scanned like
//! any other slot. `net_device_list` + `NET_DEVICES_MAX` are trampoline-
//! injected so the Rust blob stays reloc-free.

/// Production `NET_DEVICES_MAX` (`kernel/network/stack.inc`).
pub const NET_DEVICES_MAX: u32 = 16;

/// Cut AY differential PRNG seed (`'CUTY'`).
pub const NET_PTR_TO_NUM4_PRNG_SEED: u32 = 0x4355_5459;

/// Miss / null-device sentinel (legacy `or edi, -1`).
pub const NET_PTR_TO_NUM4_MISS: u32 = 0xFFFF_FFFF;

/// Pure FASM-flow walk over an explicit dword slice.
///
/// `list[i]` is the device pointer at slot `i`; return value is `i * 4` or
/// [`NET_PTR_TO_NUM4_MISS`].
#[inline(always)]
pub fn net_ptr_to_num4_from_slice(device: u32, list: &[u32]) -> u32 {
    // test ebx, ebx / jz .fail
    if device == 0 {
        return NET_PTR_TO_NUM4_MISS;
    }
    // mov ecx, max / mov edi, list_base / .loop: cmp ebx,[edi] / …
    for (i, &slot) in list.iter().enumerate() {
        if slot == device {
            // sub edi, list_base  →  i * 4
            return (i as u32).wrapping_mul(4);
        }
    }
    NET_PTR_TO_NUM4_MISS
}

/// FASM-faithful device-pointer → index×4 resolve.
///
/// # Safety
/// `list_base` must point at a readable array of at least `max` little-endian
/// `u32` device pointers (production: `net_device_list`).
#[inline(always)]
pub unsafe fn net_ptr_to_num4(device: u32, list_base: *const u32, max: u32) -> u32 {
    if device == 0 {
        return NET_PTR_TO_NUM4_MISS;
    }
    let mut i = 0u32;
    while i < max {
        // SAFETY: caller guarantees `list_base[0..max)` is readable.
        let slot = unsafe { read_u32_le(list_base.add(i as usize) as *const u8) };
        if slot == device {
            return i.wrapping_mul(4);
        }
        i = i.wrapping_add(1);
    }
    NET_PTR_TO_NUM4_MISS
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`net_ptr_to_num4`].
#[inline(always)]
pub unsafe fn net_ptr_to_num4_ptr(device: u32, list_base: *const u32, max: u32) -> u32 {
    unsafe { net_ptr_to_num4(device, list_base, max) }
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not shared with production helpers).
    fn oracle(device: u32, list: &[u32]) -> u32 {
        if device == 0 {
            return 0xFFFF_FFFF;
        }
        let mut edi_off = 0u32;
        for &slot in list {
            if slot == device {
                return edi_off;
            }
            edi_off = edi_off.wrapping_add(4);
        }
        0xFFFF_FFFF
    }

    fn run_vs_oracle(device: u32, list: &[u32]) {
        let got = net_ptr_to_num4_from_slice(device, list);
        let exp = oracle(device, list);
        assert_eq!(
            got, exp,
            "mismatch device={device:#x} list={list:?} got={got:#x} exp={exp:#x}"
        );
        // SAFETY: slice pointer is valid for `list.len()` dwords.
        let got_ptr = unsafe { net_ptr_to_num4(device, list.as_ptr(), list.len() as u32) };
        assert_eq!(got_ptr, exp);
    }

    #[test]
    fn null_device_returns_miss() {
        let list = [0x1000u32, 0x2000, 0];
        run_vs_oracle(0, &list);
        assert_eq!(net_ptr_to_num4_from_slice(0, &list), NET_PTR_TO_NUM4_MISS);
    }

    #[test]
    fn empty_list_miss() {
        let list: [u32; 0] = [];
        run_vs_oracle(0xDEAD_BEEF, &list);
    }

    #[test]
    fn hit_first_middle_last() {
        let list = [0xA0, 0xB0, 0xC0];
        run_vs_oracle(0xA0, &list);
        run_vs_oracle(0xB0, &list);
        run_vs_oracle(0xC0, &list);
        assert_eq!(net_ptr_to_num4_from_slice(0xA0, &list), 0);
        assert_eq!(net_ptr_to_num4_from_slice(0xB0, &list), 4);
        assert_eq!(net_ptr_to_num4_from_slice(0xC0, &list), 8);
    }

    #[test]
    fn holes_and_miss() {
        // Remove-without-compact: null slot in the middle.
        let list = [0x10, 0, 0x30, 0];
        run_vs_oracle(0x10, &list);
        run_vs_oracle(0x30, &list);
        run_vs_oracle(0x20, &list);
        run_vs_oracle(0, &list);
        assert_eq!(net_ptr_to_num4_from_slice(0x30, &list), 8);
        assert_eq!(net_ptr_to_num4_from_slice(0x20, &list), NET_PTR_TO_NUM4_MISS);
    }

    #[test]
    fn full_sixteen_slots() {
        let mut list = [0u32; NET_DEVICES_MAX as usize];
        for i in 0..NET_DEVICES_MAX as usize {
            list[i] = 0x1000_0000 + (i as u32) * 0x10;
        }
        run_vs_oracle(list[0], &list);
        run_vs_oracle(list[7], &list);
        run_vs_oracle(list[15], &list);
        run_vs_oracle(0xEEEE_EEEE, &list);
        assert_eq!(net_ptr_to_num4_from_slice(list[15], &list), 15 * 4);
    }

    #[test]
    fn first_match_wins_on_duplicate() {
        // Production list should not duplicate; still document first-match.
        let list = [0x55, 0x55];
        run_vs_oracle(0x55, &list);
        assert_eq!(net_ptr_to_num4_from_slice(0x55, &list), 0);
    }

    #[test]
    fn prng_corpus_50k() {
        let mut state = NET_PTR_TO_NUM4_PRNG_SEED;
        fn next(s: &mut u32) -> u32 {
            let mut x = *s;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *s = x;
            x
        }

        for _ in 0..50_000 {
            let n = (next(&mut state) % (NET_DEVICES_MAX + 1)) as usize;
            let mut list = vec![0u32; n];
            for slot in list.iter_mut() {
                // Mix of null holes and distinct non-zero fake device ptrs.
                if next(&mut state) & 7 == 0 {
                    *slot = 0;
                } else {
                    *slot = 0x8000_0000 | (next(&mut state) & 0x0FFF_FFF0) | 0x10;
                }
            }

            let query = if !list.is_empty() && next(&mut state) & 3 != 0 {
                // Prefer querying a live slot when present.
                let live: Vec<u32> = list.iter().copied().filter(|&p| p != 0).collect();
                if !live.is_empty() && next(&mut state) & 1 != 0 {
                    live[(next(&mut state) as usize) % live.len()]
                } else if next(&mut state) & 7 == 0 {
                    0
                } else {
                    next(&mut state) | 1
                }
            } else if next(&mut state) & 7 == 0 {
                0
            } else {
                next(&mut state) | 1
            };
            run_vs_oracle(query, &list);
        }
    }
}
