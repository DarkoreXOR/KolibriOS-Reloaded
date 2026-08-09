//! Cut R: `xfs._.extent_unpack` — unpack one big-endian XFS `xfs_bmbt_rec`.
//!
//! Matches `kernel/fs/xfs.asm` FASM leaf semantics (`movbe` / `shrd` / masks)
//! writing into an `xfs_bmbt_irec` (the `XFS.extent` sub-object).
//!
//! No tables / `.rodata`. Freestanding path stores fields as explicit `u32`
//! writes — never calls `memset`/`memcpy` (reloc-free requirement).
//!
//! ## Layout (`xfs_bmbt_irec`, little-endian host memory)
//!
//! ```text
//! +0  br_startoff.lo     dd
//! +4  br_startoff.hi     dd
//! +8  br_startblock.lo   dd
//! +12 br_startblock.hi   dd
//! +16 br_blockcount      dd
//! +20 br_state           dd
//! sizeof = 24
//! ```
//!
//! The public kernel ABI keeps `EBP → XFS` and writes at `EBP+XFS.extent`;
//! the FASM trampoline passes `&XFS.extent` as an explicit pointer so this
//! module does not need `offsetof(XFS)`.

/// Unpacked extent fields matching FASM `xfs_bmbt_irec` / `XFS.extent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XfsBmbtIrec {
    pub br_startoff: u64,
    pub br_startblock: u64,
    pub br_blockcount: u32,
    pub br_state: u32,
}

/// Byte size of `xfs_bmbt_irec` in the KolibriOS FASM layout.
pub const XFS_BMBT_IREC_SIZE: usize = 24;

/// Offset of `br_startoff.lo` within `xfs_bmbt_irec`.
pub const OFF_STARTOFF_LO: usize = 0;
/// Offset of `br_startoff.hi` within `xfs_bmbt_irec`.
pub const OFF_STARTOFF_HI: usize = 4;
/// Offset of `br_startblock.lo` within `xfs_bmbt_irec`.
pub const OFF_STARTBLOCK_LO: usize = 8;
/// Offset of `br_startblock.hi` within `xfs_bmbt_irec`.
pub const OFF_STARTBLOCK_HI: usize = 12;
/// Offset of `br_blockcount` within `xfs_bmbt_irec`.
pub const OFF_BLOCKCOUNT: usize = 16;
/// Offset of `br_state` within `xfs_bmbt_irec`.
pub const OFF_STATE: usize = 20;

/// PRNG seed for differential / random corpus (`'CUXR'`).
pub const XFS_EXTENT_UNPACK_PRNG_SEED: u32 = 0x4355_5852;

/// Unpack one 16-byte big-endian `xfs_bmbt_rec` exactly as FASM does.
///
/// Input `rec` is the on-disk / in-memory packed record (byte 0 = MSB of the
/// 128-bit BE value). Output fields match FASM stores into `XFS.extent.*`.
#[inline(always)]
pub fn xfs_extent_unpack(rec: &[u8; 16]) -> XfsBmbtIrec {
    // movbe edx, [ebx+0]
    let mut edx = u32::from_be_bytes([rec[0], rec[1], rec[2], rec[3]]);

    // test edx, 0x80000000 / setnz al / zero-extend → br_state
    let br_state: u32 = if (edx & 0x8000_0000) != 0 { 1 } else { 0 };

    // and edx, 0x7fffffff
    edx &= 0x7fff_ffff;

    // movbe eax, [ebx+4]
    let mut eax = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);

    // shrd eax, edx, 9  →  eax = (eax >> 9) | (edx << 23)
    let startoff_lo = (eax >> 9) | (edx << 23);
    // shr edx, 9
    let startoff_hi = edx >> 9;

    // movbe edx, [ebx+4] ; movbe eax, [ebx+8] ; movbe ecx, [ebx+12]
    edx = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);
    eax = u32::from_be_bytes([rec[8], rec[9], rec[10], rec[11]]);
    let ecx = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]);

    // and edx, 0x000001ff
    edx &= 0x0000_01ff;

    // shrd ecx, eax, 21 → startblock.lo
    let startblock_lo = (ecx >> 21) | (eax << 11);
    // shrd eax, edx, 21 → startblock.hi
    let startblock_hi = (eax >> 21) | (edx << 11);

    // movbe eax, [ebx+12] ; and eax, 0x001fffff
    let br_blockcount = ecx & 0x001f_ffff;

    XfsBmbtIrec {
        br_startoff: (startoff_lo as u64) | ((startoff_hi as u64) << 32),
        br_startblock: (startblock_lo as u64) | ((startblock_hi as u64) << 32),
        br_blockcount,
        br_state,
    }
}

