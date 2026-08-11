//! Cut AW: `xfs._.blkrel2sectabs` — XFS AG-relative block → absolute sector.
//!
//! Matches `kernel/fs/xfs.asm` FASM leaf semantics:
//! 1. `ag = (edx:eax) >> agblklog` (x86 shift count masked to 5 bits)
//! 2. `mul agblocks` uses **only EAX** (AG# high dword discarded — legacy quirk)
//! 3. add `(block & agblockmask)` to the 64-bit product
//! 4. `<<= sectpblog` (again `& 31`) → absolute sector in `edx:eax`
//!
//! No tables / GOT / external calls — reloc-free blob.

/// Cut AW differential PRNG seed (`'CUTW'`).
pub const XFS_BLKREL2SECTABS_PRNG_SEED: u32 = 0x4355_5457;

/// FASM-faithful AG-relative block → absolute sector translation.
///
/// Returns `(sector_lo, sector_hi)`.
#[inline(always)]
pub fn xfs_blkrel2sectabs(
    block_lo: u32,
    block_hi: u32,
    agblklog: u32,
    agblocks: u32,
    mask_lo: u32,
    mask_hi: u32,
    sectpblog: u32,
) -> (u32, u32) {
    // x86 32-bit SHRD/SHR/SHL/SHLD: count masked to 5 bits.
    let ag_shift = agblklog & 31;
    let sect_shift = sectpblog & 31;

    // shrd eax, edx, cl / shr edx, cl
    let block = ((block_hi as u64) << 32) | (block_lo as u64);
    let ag_full = block >> ag_shift;
    // mul [agblocks] uses only EAX — drop AG# bits 32..63
    let ag_lo = ag_full as u32;
    let product = (ag_lo as u64).wrapping_mul(agblocks as u64);

    let masked =
        ((block_lo & mask_lo) as u64) | (((block_hi & mask_hi) as u64) << 32);
    let abs_block = product.wrapping_add(masked);

    // shld edx, eax, cl / shl eax, cl
    let sector = abs_block << sect_shift;
    (sector as u32, (sector >> 32) as u32)
}

/// Pointer-free naming symmetry (same as pure helper).
#[inline(always)]
pub fn xfs_blkrel2sectabs_from_regs(
    block_lo: u32,
    block_hi: u32,
    agblklog: u32,
    agblocks: u32,
    mask_lo: u32,
    mask_hi: u32,
    sectpblog: u32,
) -> (u32, u32) {
    xfs_blkrel2sectabs(
        block_lo, block_hi, agblklog, agblocks, mask_lo, mask_hi, sectpblog,
    )
}

