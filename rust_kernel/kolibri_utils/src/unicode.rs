//! Unicode helpers matching `kernel/unicode.inc`.
//!
//! CP866 mapping matches `uni2ansi_char` in `kernel/fs/parse_fn.inc` (embedded here so
//! `unicode.cp866.encode` does not FFI back into FASM).

/// Result of one `unicode.utf8.decode` step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf8DecodeStep {
    pub codepoint: u32,
    pub consumed: usize,
}

/// Decode one UTF-8 scalar from `input`, matching FASM `unicode.utf8.decode`.
///
/// On empty input, returns `None` (FASM jumps to `.done` without writing `EAX`).
/// On error, returns codepoint `0xFFFD` and `consumed == 1`.
///
/// Inlined into the FFI entry so `.text.rust_unicode_utf8_decode` stays
/// self-contained (no cross-section call / relocation) for FASM embedding.
/// FASM does **not** reject overlongs, surrogates, or `> U+10FFFF` beyond
/// what the bit-shift parse produces — this mirrors that behavior.
#[inline(always)]
pub fn utf8_decode(input: &[u8]) -> Option<Utf8DecodeStep> {
    if input.is_empty() {
        return None;
    }

    let mut eax = u32::from(input[0]);
    let esi = 0usize;
    let ecx = input.len();

    // test al, al / jns .read1
    if (eax as u8) & 0x80 == 0 {
        return Some(Utf8DecodeStep {
            codepoint: eax,
            consumed: 1,
        });
    }

    // shl al, 2 / jnc .error
    let mut al = eax as u8;
    let mut cf = (al >> 6) & 1 == 1;
    al = al.wrapping_shl(2);
    eax = (eax & 0xffff_ff00) | u32::from(al);
    if !cf {
        return Some(error_step());
    }

    // shl al, 1 / jnc .read2
    cf = (al >> 7) & 1 == 1;
    al = al.wrapping_shl(1);
    eax = (eax & 0xffff_ff00) | u32::from(al);
    if !cf {
        return Some(read2(input, eax, esi, ecx));
    }

    // shl al, 1 / jnc .read3
    cf = (al >> 7) & 1 == 1;
    al = al.wrapping_shl(1);
    eax = (eax & 0xffff_ff00) | u32::from(al);
    if !cf {
        return Some(read3(input, eax, esi, ecx));
    }

    // shl al, 1 / jnc .read4
    cf = (al >> 7) & 1 == 1;
    al = al.wrapping_shl(1);
    eax = (eax & 0xffff_ff00) | u32::from(al);
    if !cf {
        return Some(read4(input, eax, esi, ecx));
    }

    Some(error_step())
}

#[inline(always)]
fn error_step() -> Utf8DecodeStep {
    Utf8DecodeStep {
        codepoint: 0xFFFD,
        consumed: 1,
    }
}

/// Continuation check: `shl al,1; jnc err; shl al,1; jc err`.
/// Returns the transformed `al` after both shifts (as FASM leaves it).
#[inline(always)]
fn cont_ok(al_in: u8) -> Option<u8> {
    let mut al = al_in;
    let cf1 = (al >> 7) & 1 == 1;
    al = al.wrapping_shl(1);
    if !cf1 {
        return None;
    }
    let cf2 = (al >> 7) & 1 == 1;
    al = al.wrapping_shl(1);
    if cf2 {
        return None;
    }
    Some(al)
}

#[inline(always)]
fn read2(input: &[u8], mut eax: u32, esi: usize, ecx: usize) -> Utf8DecodeStep {
    if ecx < 2 {
        return error_step();
    }
    eax <<= 5;
    let c1 = input[esi + 1];
    eax = (eax & 0xffff_ff00) | u32::from(c1);
    let Some(new_al) = cont_ok(c1) else {
        return error_step();
    };
    eax = (eax & 0xffff_ff00) | u32::from(new_al);
    eax >>= 2;
    Utf8DecodeStep {
        codepoint: eax,
        consumed: 2,
    }
}

