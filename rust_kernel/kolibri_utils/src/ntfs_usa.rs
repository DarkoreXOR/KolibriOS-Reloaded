//! Cut J: `ntfs_restore_usa` — validate/restore NTFS Update Sequence Array.
//!
//! Matches `kernel/fs/ntfs.inc` FASM leaf semantics (USA size check, USN
//! compare at sector end-words, strided restore, partial mutate on mid-loop
//! reject, CF ok/fail). No tables / `.rodata`.
//!
//! Freestanding FFI path uses explicit `u16` loads/stores — never
//! `memcpy`/`memset`/`memmove` (those create reloc/GOT blockers; Cut I lesson).

/// NTFS record header field offsets (LOCAL FACT — `ntfs.inc`).
pub const UPDATE_SEQUENCE_OFFSET: usize = 4;
pub const UPDATE_SEQUENCE_SIZE: usize = 6;

/// Bytes from sector start to the protected end-word.
pub const SECTOR_END_WORD_OFF: usize = 0x1FE;
/// Sector stride in bytes (512).
pub const SECTOR_SIZE: usize = 512;

/// Result of [`ntfs_restore_usa`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsaRestoreResult {
    /// `true` = OK (FASM CF=0); `false` = fail (FASM CF=1).
    pub ok: bool,
}

/// Read a little-endian `u16` from `p` (two bytes). Explicit — no memcpy.
#[inline(always)]
unsafe fn load_u16(p: *const u8) -> u16 {
    let b0 = unsafe { *p };
    let b1 = unsafe { *p.add(1) };
    u16::from_le_bytes([b0, b1])
}

/// Write a little-endian `u16` to `p` (two bytes). Explicit — no memset.
#[inline(always)]
unsafe fn store_u16(p: *mut u8, v: u16) {
    let bytes = v.to_le_bytes();
    unsafe {
        *p = bytes[0];
        *p.add(1) = bytes[1];
    }
}

/// Restore USA into `record` of `size` bytes (FASM `ntfs_restore_usa`).
///
/// Mutates sector end-words in place. On mid-loop USN mismatch, earlier
/// sectors remain restored (partial mutate). Does not handle the FASM
/// `LOOP` with `ECX=0` pathology (`size < 512` with matching USA size 1);
/// callers must not rely on that undefined case.
///
/// # Safety
/// `record` must be readable/writable for `size` bytes (and for the USA
/// region / sector ends implied by the header when `size >= 512`).
#[inline(always)]
pub unsafe fn ntfs_restore_usa(record: *mut u8, size: u32) -> UsaRestoreResult {
    let sectors = size >> 9;
    let expected_usa = sectors.wrapping_add(1) as u16;

    let usa_size = unsafe { load_u16(record.add(UPDATE_SEQUENCE_SIZE)) };
    if usa_size != expected_usa {
        return UsaRestoreResult { ok: false };
    }

    let usa_off = unsafe { load_u16(record.add(UPDATE_SEQUENCE_OFFSET)) } as usize;
    let mut usa = unsafe { record.add(usa_off) };
    let usn = unsafe { load_u16(usa) };
    usa = unsafe { usa.add(2) };

    // Pre-stosw EDI = record+0x1FE; after stosw+add 0x1FE net stride is +0x200.
    let mut end_word = unsafe { record.add(SECTOR_END_WORD_OFF) };
    let mut remaining = sectors;

    while remaining > 0 {
        let cur = unsafe { load_u16(end_word) };
        if cur != usn {
            return UsaRestoreResult { ok: false };
        }
        let orig = unsafe { load_u16(usa) };
        usa = unsafe { usa.add(2) };
        unsafe { store_u16(end_word, orig) };
        end_word = unsafe { end_word.add(SECTOR_SIZE) };
        remaining -= 1;
    }

    UsaRestoreResult { ok: true }
}

/// FFI helper: returns `0` OK / `1` fail for trampoline CF mapping.
///
/// # Safety
/// Same as [`ntfs_restore_usa`].
#[inline(always)]
pub unsafe fn ntfs_restore_usa_ptr(record: *mut u8, size: u32) -> u32 {
    if unsafe { ntfs_restore_usa(record, size) }.ok {
        0
    } else {
        1
    }
}

