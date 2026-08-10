//! Cut AG: `ntfs_test_bootsec` — NTFS bootsector multi-rule validation (CF).
//!
//! Matches `kernel/fs/ntfs.inc` FASM leaf semantics:
//! 1. OEM name at +3 == `NTFS    `
//! 2. Bytes/sector at +11 == `0x200`
//! 3. Sectors/cluster at +13 is a non-zero power of two
//! 4. FAT-compat fields at +14/+16/+20/+22/+32 are zero
//! 5. TotalSectors qword at +0x28: high dword == 0 and low <= partition size
//! 6. `$MFT` / `$MFTMirr` LCNs: high dword == 0; `spc * LCN_lo` fits in 32 bits
//!    and is <= partition size
//! 7–8. Clusters per FRS / IndexAlloc: `-31..=-9` **or** non-zero power of two
//!
//! No globals / tables / `.rodata`. Buffer + partition size are explicit args
//! so the freestanding blob stays reloc-free.

/// Minimum bytes the leaf reads (through ClustersPerIndex at `+0x44`).
pub const NTFS_BOOTSEC_MIN_LEN: usize = 0x45;

/// Cut AG differential PRNG seed (`'CUTG'`).
pub const NTFS_TEST_BOOTSEC_PRNG_SEED: u32 = 0x4355_5447;

/// FASM-faithful NTFS bootsector validity check (CF clear = valid).
///
/// Returns `true` when the bootsector is **valid** (legacy clears CF).
///
/// `boot` must be at least [`NTFS_BOOTSEC_MIN_LEN`] bytes; shorter buffers
/// are treated as invalid (defensive — production callers pass a 512-byte
/// sector).
#[inline(always)]
pub fn ntfs_test_bootsec(boot: &[u8], partition_sectors: u32) -> bool {
    if boot.len() < NTFS_BOOTSEC_MIN_LEN {
        return false;
    }

    // 1. OEM == 'NTFS    ' at +3..+10
    if boot[3] != b'N'
        || boot[4] != b'T'
        || boot[5] != b'F'
        || boot[6] != b'S'
        || boot[7] != b' '
        || boot[8] != b' '
        || boot[9] != b' '
        || boot[10] != b' '
    {
        return false;
    }

    // 2. Bytes per sector == 0x200
    if read_u16_le(boot, 11) != 0x200 {
        return false;
    }

    // 3. Sectors per cluster: non-zero power of two
    //    movzx eax, byte; dec eax; js .no; test al, [byte]
    let spc = boot[13];
    if !is_pow2_nonzero_u8(spc) {
        return false;
    }

    // 4. FAT parameters must be zero
    if read_u16_le(boot, 14) != 0 {
        return false;
    }
    if read_u32_le(boot, 16) != 0 {
        return false;
    }
    if boot[20] != 0 {
        return false;
    }
    if read_u16_le(boot, 22) != 0 {
        return false;
    }
    if read_u32_le(boot, 32) != 0 {
        return false;
    }

    // 5. Number of sectors <= partition size (high dword must be 0)
    if read_u32_le(boot, 0x2C) != 0 {
        return false;
    }
    if read_u32_le(boot, 0x28) > partition_sectors {
        return false;
    }

    // 6. $MFT and $MFTMirr within partition
    if !lcn_within_partition(boot, 0x30, spc, partition_sectors) {
        return false;
    }
    if !lcn_within_partition(boot, 0x38, spc, partition_sectors) {
        return false;
    }

    // 7–8. Clusters per FRS / IndexAllocationBuffer
    if !clusters_per_record_ok(boot[0x40] as i8) {
        return false;
    }
    if !clusters_per_record_ok(boot[0x44] as i8) {
        return false;
    }

    true
}

/// Pointer-form wrapper for the FFI boundary.
///
/// Returns `0` = valid (CF clear), `1` = invalid (CF set), matching Cut Z
/// `is_partition_table_entry` EAX→CF trampoline polarity.
///
/// # Safety
/// `boot` must be readable for at least [`NTFS_BOOTSEC_MIN_LEN`] bytes.
#[inline(always)]
pub unsafe fn ntfs_test_bootsec_ptr(boot: *const u8, partition_sectors: u32) -> u32 {
    // SAFETY: caller guarantees a readable bootsector buffer.
    let slice = unsafe { core::slice::from_raw_parts(boot, NTFS_BOOTSEC_MIN_LEN) };
    if ntfs_test_bootsec(slice, partition_sectors) {
        0
    } else {
        1
    }
}