/// FFI helper: write sector_hi to `out_hi`, return sector_lo.
///
/// # Safety
/// `out_hi` must be writable.
#[inline(always)]
pub unsafe fn xfs_blkrel2sectabs_ptr(
    block_lo: u32,
    block_hi: u32,
    agblklog: u32,
    agblocks: u32,
    mask_lo: u32,
    mask_hi: u32,
    sectpblog: u32,
    out_hi: *mut u32,
) -> u32 {
    let (lo, hi) = xfs_blkrel2sectabs(
        block_lo, block_hi, agblklog, agblocks, mask_lo, mask_hi, sectpblog,
    );
    // SAFETY: trampoline passes a live stack slot.
    unsafe {
        *out_hi = hi;
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not derived from the Rust helper body).
    fn oracle(
        block_lo: u32,
        block_hi: u32,
        agblklog: u32,
        agblocks: u32,
        mask_lo: u32,
        mask_hi: u32,
        sectpblog: u32,
    ) -> (u32, u32) {
        let cl = (agblklog & 31) as u8;
        // Simulate edx:eax = block, then shrd eax,edx,cl / shr edx,cl
        let mut eax = block_lo;
        let mut edx = block_hi;
        if cl != 0 {
            let new_eax = (eax >> cl) | (edx << (32 - cl));
            let new_edx = edx >> cl;
            eax = new_eax;
            edx = new_edx;
        }
        // mul [agblocks] — EDX overwritten with product hi; EAX=ag ignored hi
        let _ = edx; // discarded by mul (legacy)
        let product = (eax as u64).wrapping_mul(agblocks as u64);
        eax = product as u32;
        edx = (product >> 32) as u32;

        let ecx = block_lo & mask_lo;
        let esi = block_hi & mask_hi;
        let (sum, carry) = eax.overflowing_add(ecx);
        eax = sum;
        edx = edx.wrapping_add(esi).wrapping_add(u32::from(carry));

        let cl2 = (sectpblog & 31) as u8;
        if cl2 != 0 {
            let new_edx = (edx << cl2) | (eax >> (32 - cl2));
            let new_eax = eax << cl2;
            eax = new_eax;
            edx = new_edx;
        }
        (eax, edx)
    }

    fn check(
        block_lo: u32,
        block_hi: u32,
        agblklog: u32,
        agblocks: u32,
        mask_lo: u32,
        mask_hi: u32,
        sectpblog: u32,
    ) {
        let got = xfs_blkrel2sectabs(
            block_lo, block_hi, agblklog, agblocks, mask_lo, mask_hi, sectpblog,
        );
        let exp = oracle(
            block_lo, block_hi, agblklog, agblocks, mask_lo, mask_hi, sectpblog,
        );
        assert_eq!(
            got, exp,
            "mismatch block={block_hi:08x}:{block_lo:08x} agblklog={agblklog} \
             agblocks={agblocks} mask={mask_hi:08x}:{mask_lo:08x} \
             sectpblog={sectpblog} got={got:?} exp={exp:?}"
        );
    }

    fn mask_from_agblklog(agblklog: u32) -> (u32, u32) {
        // FASM: eax=1; edx=0; shld/shl by (agblklog&31); sub 1
        let cl = agblklog & 31;
        if cl == 0 {
            // 1<<0 - 1 = 0
            return (0, 0);
        }
        let one: u64 = 1u64 << cl;
        let m = one.wrapping_sub(1);
        (m as u32, (m >> 32) as u32)
    }

    #[test]
    fn zero_block_zero_params() {
        check(0, 0, 0, 0, 0, 0, 0);
        check(0, 0, 10, 100, 0x3ff, 0, 3);
    }

    #[test]
    fn ag0_relative_only() {
        // agblklog=10 → mask=0x3ff; block within AG0
        let (ml, mh) = mask_from_agblklog(10);
        check(0x123, 0, 10, 1024, ml, mh, 3);
        check(0x3ff, 0, 10, 1024, ml, mh, 3);
    }

    #[test]
    fn first_ag_boundary() {
        let (ml, mh) = mask_from_agblklog(10);
        // AG1 block 0: bitfield = 1 << 10
        check(0x400, 0, 10, 1024, ml, mh, 3);
        // AG1 block 5
        check(0x405, 0, 10, 1024, ml, mh, 0);
    }

    #[test]
    fn sectpblog_zero_and_nonzero() {
        let (ml, mh) = mask_from_agblklog(8);
        check(0x101, 0, 8, 256, ml, mh, 0);
        check(0x101, 0, 8, 256, ml, mh, 3);
        check(0x101, 0, 8, 256, ml, mh, 9);
    }

    #[test]
    fn shift_count_mask_31_quirk() {
        // agblklog=32 → &31 = 0 → no AG extract shift
        check(0xABCD, 0x11, 32, 7, 0xffff, 0, 3);
        // sectpblog=32 → &31 = 0 → no sector shift
        let (ml, mh) = mask_from_agblklog(4);
        check(0x15, 0, 4, 16, ml, mh, 32);
    }

    #[test]
    fn mul_discards_ag_high() {
        // After shift, AG# hi bits discarded: block with hi bits set, agblklog=0
        // → ag_full = block, but mul uses only lo
        check(0x10, 0xABCD_0000, 0, 3, 0, 0, 1);
    }

    #[test]
    fn high_mask_bits() {
        // agblklog=40 → &31=8; mask from full FASM uses unmasked cl for mask gen
        // in mount — but leaf uses stored mask dwords as-is.
        check(
            0xFFFF_FFFF,
            0x00FF,
            12,
            4096,
            0xFFFF_FFFF,
            0x000F,
            3,
        );
    }

    #[test]
    fn named_vectors_match_oracle() {
        let cases = [
            (0u32, 0u32, 10u32, 1024u32, 0x3ffu32, 0u32, 3u32),
            (1, 0, 10, 1024, 0x3ff, 0, 3),
            (0x400, 0, 10, 1024, 0x3ff, 0, 3),
            (0x7ff, 0, 10, 1024, 0x3ff, 0, 0),
            (0, 1, 20, 100, 0xfffff, 0, 3),
            (0xDEAD_BEEF, 0x1, 15, 0x8000, 0x7fff, 0, 3),
        ];
        for (lo, hi, al, ab, ml, mh, sl) in cases {
            check(lo, hi, al, ab, ml, mh, sl);
        }
    }

    #[test]
    fn prng_50k_cutw() {
        let mut s = XFS_BLKREL2SECTABS_PRNG_SEED;
        for _ in 0..50_000 {
            // xorshift32
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let block_lo = s;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let block_hi = s & 0xFFFF; // keep domain plausible
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let agblklog = s % 33; // include 32 for &31 quirk
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let agblocks = (s % 0x1_0000).max(1);
            let (ml, mh) = mask_from_agblklog(agblklog);
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let sectpblog = s % 33;
            check(
                block_lo, block_hi, agblklog, agblocks, ml, mh, sectpblog,
            );
        }
    }

    #[test]
    fn ptr_out_hi() {
        let mut hi = 0u32;
        let lo = unsafe {
            xfs_blkrel2sectabs_ptr(0x405, 0, 10, 1024, 0x3ff, 0, 3, &mut hi)
        };
        let (e_lo, e_hi) = oracle(0x405, 0, 10, 1024, 0x3ff, 0, 3);
        assert_eq!(lo, e_lo);
        assert_eq!(hi, e_hi);
    }
}
