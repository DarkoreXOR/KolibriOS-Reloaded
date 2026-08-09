//! Cut X: `set_io_access_rights` — TSS I/O permission bitmap BTR/BTS.
//!
//! Matches `kernel/kernel.asm` FASM leaf semantics:
//! * `ebp == 0` → `btr [io_map], port` (enable access / clear deny bit)
//! * `ebp != 0` → `bts [io_map], port` (disable access / set deny bit)
//!
//! The FASM trampoline injects `tss._io_map_0` so this blob stays reloc-free.
//! No tables / `.rodata`. Freestanding path uses only raw pointer arithmetic.

/// I/O permission bitmap size in bytes (`_io_map_0` + `_io_map_1` = 8192).
pub const IO_MAP_BYTES: usize = 8192;

/// Number of port bits covered by the full TSS I/O map.
pub const IO_MAP_BITS: u32 = (IO_MAP_BYTES as u32) * 8;

/// Cut X differential PRNG seed (`'CUTX'`).
pub const SET_IO_ACCESS_RIGHTS_PRNG_SEED: u32 = 0x4355_5458;

/// FASM-faithful I/O bitmap update (BTR enable / BTS disable).
///
/// `port` is the absolute bit index into `io_map` (legacy EAX).
/// `clear_access` is legacy EBP: `0` clears the bit (enable), any nonzero
/// sets the bit (disable). Matches `test ebp, ebp` / `jnz .siar1`.
///
/// # Safety
/// `io_map` must be writable for at least `((port / 8) + 1)` bytes.
/// Production callers keep `port < 65536` (full 8 KiB map).
#[inline(always)]
pub unsafe fn set_io_access_rights(port: u32, clear_access: u32, io_map: *mut u8) {
    let byte_index = (port >> 3) as usize;
    let bit = (port & 7) as u8;
    let mask = 1u8 << bit;
    let p = unsafe { io_map.add(byte_index) };
    let cur = unsafe { *p };
    if clear_access == 0 {
        // btr — clear bit
        unsafe { *p = cur & !mask };
    } else {
        // bts — set bit
        unsafe { *p = cur | mask };
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`set_io_access_rights`].
#[inline(always)]
pub unsafe fn set_io_access_rights_ptr(port: u32, clear_access: u32, io_map: *mut u8) {
    unsafe { set_io_access_rights(port, clear_access, io_map) }
}

/// Read one permission bit from an I/O map (`1` = denied / set).
///
/// # Safety
/// `io_map` must be readable for `((port / 8) + 1)` bytes.
#[inline(always)]
pub unsafe fn io_map_bit(io_map: *const u8, port: u32) -> u8 {
    let byte_index = (port >> 3) as usize;
    let bit = (port & 7) as u8;
    let b = unsafe { *io_map.add(byte_index) };
    (b >> bit) & 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (same BTR/BTS memory model).
    fn oracle(port: u32, clear_access: u32, map: &mut [u8]) {
        let byte_index = (port >> 3) as usize;
        let bit = (port & 7) as u8;
        let mask = 1u8 << bit;
        if clear_access == 0 {
            map[byte_index] &= !mask;
        } else {
            map[byte_index] |= mask;
        }
    }

    fn run_vs_oracle(port: u32, clear_access: u32, initial: u8) {
        let mut rust_map = [initial; IO_MAP_BYTES];
        let mut ora_map = [initial; IO_MAP_BYTES];
        unsafe {
            set_io_access_rights(port, clear_access, rust_map.as_mut_ptr());
        }
        oracle(port, clear_access, &mut ora_map);
        assert_eq!(
            rust_map, ora_map,
            "mismatch port={port:#x} clear={clear_access:#x} initial={initial:#x}"
        );
    }

    #[test]
    fn enable_clears_bit_disable_sets_bit() {
        // Port 0 bit 0 of byte 0
        run_vs_oracle(0, 0, 0xFF);
        run_vs_oracle(0, 1, 0x00);
        // Port 7 bit 7 of byte 0
        run_vs_oracle(7, 0, 0xFF);
        run_vs_oracle(7, 1, 0x00);
        // Port 8 → byte 1
        run_vs_oracle(8, 0, 0xFF);
        run_vs_oracle(8, 1, 0x00);
        // High end of map
        run_vs_oracle(65535, 0, 0xFF);
        run_vs_oracle(65535, 1, 0x00);
        run_vs_oracle(65534, 0, 0xFF);
        run_vs_oracle(0x1234, 1, 0xA5);
    }

    #[test]
    fn nonzero_ebp_means_disable() {
        // Legacy: test ebp,ebp / jnz — any nonzero takes BTS path.
        for clear in [1u32, 2, 0x80, 0xFFFF_FFFF] {
            run_vs_oracle(0x42, clear, 0x00);
            run_vs_oracle(0x42, clear, 0xFF);
        }
    }

    #[test]
    fn idempotent_set_and_clear() {
        let mut map = [0xAAu8; IO_MAP_BYTES];
        unsafe {
            set_io_access_rights(100, 1, map.as_mut_ptr());
            assert_eq!(io_map_bit(map.as_ptr(), 100), 1);
            set_io_access_rights(100, 1, map.as_mut_ptr());
            assert_eq!(io_map_bit(map.as_ptr(), 100), 1);
            set_io_access_rights(100, 0, map.as_mut_ptr());
            assert_eq!(io_map_bit(map.as_ptr(), 100), 0);
            set_io_access_rights(100, 0, map.as_mut_ptr());
            assert_eq!(io_map_bit(map.as_ptr(), 100), 0);
        }
    }

    #[test]
    fn neighbor_bits_untouched() {
        let mut map = [0u8; IO_MAP_BYTES];
        // Fill byte 5 with a known pattern, touch port in that byte.
        map[5] = 0b1010_1010;
        let port = (5 * 8) + 3; // bit 3 → becomes set
        unsafe {
            set_io_access_rights(port, 1, map.as_mut_ptr());
        }
        assert_eq!(map[5], 0b1010_1010 | (1 << 3));
        // Other bytes untouched
        assert!(map[..5].iter().all(|&b| b == 0));
        assert!(map[6..].iter().all(|&b| b == 0));
    }

    #[test]
    fn exhaustive_all_ports_enable_from_ones() {
        let mut rust_map = [0xFFu8; IO_MAP_BYTES];
        let mut ora_map = [0xFFu8; IO_MAP_BYTES];
        for port in 0..IO_MAP_BITS {
            unsafe {
                set_io_access_rights(port, 0, rust_map.as_mut_ptr());
            }
            oracle(port, 0, &mut ora_map);
        }
        assert_eq!(rust_map, ora_map);
        assert!(rust_map.iter().all(|&b| b == 0));
    }

    #[test]
    fn exhaustive_all_ports_disable_from_zeros() {
        let mut rust_map = [0u8; IO_MAP_BYTES];
        let mut ora_map = [0u8; IO_MAP_BYTES];
        for port in 0..IO_MAP_BITS {
            unsafe {
                set_io_access_rights(port, 1, rust_map.as_mut_ptr());
            }
            oracle(port, 1, &mut ora_map);
        }
        assert_eq!(rust_map, ora_map);
        assert!(rust_map.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn range_loop_matches_r_f_port_area_enable() {
        // Mimic r_f_port_area enable loop: for eax in ecx..=edx, ebp=0.
        let start = 0x300u32;
        let end = 0x30Fu32;
        let mut rust_map = [0xFFu8; IO_MAP_BYTES];
        let mut ora_map = [0xFFu8; IO_MAP_BYTES];
        let mut eax = start;
        loop {
            unsafe {
                set_io_access_rights(eax, 0, rust_map.as_mut_ptr());
            }
            oracle(eax, 0, &mut ora_map);
            if eax == end {
                break;
            }
            eax = eax.wrapping_add(1);
        }
        assert_eq!(rust_map, ora_map);
        for p in start..=end {
            unsafe {
                assert_eq!(io_map_bit(rust_map.as_ptr(), p), 0);
            }
        }
        // Outside range still denied
        unsafe {
            assert_eq!(io_map_bit(rust_map.as_ptr(), start - 1), 1);
            assert_eq!(io_map_bit(rust_map.as_ptr(), end + 1), 1);
        }
    }

    #[test]
    fn range_loop_matches_r_f_port_area_disable() {
        let start = 0x400u32;
        let end = 0x41Fu32;
        let mut rust_map = [0u8; IO_MAP_BYTES];
        let mut ora_map = [0u8; IO_MAP_BYTES];
        // Pre-enable
        for p in start..=end {
            unsafe {
                set_io_access_rights(p, 0, rust_map.as_mut_ptr());
            }
            oracle(p, 0, &mut ora_map);
        }
        let mut eax = start;
        loop {
            unsafe {
                set_io_access_rights(eax, 1, rust_map.as_mut_ptr());
            }
            oracle(eax, 1, &mut ora_map);
            if eax == end {
                break;
            }
            eax = eax.wrapping_add(1);
        }
        assert_eq!(rust_map, ora_map);
        for p in start..=end {
            unsafe {
                assert_eq!(io_map_bit(rust_map.as_ptr(), p), 1);
            }
        }
    }

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn prng_mixed_ops_vs_oracle() {
        let mut state = SET_IO_ACCESS_RIGHTS_PRNG_SEED;
        let mut rust_map = [0xFFu8; IO_MAP_BYTES]; // boot-like: all denied
        let mut ora_map = [0xFFu8; IO_MAP_BYTES];
        for _ in 0..50_000 {
            let port = xorshift32(&mut state) & 0xFFFF;
            let clear = xorshift32(&mut state) & 1;
            // Occasionally use non-{0,1} ebp to exercise nonzero path.
            let clear = if (xorshift32(&mut state) & 0x1F) == 0 {
                xorshift32(&mut state) | 1
            } else {
                clear
            };
            unsafe {
                set_io_access_rights(port, clear, rust_map.as_mut_ptr());
            }
            oracle(port, clear, &mut ora_map);
        }
        assert_eq!(rust_map, ora_map);
    }

    #[test]
    fn byte_boundaries_and_alignment() {
        for port in [0u32, 1, 7, 8, 9, 15, 16, 31, 32, 255, 256, 4095, 4096, 8191, 65535]
        {
            run_vs_oracle(port, 0, 0xFF);
            run_vs_oracle(port, 1, 0x00);
            run_vs_oracle(port, 0, 0x55);
            run_vs_oracle(port, 1, 0xAA);
        }
    }
}