/// Build a minimal valid NTFS bootsector (zeros elsewhere).
#[inline(always)]
pub fn make_valid_ntfs_bootsec(
    spc: u8,
    total_sectors: u32,
    mft_lcn: u32,
    mftmirr_lcn: u32,
    clusters_per_frs: i8,
    clusters_per_index: i8,
) -> [u8; 512] {
    let mut b = [0u8; 512];
    // OEM "NTFS    "
    b[3] = b'N';
    b[4] = b'T';
    b[5] = b'F';
    b[6] = b'S';
    b[7] = b' ';
    b[8] = b' ';
    b[9] = b' ';
    b[10] = b' ';
    write_u16_le(&mut b, 11, 0x200);
    b[13] = spc;
    write_u32_le(&mut b, 0x28, total_sectors);
    write_u32_le(&mut b, 0x30, mft_lcn);
    write_u32_le(&mut b, 0x38, mftmirr_lcn);
    b[0x40] = clusters_per_frs as u8;
    b[0x44] = clusters_per_index as u8;
    b
}

#[inline(always)]
fn lcn_within_partition(boot: &[u8], lcn_off: usize, spc: u8, partition_sectors: u32) -> bool {
    // high dword must be 0
    if read_u32_le(boot, lcn_off + 4) != 0 {
        return false;
    }
    let lcn_lo = read_u32_le(boot, lcn_off);
    // mul: EDX:EAX = spc * lcn_lo; reject if EDX != 0 or EAX > partition
    let product = (spc as u64).wrapping_mul(lcn_lo as u64);
    if (product >> 32) != 0 {
        return false;
    }
    (product as u32) <= partition_sectors
}

/// Clusters-per-FRS / IndexAlloc rule from FASM:
/// `movsx` / `cmp -31` / `cmp -9` / else `dec`+`js` / `test [byte], al`.
#[inline(always)]
fn clusters_per_record_ok(v: i8) -> bool {
    if v < -31 {
        return false;
    }
    if v <= -9 {
        return true;
    }
    // Power-of-two path on the original memory byte.
    let mem = v as u8;
    let dec = (v as i32).wrapping_sub(1);
    if dec < 0 {
        return false;
    }
    (mem & (dec as u8)) == 0
}

#[inline(always)]
fn is_pow2_nonzero_u8(b: u8) -> bool {
    if b == 0 {
        return false;
    }
    (b & b.wrapping_sub(1)) == 0
}

#[inline(always)]
fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

#[inline(always)]
fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

#[inline(always)]
fn write_u16_le(buf: &mut [u8], off: usize, v: u16) {
    let b = v.to_le_bytes();
    buf[off] = b[0];
    buf[off + 1] = b[1];
}

#[inline(always)]
fn write_u32_le(buf: &mut [u8], off: usize, v: u32) {
    let b = v.to_le_bytes();
    buf[off] = b[0];
    buf[off + 1] = b[1];
    buf[off + 2] = b[2];
    buf[off + 3] = b[3];
}

/// Independent FASM-flow oracle (duplicated control flow; not a call through
/// [`ntfs_test_bootsec`]).
pub fn fasm_oracle_ntfs_test_bootsec(boot: &[u8], partition_sectors: u32) -> bool {
    if boot.len() < NTFS_BOOTSEC_MIN_LEN {
        return false;
    }

    // 1. cmp dword [ebx+3],'NTFS' / cmp dword [ebx+7],'    '
    let oem_lo = read_u32_le(boot, 3);
    let oem_hi = read_u32_le(boot, 7);
    if oem_lo != u32::from_le_bytes(*b"NTFS") || oem_hi != u32::from_le_bytes(*b"    ") {
        return false;
    }

    // 2. cmp word [ebx+11], 0x200
    if read_u16_le(boot, 11) != 0x200 {
        return false;
    }

    // 3. movzx eax, byte [ebx+13]; dec eax; js .no; test al, [ebx+13]
    let spc = boot[13];
    let mut eax: u32 = spc as u32;
    eax = eax.wrapping_sub(1);
    if (eax as i32) < 0 {
        return false;
    }
    if ((eax as u8) & spc) != 0 {
        return false;
    }

    // 4. FAT zero fields
    if read_u16_le(boot, 14) != 0
        || read_u32_le(boot, 16) != 0
        || boot[20] != 0
        || read_u16_le(boot, 22) != 0
        || read_u32_le(boot, 32) != 0
    {
        return false;
    }

    // 5. cmp dword [ebx+0x2C],0 / ja; cmp [ebx+0x28], edx / ja
    if read_u32_le(boot, 0x2C) != 0 {
        return false;
    }
    if read_u32_le(boot, 0x28) > partition_sectors {
        return false;
    }

    // 6. MFT / MFTMirr — push edx; mul; test edx; pop; cmp eax,edx
    if !oracle_lcn_ok(boot, 0x30, spc, partition_sectors) {
        return false;
    }
    if !oracle_lcn_ok(boot, 0x38, spc, partition_sectors) {
        return false;
    }

    // 7–8. movsx / cmp -31 / cmp -9 / else dec+js / test [byte],al
    if !oracle_clusters_per_record(boot[0x40]) {
        return false;
    }
    if !oracle_clusters_per_record(boot[0x44]) {
        return false;
    }

    true
}

