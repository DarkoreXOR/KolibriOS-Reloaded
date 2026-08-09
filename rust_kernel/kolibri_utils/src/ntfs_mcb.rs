//! Cut I: `ntfs_decode_mcb_entry` — decode one NTFS MCB (data-run) entry.
//!
//! Matches `kernel/fs/ntfs.inc` FASM leaf semantics (VLE nibbles, zero-pad /
//! sign-extend, partial buffer writes, CF more/end). No tables / `.rodata`.
//!
//! Freestanding FFI path builds u64 values in registers and stores them —
//! never calls `memset`/`memcpy` (those create reloc-hard blockers).

/// Result of decoding one MCB entry into a caller 16-byte buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McbDecodeResult {
    /// `true` = more (FASM CF=1); `false` = end (FASM CF=0).
    pub more: bool,
    /// Bytes consumed from the packed stream (including the header byte when read).
    pub consumed: u32,
}

/// Decode one NTFS MCB entry from `entry` into `buf` (exactly 16 bytes).
///
/// Buffer layout (little-endian), matching FASM caller use of `[esp]` / `[esp+8]`:
/// - bytes `[0..8)`: run size (zero-padded unsigned)
/// - bytes `[8..16)`: cluster delta (sign-extended signed)
///
/// On early reject, only the bytes FASM would have written are updated; the
/// remainder of `buf` is left unchanged (partial mutate).
#[inline(always)]
pub fn ntfs_decode_mcb_entry(entry: &[u8], buf: &mut [u8; 16]) -> McbDecodeResult {
    if entry.is_empty() {
        return McbDecodeResult {
            more: false,
            consumed: 0,
        };
    }

    let header = entry[0];
    if header == 0 {
        return McbDecodeResult {
            more: false,
            consumed: 1,
        };
    }

    let length_len = (header & 0x0F) as usize;
    if length_len > 8 {
        return McbDecodeResult {
            more: false,
            consumed: 1,
        };
    }

    if entry.len() < 1 + length_len {
        return McbDecodeResult {
            more: false,
            consumed: 1,
        };
    }

    // Copy length field bytes (partial write path may stop here).
    let mut i = 0;
    while i < length_len {
        buf[i] = entry[1 + i];
        i += 1;
    }

    let last_len_byte = if length_len == 0 {
        header
    } else {
        entry[length_len]
    };
    if last_len_byte >= 0x80 {
        return McbDecodeResult {
            more: false,
            consumed: (1 + length_len) as u32,
        };
    }

    // Zero-pad run size to 8 via u64 store (no memset).
    let mut run: u64 = 0;
    i = 0;
    while i < length_len {
        run |= (buf[i] as u64) << (8 * i);
        i += 1;
    }
    write_u64_le(buf, 0, run);

    let cluster_len = (header >> 4) as usize;
    if cluster_len > 8 {
        return McbDecodeResult {
            more: false,
            consumed: (1 + length_len) as u32,
        };
    }

    let cluster_off = 1 + length_len;
    if entry.len() < cluster_off + cluster_len {
        return McbDecodeResult {
            more: false,
            consumed: (1 + length_len) as u32,
        };
    }

    let mut cluster: u64 = 0;
    i = 0;
    while i < cluster_len {
        let b = entry[cluster_off + i];
        cluster |= (b as u64) << (8 * i);
        i += 1;
    }

    let last_cluster_ref = if cluster_len == 0 {
        last_len_byte
    } else {
        entry[cluster_off + cluster_len - 1]
    };
    if last_cluster_ref >= 0x80 {
        if cluster_len < 8 {
            cluster |= (!0u64) << (8 * cluster_len);
        }
    }

    write_u64_le(buf, 8, cluster);

    McbDecodeResult {
        more: true,
        consumed: (1 + length_len + cluster_len) as u32,
    }
}

#[inline(always)]
fn write_u64_le(buf: &mut [u8; 16], off: usize, v: u64) {
    let bytes = v.to_le_bytes();
    buf[off] = bytes[0];
    buf[off + 1] = bytes[1];
    buf[off + 2] = bytes[2];
    buf[off + 3] = bytes[3];
    buf[off + 4] = bytes[4];
    buf[off + 5] = bytes[5];
    buf[off + 6] = bytes[6];
    buf[off + 7] = bytes[7];
}

