//! Cut BO: `xfs._.get_last_dirblock` — locate the last data directory block.
//!
//! Matches the FASM leaf in `kernel/fs/xfs.asm`:
//! 1. load `di_nextents` big-endian dword from the inode at `nextents_offset`
//! 2. compute `last_rec = inode + inode_core_size + nextents*16 - 16`
//! 3. unpack only the fields needed from that final `xfs_bmbt_rec`
//! 4. return `br_startoff + ((br_blockcount >> dirblklog) - 1)` in `EDX:EAX`
//!
//! The implementation intentionally preserves the legacy quirks:
//! - `nextents` is read as a full big-endian dword
//! - record offset arithmetic wraps in 32 bits
//! - shift counts are masked to 5 bits like x86 `shr`
//! - `blockcount == 0` underflows after `dec eax`
//!
//! No tables / GOT / external calls — reloc-free blob.

/// Cut BO differential PRNG seed (`'CUBO'`).
pub const XFS_GET_LAST_DIRBLOCK_PRNG_SEED: u32 = 0x4355_424F;

const XFS_BMBT_REC_SIZE_SHIFT: u32 = 4;
const XFS_BMBT_REC_SIZE: u32 = 1 << XFS_BMBT_REC_SIZE_SHIFT;

#[inline(always)]
fn read_be_u32(ptr: *const u8) -> u32 {
    // SAFETY: caller guarantees four readable bytes.
    unsafe { u32::from_be_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) }
}

/// FASM-faithful decode of the final directory data block from one packed extent.
#[inline(always)]
pub fn xfs_get_last_dirblock_from_extent(rec: &[u8; 16], dirblklog: u32) -> (u32, u32) {
    let mut edx = u32::from_be_bytes([rec[0], rec[1], rec[2], rec[3]]) & 0x7fff_ffff;
    let eax = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);
    let startoff_lo = (eax >> 9) | (edx << 23);
    edx >>= 9;
    let startoff_hi = edx;

    let blockcount = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]) & 0x001f_ffff;
    let last_in_extent = (blockcount >> (dirblklog & 31)).wrapping_sub(1);
    let (lo, carry) = startoff_lo.overflowing_add(last_in_extent);
    let hi = startoff_hi.wrapping_add(u32::from(carry));
    (lo, hi)
}

/// FASM-faithful `xfs._.get_last_dirblock`.
#[inline(always)]
pub fn xfs_get_last_dirblock(
    inode: *const u8,
    nextents_offset: u32,
    inode_core_size: u32,
    dirblklog: u32,
) -> (u32, u32) {
    let nextents = read_be_u32((inode as usize).wrapping_add(nextents_offset as usize) as *const u8);
    let rec_off = inode_core_size
        .wrapping_add(nextents.wrapping_shl(XFS_BMBT_REC_SIZE_SHIFT))
        .wrapping_sub(XFS_BMBT_REC_SIZE);
    let rec_ptr = (inode as usize).wrapping_add(rec_off as usize) as *const u8;
    let rec = [
        read_be_byte(rec_ptr, 0),
        read_be_byte(rec_ptr, 1),
        read_be_byte(rec_ptr, 2),
        read_be_byte(rec_ptr, 3),
        read_be_byte(rec_ptr, 4),
        read_be_byte(rec_ptr, 5),
        read_be_byte(rec_ptr, 6),
        read_be_byte(rec_ptr, 7),
        read_be_byte(rec_ptr, 8),
        read_be_byte(rec_ptr, 9),
        read_be_byte(rec_ptr, 10),
        read_be_byte(rec_ptr, 11),
        read_be_byte(rec_ptr, 12),
        read_be_byte(rec_ptr, 13),
        read_be_byte(rec_ptr, 14),
        read_be_byte(rec_ptr, 15),
    ];
    xfs_get_last_dirblock_from_extent(&rec, dirblklog)
}

#[inline(always)]
fn read_be_byte(ptr: *const u8, off: usize) -> u8 {
    // SAFETY: caller guarantees the extent record bytes are readable.
    unsafe { *ptr.add(off) }
}