#[inline(always)]
fn read3(input: &[u8], mut eax: u32, esi: usize, ecx: usize) -> Utf8DecodeStep {
    if ecx < 3 {
        return error_step();
    }
    eax <<= 4;
    let c1 = input[esi + 1];
    eax = (eax & 0xffff_ff00) | u32::from(c1);
    let Some(new_al) = cont_ok(c1) else {
        return error_step();
    };
    eax = (eax & 0xffff_ff00) | u32::from(new_al);
    eax <<= 6;
    let c2 = input[esi + 2];
    eax = (eax & 0xffff_ff00) | u32::from(c2);
    let Some(new_al) = cont_ok(c2) else {
        return error_step();
    };
    eax = (eax & 0xffff_ff00) | u32::from(new_al);
    eax >>= 2;
    Utf8DecodeStep {
        codepoint: eax,
        consumed: 3,
    }
}

#[inline(always)]
fn read4(input: &[u8], mut eax: u32, esi: usize, ecx: usize) -> Utf8DecodeStep {
    if ecx < 4 {
        return error_step();
    }
    eax <<= 3;
    let c1 = input[esi + 1];
    eax = (eax & 0xffff_ff00) | u32::from(c1);
    let Some(new_al) = cont_ok(c1) else {
        return error_step();
    };
    eax = (eax & 0xffff_ff00) | u32::from(new_al);
    eax <<= 6;
    let c2 = input[esi + 2];
    eax = (eax & 0xffff_ff00) | u32::from(c2);
    let Some(new_al) = cont_ok(c2) else {
        return error_step();
    };
    eax = (eax & 0xffff_ff00) | u32::from(new_al);
    eax <<= 6;
    let c3 = input[esi + 3];
    eax = (eax & 0xffff_ff00) | u32::from(c3);
    let Some(new_al) = cont_ok(c3) else {
        return error_step();
    };
    eax = (eax & 0xffff_ff00) | u32::from(new_al);
    eax >>= 2;
    Utf8DecodeStep {
        codepoint: eax,
        consumed: 4,
    }
}

/// Encode one code point to UTF-16 as FASM `unicode.utf16.encode` packs into `EAX`.
///
/// Inlined into the FFI entry so `.text.rust_unicode_utf16_encode` stays
/// self-contained (no cross-section call / relocation) for FASM embedding.
#[inline(always)]
pub fn utf16_encode(cp: u32) -> u32 {
    if cp >= 0x11_0000 {
        return 0xFFFD;
    }
    if cp >= 0x1_0000 {
        let mut eax = cp - 0x1_0000;
        eax <<= 6;
        let ax = (eax as u16) >> 6;
        eax = (eax & 0xffff_0000) | u32::from(ax);
        eax = eax.rotate_right(16);
        eax |= 0xDC00_D800;
        return eax;
    }
    if cp >= 0xE000 {
        return cp;
    }
    if cp < 0xD800 {
        return cp;
    }
    0xFFFD
}

/// Encode Unicode to CP866 byte (low 8 bits), matching `uni2ansi_char`.
///
/// Inlined into the FFI entry so `.text.rust_unicode_cp866_encode` stays
/// self-contained (no cross-section call / `.rodata` / relocation) for FASM embedding.
/// Mapping uses immediates only — no lookup table in `.rodata`.
#[inline(always)]
pub fn cp866_encode(cp: u32) -> u32 {
    let ax = cp as u16;
    if ax < 0x80 {
        return u32::from(ax as u8);
    }
    if ax == 0xB6 {
        return 20;
    }
    if ax < 0x400 {
        return u32::from(b'_');
    }
    if ax < 0x410 {
        return special_40x(ax);
    }
    if ax < 0x440 {
        return u32::from((ax as u8).wrapping_add(0x70));
    }
    if ax < 0x450 {
        return u32::from((ax as u8).wrapping_add(0xA0));
    }
    if ax < 0x460 {
        return special_40x(ax);
    }
    u32::from(b'_')
}

