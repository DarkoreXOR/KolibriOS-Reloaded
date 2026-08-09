//! CP866 case folding matching `cp866toUpper` in `kernel/fs/parse_fn.inc`.

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
}
