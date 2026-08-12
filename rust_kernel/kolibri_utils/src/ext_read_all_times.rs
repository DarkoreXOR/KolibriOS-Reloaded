//! Cut BR: `ext_read_all_times` — EXT inode triple-timestamp → BDFE fan-out.
//!
//! Matches the FASM leaf in `kernel/fs/ext.inc`:
//! - fast path when `extraISize >= 24`: cr/a/m times with all Extra fields;
//! - slow path: derive extra-field count from `(extraISize - 4) / 4`, select
//!   crTime vs cTime for the first slot, and conditionally attach Extra dwords.
//!
//! Composes inlined Cut AL + T helpers (`ext_unix_to_secs` + `fs_time2bdfe_ptr`) —
//! no cross-Rust-blob calls.

use crate::time::{ext_unix_to_secs, fs_time2bdfe_ptr};

/// Cut BR differential PRNG seed (`'CUBR'`).
pub const EXT_READ_ALL_TIMES_PRNG_SEED: u32 = 0x4355_4252;

const INODE_EXTRA_ISIZE: u32 = 128;
const INODE_ATIME: u32 = 8;
const INODE_CTIME: u32 = 12;
const INODE_MTIME: u32 = 16;
const INODE_CTIME_EXTRA: u32 = 132;
const INODE_MTIME_EXTRA: u32 = 136;
const INODE_ATIME_EXTRA: u32 = 140;
const INODE_CRTIME: u32 = 144;
const INODE_CRTIME_EXTRA: u32 = 148;

const BDFE_BLOCK: usize = 8;
const OUT_BLOCKS: usize = 3;

#[inline(always)]
fn read_u16_le(ptr: *const u8, off: u32) -> u16 {
    let p = (ptr as usize).wrapping_add(off as usize) as *const u8;
    // SAFETY: caller guarantees readable inode buffer.
    unsafe { u16::from_le_bytes([*p, *p.add(1)]) }
}

#[inline(always)]
fn read_u32_le(ptr: *const u8, off: u32) -> u32 {
    let p = (ptr as usize).wrapping_add(off as usize) as *const u8;
    // SAFETY: caller guarantees readable inode buffer.
    unsafe { u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) }
}

#[inline(always)]
fn write_ext_time(out: *mut u8, i_time: u32, extra: u32) {
    let secs = ext_unix_to_secs(i_time, extra);
    // SAFETY: caller guarantees writable 8-byte BDFE block.
    unsafe { fs_time2bdfe_ptr(secs, out) };
}