/// FASM `.table db 1, 51h, 4, 54h, 7, 57h, 0Eh, 5Eh` + `repnz scasb` → `0xF7 - remaining_ecx`.
///
/// Table is materialized on the stack with volatile stores so LLVM cannot
/// promote it to `.rodata` (which would add GOTOFF relocations and break the
/// reloc-free FASM blob extract used by CRC/UTF-16).
#[inline(always)]
fn special_40x(ax: u16) -> u32 {
    let al = ax as u8;
    let mut table = [0u8; 8];
    // SAFETY: `table` is a local array; indices 0..8 are in bounds.
    unsafe {
        let p = table.as_mut_ptr();
        core::ptr::write_volatile(p.add(0), 0x01);
        core::ptr::write_volatile(p.add(1), 0x51);
        core::ptr::write_volatile(p.add(2), 0x04);
        core::ptr::write_volatile(p.add(3), 0x54);
        core::ptr::write_volatile(p.add(4), 0x07);
        core::ptr::write_volatile(p.add(5), 0x57);
        core::ptr::write_volatile(p.add(6), 0x0E);
        core::ptr::write_volatile(p.add(7), 0x5E);
    }
    let mut ecx: u8 = 8;
    let mut i = 0usize;
    while i < 8 {
        ecx = ecx.wrapping_sub(1);
        // SAFETY: `i` is 0..8.
        let t = unsafe { core::ptr::read_volatile(table.as_ptr().add(i)) };
        if t == al {
            return u32::from(0xF7u8.wrapping_sub(ecx));
        }
        i += 1;
    }
    u32::from(b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-faithful oracle for utf16 (mirrors `unicode.inc` control flow).
    fn oracle_utf16(mut eax: u32) -> u32 {
        if eax >= 0x11_0000 {
            return 0xFFFD;
        }
        if eax >= 0x1_0000 {
            eax -= 0x1_0000;
            eax <<= 6;
            let ax = (eax as u16) >> 6;
            eax = (eax & 0xffff_0000) | u32::from(ax);
            eax = eax.rotate_right(16);
            return eax | 0xDC00_D800;
        }
        if eax >= 0xE000 {
            return eax;
        }
        if eax < 0xD800 {
            return eax;
        }
        0xFFFD
    }

    #[test]
    fn utf8_ascii() {
        let s = utf8_decode(b"A").unwrap();
        assert_eq!(s.codepoint, b'A' as u32);
        assert_eq!(s.consumed, 1);
    }

    #[test]
    fn utf8_empty() {
        assert!(utf8_decode(b"").is_none());
    }

    #[test]
    fn utf8_cyrillic_a() {
        let s = utf8_decode(&[0xD0, 0x90]).unwrap();
        assert_eq!(s.codepoint, 0x0410);
        assert_eq!(s.consumed, 2);
    }

    #[test]
    fn utf8_euro() {
        let s = utf8_decode(&[0xE2, 0x82, 0xAC]).unwrap();
        assert_eq!(s.codepoint, 0x20AC);
        assert_eq!(s.consumed, 3);
    }

    #[test]
    fn utf8_emoji() {
        let s = utf8_decode(&[0xF0, 0x9F, 0x98, 0x80]).unwrap();
        assert_eq!(s.codepoint, 0x1F600);
        assert_eq!(s.consumed, 4);
    }

    #[test]
    fn utf8_truncated_error() {
        let s = utf8_decode(&[0xD0]).unwrap();
        assert_eq!(s.codepoint, 0xFFFD);
        assert_eq!(s.consumed, 1);
    }

    #[test]
    fn utf8_invalid_cont() {
        let s = utf8_decode(&[0xD0, 0x20]).unwrap();
        assert_eq!(s.codepoint, 0xFFFD);
        assert_eq!(s.consumed, 1);
    }

    #[test]
    fn utf8_invalid_lead_fe() {
        let s = utf8_decode(&[0xFE]).unwrap();
        assert_eq!(s.codepoint, 0xFFFD);
        assert_eq!(s.consumed, 1);
    }

    #[test]
    fn utf8_nul() {
        let s = utf8_decode(&[0x00, 0x41]).unwrap();
        assert_eq!(s.codepoint, 0);
        assert_eq!(s.consumed, 1);
    }

    /// Independent second transcription of FASM `unicode.utf8.decode`.
    ///
    /// Mirrors register ops on `EAX`/`AL` + length gate + error consume-1.
    /// Does **not** call production helpers — used as differential oracle.
    fn oracle_utf8(input: &[u8]) -> Option<Utf8DecodeStep> {
        if input.is_empty() {
            return None;
        }
        let mut eax = u32::from(input[0]);
        let ecx = input.len();

        if (eax as u8) & 0x80 == 0 {
            return Some(Utf8DecodeStep {
                codepoint: eax,
                consumed: 1,
            });
        }

        let mut al = eax as u8;
        // shl al,2 / jnc .error  → CF was bit6 of original
        let cf = (al & 0x40) != 0;
        al = al.wrapping_shl(2);
        eax = (eax & !0xff) | u32::from(al);
        if !cf {
            return Some(Utf8DecodeStep {
                codepoint: 0xFFFD,
                consumed: 1,
            });
        }

        // shl al,1 / jnc .read2
        let cf = (al & 0x80) != 0;
        al = al.wrapping_shl(1);
        eax = (eax & !0xff) | u32::from(al);
        if !cf {
            return Some(oracle_read_n(input, eax, ecx, 2));
        }

        let cf = (al & 0x80) != 0;
        al = al.wrapping_shl(1);
        eax = (eax & !0xff) | u32::from(al);
        if !cf {
            return Some(oracle_read_n(input, eax, ecx, 3));
        }

        let cf = (al & 0x80) != 0;
        al = al.wrapping_shl(1);
        eax = (eax & !0xff) | u32::from(al);
        if !cf {
            return Some(oracle_read_n(input, eax, ecx, 4));
        }

        Some(Utf8DecodeStep {
            codepoint: 0xFFFD,
            consumed: 1,
        })
    }

    fn oracle_cont(al_in: u8) -> Option<u8> {
        let mut al = al_in;
        let cf1 = (al & 0x80) != 0;
        al = al.wrapping_shl(1);
        if !cf1 {
            return None;
        }
        let cf2 = (al & 0x80) != 0;
        al = al.wrapping_shl(1);
        if cf2 {
            return None;
        }
        Some(al)
    }

    fn oracle_read_n(input: &[u8], mut eax: u32, ecx: usize, n: usize) -> Utf8DecodeStep {
        if ecx < n {
            return Utf8DecodeStep {
                codepoint: 0xFFFD,
                consumed: 1,
            };
        }
        // FASM: read2 shl eax,5; read3 shl eax,4; read4 shl eax,3
        let first_shift = match n {
            2 => 5u32,
            3 => 4,
            4 => 3,
            _ => unreachable!(),
        };
        eax <<= first_shift;
        for k in 1..n {
            let c = input[k];
            eax = (eax & !0xff) | u32::from(c);
            let Some(new_al) = oracle_cont(c) else {
                return Utf8DecodeStep {
                    codepoint: 0xFFFD,
                    consumed: 1,
                };
            };
            eax = (eax & !0xff) | u32::from(new_al);
            if k + 1 < n {
                eax <<= 6;
            }
        }
        eax >>= 2;
        Utf8DecodeStep {
            codepoint: eax,
            consumed: n,
        }
    }

    #[test]
    fn utf8_matches_oracle_named_vectors() {
        let cases: &[&[u8]] = &[
            b"",
            b"A",
            b"Z",
            b"0",
            &[0x00],
            &[0x7F],
            &[0x80], // lone continuation → error
            &[0xC2, 0x80], // U+0080
            &[0xDF, 0xBF], // U+07FF
            &[0xE0, 0xA0, 0x80], // U+0800
            &[0xEF, 0xBF, 0xBF], // U+FFFF
            &[0xF0, 0x90, 0x80, 0x80], // U+10000
            &[0xF4, 0x8F, 0xBF, 0xBF], // U+10FFFF
            &[0xF0, 0x9F, 0x98, 0x80], // U+1F600
            &[0xD0], // truncated 2-byte
            &[0xE2, 0x82], // truncated 3-byte
            &[0xF0, 0x9F, 0x98], // truncated 4-byte
            &[0xD0, 0x20], // bad continuation
            &[0xE2, 0x20, 0xAC],
            &[0xF0, 0x20, 0x98, 0x80],
            &[0xC0, 0x80], // overlong NUL (FASM accepts bit-parse result)
            &[0xED, 0xA0, 0x80], // U+D800 surrogate encoding (FASM accepts)
            &[0xF4, 0x90, 0x80, 0x80], // > U+10FFFF bit-parse (FASM accepts)
            &[0xF8], // 5-byte lead → error
            &[0xFE],
            &[0xFF],
            &[0xC2], // truncated
            &[0xC2, 0xC0], // cont with high bits wrong
        ];
        for input in cases {
            assert_eq!(
                utf8_decode(input),
                oracle_utf8(input),
                "input={input:02x?}"
            );
        }
    }

    /// Exhaustive: every 1-byte buffer and every 2-byte buffer.
    #[test]
    fn utf8_exhaustive_len1_len2_matches_oracle() {
        for b0 in 0u8..=255 {
            let input = [b0];
            assert_eq!(
                utf8_decode(&input),
                oracle_utf8(&input),
                "len1 {b0:#04x}"
            );
        }
        for b0 in 0u8..=255 {
            for b1 in 0u8..=255 {
                let input = [b0, b1];
                assert_eq!(
                    utf8_decode(&input),
                    oracle_utf8(&input),
                    "len2 {b0:#04x} {b1:#04x}"
                );
            }
        }
    }

    /// Exhaustive 3-byte for leads that select `.read3` (0xE0..=0xEF), plus
    /// sampled other leads; full 3-byte space is 16M and too heavy for unit CI.
    #[test]
    fn utf8_exhaustive_3byte_read3_leads_matches_oracle() {
        for b0 in 0xE0u8..=0xEF {
            for b1 in 0u8..=255 {
                for b2 in 0u8..=255 {
                    let input = [b0, b1, b2];
                    assert_eq!(
                        utf8_decode(&input),
                        oracle_utf8(&input),
                        "len3 {b0:#04x}{b1:#04x}{b2:#04x}"
                    );
                }
            }
        }
    }

    /// Exhaustive 4-byte for leads that select `.read4` (0xF0..=0xF7).
    /// 8×256×256×256 = 134M — too large. Cover all cont bytes for boundary
    /// leads + full third/fourth for fixed second-byte samples.
    #[test]
    fn utf8_exhaustive_4byte_boundary_leads_matches_oracle() {
        for &b0 in &[0xF0u8, 0xF1, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7] {
            // Fix two continuation patterns: all-valid-ish and all-invalid.
            for b1 in 0u8..=255 {
                for &b2 in &[0x00u8, 0x80, 0xBF, 0xC0, 0xFF] {
                    for &b3 in &[0x00u8, 0x80, 0xBF, 0xC0, 0xFF] {
                        let input = [b0, b1, b2, b3];
                        assert_eq!(
                            utf8_decode(&input),
                            oracle_utf8(&input),
                            "len4 {input:02x?}"
                        );
                    }
                }
            }
        }
        // Full sweep of last two bytes for U+1F600 lead prefix and U+10FFFF.
        for b2 in 0u8..=255 {
            for b3 in 0u8..=255 {
                for input in [[0xF0, 0x9F, b2, b3], [0xF4, 0x8F, b2, b3]] {
                    assert_eq!(
                        utf8_decode(&input),
                        oracle_utf8(&input),
                        "len4tail {input:02x?}"
                    );
                }
            }
        }
    }

    #[test]
    fn utf8_ascii_range_and_boundaries() {
        for &ch in &[b'A', b'Z', b'0', 0x7Fu8] {
            let s = utf8_decode(&[ch]).unwrap();
            assert_eq!(s.codepoint, u32::from(ch));
            assert_eq!(s.consumed, 1);
        }
        let u80 = utf8_decode(&[0xC2, 0x80]).unwrap();
        assert_eq!(u80.codepoint, 0x80);
        assert_eq!(u80.consumed, 2);
        let u7ff = utf8_decode(&[0xDF, 0xBF]).unwrap();
        assert_eq!(u7ff.codepoint, 0x07FF);
        let u800 = utf8_decode(&[0xE0, 0xA0, 0x80]).unwrap();
        assert_eq!(u800.codepoint, 0x0800);
        let uffff = utf8_decode(&[0xEF, 0xBF, 0xBF]).unwrap();
        assert_eq!(uffff.codepoint, 0xFFFF);
        let u10000 = utf8_decode(&[0xF0, 0x90, 0x80, 0x80]).unwrap();
        assert_eq!(u10000.codepoint, 0x10000);
        let u10ffff = utf8_decode(&[0xF4, 0x8F, 0xBF, 0xBF]).unwrap();
        assert_eq!(u10ffff.codepoint, 0x10FFFF);
    }

    #[test]
    fn utf16_matches_oracle_boundaries() {
        for cp in [
            0u32,
            0x7f,
            0x80,
            0x7ff,
            0x800,
            0xd7ff,
            0xd800,
            0xd801,
            0xdbff,
            0xdc00,
            0xdfff,
            0xe000,
            0xe001,
            0xfffd,
            0xffff,
            0x10000,
            0x10001,
            0x1f600,
            0x10ffff,
            0x110000,
            0x110001,
            0xffff_ffff,
        ] {
            assert_eq!(utf16_encode(cp), oracle_utf16(cp), "cp={cp:#x}");
        }
    }

    /// Exhaustive differential vs FASM-faithful oracle over the full API domain.
    ///
    /// Covers BMP, surrogates, supplementary plane through U+10FFFF, and a
    /// band of invalid values at/above 0x110000 (FASM rejects with 0xFFFD).
    #[test]
    fn utf16_exhaustive_matches_oracle() {
        // Valid Unicode scalar range for this API is checked as 0..0x110000;
        // also cover 0x110000..0x120000 and a few high sentinels.
        for cp in 0u32..=0x120_000 {
            assert_eq!(utf16_encode(cp), oracle_utf16(cp), "cp={cp:#x}");
        }
        for cp in [0x200_000u32, 0x7fff_ffff, 0x8000_0000, 0xffff_fffe, 0xffff_ffff] {
            assert_eq!(utf16_encode(cp), oracle_utf16(cp), "cp={cp:#x}");
        }
    }

    #[test]
    fn utf16_supplementary_words() {
        let enc = utf16_encode(0x1F600);
        assert_eq!(enc as u16, 0xD83D);
        assert_eq!((enc >> 16) as u16, 0xDE00);
        // U+10000 → high sur 0xD800, low sur 0xDC00 packed as FASM does.
        assert_eq!(utf16_encode(0x10000), 0xDC00_D800);
        assert_eq!(utf16_encode(0xD800), 0xFFFD);
        assert_eq!(utf16_encode(0x41), 0x41);
    }

    /// Independent FASM-faithful oracle for CP866 (`uni2ansi_char` in parse_fn.inc).
    fn oracle_cp866(cp: u32) -> u32 {
        let ax = cp as u16;
        if ax < 0x80 {
            return u32::from(ax as u8);
        }
        if ax == 0xB6 {
            return 20;
        }
        if ax < 0x400 {
            return u32::from(b'_');
        }
        if ax < 0x410 {
            return oracle_special_40x(ax);
        }
        if ax < 0x440 {
            return u32::from((ax as u8).wrapping_add(0x70));
        }
        if ax < 0x450 {
            return u32::from((ax as u8).wrapping_add(0xA0));
        }
        if ax < 0x460 {
            return oracle_special_40x(ax);
        }
        u32::from(b'_')
    }

    /// Second transcription of FASM `repnz scasb` over `.table`.
    fn oracle_special_40x(ax: u16) -> u32 {
        const TABLE: [u8; 8] = [0x01, 0x51, 0x04, 0x54, 0x07, 0x57, 0x0E, 0x5E];
        let al = ax as u8;
        let mut ecx: u8 = 8;
        for &t in &TABLE {
            ecx = ecx.wrapping_sub(1);
            if t == al {
                return u32::from(0xF7u8.wrapping_sub(ecx));
            }
        }
        u32::from(b'_')
    }

    #[test]
    fn cp866_ascii_and_cyrillic() {
        assert_eq!(cp866_encode(0x41) as u8, b'A');
        assert_eq!(cp866_encode(0x0410) as u8, 0x80);
        assert_eq!(cp866_encode(0x043F) as u8, 0xAF);
        assert_eq!(cp866_encode(0x0440) as u8, 0xE0);
        assert_eq!(cp866_encode(0x044F) as u8, 0xEF);
        assert_eq!(cp866_encode(0x0401) as u8, 0xF0);
        assert_eq!(cp866_encode(0x0451) as u8, 0xF1);
        assert_eq!(cp866_encode(0x0404) as u8, 0xF2);
        assert_eq!(cp866_encode(0x0454) as u8, 0xF3);
        assert_eq!(cp866_encode(0x0407) as u8, 0xF4);
        assert_eq!(cp866_encode(0x0457) as u8, 0xF5);
        assert_eq!(cp866_encode(0x040E) as u8, 0xF6);
        assert_eq!(cp866_encode(0x045E) as u8, 0xF7);
        assert_eq!(cp866_encode(0x00B6) as u8, 20);
        assert_eq!(cp866_encode(0x1234) as u8, b'_');
        assert_eq!(cp866_encode(0x0400) as u8, b'_');
        assert_eq!(cp866_encode(0x0402) as u8, b'_');
        assert_eq!(cp866_encode(0x0450) as u8, b'_');
    }

    #[test]
    fn cp866_matches_oracle_boundaries() {
        for cp in [
            0u32,
            0x7f,
            0x80,
            0xb5,
            0xb6,
            0xb7,
            0x3ff,
            0x400,
            0x401,
            0x40f,
            0x410,
            0x43f,
            0x440,
            0x44f,
            0x450,
            0x451,
            0x45f,
            0x460,
            0x461,
            0x7ff,
            0xffff,
            0x1_0000,
            0x1f600,
            0xffff_ffff,
        ] {
            assert_eq!(cp866_encode(cp), oracle_cp866(cp), "cp={cp:#x}");
        }
    }

    /// Exhaustive differential vs FASM-faithful oracle over the full `AX` domain.
    ///
    /// FASM `uni2ansi_char` only reads `AX`, so every meaningful input is in
    /// `0..=0xFFFF`. Also check a few high-bit sentinels (truncated like FASM).
    #[test]
    fn cp866_exhaustive_matches_oracle() {
        for cp in 0u32..=0xFFFF {
            assert_eq!(cp866_encode(cp), oracle_cp866(cp), "cp={cp:#x}");
        }
        for cp in [0x1_0000u32, 0x1_0041, 0x1f600, 0x10ffff, 0x8000_0041, 0xffff_ffff] {
            assert_eq!(cp866_encode(cp), oracle_cp866(cp), "cp={cp:#x}");
        }
    }

    #[test]
    fn roundtrip_name_style() {
        let name = "Тест".as_bytes();
        let mut i = 0;
        let mut out = std::vec::Vec::new();
        while i < name.len() {
            let step = utf8_decode(&name[i..]).unwrap();
            i += step.consumed;
            let enc = utf16_encode(step.codepoint);
            out.push(enc as u16);
            let hi = (enc >> 16) as u16;
            if hi != 0 {
                out.push(hi);
            }
        }
        assert_eq!(out.len(), 4);
    }
}