// ---------------------------------------------------------------------------
// Host tests / FASM-faithful oracle (not linked into freestanding blob)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Separately coded FASM-faithful oracle (must not share helper guts with
    /// the production path beyond the documented algorithm steps).
    fn fasm_oracle(record: &mut [u8], size: u32) -> bool {
        let sectors = (size >> 9) as usize;
        let expected_usa = (sectors as u32).wrapping_add(1) as u16;
        if record.len() < UPDATE_SEQUENCE_SIZE + 2 {
            return false;
        }
        let usa_size = u16::from_le_bytes([
            record[UPDATE_SEQUENCE_SIZE],
            record[UPDATE_SEQUENCE_SIZE + 1],
        ]);
        if usa_size != expected_usa {
            return false;
        }
        let usa_off = u16::from_le_bytes([
            record[UPDATE_SEQUENCE_OFFSET],
            record[UPDATE_SEQUENCE_OFFSET + 1],
        ]) as usize;
        if usa_off + 2 + sectors * 2 > record.len() {
            // FASM would still read; for host safety treat as fail only if we
            // cannot mirror — tests always size buffers adequately.
            return false;
        }
        let usn = u16::from_le_bytes([record[usa_off], record[usa_off + 1]]);
        let mut usa_i = usa_off + 2;
        let mut end = SECTOR_END_WORD_OFF;
        for _ in 0..sectors {
            if end + 2 > record.len() {
                return false;
            }
            let cur = u16::from_le_bytes([record[end], record[end + 1]]);
            if cur != usn {
                return false;
            }
            let orig = u16::from_le_bytes([record[usa_i], record[usa_i + 1]]);
            record[end] = (orig & 0xff) as u8;
            record[end + 1] = (orig >> 8) as u8;
            usa_i += 2;
            end += SECTOR_SIZE;
        }
        true
    }

    fn run_both(mut a: Vec<u8>, size: u32) -> (bool, bool, Vec<u8>, Vec<u8>) {
        let mut b = a.clone();
        let ok_prod = unsafe { ntfs_restore_usa(a.as_mut_ptr(), size) }.ok;
        let ok_oracle = fasm_oracle(&mut b, size);
        (ok_prod, ok_oracle, a, b)
    }

    fn assert_diff(record: Vec<u8>, size: u32) {
        let (ok_a, ok_b, a, b) = run_both(record, size);
        assert_eq!(ok_a, ok_b, "CF/ok mismatch");
        assert_eq!(a, b, "buffer mismatch");
    }

    /// Build a valid N-sector record: header + USA at `usa_off`, sector ends = USN.
    fn make_valid(sectors: usize, usa_off: usize, usn: u16, origs: &[u16]) -> Vec<u8> {
        assert_eq!(origs.len(), sectors);
        let size = sectors * SECTOR_SIZE;
        let mut r = vec![0xA5u8; size];
        r[UPDATE_SEQUENCE_OFFSET] = (usa_off & 0xff) as u8;
        r[UPDATE_SEQUENCE_OFFSET + 1] = (usa_off >> 8) as u8;
        let usa_words = (sectors + 1) as u16;
        r[UPDATE_SEQUENCE_SIZE] = (usa_words & 0xff) as u8;
        r[UPDATE_SEQUENCE_SIZE + 1] = (usa_words >> 8) as u8;
        r[usa_off] = (usn & 0xff) as u8;
        r[usa_off + 1] = (usn >> 8) as u8;
        for (i, &o) in origs.iter().enumerate() {
            let u = usa_off + 2 + i * 2;
            r[u] = (o & 0xff) as u8;
            r[u + 1] = (o >> 8) as u8;
            let e = SECTOR_END_WORD_OFF + i * SECTOR_SIZE;
            r[e] = (usn & 0xff) as u8;
            r[e + 1] = (usn >> 8) as u8;
        }
        r
    }

    #[test]
    fn one_sector_ok_restores_end_word() {
        let r = make_valid(1, 0x28, 0xABCD, &[0x1234]);
        let size = 512u32;
        let (ok_a, ok_b, a, b) = run_both(r, size);
        assert!(ok_a && ok_b);
        assert_eq!(a, b);
        assert_eq!(u16::from_le_bytes([a[0x1FE], a[0x1FF]]), 0x1234);
    }

    #[test]
    fn two_sector_ok_restores_both() {
        let r = make_valid(2, 0x30, 0x1111, &[0xAAAA, 0xBBBB]);
        assert_diff(r, 1024);
        let mut r = make_valid(2, 0x30, 0x1111, &[0xAAAA, 0xBBBB]);
        assert!(unsafe { ntfs_restore_usa(r.as_mut_ptr(), 1024) }.ok);
        assert_eq!(u16::from_le_bytes([r[0x1FE], r[0x1FF]]), 0xAAAA);
        assert_eq!(u16::from_le_bytes([r[0x3FE], r[0x3FF]]), 0xBBBB);
    }

    #[test]
    fn four_sector_ok() {
        let origs = [0x0101u16, 0x0202, 0x0303, 0x0404];
        let r = make_valid(4, 0x28, 0x55AA, &origs);
        assert_diff(r, 2048);
    }

    #[test]
    fn usa_size_mismatch_no_mutate() {
        let mut r = make_valid(2, 0x28, 0x2222, &[1, 2]);
        r[UPDATE_SEQUENCE_SIZE] = 1; // wrong (want 3)
        r[UPDATE_SEQUENCE_SIZE + 1] = 0;
        let before = r.clone();
        let (ok_a, ok_b, a, b) = run_both(r, 1024);
        assert!(!ok_a && !ok_b);
        assert_eq!(a, before);
        assert_eq!(b, before);
    }

    #[test]
    fn usn_mismatch_first_sector_no_restore() {
        let mut r = make_valid(2, 0x28, 0x3333, &[0x1111, 0x2222]);
        r[0x1FE] = 0x00;
        r[0x1FF] = 0x00; // bad USN at first end
        let before = r.clone();
        let (ok_a, ok_b, a, b) = run_both(r, 1024);
        assert!(!ok_a && !ok_b);
        assert_eq!(a, before);
        assert_eq!(a, b);
    }

    #[test]
    fn usn_mismatch_second_sector_partial_mutate() {
        let mut r = make_valid(2, 0x28, 0x4444, &[0xAAAA, 0xBBBB]);
        r[0x3FE] = 0x00;
        r[0x3FF] = 0x00; // fail on second sector
        let (ok_a, ok_b, a, b) = run_both(r, 1024);
        assert!(!ok_a && !ok_b);
        assert_eq!(a, b);
        // First sector already restored
        assert_eq!(u16::from_le_bytes([a[0x1FE], a[0x1FF]]), 0xAAAA);
        // Second still wrong USN placeholder
        assert_eq!(u16::from_le_bytes([a[0x3FE], a[0x3FF]]), 0x0000);
    }

    #[test]
    fn size_below_512_mismatched_usa_early_fail() {
        // size>>9 = 0 → expected USA size 1; put 2 → early fail, no loop
        let mut r = vec![0x5Au8; 256];
        r[UPDATE_SEQUENCE_OFFSET] = 0x10;
        r[UPDATE_SEQUENCE_OFFSET + 1] = 0;
        r[UPDATE_SEQUENCE_SIZE] = 2;
        r[UPDATE_SEQUENCE_SIZE + 1] = 0;
        let before = r.clone();
        let (ok_a, ok_b, a, b) = run_both(r, 256);
        assert!(!ok_a && !ok_b);
        assert_eq!(a, before);
        assert_eq!(a, b);
    }

    #[test]
    fn sentinel_bytes_outside_end_words_unchanged() {
        let mut r = make_valid(2, 0x28, 0x7777, &[0x1000, 0x2000]);
        r[0] = 0xFE;
        r[0x100] = 0xFD;
        r[0x200] = 0xFC;
        assert_diff(r, 1024);
        let mut r = make_valid(2, 0x28, 0x7777, &[0x1000, 0x2000]);
        r[0] = 0xFE;
        r[0x100] = 0xFD;
        r[0x200] = 0xFC;
        assert!(unsafe { ntfs_restore_usa(r.as_mut_ptr(), 1024) }.ok);
        assert_eq!(r[0], 0xFE);
        assert_eq!(r[0x100], 0xFD);
        assert_eq!(r[0x200], 0xFC);
    }

    #[test]
    fn grid_sectors_1_to_8_usa_offsets() {
        for sectors in 1usize..=8 {
            for &usa_off in &[0x28usize, 0x2A, 0x30, 0x40] {
                let need = usa_off + 2 + sectors * 2;
                if need > SECTOR_END_WORD_OFF {
                    continue; // USA must not overlap first end-word in our synth
                }
                let origs: Vec<u16> = (0..sectors).map(|i| 0x1000 + i as u16).collect();
                let r = make_valid(sectors, usa_off, 0xBEEF, &origs);
                assert_diff(r, (sectors * SECTOR_SIZE) as u32);
            }
        }
    }

    #[test]
    fn prng_differential_200k() {
        // Documented Cut J PRNG seed.
        const SEED: u32 = 0xC07B_10E;
        let mut state = SEED;
        let mut next = || -> u32 {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };

        for _ in 0..200_000u32 {
            let sectors = (next() % 8) + 1; // 1..8 — avoid ECX=0 pathology
            let size = sectors * SECTOR_SIZE as u32;
            let usa_off = 0x28 + (next() % 8) as usize * 2;
            let usn = (next() & 0xffff) as u16;
            let mode = next() % 5;

            let origs: Vec<u16> = (0..sectors as usize)
                .map(|_| (next() & 0xffff) as u16)
                .collect();
            let mut r = make_valid(sectors as usize, usa_off, usn, &origs);

            match mode {
                0 => {
                    // valid
                }
                1 => {
                    // corrupt USA size
                    r[UPDATE_SEQUENCE_SIZE] = (next() & 0xff) as u8;
                    r[UPDATE_SEQUENCE_SIZE + 1] = 0;
                }
                2 => {
                    // corrupt a random sector end USN
                    let i = (next() as usize) % sectors as usize;
                    let e = SECTOR_END_WORD_OFF + i * SECTOR_SIZE;
                    r[e] = r[e].wrapping_add(1);
                }
                3 => {
                    // corrupt USA offset to nonsense but keep buffer sized
                    r[UPDATE_SEQUENCE_OFFSET] = 0x08;
                    r[UPDATE_SEQUENCE_OFFSET + 1] = 0;
                    // rebuild USN/origs at new offset only if space; else leave garbage
                    let no = 0x08usize;
                    if no + 2 + sectors as usize * 2 <= SECTOR_END_WORD_OFF {
                        r[no] = (usn & 0xff) as u8;
                        r[no + 1] = (usn >> 8) as u8;
                        for (i, &o) in origs.iter().enumerate() {
                            let u = no + 2 + i * 2;
                            r[u] = (o & 0xff) as u8;
                            r[u + 1] = (o >> 8) as u8;
                        }
                    }
                }
                _ => {
                    // flip a USA original word (still valid USN path)
                    let i = (next() as usize) % sectors as usize;
                    let u = usa_off + 2 + i * 2;
                    r[u] ^= 0xFF;
                }
            }

            assert_diff(r, size);
        }
    }

    #[test]
    fn ptr_return_codes() {
        let mut r = make_valid(1, 0x28, 1, &[0x55]);
        assert_eq!(unsafe { ntfs_restore_usa_ptr(r.as_mut_ptr(), 512) }, 0);
        r[UPDATE_SEQUENCE_SIZE] = 99;
        assert_eq!(unsafe { ntfs_restore_usa_ptr(r.as_mut_ptr(), 512) }, 1);
    }
}