/// Write an unpacked irec into a 24-byte FASM `xfs_bmbt_irec` buffer.
///
/// Field store order matches FASM (state, startoff, startblock, blockcount)
/// only insofar as final memory contents are identical; each field is one
/// dword/`shrd` result written to its fixed offset.
#[inline(always)]
pub fn write_xfs_bmbt_irec(out: &mut [u8; XFS_BMBT_IREC_SIZE], irec: &XfsBmbtIrec) {
    write_u32_le(out, OFF_STATE, irec.br_state);
    write_u32_le(out, OFF_STARTOFF_LO, irec.br_startoff as u32);
    write_u32_le(out, OFF_STARTOFF_HI, (irec.br_startoff >> 32) as u32);
    write_u32_le(out, OFF_STARTBLOCK_LO, irec.br_startblock as u32);
    write_u32_le(out, OFF_STARTBLOCK_HI, (irec.br_startblock >> 32) as u32);
    write_u32_le(out, OFF_BLOCKCOUNT, irec.br_blockcount);
}

/// Unpack `rec` and store into a FASM-layout `xfs_bmbt_irec` at `out`.
#[inline(always)]
pub fn xfs_extent_unpack_into(rec: &[u8; 16], out: &mut [u8; XFS_BMBT_IREC_SIZE]) {
    let irec = xfs_extent_unpack(rec);
    write_xfs_bmbt_irec(out, &irec);
}

/// Pointer-friendly wrapper for the FFI trampoline.
///
/// # Safety
/// - `extent_data` must be readable for 16 bytes.
/// - `extent_out` must point to a writable 24-byte `xfs_bmbt_irec`
///   (typically `&XFS.extent` derived from the caller's EBP).
#[inline(always)]
pub unsafe fn xfs_extent_unpack_ptr(extent_data: *const u8, extent_out: *mut u8) {
    let mut rec = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        rec[i] = unsafe { *extent_data.add(i) };
        i += 1;
    }
    let irec = xfs_extent_unpack(&rec);

    // Explicit dword stores — no memset/memcpy.
    unsafe {
        write_u32_at(extent_out, OFF_STATE, irec.br_state);
        write_u32_at(extent_out, OFF_STARTOFF_LO, irec.br_startoff as u32);
        write_u32_at(extent_out, OFF_STARTOFF_HI, (irec.br_startoff >> 32) as u32);
        write_u32_at(extent_out, OFF_STARTBLOCK_LO, irec.br_startblock as u32);
        write_u32_at(extent_out, OFF_STARTBLOCK_HI, (irec.br_startblock >> 32) as u32);
        write_u32_at(extent_out, OFF_BLOCKCOUNT, irec.br_blockcount);
    }
}

#[inline(always)]
fn write_u32_le(buf: &mut [u8; XFS_BMBT_IREC_SIZE], off: usize, v: u32) {
    let b = v.to_le_bytes();
    buf[off] = b[0];
    buf[off + 1] = b[1];
    buf[off + 2] = b[2];
    buf[off + 3] = b[3];
}

#[inline(always)]
unsafe fn write_u32_at(base: *mut u8, off: usize, v: u32) {
    let b = v.to_le_bytes();
    unsafe {
        *base.add(off) = b[0];
        *base.add(off + 1) = b[1];
        *base.add(off + 2) = b[2];
        *base.add(off + 3) = b[3];
    }
}

/// Read back an irec from a FASM-layout buffer (tests / oracles).
#[inline(always)]
pub fn read_xfs_bmbt_irec(buf: &[u8; XFS_BMBT_IREC_SIZE]) -> XfsBmbtIrec {
    let lo = |off: usize| u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
    XfsBmbtIrec {
        br_startoff: (lo(OFF_STARTOFF_LO) as u64) | ((lo(OFF_STARTOFF_HI) as u64) << 32),
        br_startblock: (lo(OFF_STARTBLOCK_LO) as u64) | ((lo(OFF_STARTBLOCK_HI) as u64) << 32),
        br_blockcount: lo(OFF_BLOCKCOUNT),
        br_state: lo(OFF_STATE),
    }
}

