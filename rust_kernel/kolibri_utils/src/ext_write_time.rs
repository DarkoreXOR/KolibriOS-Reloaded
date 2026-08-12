//! Cut BS: `ext_write_time` pack — KOS seconds (since 2001) → EXT inode i_time + extra.
//!
//! Matches the FASM leaf tail in `kernel/fs/ext.inc` after `fsGetTime`:
//! `add UNIXTIME_TO_KOS_OFFSET` / signed-negative `inc edx` / optional extra write.
//! CMOS read stays in FASM (`call fsGetTime` before Rust).

use crate::time::UNIXTIME_TO_KOS_OFFSET;

/// Cut BS differential PRNG seed (`'CUBS'`).
pub const EXT_WRITE_TIME_PRNG_SEED: u32 = 0x4355_4253;

/// Sentinel: `extra_time_ptr == -1` skips the extra-field write (FASM `cmp ecx, -1`).
pub const EXT_WRITE_TIME_NO_EXTRA: u32 = 0xFFFF_FFFF;

/// FASM-faithful pack after `fsGetTime` returns KOS seconds since 2001-01-01.
///
/// `extra_time_ptr` uses the FASM `-1` sentinel cast to `*mut u32` when no extra
/// field is present (`0xFFFF_FFFF`).
///
/// # Safety
/// `time_ptr` must be writable. When `extra_time_ptr as usize != 0xFFFF_FFFF`,
/// it must address a writable `u32`.
#[inline(always)]
pub unsafe fn ext_write_time_pack_ptr(
    kos_secs: u32,
    time_ptr: *mut u32,
    extra_time_ptr: *mut u32,
) {
    let mut eax = kos_secs;
    let mut edx = 0u32;
    let (sum, carry) = eax.overflowing_add(UNIXTIME_TO_KOS_OFFSET);
    eax = sum;
    edx = edx.wrapping_add(u32::from(carry));
    if (eax as i32) < 0 {
        edx = edx.wrapping_add(1);
    }
    // SAFETY: caller guarantees writable time slot.
    unsafe {
        *time_ptr = eax;
    }
    if extra_time_ptr as usize != EXT_WRITE_TIME_NO_EXTRA as usize {
        // SAFETY: caller guarantees writable extra slot when not -1 sentinel.
        unsafe {
            *extra_time_ptr = edx & 3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::ext_unix_to_secs;

    /// Structurally independent FASM oracle — mirrors `ext.inc` register flow.
    fn fasm_oracle_ext_write_time_pack(
        kos_secs: u32,
        time_ptr: *mut u32,
        extra_time_ptr: *mut u32,
    ) {
        let mut eax = kos_secs;
        let mut edx = 0u32;
        let offset = (365 * 31 + 8) * 24 * 60 * 60u32; // literal, not const alias
        let (sum, carry) = eax.overflowing_add(offset);
        eax = sum;
        edx = edx.wrapping_add(u32::from(carry));
        if (eax as i32) < 0 {
            edx = edx.wrapping_add(1);
        }
        unsafe {
            *time_ptr = eax;
        }
        if extra_time_ptr as usize != 0xFFFF_FFFF {
            unsafe {
                *extra_time_ptr = edx & 3;
            }
        }
    }

    fn no_extra_ptr() -> *mut u32 {
        EXT_WRITE_TIME_NO_EXTRA as *mut u32
    }

    fn check_pack(kos_secs: u32, with_extra: bool) {
        let mut time = 0xA5A5_A5A5u32;
        let mut extra = 0x5A5A_5A5Au32;
        let extra_ptr = if with_extra {
            &mut extra as *mut u32
        } else {
            no_extra_ptr()
        };
        let mut exp_time = time;
        let mut exp_extra = extra;
        let exp_extra_ptr = if with_extra {
            &mut exp_extra as *mut u32
        } else {
            no_extra_ptr()
        };
        unsafe {
            ext_write_time_pack_ptr(kos_secs, &mut time, extra_ptr);
            fasm_oracle_ext_write_time_pack(kos_secs, &mut exp_time, exp_extra_ptr);
        }
        assert_eq!(time, exp_time, "kos_secs={kos_secs:#x} time");
        if with_extra {
            assert_eq!(extra, exp_extra, "kos_secs={kos_secs:#x} extra");
        } else {
            assert_eq!(extra, 0x5A5A_5A5A, "extra must be untouched when -1");
        }
    }

    #[test]
    fn epoch_zero() {
        check_pack(0, true);
        check_pack(0, false);
    }

    #[test]
    fn one_second() {
        check_pack(1, true);
    }

    #[test]
    fn max_kos_secs_no_overflow() {
        check_pack(0xFFFF_FFFF - UNIXTIME_TO_KOS_OFFSET, true);
    }

    #[test]
    fn carry_into_extra_epoch_bit() {
        // kos_secs + OFFSET wraps u32 → adc edx; result may be negative → inc edx
        check_pack(0xFFFF_FFFF - UNIXTIME_TO_KOS_OFFSET + 1, true);
    }

    #[test]
    fn negative_i_time_sign_extend() {
        check_pack(0xFFFF_FFFF, true);
    }

    #[test]
    fn round_trip_with_read_path_where_invertible() {
        // Only pairs where ext_unix_to_secs does not clamp (lossless domain).
        let vectors: &[(u32, u32)] = &[
            (978_307_200, 0),
            (978_307_201, 0),
            (0xFFFF_FFFF, 1),
        ];
        for &(i_time, extra_in) in vectors {
            let kos = ext_unix_to_secs(i_time, extra_in);
            let mut time = 0u32;
            let mut extra_out = 0u32;
            let extra_ptr = &mut extra_out as *mut u32;
            unsafe {
                ext_write_time_pack_ptr(kos, &mut time, extra_ptr);
            }
            assert_eq!(time, i_time, "round-trip time i={i_time:#x} e={extra_in}");
            assert_eq!(extra_out & 3, extra_in & 3, "round-trip extra");
        }
    }

    #[test]
    fn prng_50k_cubs() {
        let mut s = EXT_WRITE_TIME_PRNG_SEED;
        for i in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let kos_secs = s;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let with_extra = (s & 1) != 0;

            let mut time = s.wrapping_add(1);
            let mut extra = s.wrapping_add(2);
            let extra_ptr = if with_extra {
                &mut extra as *mut u32
            } else {
                no_extra_ptr()
            };
            let mut exp_time = time;
            let mut exp_extra = extra;
            let exp_extra_ptr = if with_extra {
                &mut exp_extra as *mut u32
            } else {
                no_extra_ptr()
            };
            unsafe {
                ext_write_time_pack_ptr(kos_secs, &mut time, extra_ptr);
                fasm_oracle_ext_write_time_pack(kos_secs, &mut exp_time, exp_extra_ptr);
            }
            assert_eq!(time, exp_time, "prng#{i} kos={kos_secs:#x}");
            if with_extra {
                assert_eq!(extra, exp_extra, "prng#{i} extra kos={kos_secs:#x}");
            }
        }
    }
}
