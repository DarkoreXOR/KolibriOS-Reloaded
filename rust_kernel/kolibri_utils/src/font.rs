//! Cut N: `antiAliasing` — GUI font RGB anti-alias blend.
//!
//! Matches `kernel/gui/font.inc` FASM leaf semantics:
//! * three low bytes blended as `(3 * fg + bg) >> 2`
//! * processing via 8-bit rotates (high byte of the dword is not blended)
//! * a fourth rotate restores lane order
//!
//! The FASM body uses `BP` as a 16-bit loop counter and ends with
//! `mov ebp, ebx`. EBP restoration is owned by the FASM trampoline so the
//! Rust blob stays a pure `(fg, bg) -> blended` stdcall.

/// Cut N differential PRNG seed (documented).
pub const ANTI_ALIASING_PRNG_SEED: u32 = 0xAA11_A51A;

/// Blend one 8-bit channel: `(3 * fg + bg) >> 2` (unsigned, matches FASM
/// `lea ecx,[ecx*2+ecx]` / `add` / `shr 2` with zeroed high parts).
#[inline(always)]
fn blend_byte(fg: u8, bg: u8) -> u8 {
    let mixed = (u32::from(fg) * 3).wrapping_add(u32::from(bg));
    (mixed >> 2) as u8
}

/// KolibriOS font anti-aliasing color blend.
///
/// # Arguments
/// * `fg` — foreground RGB dword in `EAX` (typically a neighboring pixel).
/// * `bg` — background / font color dword in `EBX` (callers set `ebx = ebp`).
///
/// # Returns
/// Blended dword for `EAX`. Only the low three bytes are blended; the high
/// byte is preserved. Callers / trampoline set `EBP = EBX` separately.
#[inline(always)]
pub fn anti_aliasing(fg: u32, bg: u32) -> u32 {
    let mut eax = fg;
    let mut ebx = bg;

    // FASM: `mov bp, 3` / loop body / `dec bp` / `jnz`
    let mut bp: u16 = 3;
    loop {
        let fg_b = eax as u8;
        let bg_b = ebx as u8;
        let out = blend_byte(fg_b, bg_b);
        eax = (eax & 0xFFFF_FF00) | u32::from(out);
        eax = eax.rotate_right(8);
        ebx = ebx.rotate_right(8);
        bp = bp.wrapping_sub(1);
        if bp == 0 {
            break;
        }
    }

    // FASM: final `ror eax, 8` / `ror ebx, 8` (restore lane order).
    eax = eax.rotate_right(8);
    // ebx would also be restored; unused by return value.
    let _ = ebx.rotate_right(8);
    eax
}

#[cfg(test)]
mod tests {
    use super::{anti_aliasing, ANTI_ALIASING_PRNG_SEED};

    /// Independent step-by-step FASM oracle (mirrors font.inc:846–862) with
    /// ECX/EDX high parts treated as zero — matching `drawChar` call sites
    /// that `xor ecx, ecx` and keep EDX as a byte-sized glyph row.
    fn fasm_oracle(mut eax: u32, mut ebx: u32) -> u32 {
        let mut bp: u16 = 3;
        loop {
            let mut ecx = u32::from(eax as u8);
            let edx = u32::from(ebx as u8);
            ecx = ecx.wrapping_mul(3);
            ecx = ecx.wrapping_add(edx);
            ecx >>= 2;
            eax = (eax & !0xFFu32) | (ecx & 0xFF);
            eax = eax.rotate_right(8);
            ebx = ebx.rotate_right(8);
            bp = bp.wrapping_sub(1);
            if bp == 0 {
                break;
            }
        }
        eax = eax.rotate_right(8);
        ebx = ebx.rotate_right(8);
        let _ = ebx; // FASM then `mov ebp, ebx`
        eax
    }

    fn check(fg: u32, bg: u32) {
        let got = anti_aliasing(fg, bg);
        let exp = fasm_oracle(fg, bg);
        assert_eq!(
            got, exp,
            "fg={fg:#010x} bg={bg:#010x} got={got:#010x} exp={exp:#010x}"
        );
    }

    #[test]
    fn equal_colors_unchanged() {
        for c in [0u32, 1, 0xFF, 0x00AABBCC, 0xFFFFFFFF, 0x12345678] {
            check(c, c);
            assert_eq!(anti_aliasing(c, c), c);
        }
    }

    #[test]
    fn blend_toward_black() {
        // (3*0xFF + 0) >> 2 = 0xBF
        assert_eq!(anti_aliasing(0x0000_00FF, 0), 0x0000_00BF);
        // per-channel
        assert_eq!(anti_aliasing(0x00AA_BBCC, 0), 0x007F_8C99);
    }

    #[test]
    fn blend_toward_white() {
        // (3*0 + 0xFF) >> 2 = 0x3F
        assert_eq!(anti_aliasing(0, 0x0000_00FF), 0x0000_003F);
        assert_eq!(anti_aliasing(0, 0x00FF_FFFE), 0x003F_3F3F);
    }

    #[test]
    fn high_byte_preserved() {
        // Only low 3 bytes blend; high byte rotates through untouched.
        let fg = 0xA1_02_03_04;
        let bg = 0xB2_00_00_00;
        let out = anti_aliasing(fg, bg);
        assert_eq!(out >> 24, 0xA1);
        check(fg, bg);
    }

    #[test]
    fn named_boundaries() {
        for &(fg, bg) in &[
            (0u32, 0u32),
            (0xFF, 0),
            (0, 0xFF),
            (0xFF, 0xFF),
            (0x80, 0x80),
            (0x01, 0xFE),
            (0xFE, 0x01),
            (0x00FF_FFFF, 0),
            (0, 0x00FF_FFFF),
            (0xFFFF_FFFF, 0xFFFF_FFFF),
            (0x1234_5678, 0x9ABC_DEF0),
            (0xDEAD_BEEF, 0x0BAD_F00D),
        ] {
            check(fg, bg);
        }
    }

    #[test]
    fn grid_low_bytes() {
        // Exhaustive on low byte × sample high lanes.
        for hi in [0u32, 0x11_000_000, 0xFF00_0000, 0x00AA_0000] {
            for fg_b in 0u32..=255 {
                for bg_b in [0u32, 1, 2, 3, 7, 15, 16, 31, 32, 63, 64, 127, 128, 200, 254, 255]
                {
                    check(hi | fg_b, (hi.rotate_left(8)) | bg_b);
                }
            }
        }
    }

    #[test]
    fn exhaustive_single_channel_pairs() {
        for fg in 0u32..=255 {
            for bg in 0u32..=255 {
                check(fg, bg);
            }
        }
    }

    #[test]
    fn prng_200k() {
        // xorshift32
        let mut s = ANTI_ALIASING_PRNG_SEED;
        for _ in 0..200_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let fg = s;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let bg = s;
            check(fg, bg);
        }
    }
}
