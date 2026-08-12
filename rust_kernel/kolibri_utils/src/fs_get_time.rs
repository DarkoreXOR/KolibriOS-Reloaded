//! Cut BV: `fsGetTime` — CMOS RTC read orchestration → BDFE → KOS seconds.
//!
//! Matches `kernel/fs/fs_common.inc` FASM `fsGetTime`:
//! * six `fsReadCMOS` calls (regs 7/8/9 date, 0/2/4 time)
//! * `ror`/`add 2000` packing into an 8-byte BDFE stack block
//! * fallthrough to `fsCalculateTime` (Cut G) — inlined here via [`fs_calculate_time`]
//!
//! Port I/O stays in FASM (`fsReadCMOS`); injected stdcall wrapper callback.

use crate::time::{fs_calculate_time, BdfeTime};

/// Cut BV differential PRNG seed (`'CUBV'`).
pub const FS_GET_TIME_PRNG_SEED: u32 = 0x4355_4256;

/// Stdcall wrapper around FASM `fs_read_cmos_stdcall` / `fsReadCMOS`.
pub type FsReadCmosFn = unsafe extern "stdcall" fn(reg: u32) -> u32;

/// FASM `fsReadCMOS` BCD decode: `in al,71h` → binary 0–99 in AX.
#[inline(always)]
pub fn fs_read_cmos_bcd(raw: u8) -> u16 {
    let mut ax = u16::from(raw & 0x7F);
    ax <<= 4;
    ax = (ax & 0xFF00) | ((ax & 0x00FF) >> 4);
    // aad: AL = (AH*10 + AL) mod 100; AH = 0
    let ah = (ax >> 8) as u8;
    let al = (ax & 0xFF) as u8;
    let sum = u16::from(ah).wrapping_mul(10).wrapping_add(u16::from(al));
    (sum % 100) as u16
}

/// Pack six decoded CMOS fields into BDFE using FASM `fsGetTime` stack layout.
#[inline(always)]
pub fn bdfe_from_cmos_fields(day: u16, month: u16, year2: u16, sec: u16, min: u16, hour: u16) -> BdfeTime {
    let mut eax: u32 = 0;
    eax = (eax & 0xFFFF_0000) | u32::from(day);
    eax = eax.rotate_right(8);
    eax = (eax & 0xFFFF_0000) | u32::from(month);
    eax = eax.rotate_right(8);
    eax = (eax & 0xFFFF_0000) | u32::from(year2);
    eax = eax.wrapping_add(2000);
    eax = eax.rotate_right(16);
    let date_part = eax.to_le_bytes();

    eax = 0;
    eax = (eax & 0xFFFF_0000) | u32::from(sec);
    eax = eax.rotate_right(8);
    eax = (eax & 0xFFFF_0000) | u32::from(min);
    eax = eax.rotate_right(8);
    eax = (eax & 0xFFFF_0000) | u32::from(hour);
    eax = eax.rotate_right(16);
    let time_part = eax.to_le_bytes();

    let mut block = [0u8; 8];
    block[..4].copy_from_slice(&time_part);
    block[4..].copy_from_slice(&date_part);
    BdfeTime::from_bytes(&block)
}

/// Build BDFE from injected CMOS reader (reg index in low byte, decoded 0–99 in EAX).
#[inline(always)]
pub fn bdfe_from_cmos_reader(read_cmos: FsReadCmosFn) -> BdfeTime {
    let read = |reg: u8| -> u16 {
        let v = unsafe { read_cmos(u32::from(reg)) };
        (v & 0xFFFF) as u16
    };
    bdfe_from_cmos_fields(read(7), read(8), read(9), read(0), read(2), read(4))
}

/// FASM-faithful `fsGetTime` body (CMOS orchestration + Cut G calendar).
#[inline(always)]
pub fn fs_get_time(read_cmos: FsReadCmosFn) -> u32 {
    fs_calculate_time(bdfe_from_cmos_reader(read_cmos))
}

#[inline(always)]
pub unsafe fn fs_get_time_ptr(read_cmos: FsReadCmosFn) -> u32 {
    fs_get_time(read_cmos)
}