#[inline(always)]
fn oracle_lcn_ok(boot: &[u8], off: usize, spc: u8, partition_sectors: u32) -> bool {
    if read_u32_le(boot, off + 4) != 0 {
        return false;
    }
    let lcn_lo = read_u32_le(boot, off);
    // unsigned mul into 64-bit, then test high dword
    let wide = (spc as u64) * (lcn_lo as u64);
    let edx = (wide >> 32) as u32;
    let eax = wide as u32;
    if edx != 0 {
        return false;
    }
    eax <= partition_sectors
}

#[inline(always)]
fn oracle_clusters_per_record(raw: u8) -> bool {
    let al = raw as i8;
    if al < -31 {
        return false;
    }
    if al <= -9 {
        return true;
    }
    let mut eax = al as i32;
    eax = eax.wrapping_sub(1);
    if eax < 0 {
        return false;
    }
    (raw & (eax as u8)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> ([u8; 512], u32) {
        // spc=8, total=1000, mft=1 → 8 secs, mirr=2 → 16 secs; FRS=-10, idx=-10
        (
            make_valid_ntfs_bootsec(8, 1000, 1, 2, -10, -10),
            1000,
        )
    }

    #[test]
    fn valid_canonical() {
        let (b, part) = valid();
        assert!(ntfs_test_bootsec(&b, part));
        assert_eq!(ntfs_test_bootsec(&b, part), fasm_oracle_ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn reject_bad_oem() {
        let (mut b, part) = valid();
        b[3] = b'F';
        assert!(!ntfs_test_bootsec(&b, part));
        assert_eq!(ntfs_test_bootsec(&b, part), fasm_oracle_ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn reject_bad_bps() {
        let (mut b, part) = valid();
        write_u16_le(&mut b, 11, 0x400);
        assert!(!ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn reject_spc_zero_and_non_pow2() {
        let (mut b, part) = valid();
        b[13] = 0;
        assert!(!ntfs_test_bootsec(&b, part));
        b[13] = 3;
        assert!(!ntfs_test_bootsec(&b, part));
        b[13] = 8;
        assert!(ntfs_test_bootsec(&b, part));
        b[13] = 128;
        // mft*128 may still fit
        write_u32_le(&mut b, 0x30, 1);
        write_u32_le(&mut b, 0x38, 1);
        assert!(ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn reject_fat_fields() {
        let (mut b, part) = valid();
        write_u16_le(&mut b, 14, 1);
        assert!(!ntfs_test_bootsec(&b, part));
        write_u16_le(&mut b, 14, 0);
        write_u32_le(&mut b, 16, 1);
        assert!(!ntfs_test_bootsec(&b, part));
        write_u32_le(&mut b, 16, 0);
        b[20] = 1;
        assert!(!ntfs_test_bootsec(&b, part));
        b[20] = 0;
        write_u16_le(&mut b, 22, 1);
        assert!(!ntfs_test_bootsec(&b, part));
        write_u16_le(&mut b, 22, 0);
        write_u32_le(&mut b, 32, 1);
        assert!(!ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn reject_total_sectors() {
        let (mut b, part) = valid();
        write_u32_le(&mut b, 0x2C, 1);
        assert!(!ntfs_test_bootsec(&b, part));
        write_u32_le(&mut b, 0x2C, 0);
        write_u32_le(&mut b, 0x28, part + 1);
        assert!(!ntfs_test_bootsec(&b, part));
        write_u32_le(&mut b, 0x28, part);
        assert!(ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn reject_mft_out_of_range_and_overflow() {
        let (mut b, part) = valid();
        write_u32_le(&mut b, 0x34, 1);
        assert!(!ntfs_test_bootsec(&b, part));
        write_u32_le(&mut b, 0x34, 0);
        // spc=8, lcn such that 8*lcn > part
        write_u32_le(&mut b, 0x30, (part / 8) + 1);
        assert!(!ntfs_test_bootsec(&b, part));
        // overflow: spc=128, lcn=0x0200_0000 → product high nonzero
        b[13] = 128;
        write_u32_le(&mut b, 0x30, 0x0200_0000);
        write_u32_le(&mut b, 0x38, 1);
        assert!(!ntfs_test_bootsec(&b, part));
        assert_eq!(
            ntfs_test_bootsec(&b, part),
            fasm_oracle_ntfs_test_bootsec(&b, part)
        );
    }

    #[test]
    fn clusters_per_record_ranges() {
        let (mut b, part) = valid();
        // -31..=-9 ok
        for v in -31i8..=-9 {
            b[0x40] = v as u8;
            b[0x44] = v as u8;
            assert!(ntfs_test_bootsec(&b, part), "v={v}");
        }
        // -32 fail
        b[0x40] = (-32i8) as u8;
        assert!(!ntfs_test_bootsec(&b, part));
        // -8 fail (JS after dec)
        b[0x40] = (-8i8) as u8;
        b[0x44] = (-10i8) as u8;
        assert!(!ntfs_test_bootsec(&b, part));
        // 0 fail
        b[0x40] = 0;
        assert!(!ntfs_test_bootsec(&b, part));
        // powers of two ok
        for v in [1u8, 2, 4, 8, 16, 32, 64] {
            b[0x40] = v;
            b[0x44] = v;
            assert!(ntfs_test_bootsec(&b, part), "pow2 {v}");
        }
        // non-pow2 fail
        b[0x40] = 3;
        b[0x44] = (-10i8) as u8;
        assert!(!ntfs_test_bootsec(&b, part));
    }

    #[test]
    fn ptr_form_polarity() {
        let (b, part) = valid();
        let r = unsafe { ntfs_test_bootsec_ptr(b.as_ptr(), part) };
        assert_eq!(r, 0);
        let mut bad = b;
        bad[3] = b'X';
        let r2 = unsafe { ntfs_test_bootsec_ptr(bad.as_ptr(), part) };
        assert_eq!(r2, 1);
    }

    #[test]
    fn short_buffer_invalid() {
        let short = [0u8; 16];
        assert!(!ntfs_test_bootsec(&short, 100));
        assert!(!fasm_oracle_ntfs_test_bootsec(&short, 100));
    }

    #[test]
    fn named_boundary_partition_zero() {
        let b = make_valid_ntfs_bootsec(1, 0, 0, 0, 1, 1);
        // total=0 <= 0; mft*1=0 <= 0; ok
        assert!(ntfs_test_bootsec(&b, 0));
        let b2 = make_valid_ntfs_bootsec(1, 1, 0, 0, 1, 1);
        assert!(!ntfs_test_bootsec(&b2, 0));
    }

    #[test]
    fn differential_named_vectors() {
        let cases: &[([u8; 512], u32)] = &[
            (make_valid_ntfs_bootsec(8, 1000, 1, 2, -10, -10), 1000),
            (make_valid_ntfs_bootsec(1, 1, 0, 0, 1, 1), 1),
            (make_valid_ntfs_bootsec(64, 0xFFFF, 1, 1, -31, -9), 0xFFFF),
            (make_valid_ntfs_bootsec(8, 1000, 1, 2, 8, 16), 1000),
        ];
        for (b, part) in cases {
            assert_eq!(
                ntfs_test_bootsec(b, *part),
                fasm_oracle_ntfs_test_bootsec(b, *part)
            );
        }
    }

    #[test]
    fn differential_prng_50k() {
        // Deterministic xorshift; seed 'CUTG'
        let mut state = NTFS_TEST_BOOTSEC_PRNG_SEED;
        let mut mismatches = 0u32;
        for i in 0..50_000u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let mut b = make_valid_ntfs_bootsec(8, 10_000, 1, 2, -10, -10);
            // Mutate a selection of fields from PRNG
            let lane = (state >> 8) % 12;
            match lane {
                0 => b[3 + (state % 8) as usize] = (state & 0xFF) as u8,
                1 => write_u16_le(&mut b, 11, (state & 0xFFFF) as u16),
                2 => b[13] = (state & 0xFF) as u8,
                3 => write_u16_le(&mut b, 14, (state & 0xFFFF) as u16),
                4 => write_u32_le(&mut b, 16, state),
                5 => b[20] = (state & 0xFF) as u8,
                6 => write_u32_le(&mut b, 0x28, state),
                7 => write_u32_le(&mut b, 0x2C, state & 3),
                8 => write_u32_le(&mut b, 0x30, state),
                9 => write_u32_le(&mut b, 0x34, state & 1),
                10 => b[0x40] = (state & 0xFF) as u8,
                _ => b[0x44] = ((state >> 16) & 0xFF) as u8,
            }
            // Occasionally fully randomize the interesting window
            if state & 0x7000 == 0 {
                for off in 3..0x45 {
                    state ^= state << 7;
                    state ^= state >> 9;
                    b[off] = (state & 0xFF) as u8;
                }
            }
            let part = state.wrapping_mul(0x9E37_79B9) ^ i;
            let rust = ntfs_test_bootsec(&b, part);
            let oracle = fasm_oracle_ntfs_test_bootsec(&b, part);
            if rust != oracle {
                mismatches += 1;
                if mismatches <= 3 {
                    panic!("mismatch i={i} part={part:#x} rust={rust} oracle={oracle}");
                }
            }
        }
        assert_eq!(mismatches, 0);
    }
}