/// Pointer-friendly wrapper for the FFI trampoline.
///
/// # Safety
/// `inode` must point to a readable inode buffer; `out_hi` must be writable.
#[inline(always)]
pub unsafe fn xfs_get_last_dirblock_ptr(
    inode: *const u8,
    nextents_offset: u32,
    inode_core_size: u32,
    dirblklog: u32,
    out_hi: *mut u32,
) -> u32 {
    let (lo, hi) = xfs_get_last_dirblock(inode, nextents_offset, inode_core_size, dirblklog);
    // SAFETY: trampoline passes a live writable stack slot.
    unsafe {
        *out_hi = hi;
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_extent(startoff: u64, blockcount: u32) -> [u8; 16] {
        let mut be = [0u8; 16];
        let startoff_hi = (startoff >> 32) as u32;
        let startoff_lo = startoff as u32;
        let word0 = (startoff_hi << 9) | (startoff_lo >> 23);
        let word1 = (startoff_lo << 9) & 0xffff_fe00;
        let word3 = blockcount & 0x001f_ffff;
        be[0..4].copy_from_slice(&word0.to_be_bytes());
        be[4..8].copy_from_slice(&word1.to_be_bytes());
        be[8..12].copy_from_slice(&0u32.to_be_bytes());
        be[12..16].copy_from_slice(&word3.to_be_bytes());
        be
    }

    fn oracle_extent(rec: &[u8; 16], dirblklog: u32) -> (u32, u32) {
        let mut edx = u32::from_be_bytes([rec[0], rec[1], rec[2], rec[3]]);
        edx &= 0x7fff_ffff;
        let eax = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);
        let mut start_lo = eax;
        let mut start_hi = edx;
        start_lo = (start_lo >> 9) | (start_hi << 23);
        start_hi >>= 9;

        let mut eax2 = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]);
        eax2 &= 0x001f_ffff;
        eax2 >>= dirblklog & 31;
        eax2 = eax2.wrapping_sub(1);
        let (lo, carry) = start_lo.overflowing_add(eax2);
        let hi = start_hi.wrapping_add(u32::from(carry));
        (lo, hi)
    }

    fn oracle_inode(
        inode: &[u8],
        nextents_offset: u32,
        inode_core_size: u32,
        dirblklog: u32,
    ) -> (u32, u32) {
        let off = nextents_offset as usize;
        let nextents =
            u32::from_be_bytes([inode[off], inode[off + 1], inode[off + 2], inode[off + 3]]);
        let rec_off = inode_core_size
            .wrapping_add(nextents.wrapping_shl(XFS_BMBT_REC_SIZE_SHIFT))
            .wrapping_sub(XFS_BMBT_REC_SIZE) as usize;
        let mut rec = [0u8; 16];
        rec.copy_from_slice(&inode[rec_off..rec_off + 16]);
        oracle_extent(&rec, dirblklog)
    }

    fn build_inode(nextents_offset: usize, inode_core_size: usize, extents: &[[u8; 16]]) -> Vec<u8> {
        let len = inode_core_size + extents.len() * 16 + 32;
        let mut inode = vec![0xA5; len];
        let nextents = (extents.len() as u32).to_be_bytes();
        inode[nextents_offset..nextents_offset + 4].copy_from_slice(&nextents);
        for (idx, rec) in extents.iter().enumerate() {
            let off = inode_core_size + idx * 16;
            inode[off..off + 16].copy_from_slice(rec);
        }
        inode
    }

    fn check_inode(
        nextents_offset: u32,
        inode_core_size: u32,
        dirblklog: u32,
        extents: &[[u8; 16]],
    ) {
        let inode = build_inode(nextents_offset as usize, inode_core_size as usize, extents);
        let got = xfs_get_last_dirblock(inode.as_ptr(), nextents_offset, inode_core_size, dirblklog);
        let exp = oracle_inode(&inode, nextents_offset, inode_core_size, dirblklog);
        assert_eq!(got, exp);
    }

    #[test]
    fn single_extent_block_directory_returns_zero() {
        check_inode(8, 64, 12, &[pack_extent(0, 4096)]);
    }

    #[test]
    fn single_extent_leaf_directory_returns_last_leaf_block() {
        check_inode(4, 80, 9, &[pack_extent(100, 1024)]);
    }

    #[test]
    fn last_extent_is_used() {
        check_inode(
            12,
            96,
            10,
            &[pack_extent(1, 2048), pack_extent(5000, 3072), pack_extent(9000, 1024)],
        );
    }

    #[test]
    fn shift_count_masks_to_31() {
        check_inode(0, 64, 32, &[pack_extent(123, 17)]);
        check_inode(0, 64, 33, &[pack_extent(123, 17)]);
    }

    #[test]
    fn blockcount_zero_underflows_like_fasm() {
        check_inode(0, 64, 0, &[pack_extent(7, 0)]);
    }

    #[test]
    fn carries_into_high_dword() {
        check_inode(0, 64, 0, &[pack_extent(0xFFFF_FFFF, 2)]);
    }

    #[test]
    fn direct_extent_helper_matches_oracle() {
        let rec = pack_extent(0x1234_5678_9ABC_DEF0, 0x1F_FFFF);
        assert_eq!(
            xfs_get_last_dirblock_from_extent(&rec, 3),
            oracle_extent(&rec, 3)
        );
    }

    #[test]
    fn prng_50k_cubo() {
        let mut s = XFS_GET_LAST_DIRBLOCK_PRNG_SEED;
        for _ in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let nextents_offset = (s % 32) & !3;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let dirblklog = s % 40;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let extent_count = (s % 4 + 1) as usize;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let inode_core_size = ((s % 64) & !3) + 64;

            let mut extents = Vec::with_capacity(extent_count);
            for _ in 0..extent_count {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                let start_lo = s;
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                let start_hi = s & 0x007f_ffff;
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                let blockcount = s & 0x001f_ffff;
                extents.push(pack_extent(
                    ((start_hi as u64) << 32) | start_lo as u64,
                    blockcount,
                ));
            }
            check_inode(nextents_offset, inode_core_size, dirblklog, &extents);
        }
    }

    #[test]
    fn ptr_writes_high_dword() {
        let inode = build_inode(0, 64, &[pack_extent(0xFFFF_FFFF, 2)]);
        let mut hi = 0u32;
        let lo =
            unsafe { xfs_get_last_dirblock_ptr(inode.as_ptr(), 0, 64, 0, &mut hi) };
        let exp = oracle_inode(&inode, 0, 64, 0);
        assert_eq!((lo, hi), exp);
    }
}
