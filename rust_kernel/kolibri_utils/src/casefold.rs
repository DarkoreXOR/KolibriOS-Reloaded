//! Case folding matching `cp866toUpper` / `utf16toUpper` in `kernel/fs/parse_fn.inc`.

/// Convert one CP866 byte to uppercase, matching FASM `cp866toUpper`.
///
/// Inlined into the FFI entry so `.text.rust_cp866_to_upper` stays
/// self-contained (no cross-section call / relocation) for FASM embedding.
#[inline(always)]
pub fn cp866_to_upper(ch: u8) -> u8 {
    if ch < b'a' {
        return ch;
    }
    if ch <= b'z' {
        return ch.wrapping_sub(32);
    }
    if ch < 0xA0 {
        return ch;
    }
    if ch < 0xB0 {
        return ch.wrapping_sub(32);
    }
    if ch < 0xE0 {
        return ch;
    }
    if ch < 0xF0 {
        // 0xE0..0xEF → 0x90..0x9F
        return ch.wrapping_sub(0xE0 - 0x90);
    }
    if ch > 0xF7 {
        return ch;
    }
    // 0xF0..0xF7: clear low bit (ё→Ё and similar pairs)
    ch & !1
}

/// Convert one UTF-16 code unit to uppercase, matching FASM `utf16toUpper`.
///
/// Covers Latin `'a'..'z'`, Cyrillic `U+0430..U+044F` (−32), and
/// `U+0450..U+045F` (−80). Inlined into the FFI entry so
/// `.text.rust_utf16_to_upper` stays reloc-free.
#[inline(always)]
pub fn utf16_to_upper(ch: u16) -> u16 {
    if ch < b'a' as u16 {
        return ch;
    }
    if ch <= b'z' as u16 {
        return ch.wrapping_sub(32);
    }
    if ch < 0x0430 {
        return ch;
    }
    if ch < 0x0450 {
        return ch.wrapping_sub(32);
    }
    if ch >= 0x0460 {
        return ch;
    }
    ch.wrapping_sub(80)
}

/// FASM-faithful oracle used by differential tests (mirrors parse_fn.inc control flow).
#[cfg(test)]
pub fn cp866_to_upper_fasm_oracle(ch: u8) -> u8 {
    let mut eax = u32::from(ch);
    let al = ch;

    if al < b'a' {
        return eax as u8;
    }
    if al <= b'z' {
        eax = eax.wrapping_sub(32);
        return eax as u8;
    }
    if al < 0xA0 {
        return eax as u8;
    }
    if al < 0xB0 {
        eax = eax.wrapping_sub(32);
        return eax as u8;
    }
    if al < 0xE0 {
        return eax as u8;
    }
    if al < 0xF0 {
        eax = eax.wrapping_sub(0xE0 - 0x90);
        return eax as u8;
    }
    if al > 0xF7 {
        return eax as u8;
    }
    eax &= !1u32;
    eax as u8
}

/// FASM-faithful oracle used by differential tests (mirrors parse_fn.inc control flow).
#[cfg(test)]
pub fn utf16_to_upper_fasm_oracle(ch: u16) -> u16 {
    let mut eax = u32::from(ch);
    let ax = ch;

    if ax < b'a' as u16 {
        return eax as u16;
    }
    if ax <= b'z' as u16 {
        eax = eax.wrapping_sub(32);
        return eax as u16;
    }
    if ax < 0x0430 {
        return eax as u16;
    }
    if ax < 0x0450 {
        eax = eax.wrapping_sub(32);
        return eax as u16;
    }
    if ax >= 0x0460 {
        return eax as u16;
    }
    eax = eax.wrapping_sub(80);
    eax as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_ascii() {
        assert_eq!(cp866_to_upper(b'a'), b'A');
        assert_eq!(cp866_to_upper(b'z'), b'Z');
        assert_eq!(cp866_to_upper(b'A'), b'A');
        assert_eq!(cp866_to_upper(b'@'), b'@');
        assert_eq!(cp866_to_upper(b'`'), b'`');
        assert_eq!(cp866_to_upper(b'{'), b'{');
    }

    #[test]
    fn cyrillic_cp866_ranges() {
        assert_eq!(cp866_to_upper(0xA0), 0x80);
        assert_eq!(cp866_to_upper(0xAF), 0x8F);
        assert_eq!(cp866_to_upper(0xE0), 0x90);
        assert_eq!(cp866_to_upper(0xEF), 0x9F);
        assert_eq!(cp866_to_upper(0x9F), 0x9F);
        assert_eq!(cp866_to_upper(0xB0), 0xB0);
        assert_eq!(cp866_to_upper(0xDF), 0xDF);
    }

    #[test]
    fn yo_pair_and_specials() {
        assert_eq!(cp866_to_upper(0xF0), 0xF0); // Ё
        assert_eq!(cp866_to_upper(0xF1), 0xF0); // ё → Ё
        assert_eq!(cp866_to_upper(0xF2), 0xF2);
        assert_eq!(cp866_to_upper(0xF3), 0xF2);
        assert_eq!(cp866_to_upper(0xF7), 0xF6);
        assert_eq!(cp866_to_upper(0xF8), 0xF8);
        assert_eq!(cp866_to_upper(0xFF), 0xFF);
        assert_eq!(cp866_to_upper(0x00), 0x00);
    }

    #[test]
    fn exhaustive_vs_fasm_oracle() {
        for ch in 0u8..=255 {
            assert_eq!(
                cp866_to_upper(ch),
                cp866_to_upper_fasm_oracle(ch),
                "mismatch at 0x{ch:02X}"
            );
        }
    }

    #[test]
    fn utf16_latin_ascii() {
        assert_eq!(utf16_to_upper(b'a' as u16), b'A' as u16);
        assert_eq!(utf16_to_upper(b'z' as u16), b'Z' as u16);
        assert_eq!(utf16_to_upper(b'A' as u16), b'A' as u16);
        assert_eq!(utf16_to_upper(b'@' as u16), b'@' as u16);
        assert_eq!(utf16_to_upper(0x60), 0x60);
        assert_eq!(utf16_to_upper(0x7B), 0x7B);
    }

    #[test]
    fn utf16_cyrillic_bmp_ranges() {
        // а (U+0430) → А (U+0410); я (U+044F) → Я (U+042F)
        assert_eq!(utf16_to_upper(0x0430), 0x0410);
        assert_eq!(utf16_to_upper(0x044F), 0x042F);
        assert_eq!(utf16_to_upper(0x042F), 0x042F);
        // ѐ (U+0450) → Ѐ (U+0400); ё (U+0451) → Ё (U+0401)
        assert_eq!(utf16_to_upper(0x0450), 0x0400);
        assert_eq!(utf16_to_upper(0x0451), 0x0401);
        assert_eq!(utf16_to_upper(0x045F), 0x040F);
        assert_eq!(utf16_to_upper(0x0460), 0x0460);
        assert_eq!(utf16_to_upper(0xFFFF), 0xFFFF);
        assert_eq!(utf16_to_upper(0x0000), 0x0000);
    }

    #[test]
    fn utf16_exhaustive_vs_fasm_oracle() {
        for ch in 0u16..=0xFFFF {
            assert_eq!(
                utf16_to_upper(ch),
                utf16_to_upper_fasm_oracle(ch),
                "mismatch at U+{ch:04X}"
            );
        }
    }
}
