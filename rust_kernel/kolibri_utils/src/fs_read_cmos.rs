//! Cut CA: `fsReadCMOS` — CMOS port read + BCD decode leaf.
//!
//! Matches `kernel/fs/fs_common.inc` FASM `fsReadCMOS`:
//! * `out 0x70, al` / `in al, 0x71` via injected callback
//! * BCD unpack: `xor ah,ah` / `shl ax,4` / `shr al,4` / `aad`
//!
//! Port I/O stays in FASM (`fs_cmos_raw_read_stdcall`); Rust owns decode only.

use crate::fs_get_time::fs_read_cmos_bcd;

/// Cut CA differential PRNG seed (`'CUTC'`).
pub const FS_READ_CMOS_PRNG_SEED: u32 = 0x4355_5443;

/// Stdcall wrapper around FASM `fs_cmos_raw_read_stdcall` (reg → raw byte in AL).
pub type FsCmosRawReadFn = unsafe extern "stdcall" fn(reg: u32) -> u32;

/// Independent FASM-flow oracle (BCD decode without port I/O).
#[inline(always)]
pub fn fasm_oracle_fs_read_cmos(raw: u8) -> u16 {
    fs_read_cmos_bcd(raw)
}

/// Decode one CMOS register via injected raw port reader.
#[inline(always)]
pub fn fs_read_cmos(raw_read: FsCmosRawReadFn, reg: u8) -> u16 {
    let raw = unsafe { raw_read(u32::from(reg)) } as u8;
    fs_read_cmos_bcd(raw)
}

#[inline(always)]
pub unsafe fn fs_read_cmos_ptr(raw_read: FsCmosRawReadFn, reg: u32) -> u32 {
    u32::from(fs_read_cmos(raw_read, reg as u8))
}

/// Merge decoded AX into caller EAX preserving upper 16 bits (FASM quirk).
#[inline(always)]
pub fn merge_upper_eax(original_eax: u32, decoded_ax: u16) -> u32 {
    (original_eax & 0xFFFF_0000) | u32::from(decoded_ax)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_get_time::fs_read_cmos_bcd;

    fn mock_raw(reg: u32) -> u32 {
        match (reg & 0xFF) as u8 {
            0 => 0x30,
            2 => 0x45,
            4 => 0x12,
            7 => 0x15,
            8 => 0x06,
            9 => 0x26,
            _ => 0x00,
        }
    }

    unsafe extern "stdcall" fn mock_raw_fn(reg: u32) -> u32 {
        mock_raw(reg)
    }

    #[test]
    fn oracle_matches_bcd_helper() {
        for raw in 0u8..=0x99 {
            assert_eq!(
                fasm_oracle_fs_read_cmos(raw),
                fs_read_cmos_bcd(raw),
                "raw={raw}"
            );
        }
    }

    #[test]
    fn callback_path_samples() {
        assert_eq!(fs_read_cmos(mock_raw_fn, 7), 15);
        assert_eq!(fs_read_cmos(mock_raw_fn, 9), 26);
        assert_eq!(fs_read_cmos(mock_raw_fn, 0), 30);
    }

    #[test]
    fn upper_eax_merge_quirk() {
        let orig = 0xAABB_0007;
        let merged = merge_upper_eax(orig, 15);
        assert_eq!(merged, 0xAABB_000F);
    }

    #[test]
    fn prng_differential_50k() {
        let mut s = FS_READ_CMOS_PRNG_SEED;
        for _ in 0..50_000 {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let raw = (s >> 16) as u8;
            let expect = fasm_oracle_fs_read_cmos(raw);
            let got = fs_read_cmos_bcd(raw);
            assert_eq!(got, expect, "raw={raw:#04x}");
        }
    }

    #[test]
    fn seed_constant() {
        assert_eq!(FS_READ_CMOS_PRNG_SEED, 0x4355_5443);
    }
}
