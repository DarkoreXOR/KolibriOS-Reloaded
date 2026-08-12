//! Cut BT: `ntfsGetTime` pack — KOS seconds (since 2001) → NTFS FILETIME.
//!
//! Matches the FASM leaf tail in `kernel/fs/ntfs.inc` after `fsGetTime`:
//! `mov edx, 10000000` / `mul edx` / bias `add`/`adc`.
//! CMOS read stays in FASM (`call fsGetTime` before Rust).

use crate::time::filetime_from_secs_2001;

/// Cut BT differential PRNG seed (`'CUBT'`).
pub const NTFS_GET_TIME_PRNG_SEED: u32 = 0x4355_4254;

/// Pack KOS seconds since 2001-01-01 into NTFS FILETIME (EDX:EAX).
#[inline(always)]
pub fn ntfs_get_time_pack(kos_secs: u32) -> (u32, u32) {
    filetime_from_secs_2001(kos_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{ntfs_calculate_time, BdfeTime, NTFS_FILETIME_BIAS_HI, NTFS_FILETIME_BIAS_LO, NTFS_FILETIME_PER_SEC};

    /// Structurally independent FASM oracle — literal constants, register flow.
    fn fasm_oracle_ntfs_get_time_pack(kos_secs: u32) -> (u32, u32) {
        let mut eax = kos_secs;
        let mut edx = 10_000_000u32;
        let product = (eax as u64).wrapping_mul(edx as u64);
        eax = product as u32;
        edx = (product >> 32) as u32;
        let bias_lo = 3365781504u32;
        let bias_hi = 29389701u32;
        let (sum, carry) = eax.overflowing_add(bias_lo);
        eax = sum;
        edx = edx.wrapping_add(bias_hi).wrapping_add(u32::from(carry));
        (eax, edx)
    }

    fn check_pack(kos_secs: u32) {
        let (lo, hi) = ntfs_get_time_pack(kos_secs);
        let (elo, ehi) = fasm_oracle_ntfs_get_time_pack(kos_secs);
        assert_eq!(lo, elo, "kos_secs={kos_secs:#x} lo");
        assert_eq!(hi, ehi, "kos_secs={kos_secs:#x} hi");
    }

    #[test]
    fn epoch_zero_bias() {
        let (lo, hi) = ntfs_get_time_pack(0);
        assert_eq!(lo, NTFS_FILETIME_BIAS_LO);
        assert_eq!(hi, NTFS_FILETIME_BIAS_HI);
        check_pack(0);
    }

    #[test]
    fn one_second_step() {
        let (lo, hi) = ntfs_get_time_pack(1);
        let (elo, ehi) = ntfs_get_time_pack(0);
        assert_eq!(lo, elo.wrapping_add(NTFS_FILETIME_PER_SEC));
        assert_eq!(hi, ehi); // no carry at +1s
        check_pack(1);
    }

    #[test]
    fn matches_calculate_time_composition() {
        // ntfsGetTime(secs) == ntfsCalculateTime(BDFE) when secs = fsCalculateTime(BDFE)
        let t = BdfeTime {
            sec: 0,
            min: 0,
            hour: 0,
            day: 1,
            month: 1,
            year: 2001,
        };
        let kos = crate::time::fs_calculate_time(t);
        let (g_lo, g_hi) = ntfs_get_time_pack(kos);
        let (c_lo, c_hi) = ntfs_calculate_time(t);
        assert_eq!(g_lo, c_lo);
        assert_eq!(g_hi, c_hi);
    }

    #[test]
    fn max_u32_secs() {
        check_pack(0xFFFF_FFFF);
    }

    #[test]
    fn carry_into_hi_word() {
        // Force adc into hi after bias add
        check_pack(0xFFFF_FFFF - NTFS_FILETIME_BIAS_LO);
    }

    #[test]
    fn prng_50k_cubt() {
        let mut s = NTFS_GET_TIME_PRNG_SEED;
        for i in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let kos_secs = s;
            let (lo, hi) = ntfs_get_time_pack(kos_secs);
            let (elo, ehi) = fasm_oracle_ntfs_get_time_pack(kos_secs);
            assert_eq!(lo, elo, "prng#{i} kos={kos_secs:#x} lo");
            assert_eq!(hi, ehi, "prng#{i} kos={kos_secs:#x} hi");
        }
    }
}