/// Pointer-friendly wrapper for the FFI trampoline (byte-walk, FASM-faithful).
///
/// Reads only the bytes FASM would via `lodsb` / `rep movsb`. Builds padded
/// fields as `u64` in registers — no `memset` (reloc-free requirement).
///
/// # Safety
/// - `esi_inout` must point to a valid `*mut u8` advanced by consumed bytes.
/// - On entry `*esi_inout` must be readable for those consumed bytes.
/// - `buffer` must point to a writable 16-byte region.
#[inline(always)]
pub unsafe fn ntfs_decode_mcb_entry_ptr(
    esi_inout: *mut *mut u8,
    buffer: *mut u8,
) -> u32 {
    let mut esi = unsafe { *esi_inout };

    // lodsb
    let header = unsafe { *esi };
    esi = unsafe { esi.add(1) };
    if header == 0 {
        unsafe { *esi_inout = esi };
        return 0;
    }

    let length_len = (header & 0x0F) as u32;
    if length_len > 8 {
        unsafe { *esi_inout = esi };
        return 0;
    }

    // rep movsb length → assemble run u64 (and keep partial bytes on reject)
    let mut run: u64 = 0;
    let mut i: u32 = 0;
    while i < length_len {
        let b = unsafe { *esi };
        esi = unsafe { esi.add(1) };
        run |= (b as u64) << (8 * i);
        // Partial-mutate path needs the raw prefix in memory before reject.
        unsafe {
            *buffer.add(i as usize) = b;
        }
        i += 1;
    }

    let last_len_byte = unsafe { *esi.sub(1) };
    if last_len_byte >= 0x80 {
        unsafe { *esi_inout = esi };
        return 0;
    }

    // Zero-padded run size store (full 8 bytes).
    unsafe {
        store_u64_le(buffer, run);
    }

    let cluster_len = (header >> 4) as u32;
    if cluster_len > 8 {
        unsafe { *esi_inout = esi };
        return 0;
    }

    let mut cluster: u64 = 0;
    i = 0;
    while i < cluster_len {
        let b = unsafe { *esi };
        esi = unsafe { esi.add(1) };
        cluster |= (b as u64) << (8 * i);
        i += 1;
    }

    let last_cluster_ref = unsafe { *esi.sub(1) };
    if last_cluster_ref >= 0x80 && cluster_len < 8 {
        cluster |= (!0u64) << (8 * cluster_len);
    }

    unsafe {
        store_u64_le(buffer.add(8), cluster);
        *esi_inout = esi;
    }
    1
}

