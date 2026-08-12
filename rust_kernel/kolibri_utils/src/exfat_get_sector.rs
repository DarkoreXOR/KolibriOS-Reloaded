//! Cut CL: `exFAT_get_sector` — exFAT cluster/offset → absolute sector.
//!
//! Matches `kernel/fs/exfat.inc` FASM leaf:
//! ```text
//!   push ecx
//!   ecx = [eax]          ; cluster
//!   dec ecx / dec ecx    ; cluster - 2
//!   imul ecx, [ebp+exFAT.SECTORS_PER_CLUSTER]
//!   add ecx, [ebp+exFAT.CLUSTER_HEAP_START]
//!   add ecx, [eax+4]     ; sector_in_cluster
//!   eax = ecx
//!   pop ecx
//! ```
//!
//! Same 32-bit wrap semantics as Cut CK `fat_get_sector`; the production field
//! is `CLUSTER_HEAP_START` rather than FAT `DATA_START`.

use crate::fat_get_sector::fat_get_sector;

/// Cut CL differential PRNG seed (`'ESEC'`).
pub const EXFAT_GET_SECTOR_PRNG_SEED: u32 = 0x4553_4543;

/// FASM-faithful cluster/offset → absolute sector (32-bit wrap).
///
/// `cluster_heap_start` is the exFAT `CLUSTER_HEAP_START` field (not FAT
/// `DATA_START`). Math is identical to [`fat_get_sector`].
#[inline(always)]
pub fn exfat_get_sector(
    cluster: u32,
    sector_ofs: u32,
    sectors_per_cluster: u32,
    cluster_heap_start: u32,
) -> u32 {
    fat_get_sector(cluster, sector_ofs, sectors_per_cluster, cluster_heap_start)
}

/// Pointer pair helper used by the stdcall FFI export.
///
/// # Safety
/// `pair` must point to two consecutive `u32` values: cluster, sector_ofs.
#[inline(always)]
pub unsafe fn exfat_get_sector_ptr(
    pair: *const u32,
    sectors_per_cluster: u32,
    cluster_heap_start: u32,
) -> u32 {
    let cluster = unsafe { *pair };
    let sector_ofs = unsafe { *pair.add(1) };
    exfat_get_sector(cluster, sector_ofs, sectors_per_cluster, cluster_heap_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent oracle: i64 arithmetic truncated to u32.
    /// Must not duplicate the Rust wrapping_sub/mul sequencing.
    fn oracle_exfat_get_sector(
        cluster: u32,
        sector_ofs: u32,
        sectors_per_cluster: u32,
        cluster_heap_start: u32,
    ) -> u32 {
        let sector = (cluster as i64 - 2)
            .wrapping_mul(sectors_per_cluster as i64)
            .wrapping_add(cluster_heap_start as i64)
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
    fn esec_cluster2_ofs0() {
        assert_eq!(exfat_get_sector(2, 0, 8, 0x40), 0x40);
        assert_eq!(oracle_exfat_get_sector(2, 0, 8, 0x40), 0x40);
    }

    #[test]
    fn esec_cluster3_ofs1() {
        // (3-2)*8 + 0x40 + 1 = 0x49
        assert_eq!(exfat_get_sector(3, 1, 8, 0x40), 0x49);
        assert_eq!(oracle_exfat_get_sector(3, 1, 8, 0x40), 0x49);
    }

    #[test]
    fn esec_cluster_underflow() {
        let got = exfat_get_sector(0, 0, 1, 0);
        let exp = oracle_exfat_get_sector(0, 0, 1, 0);
        assert_eq!(got, exp);
        assert_eq!(got, 0u32.wrapping_sub(2));
    }

    #[test]
    fn esec_cluster1_underflow() {
        let got = exfat_get_sector(1, 5, 4, 10);
        let exp = oracle_exfat_get_sector(1, 5, 4, 10);
        assert_eq!(got, exp);
    }

    #[test]
    fn esec_max_wrap() {
        let got = exfat_get_sector(u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        let exp = oracle_exfat_get_sector(u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        assert_eq!(got, exp);
    }

    #[test]
    fn esec_ofs_last_in_cluster() {
        // spc=64, ofs=63, cluster=100, heap=512
        // (100-2)*64 + 512 + 63 = 6847
        assert_eq!(exfat_get_sector(100, 63, 64, 512), 6847);
        assert_eq!(oracle_exfat_get_sector(100, 63, 64, 512), 6847);
    }

    #[test]
    fn esec_ptr_pair() {
        let pair = [7u32, 3u32];
        let got = unsafe { exfat_get_sector_ptr(pair.as_ptr(), 16, 100) };
        // (7-2)*16 + 100 + 3 = 183
        assert_eq!(got, 183);
        assert_eq!(oracle_exfat_get_sector(7, 3, 16, 100), 183);
    }

    #[test]
    fn esec_spc1_identity_shift() {
        assert_eq!(exfat_get_sector(10, 0, 1, 50), 58);
        assert_eq!(oracle_exfat_get_sector(10, 0, 1, 50), 58);
    }

    #[test]
    fn esec_zero_spc() {
        assert_eq!(exfat_get_sector(99, 7, 0, 1000), 1007);
        assert_eq!(oracle_exfat_get_sector(99, 7, 0, 1000), 1007);
    }

    #[test]
    fn esec_prng_50000_matches_oracle() {
        let mut state = EXFAT_GET_SECTOR_PRNG_SEED;
        for _ in 0..50_000 {
            let cluster = xorshift32(&mut state);
            let ofs = xorshift32(&mut state);
            let spc = xorshift32(&mut state);
            let heap = xorshift32(&mut state);
            let got = exfat_get_sector(cluster, ofs, spc, heap);
            let exp = oracle_exfat_get_sector(cluster, ofs, spc, heap);
            assert_eq!(
                got, exp,
                "mismatch cluster={cluster:#x} ofs={ofs:#x} spc={spc:#x} heap={heap:#x}"
            );
        }
    }
}
