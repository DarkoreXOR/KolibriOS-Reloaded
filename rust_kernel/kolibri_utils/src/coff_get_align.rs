//! Cut BK: `coff_get_align` — PE/COFF section Characteristics → alignment mask.
//!
//! Matches `kernel/core/dll.inc` FASM leaf semantics:
//! ```text
//!   mov cl, byte [edx + COFF_SECTION.Characteristics+2]
//!   mov eax, 1
//!   shr cl, 4
//!   dec cl
//!   js  .default
//!   cmp cl, 12
//!   jbe @f
//! .default:
//!   mov cl, 12
//! @@:
//!   shl eax, cl
//!   dec eax          ; mask = (1 << n) - 1
//! ```
//!
//! Rules (from FASM comments):
//! * align field absent / invalid (`js` after `dec`) → default 4K (`cl=12`)
//! * field encodes log2+1; after `dec`, clamp if `cl > 12` → 4K
//! * otherwise use `(1 << cl) - 1`
//!
//! Pure decode — no globals, locks, or `.rodata`.

/// Cut BK differential PRNG seed (`'CUBK'`).
pub const COFF_GET_ALIGN_PRNG_SEED: u32 = 0x4355_424B;

/// `COFF_SECTION.Characteristics` offset (`sizeof.COFF_SECTION` = 40).
pub const OFF_CHARACTERISTICS: usize = 36;

/// Byte within Characteristics that holds bits 16..23 (align nibble in high half).
pub const OFF_CHARACTERISTICS_ALIGN_BYTE: usize = OFF_CHARACTERISTICS + 2;

#[cfg(test)]
const SECTION_BUF_LEN: usize = 40;

/// Default / clamp alignment exponent (4K → mask `0xFFF`).
pub const ALIGN_EXP_DEFAULT: u32 = 12;

/// FASM-faithful alignment mask from Characteristics dword (LE).
///
/// Only bits 20..23 of `characteristics` matter (high nibble of byte at +2).
#[inline(always)]
pub fn coff_get_align_from_characteristics(characteristics: u32) -> u32 {
    // mov cl, byte [char+2]; shr cl, 4
    let mut cl = ((characteristics >> 16) & 0xff) >> 4;
    // dec cl; js .default  — when pre-dec field was 0, cl wraps to 0xFF
    if cl == 0 {
        cl = ALIGN_EXP_DEFAULT;
    } else {
        cl = cl.wrapping_sub(1);
        // cmp cl, 12; jbe @f / else default
        if cl > ALIGN_EXP_DEFAULT {
            cl = ALIGN_EXP_DEFAULT;
        }
    }
    // mov eax,1 / shl eax,cl / dec eax
    (1u32 << cl) - 1
}

/// Pointer form: read Characteristics from a `COFF_SECTION` and decode.
///
/// # Safety
/// `section` must point to a readable `COFF_SECTION` (at least through
/// `Characteristics`).
#[inline(always)]
pub unsafe fn coff_get_align_ptr(section: *const u8) -> u32 {
    // SAFETY: caller guarantees Characteristics dword is readable.
    let b = unsafe { core::slice::from_raw_parts(section.add(OFF_CHARACTERISTICS), 4) };
    let characteristics = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    coff_get_align_from_characteristics(characteristics)
}

/// Build a minimal synthetic section buffer with only Characteristics set.
#[cfg(test)]
pub fn make_section_chars(characteristics: u32) -> [u8; SECTION_BUF_LEN] {
    let mut buf = [0u8; SECTION_BUF_LEN];
    let bytes = characteristics.to_le_bytes();
    buf[OFF_CHARACTERISTICS..OFF_CHARACTERISTICS + 4].copy_from_slice(&bytes);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not a call to the SUT).
    fn oracle(characteristics: u32) -> u32 {
        // mov cl, byte [char+2]
        let mut cl = ((characteristics >> 16) as u8) as u32;
        // shr cl, 4
        cl >>= 4;
        // dec cl
        let (dec, _) = cl.overflowing_sub(1);
        // js .default — SF set when result bit7 of 8-bit cl is set
        let sf = (dec as u8) & 0x80 != 0;
        let exp = if sf {
            ALIGN_EXP_DEFAULT
        } else if dec > ALIGN_EXP_DEFAULT {
            ALIGN_EXP_DEFAULT
        } else {
            dec
        };
        (1u32 << exp) - 1
    }

    fn check(characteristics: u32) {
        let got = coff_get_align_from_characteristics(characteristics);
        let exp = oracle(characteristics);
        assert_eq!(
            got, exp,
            "mismatch chars={characteristics:#010x} got={got:#x} exp={exp:#x}"
        );
        let sec = make_section_chars(characteristics);
        let ptr_got = unsafe { coff_get_align_ptr(sec.as_ptr()) };
        assert_eq!(ptr_got, exp);
    }

    #[test]
    fn default_when_align_field_zero() {
        // bits 20..23 = 0 → after shr/dec → js → 4K
        check(0x0000_0000);
        check(0x000F_FFFF); // junk low bits ignored
        check(0xFF0F_FFFF); // high byte junk ignored
    }

    #[test]
    fn one_byte_align_mask_zero() {
        // IMAGE_SCN_ALIGN_1BYTES = 0x00100000 → field=1 → cl=0 → mask=0
        check(0x0010_0000);
        assert_eq!(coff_get_align_from_characteristics(0x0010_0000), 0);
    }

    #[test]
    fn two_byte_through_4k() {
        // field=2 → mask=1; field=3 → 3; ... field=13 → 0xFFF
        check(0x0020_0000);
        assert_eq!(coff_get_align_from_characteristics(0x0020_0000), 0x1);
        check(0x0030_0000);
        assert_eq!(coff_get_align_from_characteristics(0x0030_0000), 0x3);
        check(0x00C0_0000); // 2^11 → mask 0x7FF
        assert_eq!(coff_get_align_from_characteristics(0x00C0_0000), 0x7FF);
        check(0x00D0_0000); // 4K
        assert_eq!(coff_get_align_from_characteristics(0x00D0_0000), 0xFFF);
    }

    #[test]
    fn clamp_above_4k() {
        // field=14,15 → cl=13,14 → >12 → default 4K
        check(0x00E0_0000);
        assert_eq!(coff_get_align_from_characteristics(0x00E0_0000), 0xFFF);
        check(0x00F0_0000);
        assert_eq!(coff_get_align_from_characteristics(0x00F0_0000), 0xFFF);
    }

    #[test]
    fn low_nibble_of_align_byte_ignored() {
        // byte[2] = 0xD5 → shr 4 → 0xD; same as 0xD0
        check(0x00D5_0000);
        assert_eq!(
            coff_get_align_from_characteristics(0x00D5_0000),
            coff_get_align_from_characteristics(0x00D0_0000)
        );
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
    fn prng_50k_matches_oracle() {
        let mut state = COFF_GET_ALIGN_PRNG_SEED;
        for _ in 0..50_000 {
            check(xorshift32(&mut state));
        }
    }
}