#[inline(always)]
unsafe fn store_u64_le(dst: *mut u8, v: u64) {
    let bytes = v.to_le_bytes();
    unsafe {
        *dst = bytes[0];
        *dst.add(1) = bytes[1];
        *dst.add(2) = bytes[2];
        *dst.add(3) = bytes[3];
        *dst.add(4) = bytes[4];
        *dst.add(5) = bytes[5];
        *dst.add(6) = bytes[6];
        *dst.add(7) = bytes[7];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FASM-faithful host oracle (separately coded control flow for differential).
    fn fasm_oracle(entry: &[u8], buf_in: [u8; 16]) -> (bool, u32, [u8; 16]) {
        let mut buf = buf_in;
        if entry.is_empty() {
            return (false, 0, buf);
        }
        let header = entry[0];
        if header == 0 {
            return (false, 1, buf);
        }
        let length_len = (header & 0x0F) as usize;
        if length_len > 8 {
            return (false, 1, buf);
        }
        if entry.len() < 1 + length_len {
            return (false, 1, buf);
        }
        for i in 0..length_len {
            buf[i] = entry[1 + i];
        }
        let last_len_byte = if length_len == 0 {
            header
        } else {
            entry[length_len]
        };
        if last_len_byte >= 0x80 {
            return (false, (1 + length_len) as u32, buf);
        }
        for i in length_len..8 {
            buf[i] = 0;
        }
        let cluster_len = (header >> 4) as usize;
        if cluster_len > 8 {
            return (false, (1 + length_len) as u32, buf);
        }
        let cluster_off = 1 + length_len;
        if entry.len() < cluster_off + cluster_len {
            return (false, (1 + length_len) as u32, buf);
        }
        for i in 0..cluster_len {
            buf[8 + i] = entry[cluster_off + i];
        }
        let last_cluster_ref = if cluster_len == 0 {
            last_len_byte
        } else {
            entry[cluster_off + cluster_len - 1]
        };
        let fill: u8 = if last_cluster_ref >= 0x80 { 0xFF } else { 0x00 };
        for i in cluster_len..8 {
            buf[8 + i] = fill;
        }
        (true, (1 + length_len + cluster_len) as u32, buf)
    }

    fn check(entry: &[u8], fill: u8) {
        let mut buf = [fill; 16];
        let (omore, ocons, obuf) = fasm_oracle(entry, buf);
        let r = ntfs_decode_mcb_entry(entry, &mut buf);
        assert_eq!(r.more, omore, "more mismatch entry={entry:02x?}");
        assert_eq!(r.consumed, ocons, "consumed mismatch entry={entry:02x?}");
        assert_eq!(buf, obuf, "buf mismatch entry={entry:02x?}");
    }

    #[test]
    fn end_marker_header_zero() {
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&[0x00], &mut buf);
        assert!(!r.more);
        assert_eq!(r.consumed, 1);
        assert_eq!(buf, [0xA5; 16]);
    }

    #[test]
    fn simple_positive_run() {
        let entry = [0x11, 0x05, 0x02];
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(r.more);
        assert_eq!(r.consumed, 3);
        assert_eq!(&buf[0..8], &[0x05, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&buf[8..16], &[0x02, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn negative_cluster_delta() {
        let entry = [0x11, 0x03, 0xFE];
        let mut buf = [0x00; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(r.more);
        assert_eq!(&buf[8..16], &[0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn length_high_bit_reject_partial() {
        let entry = [0x11, 0x80, 0x01];
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(!r.more);
        assert_eq!(r.consumed, 2);
        assert_eq!(buf[0], 0x80);
        assert_eq!(&buf[1..], &[0xA5; 15]);
    }

    #[test]
    fn length_nibble_too_large() {
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&[0x09, 0, 0, 0, 0, 0, 0, 0, 0, 0], &mut buf);
        assert!(!r.more);
        assert_eq!(r.consumed, 1);
        assert_eq!(buf, [0xA5; 16]);
    }

    #[test]
    fn cluster_nibble_too_large() {
        let entry = [0x91, 0x05];
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(!r.more);
        assert_eq!(r.consumed, 2);
        assert_eq!(&buf[0..8], &[0x05, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&buf[8..16], &[0xA5; 8]);
    }

    #[test]
    fn zero_length_nibble_uses_header_for_high_bit() {
        let entry = [0x10, 0x01];
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(r.more);
        assert_eq!(r.consumed, 2);
        assert_eq!(&buf[0..8], &[0; 8]);
        assert_eq!(&buf[8..16], &[0x01, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn zero_length_nibble_header_high_bit_rejects() {
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&[0x80], &mut buf);
        assert!(!r.more);
        assert_eq!(r.consumed, 1);
        assert_eq!(buf, [0xA5; 16]);
    }

    #[test]
    fn zero_cluster_len_sign_from_prior_byte() {
        let entry = [0x01, 0x05];
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(r.more);
        assert_eq!(r.consumed, 2);
        assert_eq!(&buf[0..8], &[0x05, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&buf[8..16], &[0; 8]);
    }

    #[test]
    fn max_sizes_8_8() {
        let mut entry = [0u8; 17];
        entry[0] = 0x88;
        for i in 0..8 {
            entry[1 + i] = 0x10 + i as u8;
            entry[9 + i] = 0x20 + i as u8;
        }
        let mut buf = [0xA5; 16];
        let r = ntfs_decode_mcb_entry(&entry, &mut buf);
        assert!(r.more);
        assert_eq!(r.consumed, 17);
        assert_eq!(&buf[0..8], &entry[1..9]);
        assert_eq!(&buf[8..16], &entry[9..17]);
    }

    #[test]
    fn named_cases_match_oracle() {
        let cases: &[&[u8]] = &[
            &[0x00],
            &[0x11, 0x01, 0x02],
            &[0x11, 0x01, 0xFE],
            &[0x21, 0x05, 0x01, 0x02],
            &[0x12, 0x34, 0x56, 0x78],
            &[0x01, 0x00],
            &[0x10, 0x00],
            &[0x0F],
            &[0xF1, 0x01],
            &[0x11, 0xFF, 0x01],
            &[0x22, 0x01, 0x02, 0x03, 0x80],
            &[0x88, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        ];
        for e in cases {
            check(e, 0xA5);
            check(e, 0x00);
            check(e, 0xFF);
        }
    }

    #[test]
    fn exhaustive_small_headers_differential() {
        let patterns: &[&[u8]] = &[
            &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10],
            &[0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F],
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
            &[0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00],
            &[0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF],
            &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            &[0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7, 0xF6, 0xF5, 0xF4, 0xF3, 0xF2, 0xF1, 0xF0, 0xEF],
        ];
        for header in 0u8..=255 {
            let llen = (header & 0x0F) as usize;
            let clen = (header >> 4) as usize;
            let need = 1 + llen.min(8) + clen.min(8);
            for pat in patterns {
                let mut entry = vec![header];
                entry.extend_from_slice(&pat[..need.saturating_sub(1).min(pat.len())]);
                while entry.len() < need {
                    entry.push(0x5A);
                }
                check(&entry, 0xA5);
            }
        }
    }

    #[test]
    fn prng_differential_200k() {
        const SEED: u32 = 0xC07B10D;
        let mut state = SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..200_000 {
            let header = (next() & 0xFF) as u8;
            let llen = (header & 0x0F) as usize;
            let clen = (header >> 4) as usize;
            let need = 1 + llen.min(8) + clen.min(8);
            let mut entry = vec![header];
            while entry.len() < need.max(1) {
                entry.push((next() & 0xFF) as u8);
            }
            let fill = (next() & 0xFF) as u8;
            check(&entry, fill);
        }
    }

    #[test]
    fn ptr_wrapper_advances_esi() {
        let entry = [0x11u8, 0x05, 0x02, 0x99];
        let mut buf = [0xA5u8; 16];
        let mut ptr = entry.as_ptr() as *mut u8;
        let more = unsafe { ntfs_decode_mcb_entry_ptr(&mut ptr, buf.as_mut_ptr()) };
        assert_eq!(more, 1);
        assert_eq!(ptr as usize, entry.as_ptr() as usize + 3);
        assert_eq!(&buf[0..8], &[0x05, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&buf[8..16], &[0x02, 0, 0, 0, 0, 0, 0, 0]);
    }
}