/// Independent FASM-faithful reference used only by host differential tests.
///
/// Second transcription of the `movbe`/`shrd` schedule so `xfs_extent_unpack`
/// cannot silently share a single helper with the oracle.
#[cfg(test)]
fn fasm_extent_unpack_reference(rec: &[u8; 16]) -> XfsBmbtIrec {
    let w0 = u32::from_be_bytes([rec[0], rec[1], rec[2], rec[3]]);
    let w1 = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);
    let w2 = u32::from_be_bytes([rec[8], rec[9], rec[10], rec[11]]);
    let w3 = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]);

    let br_state = u32::from((w0 & 0x8000_0000) != 0);
    let edx = w0 & 0x7fff_ffff;
    let startoff_lo = (w1 >> 9) | (edx << 23);
    let startoff_hi = edx >> 9;

    let edx2 = w1 & 0x1ff;
    let startblock_lo = (w3 >> 21) | (w2 << 11);
    let startblock_hi = (w2 >> 21) | (edx2 << 11);
    let br_blockcount = w3 & 0x001f_ffff;

    XfsBmbtIrec {
        br_startoff: (startoff_lo as u64) | ((startoff_hi as u64) << 32),
        br_startblock: (startblock_lo as u64) | ((startblock_hi as u64) << 32),
        br_blockcount,
        br_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_be_fields(
        state: u32,
        startoff: u64,
        startblock: u64,
        blockcount: u32,
    ) -> [u8; 16] {
        // Rebuild the 128-bit BE record from logical fields (inverse of unpack).
        // layout: bit127=state, bits126:73=startoff(54), bits72:21=startblock(52),
        // bits20:0=blockcount(21).
        assert!(state <= 1);
        assert!(startoff < (1u64 << 54));
        assert!(startblock < (1u64 << 52));
        assert!(blockcount < (1u32 << 21));

        let mut bits: u128 = 0;
        bits |= (state as u128) << 127;
        bits |= (startoff as u128) << 73;
        bits |= (startblock as u128) << 21;
        bits |= blockcount as u128;
        bits.to_be_bytes()
    }

    #[test]
    fn layout_constants() {
        assert_eq!(XFS_BMBT_IREC_SIZE, 24);
        assert_eq!(OFF_STARTOFF_LO, 0);
        assert_eq!(OFF_STARTOFF_HI, 4);
        assert_eq!(OFF_STARTBLOCK_LO, 8);
        assert_eq!(OFF_STARTBLOCK_HI, 12);
        assert_eq!(OFF_BLOCKCOUNT, 16);
        assert_eq!(OFF_STATE, 20);
    }

    #[test]
    fn all_zero() {
        let rec = [0u8; 16];
        let got = xfs_extent_unpack(&rec);
        assert_eq!(got, fasm_extent_unpack_reference(&rec));
        assert_eq!(
            got,
            XfsBmbtIrec {
                br_startoff: 0,
                br_startblock: 0,
                br_blockcount: 0,
                br_state: 0,
            }
        );
    }

    #[test]
    fn all_ff() {
        let rec = [0xffu8; 16];
        let got = xfs_extent_unpack(&rec);
        assert_eq!(got, fasm_extent_unpack_reference(&rec));
        // state bit set; startoff = 0x3F_FFFF_FFFF_FFFF (54 ones);
        // startblock = 0xF_FFFF_FFFF_FFFF (52 ones); blockcount = 0x1F_FFFF.
        assert_eq!(got.br_state, 1);
        assert_eq!(got.br_startoff, (1u64 << 54) - 1);
        assert_eq!(got.br_startblock, (1u64 << 52) - 1);
        assert_eq!(got.br_blockcount, 0x001f_ffff);
    }

    #[test]
    fn state_bit_clear_and_set() {
        let clear = pack_be_fields(0, 0x123, 0x456, 0x789);
        let set = pack_be_fields(1, 0x123, 0x456, 0x789);
        let a = xfs_extent_unpack(&clear);
        let b = xfs_extent_unpack(&set);
        assert_eq!(a.br_state, 0);
        assert_eq!(b.br_state, 1);
        assert_eq!(a.br_startoff, 0x123);
        assert_eq!(b.br_startoff, 0x123);
        assert_eq!(a, fasm_extent_unpack_reference(&clear));
        assert_eq!(b, fasm_extent_unpack_reference(&set));
    }

    #[test]
    fn mask_and_shrd_boundaries() {
        // startoff just below/above 32-bit boundary (needs hi dword).
        let r1 = pack_be_fields(0, 0xffff_ffff, 0, 0);
        let g1 = xfs_extent_unpack(&r1);
        assert_eq!(g1.br_startoff, 0xffff_ffff);
        assert_eq!(g1, fasm_extent_unpack_reference(&r1));

        let r2 = pack_be_fields(0, 0x1_0000_0000, 0, 0);
        let g2 = xfs_extent_unpack(&r2);
        assert_eq!(g2.br_startoff, 0x1_0000_0000);
        assert_eq!(g2, fasm_extent_unpack_reference(&r2));

        // max startoff (54-bit)
        let r3 = pack_be_fields(1, (1u64 << 54) - 1, 0, 0);
        let g3 = xfs_extent_unpack(&r3);
        assert_eq!(g3.br_startoff, (1u64 << 54) - 1);
        assert_eq!(g3.br_state, 1);

        // startblock 32-bit boundary + max 52-bit
        let r4 = pack_be_fields(0, 0, 0xffff_ffff, 0);
        assert_eq!(xfs_extent_unpack(&r4).br_startblock, 0xffff_ffff);
        let r5 = pack_be_fields(0, 0, 0x1_0000_0000, 0);
        assert_eq!(xfs_extent_unpack(&r5).br_startblock, 0x1_0000_0000);
        let r6 = pack_be_fields(0, 0, (1u64 << 52) - 1, 0);
        assert_eq!(xfs_extent_unpack(&r6).br_startblock, (1u64 << 52) - 1);

        // blockcount 0 / max 21-bit
        let r7 = pack_be_fields(0, 0, 0, 0);
        assert_eq!(xfs_extent_unpack(&r7).br_blockcount, 0);
        let r8 = pack_be_fields(0, 0, 0, 0x001f_ffff);
        assert_eq!(xfs_extent_unpack(&r8).br_blockcount, 0x001f_ffff);
    }

    #[test]
    fn every_byte_distinct() {
        let mut rec = [0u8; 16];
        for i in 0..16 {
            rec[i] = (0x10 + i) as u8;
        }
        let got = xfs_extent_unpack(&rec);
        assert_eq!(got, fasm_extent_unpack_reference(&rec));
    }

    #[test]
    fn write_does_not_touch_padding() {
        let rec = pack_be_fields(1, 0xABC, 0xDEF, 0x123);
        let mut buf = [0xA5u8; 32];
        let mut irec_buf = [0u8; XFS_BMBT_IREC_SIZE];
        xfs_extent_unpack_into(&rec, &mut irec_buf);
        buf[..XFS_BMBT_IREC_SIZE].copy_from_slice(&irec_buf);
        for i in XFS_BMBT_IREC_SIZE..32 {
            assert_eq!(buf[i], 0xA5);
        }
        let read = read_xfs_bmbt_irec(&irec_buf);
        assert_eq!(read, xfs_extent_unpack(&rec));
        assert_eq!(read.br_state, 1);
        assert_eq!(read.br_startoff, 0xABC);
        assert_eq!(read.br_startblock, 0xDEF);
        assert_eq!(read.br_blockcount, 0x123);
    }

    #[test]
    fn ptr_wrapper_matches() {
        let rec = pack_be_fields(1, 0x1111, 0x2222, 0x333);
        let mut out = [0xA5u8; XFS_BMBT_IREC_SIZE];
        unsafe {
            xfs_extent_unpack_ptr(rec.as_ptr(), out.as_mut_ptr());
        }
        assert_eq!(read_xfs_bmbt_irec(&out), xfs_extent_unpack(&rec));
    }

    #[test]
    fn differential_vs_reference_random_corpus() {
        let mut state = XFS_EXTENT_UNPACK_PRNG_SEED;
        let mut next = || {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..200_000 {
            let mut rec = [0u8; 16];
            for b in &mut rec {
                *b = (next() & 0xff) as u8;
            }
            let a = xfs_extent_unpack(&rec);
            let b = fasm_extent_unpack_reference(&rec);
            assert_eq!(a, b, "rec={rec:02x?}");
        }
    }

    #[test]
    fn packed_field_roundtrip_sample() {
        // Spot-check packer vs unpacker for structured vectors.
        let samples: &[(u32, u64, u64, u32)] = &[
            (0, 0, 0, 0),
            (1, 0, 0, 0),
            (0, (1 << 54) - 1, (1 << 52) - 1, 0x001f_ffff),
            (1, 1, 1, 1),
            (0, 0x2a_bcde_f012, 0x1_2345_6789, 0x10_abcd),
        ];
        for &(st, so, sb, bc) in samples {
            let rec = pack_be_fields(st, so, sb, bc);
            let got = xfs_extent_unpack(&rec);
            assert_eq!(got.br_state, st);
            assert_eq!(got.br_startoff, so);
            assert_eq!(got.br_startblock, sb);
            assert_eq!(got.br_blockcount, bc);
            assert_eq!(got, fasm_extent_unpack_reference(&rec));
        }
    }

    #[test]
    fn shrd9_boundary_byte_patterns() {
        // Force edx low 9 bits / eax high interactions around the SHRD 9 split.
        for bit in 0u32..32 {
            let mut hi = 0u32;
            if bit < 31 {
                // place a single 1 in the masked startoff region of word0
                hi = 1u32 << bit; // bit31 is state; skip when testing startoff alone
            }
            let mid = 1u32 << (bit % 32);
            let mut rec = [0u8; 16];
            rec[0..4].copy_from_slice(&hi.to_be_bytes());
            rec[4..8].copy_from_slice(&mid.to_be_bytes());
            let got = xfs_extent_unpack(&rec);
            assert_eq!(got, fasm_extent_unpack_reference(&rec));
        }
    }
}