/// FASM-faithful `ext_read_all_times`.
///
/// # Safety
/// `inode` must point to a readable `INODE`-sized buffer (≥ 152 bytes used).
/// `out` must be writable for 24 bytes (3× BDFE).
#[inline(always)]
pub unsafe fn ext_read_all_times_ptr(inode: *const u8, out: *mut u8) {
    let extra_i_size = read_u16_le(inode, INODE_EXTRA_ISIZE) as u32;
    let mut edi = out;

    if extra_i_size >= 24 {
        write_ext_time(
            edi,
            read_u32_le(inode, INODE_CRTIME),
            read_u32_le(inode, INODE_CRTIME_EXTRA),
        );
        edi = edi.add(BDFE_BLOCK);
        write_ext_time(
            edi,
            read_u32_le(inode, INODE_ATIME),
            read_u32_le(inode, INODE_ATIME_EXTRA),
        );
        edi = edi.add(BDFE_BLOCK);
        write_ext_time(
            edi,
            read_u32_le(inode, INODE_MTIME),
            read_u32_le(inode, INODE_MTIME_EXTRA),
        );
        return;
    }

    let mut ecx = extra_i_size.wrapping_sub(4);
    if extra_i_size < 4 {
        ecx = 0;
    } else {
        ecx >>= 2;
    }

    let (time0, extra0) = if ecx >= 4 {
        (read_u32_le(inode, INODE_CRTIME), 0)
    } else {
        let extra = if ecx >= 1 {
            read_u32_le(inode, INODE_CTIME_EXTRA)
        } else {
            0
        };
        (read_u32_le(inode, INODE_CTIME), extra)
    };
    write_ext_time(edi, time0, extra0);
    edi = edi.add(BDFE_BLOCK);

    let extra1 = if ecx >= 3 {
        read_u32_le(inode, INODE_ATIME_EXTRA)
    } else {
        0
    };
    write_ext_time(edi, read_u32_le(inode, INODE_ATIME), extra1);
    edi = edi.add(BDFE_BLOCK);

    let extra2 = if ecx >= 2 {
        read_u32_le(inode, INODE_MTIME_EXTRA)
    } else {
        0
    };
    write_ext_time(edi, read_u32_le(inode, INODE_MTIME), extra2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{fasm_oracle_ext_read_time, BdfeTime};

    fn t(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> BdfeTime {
        BdfeTime {
            year: y,
            month: mo,
            day: d,
            hour: h,
            min: mi,
            sec: s,
        }
    }

    fn bdfe_to_bytes(bt: &BdfeTime) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0] = bt.sec;
        b[1] = bt.min;
        b[2] = bt.hour;
        b[4] = bt.day;
        b[5] = bt.month;
        b[6..8].copy_from_slice(&bt.year.to_le_bytes());
        b
    }

    fn oracle_ext_read_all_times(inode: &[u8], out: &mut [u8; 24]) {
        let extra_i_size = u16::from_le_bytes([inode[128], inode[129]]) as u32;
        let mut edi = 0usize;

        let mut emit = |i_time: u32, extra: u32| {
            let bytes = bdfe_to_bytes(&fasm_oracle_ext_read_time(i_time, extra));
            out[edi..edi + BDFE_BLOCK].copy_from_slice(&bytes);
            edi += BDFE_BLOCK;
        };

        if extra_i_size >= 24 {
            emit(
                u32::from_le_bytes(inode[144..148].try_into().unwrap()),
                u32::from_le_bytes(inode[148..152].try_into().unwrap()),
            );
            emit(
                u32::from_le_bytes(inode[8..12].try_into().unwrap()),
                u32::from_le_bytes(inode[140..144].try_into().unwrap()),
            );
            emit(
                u32::from_le_bytes(inode[16..20].try_into().unwrap()),
                u32::from_le_bytes(inode[136..140].try_into().unwrap()),
            );
            return;
        }

        let mut ecx = extra_i_size.wrapping_sub(4);
        if extra_i_size < 4 {
            ecx = 0;
        } else {
            ecx >>= 2;
        }

        if ecx >= 4 {
            emit(u32::from_le_bytes(inode[144..148].try_into().unwrap()), 0);
        } else {
            let extra = if ecx >= 1 {
                u32::from_le_bytes(inode[132..136].try_into().unwrap())
            } else {
                0
            };
            emit(u32::from_le_bytes(inode[12..16].try_into().unwrap()), extra);
        }

        let extra1 = if ecx >= 3 {
            u32::from_le_bytes(inode[140..144].try_into().unwrap())
        } else {
            0
        };
        emit(u32::from_le_bytes(inode[8..12].try_into().unwrap()), extra1);

        let extra2 = if ecx >= 2 {
            u32::from_le_bytes(inode[136..140].try_into().unwrap())
        } else {
            0
        };
        emit(u32::from_le_bytes(inode[16..20].try_into().unwrap()), extra2);
    }

    fn write_u16_le(buf: &mut [u8], off: usize, v: u16) {
        buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn write_u32_le(buf: &mut [u8], off: usize, v: u32) {
        buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn check_inode(extra_i_size: u16, fields: &[(u32, u32)]) {
        let mut inode = vec![0u8; 160];
        write_u16_le(&mut inode, INODE_EXTRA_ISIZE as usize, extra_i_size);
        for &(off, val) in fields {
            write_u32_le(&mut inode, off as usize, val);
        }
        let mut out = [0xA5u8; 24];
        let mut exp = [0u8; 24];
        unsafe { ext_read_all_times_ptr(inode.as_ptr(), out.as_mut_ptr()) };
        oracle_ext_read_all_times(&inode, &mut exp);
        assert_eq!(out, exp, "inode extraISize={extra_i_size}");
    }

    #[test]
    fn no_extra_fields_use_ctime_only() {
        check_inode(4, &[(INODE_CTIME, 978_307_200), (INODE_ATIME, 978_307_201), (INODE_MTIME, 978_307_202)]);
    }

    #[test]
    fn partial_extra_one_ctime_extra() {
        check_inode(
            8,
            &[
                (INODE_CTIME, 978_307_200),
                (INODE_CTIME_EXTRA, 1),
                (INODE_ATIME, 978_307_201),
                (INODE_MTIME, 978_307_202),
            ],
        );
    }

    #[test]
    fn partial_extra_two_m_time_extra() {
        check_inode(
            12,
            &[
                (INODE_CTIME, 978_307_200),
                (INODE_MTIME, 978_307_202),
                (INODE_MTIME_EXTRA, 2),
                (INODE_ATIME, 978_307_201),
            ],
        );
    }

    #[test]
    fn partial_extra_three_a_time_extra() {
        check_inode(
            16,
            &[
                (INODE_CTIME, 978_307_200),
                (INODE_ATIME, 978_307_201),
                (INODE_ATIME_EXTRA, 3),
                (INODE_MTIME, 978_307_202),
            ],
        );
    }

    #[test]
    fn partial_extra_four_uses_cr_time() {
        check_inode(
            20,
            &[
                (INODE_CRTIME, 978_307_203),
                (INODE_ATIME, 978_307_201),
                (INODE_MTIME, 978_307_202),
            ],
        );
    }

    #[test]
    fn fast_path_all_extra() {
        check_inode(
            24,
            &[
                (INODE_CRTIME, 978_307_203),
                (INODE_CRTIME_EXTRA, 1),
                (INODE_ATIME, 978_307_201),
                (INODE_ATIME_EXTRA, 2),
                (INODE_MTIME, 978_307_202),
                (INODE_MTIME_EXTRA, 3),
            ],
        );
    }

    #[test]
    fn extra_isize_below_four_zeros_count() {
        check_inode(2, &[(INODE_CTIME, 0), (INODE_ATIME, 0), (INODE_MTIME, 0)]);
    }

    #[test]
    fn prng_50k_cubr() {
        let mut s = EXT_READ_ALL_TIMES_PRNG_SEED;
        for _ in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let extra_i_size = (s % 32) as u16;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;

            let mut inode = vec![0u8; 160];
            write_u16_le(&mut inode, INODE_EXTRA_ISIZE as usize, extra_i_size);

            let fields = [
                INODE_CTIME,
                INODE_MTIME,
                INODE_ATIME,
                INODE_CTIME_EXTRA,
                INODE_MTIME_EXTRA,
                INODE_ATIME_EXTRA,
                INODE_CRTIME,
                INODE_CRTIME_EXTRA,
            ];
            for off in fields {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                write_u32_le(&mut inode, off as usize, s);
            }

            let mut out = [0u8; 24];
            let mut exp = [0u8; 24];
            unsafe { ext_read_all_times_ptr(inode.as_ptr(), out.as_mut_ptr()) };
            oracle_ext_read_all_times(&inode, &mut exp);
            assert_eq!(out, exp, "prng extraISize={extra_i_size}");
        }
    }
}