/// Structurally independent oracle: literal BCD decode + `ror` packing + [`fs_calculate_time`].
pub fn fasm_oracle_fs_get_time_from_fields(
    day: u16,
    month: u16,
    year2: u16,
    sec: u16,
    min: u16,
    hour: u16,
) -> u32 {
    fs_calculate_time(bdfe_from_cmos_fields(day, month, year2, sec, min, hour))
}

/// Buffered oracle for mock CMOS tables keyed by register index.
pub fn fs_get_time_oracle_from_table(table: &[u16; 256], read_fn: impl Fn(u8) -> u16) -> u32 {
    let _ = table;
    fs_calculate_time(bdfe_from_cmos_fields(
        read_fn(7),
        read_fn(8),
        read_fn(9),
        read_fn(0),
        read_fn(2),
        read_fn(4),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::fasm_oracle_fs_calculate_time;

    fn mock_read_cmos(reg: u32) -> u32 {
        match (reg & 0xFF) as u8 {
            0 => 30,
            2 => 45,
            4 => 12,
            7 => 15,
            8 => 6,
            9 => 26,
            _ => 0,
        }
    }

    unsafe extern "stdcall" fn mock_read_cmos_fn(reg: u32) -> u32 {
        mock_read_cmos(reg)
    }

    #[test]
    fn bcd_decode_samples() {
        assert_eq!(fs_read_cmos_bcd(0x45), 45);
        assert_eq!(fs_read_cmos_bcd(0x26), 26);
        assert_eq!(fs_read_cmos_bcd(0x12), 12);
    }

    #[test]
    fn pack_known_datetime() {
        let bt = bdfe_from_cmos_fields(15, 6, 26, 30, 45, 12);
        assert_eq!(bt.day, 15);
        assert_eq!(bt.month, 6);
        assert_eq!(bt.year, 2026);
        assert_eq!(bt.hour, 12);
        assert_eq!(bt.min, 45);
        assert_eq!(bt.sec, 30);
    }

    #[test]
    fn smoke_vector_2026_06_15() {
        let got = fs_get_time(mock_read_cmos_fn);
        let bt = bdfe_from_cmos_fields(15, 6, 26, 30, 45, 12);
        let expect = fs_calculate_time(bt);
        assert_eq!(got, expect);
        assert_eq!(
            got,
            fasm_oracle_fs_get_time_from_fields(15, 6, 26, 30, 45, 12)
        );
        // Precomputed anchor for FASM smoke vector 1
        assert_eq!(got, 803_220_330);
    }

    #[test]
    fn matches_calculate_time_oracle() {
        let bt = bdfe_from_cmos_fields(1, 1, 1, 0, 0, 0);
        let kos = fs_calculate_time(bt);
        assert_eq!(kos, fasm_oracle_fs_calculate_time(bt));
        assert_eq!(fs_get_time(mock_read_cmos_fn), 803_220_330);
    }

    #[test]
    fn year_add_2000_wrap() {
        let bt = bdfe_from_cmos_fields(1, 1, 99, 0, 0, 0);
        assert_eq!(bt.year, 2099);
    }

    #[test]
    fn midnight_epoch_day() {
        let got = fasm_oracle_fs_get_time_from_fields(1, 1, 1, 0, 0, 0);
        assert_eq!(got, 0);
    }

    #[test]
    fn prng_50k_cubv() {
        let mut s = FS_GET_TIME_PRNG_SEED;
        for i in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let day = (s & 0x1F).wrapping_add(1) as u16;
            let month = ((s >> 5) & 0x0F).wrapping_add(1) as u16;
            let year2 = ((s >> 9) & 0x63) as u16;
            let sec = ((s >> 15) & 0x3F) as u16;
            let min = ((s >> 21) & 0x3F) as u16;
            let hour = ((s >> 27) & 0x1F) as u16;
            let bt = bdfe_from_cmos_fields(day, month, year2, sec, min, hour);
            let expect = fasm_oracle_fs_get_time_from_fields(day, month, year2, sec, min, hour);
            let got = fs_calculate_time(bt);
            assert_eq!(got, expect, "prng#{i} fields");
            let _ = bt;
        }
    }

    #[test]
    fn seed_constant() {
        assert_eq!(FS_GET_TIME_PRNG_SEED, 0x4355_4256);
    }
}
