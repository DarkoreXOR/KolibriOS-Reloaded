//! Cut CK: `fat_get_sector` — FAT cluster/offset → absolute sector.
//!
//! Matches `kernel/fs/fat.inc` FASM leaf:
//! ```text
//!   push ecx
//!   ecx = [eax]          ; cluster
//!   dec ecx / dec ecx    ; cluster - 2
//!   imul ecx, [ebp+FAT.SECTORS_PER_CLUSTER]
//!   add ecx, [ebp+FAT.DATA_START]
//!   add ecx, [eax+4]     ; sector_in_cluster
//!   eax = ecx
//!   pop ecx
//! ```
//!
//! 32-bit `imul` wrap and unsigned underflow for `cluster < 2` are retained.

/// Cut CK differential PRNG seed (`'FSEC'`).
pub const FAT_GET_SECTOR_PRNG_SEED: u32 = 0x4653_4543;

/// FASM-faithful cluster/offset → absolute sector (32-bit wrap).
#[inline(always)]
pub fn fat_get_sector(
    cluster: u32,
    sector_ofs: u32,
    sectors_per_cluster: u32,
    data_start: u32,
) -> u32 {
    // Match `dec ecx; dec ecx` then `imul ecx, spc` (truncate to 32 bits).
    let mut ecx = cluster.wrapping_sub(2);
    ecx = ecx.wrapping_mul(sectors_per_cluster);
    ecx = ecx.wrapping_add(data_start);
    ecx.wrapping_add(sector_ofs)
}

/// Pointer pair helper used by the stdcall FFI export.
///
/// # Safety
/// `pair` must point to two consecutive `u32` values: cluster, sector_ofs.
#[inline(always)]
pub unsafe fn fat_get_sector_ptr(
    pair: *const u32,
    sectors_per_cluster: u32,
    data_start: u32,
) -> u32 {
    let cluster = unsafe { *pair };
    let sector_ofs = unsafe { *pair.add(1) };
    fat_get_sector(cluster, sector_ofs, sectors_per_cluster, data_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent oracle: i64 arithmetic truncated to u32.
    /// Must not duplicate the Rust wrapping_sub/mul sequencing.
    fn oracle_fat_get_sector(
        cluster: u32,
        sector_ofs: u32,
        sectors_per_cluster: u32,
        data_start: u32,
    ) -> u32 {
        let sector = (cluster as i64 - 2)
            .wrapping_mul(sectors_per_cluster as i64)
            .wrapping_add(data_start as i64)
            .wrapping_add(sector_ofs as i64);
        sector as u32
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
    fn fsec_cluster2_ofs0() {
        assert_eq!(fat_get_sector(2, 0, 8, 0x20), 0x20);
        assert_eq!(oracle_fat_get_sector(2, 0, 8, 0x20), 0x20);
    }

    #[test]
    fn fsec_cluster3_ofs1() {
        // (3-2)*8 + 0x20 + 1 = 0x29
        assert_eq!(fat_get_sector(3, 1, 8, 0x20), 0x29);
        assert_eq!(oracle_fat_get_sector(3, 1, 8, 0x20), 0x29);
    }

    #[test]
    fn fsec_cluster_underflow() {
        // cluster 0 → (-2)*spc wrapping
        let got = fat_get_sector(0, 0, 1, 0);
        let exp = oracle_fat_get_sector(0, 0, 1, 0);
        assert_eq!(got, exp);
        assert_eq!(got, 0u32.wrapping_sub(2));
    }

    #[test]
    fn fsec_cluster1_underflow() {
        let got = fat_get_sector(1, 5, 4, 10);
        let exp = oracle_fat_get_sector(1, 5, 4, 10);
        assert_eq!(got, exp);
    }

    #[test]
    fn fsec_max_wrap() {
        let got = fat_get_sector(u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        let exp = oracle_fat_get_sector(u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        assert_eq!(got, exp);
    }

    #[test]
    fn fsec_ofs_last_in_cluster() {
        // spc=64, ofs=63, cluster=100, data=512
        // (100-2)*64 + 512 + 63 = 98*64 + 575 = 6272 + 575 = 6847
        assert_eq!(fat_get_sector(100, 63, 64, 512), 6847);
        assert_eq!(oracle_fat_get_sector(100, 63, 64, 512), 6847);
    }

    #[test]
    fn fsec_ptr_pair() {
        let pair = [7u32, 3u32];
        let got = unsafe { fat_get_sector_ptr(pair.as_ptr(), 16, 100) };
        // (7-2)*16 + 100 + 3 = 80 + 100 + 3 = 183
        assert_eq!(got, 183);
        assert_eq!(oracle_fat_get_sector(7, 3, 16, 100), 183);
    }

    #[test]
    fn fsec_spc1_identity_shift() {
        assert_eq!(fat_get_sector(10, 0, 1, 50), 58);
        assert_eq!(oracle_fat_get_sector(10, 0, 1, 50), 58);
    }

    #[test]
    fn fsec_zero_spc() {
        // imul by 0 → only data_start + ofs
        assert_eq!(fat_get_sector(99, 7, 0, 1000), 1007);
        assert_eq!(oracle_fat_get_sector(99, 7, 0, 1000), 1007);
    }

    #[test]
    fn fsec_prng_50000_matches_oracle() {
        let mut state = FAT_GET_SECTOR_PRNG_SEED;
        for _ in 0..50_000 {
            let cluster = xorshift32(&mut state);
            let ofs = xorshift32(&mut state);
            let spc = xorshift32(&mut state);
            let data = xorshift32(&mut state);
            let got = fat_get_sector(cluster, ofs, spc, data);
            let exp = oracle_fat_get_sector(cluster, ofs, spc, data);
            assert_eq!(
                got, exp,
                "mismatch cluster={cluster:#x} ofs={ofs:#x} spc={spc:#x} data={data:#x}"
            );
        }
    }
}
