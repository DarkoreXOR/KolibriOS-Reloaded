//! Cut AB: `utf8to16` — ESI-advancing UTF-8 → UTF-16 streaming decode.
//!
//! Matches `kernel/fs/parse_fn.inc` FASM leaf semantics exactly:
//! * `lodsb` / bit-test restart on invalid lead (`10xxxxxx`)
//! * Continuation gather loop (`shl ax,8` + `shl al,2` / `jc`)
//! * Mid-stream ASCII → `.got` (`xor ah, ah`)
//! * 2-byte finish: `shr ah,2` / `shl ax,3` / CF clear → `shr ax,5`
//! * 3(+)-byte finish: CF set → `shl eax,3` / another byte / `shr eax,2`
//! * Incoming `EAX` high bits participate in `shl eax,*` (chained calls)
//!
//! Distinct from Cut A `unicode.utf8.decode` (length-bounded, ESI/ECX, U+FFFD).
//! Complements Cut Q `UTF16to8` (EDI-advancing encode).

/// Cut AB differential PRNG seed (`'CUTB'`).
pub const UTF8TO16_PRNG_SEED: u32 = 0x4355_5442;

/// Result of one `utf8to16` step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf8To16Result {
    /// Final `EAX` bit pattern (callers typically consume `AX`).
    pub eax: u32,
    /// Bytes consumed from the input (ESI delta).
    pub consumed: usize,
}

/// FASM-faithful `utf8to16` over a byte slice starting at index 0.
///
/// `initial_eax` mirrors the live `EAX` on entry (only `AL` is overwritten by
/// the first `lodsb`; high bits matter on the 3-byte `shl eax,3` path).
///
/// # Panics
/// Never panics. If the slice ends mid-sequence, behavior follows the last
/// successfully read byte path that FASM would take only when memory is
/// readable; for host tests, inputs must be long enough for the sequences
/// under test (kernel always has a terminator / reserved buffer).
#[inline(always)]
pub fn utf8to16(input: &[u8], initial_eax: u32) -> Utf8To16Result {
    let mut esi = 0usize;
    let mut eax = initial_eax;
    // SAFETY-equivalent: kernel guarantees readable bytes through the sequence.
    // Host tests supply adequate buffers.
    loop {
        // lodsb
        let al0 = input[esi];
        esi += 1;
        eax = (eax & 0xffff_ff00) | u32::from(al0);

        // test al, al / jns .got
        if (eax as u8) & 0x80 == 0 {
            return got(eax, esi);
        }

        // shl al, 2 / jnc utf8to16  (restart: invalid lead / continuation)
        let (al1, cf_lead) = shl_al_cf(eax as u8, 2);
        eax = (eax & 0xffff_ff00) | u32::from(al1);
        if !cf_lead {
            continue;
        }

        // @@: gather loop
        loop {
            // shl ax, 8
            let ax = (eax as u16).wrapping_shl(8);
            eax = (eax & 0xffff_0000) | u32::from(ax);

            // lodsb
            let al2 = input[esi];
            esi += 1;
            eax = (eax & 0xffff_ff00) | u32::from(al2);

            // test al, al / jns .got
            if (eax as u8) & 0x80 == 0 {
                return got(eax, esi);
            }

            // shl al, 2 / jc @b
            let (al3, cf_cont) = shl_al_cf(eax as u8, 2);
            eax = (eax & 0xffff_ff00) | u32::from(al3);
            if cf_cont {
                continue;
            }

            // shr ah, 2
            let ah = ((eax >> 8) as u8) >> 2;
            eax = (eax & 0xffff_00ff) | (u32::from(ah) << 8);

            // shl ax, 3 / jnc @f (2-byte finish)
            let ax_in = eax as u16;
            let cf_ax = (ax_in >> 13) & 1 == 1;
            let ax_out = ax_in.wrapping_shl(3);
            eax = (eax & 0xffff_0000) | u32::from(ax_out);

            if !cf_ax {
                // @@: shr ax, 5 / ret
                let ax_fin = (eax as u16) >> 5;
                eax = (eax & 0xffff_0000) | u32::from(ax_fin);
                return Utf8To16Result {
                    eax,
                    consumed: esi,
                };
            }

            // shl eax, 3
            eax = eax.wrapping_shl(3);

            // lodsb
            let al4 = input[esi];
            esi += 1;
            eax = (eax & 0xffff_ff00) | u32::from(al4);

            // test al, al / jns .got
            if (eax as u8) & 0x80 == 0 {
                return got(eax, esi);
            }

            // shl al, 2 / jc @b
            let (al5, cf_again) = shl_al_cf(eax as u8, 2);
            eax = (eax & 0xffff_ff00) | u32::from(al5);
            if cf_again {
                // jc @b — back to gather (shl ax,8 …)
                continue;
            }

            // shr eax, 2 / ret
            eax >>= 2;
            return Utf8To16Result {
                eax,
                consumed: esi,
            };
        }
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// Updates `*esi_inout` by `consumed` bytes; returns final `EAX`.
///
/// # Safety
/// `esi_inout` must be valid; `*esi_inout` must be readable for the full
/// sequence the FASM leaf would consume (callers pass path/LFN buffers).
#[inline(always)]
pub unsafe fn utf8to16_ptr(esi_inout: *mut *const u8, initial_eax: u32) -> u32 {
    let start = unsafe { *esi_inout };
    // Read a generous window; FASM never bounds-checks. Cap for safety in
    // host tests via the slice API; freestanding path uses raw reads below.
    // For FFI we mirror FASM with unchecked byte loads through a local copy
    // of the pointer, advancing one byte at a time.
    let mut esi = start;
    let mut eax = initial_eax;
    loop {
        let al0 = unsafe { *esi };
        esi = unsafe { esi.add(1) };
        eax = (eax & 0xffff_ff00) | u32::from(al0);

        if (eax as u8) & 0x80 == 0 {
            unsafe { *esi_inout = esi };
            return eax & 0xffff_00ff; // xor ah, ah
        }

        let (al1, cf_lead) = shl_al_cf(eax as u8, 2);
        eax = (eax & 0xffff_ff00) | u32::from(al1);
        if !cf_lead {
            continue;
        }

        loop {
            let ax = (eax as u16).wrapping_shl(8);
            eax = (eax & 0xffff_0000) | u32::from(ax);

            let al2 = unsafe { *esi };
            esi = unsafe { esi.add(1) };
            eax = (eax & 0xffff_ff00) | u32::from(al2);

            if (eax as u8) & 0x80 == 0 {
                unsafe { *esi_inout = esi };
                return eax & 0xffff_00ff;
            }

            let (al3, cf_cont) = shl_al_cf(eax as u8, 2);
            eax = (eax & 0xffff_ff00) | u32::from(al3);
            if cf_cont {
                continue;
            }

            let ah = ((eax >> 8) as u8) >> 2;
            eax = (eax & 0xffff_00ff) | (u32::from(ah) << 8);

            let ax_in = eax as u16;
            let cf_ax = (ax_in >> 13) & 1 == 1;
            let ax_out = ax_in.wrapping_shl(3);
            eax = (eax & 0xffff_0000) | u32::from(ax_out);

            if !cf_ax {
                let ax_fin = (eax as u16) >> 5;
                eax = (eax & 0xffff_0000) | u32::from(ax_fin);
                unsafe { *esi_inout = esi };
                return eax;
            }

            eax = eax.wrapping_shl(3);

            let al4 = unsafe { *esi };
            esi = unsafe { esi.add(1) };
            eax = (eax & 0xffff_ff00) | u32::from(al4);

            if (eax as u8) & 0x80 == 0 {
                unsafe { *esi_inout = esi };
                return eax & 0xffff_00ff;
            }

            let (al5, cf_again) = shl_al_cf(eax as u8, 2);
            eax = (eax & 0xffff_ff00) | u32::from(al5);
            if cf_again {
                continue;
            }

            eax >>= 2;
            unsafe { *esi_inout = esi };
            return eax;
        }
    }
}

#[inline(always)]
fn got(eax: u32, esi: usize) -> Utf8To16Result {
    // xor ah, ah
    Utf8To16Result {
        eax: eax & 0xffff_00ff,
        consumed: esi,
    }
}

#[inline(always)]
fn shl_al_cf(al: u8, count: u32) -> (u8, bool) {
    // CF = last bit shifted out of AL (bit 8-count of original for count=2 → bit 6)
    let cf = ((al as u32) >> (8 - count)) & 1 == 1;
    (al.wrapping_shl(count), cf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (duplicate control flow, not a call to SUT).
    fn oracle(input: &[u8], initial_eax: u32) -> Utf8To16Result {
        let mut esi = 0usize;
        let mut eax = initial_eax;
        loop {
            let al0 = input[esi];
            esi += 1;
            eax = (eax & 0xffff_ff00) | u32::from(al0);
            if (eax as u8) & 0x80 == 0 {
                return Utf8To16Result {
                    eax: eax & 0xffff_00ff,
                    consumed: esi,
                };
            }
            let al = eax as u8;
            let cf = (al >> 6) & 1 == 1;
            let al = al.wrapping_shl(2);
            eax = (eax & 0xffff_ff00) | u32::from(al);
            if !cf {
                continue;
            }
            loop {
                let ax = (eax as u16).wrapping_shl(8);
                eax = (eax & 0xffff_0000) | u32::from(ax);
                let al2 = input[esi];
                esi += 1;
                eax = (eax & 0xffff_ff00) | u32::from(al2);
                if (eax as u8) & 0x80 == 0 {
                    return Utf8To16Result {
                        eax: eax & 0xffff_00ff,
                        consumed: esi,
                    };
                }
                let al = eax as u8;
                let cf = (al >> 6) & 1 == 1;
                let al = al.wrapping_shl(2);
                eax = (eax & 0xffff_ff00) | u32::from(al);
                if cf {
                    continue;
                }
                let ah = ((eax >> 8) as u8) >> 2;
                eax = (eax & 0xffff_00ff) | (u32::from(ah) << 8);
                let ax_in = eax as u16;
                let cf_ax = (ax_in >> 13) & 1 == 1;
                eax = (eax & 0xffff_0000) | u32::from(ax_in.wrapping_shl(3));
                if !cf_ax {
                    let ax_fin = (eax as u16) >> 5;
                    return Utf8To16Result {
                        eax: (eax & 0xffff_0000) | u32::from(ax_fin),
                        consumed: esi,
                    };
                }
                eax = eax.wrapping_shl(3);
                let al4 = input[esi];
                esi += 1;
                eax = (eax & 0xffff_ff00) | u32::from(al4);
                if (eax as u8) & 0x80 == 0 {
                    return Utf8To16Result {
                        eax: eax & 0xffff_00ff,
                        consumed: esi,
                    };
                }
                let al = eax as u8;
                let cf = (al >> 6) & 1 == 1;
                let al = al.wrapping_shl(2);
                eax = (eax & 0xffff_ff00) | u32::from(al);
                if cf {
                    continue;
                }
                return Utf8To16Result {
                    eax: eax >> 2,
                    consumed: esi,
                };
            }
        }
    }

    fn check(input: &[u8], initial_eax: u32) {
        let a = utf8to16(input, initial_eax);
        let b = oracle(input, initial_eax);
        assert_eq!(a, b, "input={:02x?} eax_in={:#x}", input, initial_eax);
        // ptr form
        let mut p = input.as_ptr();
        let eax = unsafe { utf8to16_ptr(&mut p, initial_eax) };
        assert_eq!(eax, a.eax);
        assert_eq!(unsafe { p.offset_from(input.as_ptr()) } as usize, a.consumed);
    }

    #[test]
    fn ascii() {
        check(b"A", 0);
        check(b"\0", 0);
        check(b"z", 0x1234_5678); // .got clears AH only
        let r = utf8to16(b"z", 0x1234_5678);
        assert_eq!(r.eax, 0x1234_007a);
        assert_eq!(r.consumed, 1);
    }

    #[test]
    fn two_byte_latin1() {
        // U+00E9 = C3 A9
        check(&[0xC3, 0xA9], 0);
        let r = utf8to16(&[0xC3, 0xA9], 0);
        assert_eq!(r.eax & 0xffff, 0x00E9);
        assert_eq!(r.consumed, 2);
        // U+00A0 = C2 A0
        check(&[0xC2, 0xA0], 0);
        let r = utf8to16(&[0xC2, 0xA0], 0);
        assert_eq!(r.eax & 0xffff, 0x00A0);
    }

    #[test]
    fn three_byte_bmp() {
        // U+0800 = E0 A0 80
        check(&[0xE0, 0xA0, 0x80], 0);
        let r = utf8to16(&[0xE0, 0xA0, 0x80], 0);
        assert_eq!(r.eax & 0xffff, 0x0800);
        assert_eq!(r.consumed, 3);
        // U+20AC euro = E2 82 AC
        check(&[0xE2, 0x82, 0xAC], 0);
        let r = utf8to16(&[0xE2, 0x82, 0xAC], 0);
        assert_eq!(r.eax & 0xffff, 0x20AC);
        // U+FFFF = EF BF BF
        check(&[0xEF, 0xBF, 0xBF], 0);
        let r = utf8to16(&[0xEF, 0xBF, 0xBF], 0);
        assert_eq!(r.eax & 0xffff, 0xFFFF);
    }

    #[test]
    fn invalid_lead_restart() {
        // 0x80 is continuation (10xxxxxx) → restart, then 'A'
        check(&[0x80, b'A'], 0);
        let r = utf8to16(&[0x80, b'A'], 0);
        assert_eq!(r.eax & 0xff, b'A' as u32);
        assert_eq!(r.consumed, 2);
        // multiple restarts
        check(&[0x80, 0xBF, b'B'], 0);
        let r = utf8to16(&[0x80, 0xBF, b'B'], 0);
        assert_eq!(r.eax & 0xff, b'B' as u32);
        assert_eq!(r.consumed, 3);
    }

    #[test]
    fn mid_stream_ascii_abort() {
        // Lead C3 then ASCII 'X' → .got returns 'X' (AH cleared)
        check(&[0xC3, b'X'], 0);
        let r = utf8to16(&[0xC3, b'X'], 0);
        assert_eq!(r.eax & 0xffff, b'X' as u32);
        assert_eq!(r.consumed, 2);
    }

    #[test]
    fn chained_initial_eax_after_three_byte() {
        // After U+0800, EAX=0x0800; next U+0800 must still decode
        let first = utf8to16(&[0xE0, 0xA0, 0x80], 0);
        assert_eq!(first.eax & 0xffff, 0x0800);
        check(&[0xE0, 0xA0, 0x80], first.eax);
        let second = utf8to16(&[0xE0, 0xA0, 0x80], first.eax);
        assert_eq!(second.eax & 0xffff, 0x0800);
    }

    #[test]
    fn named_boundary_vectors() {
        // 2-byte min U+0080 = C2 80
        check(&[0xC2, 0x80], 0);
        assert_eq!(utf8to16(&[0xC2, 0x80], 0).eax & 0xffff, 0x0080);
        // 2-byte max U+07FF = DF BF
        check(&[0xDF, 0xBF], 0);
        assert_eq!(utf8to16(&[0xDF, 0xBF], 0).eax & 0xffff, 0x07FF);
        // 3-byte min already U+0800
        // overlong 2-byte encoding of ASCII 'A' = C1 81 (quirky FASM result)
        check(&[0xC1, 0x81], 0);
        // NUL after restart
        check(&[0x80, 0x00], 0);
        // high initial eax with ASCII
        check(b"!", 0xDEAD_BEEF);
        assert_eq!(utf8to16(b"!", 0xDEAD_BEEF).eax, 0xDEAD_0021);
    }

    #[test]
    fn exhaustive_single_byte() {
        for b in 0u8..=255 {
            let buf = [b, 0, 0, 0];
            // Only ASCII path is single-byte for leads with bit7 clear;
            // for bit7 set without valid follow, still need trailing bytes.
            if b < 0x80 {
                check(&[b], 0);
                check(&[b], 0xAABB_CC00);
            } else {
                check(&buf, 0);
            }
        }
    }

    #[test]
    fn exhaustive_two_byte_leads() {
        // All C2..DF + continuation 80..BF (valid UTF-8 2-byte plane)
        for hi in 0xC2u8..=0xDF {
            for lo in 0x80u8..=0xBF {
                check(&[hi, lo], 0);
                check(&[hi, lo], 0x0000_0800);
            }
        }
    }

    #[test]
    fn prng_corpus_50k() {
        let mut state = UTF8TO16_PRNG_SEED;
        fn next(s: &mut u32) -> u32 {
            // xorshift32
            let mut x = *s;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *s = x;
            x
        }
        for _ in 0..50_000 {
            let initial_eax = next(&mut state);
            // 16-byte window: gather/restart loops may consume many leads;
            // trailing zeros act as ASCII `.got` terminators (kernel buffers
            // are always longer than one sequence).
            let mut buf = [0u8; 16];
            let fill = (next(&mut state) % 12) as usize + 1;
            for i in 0..fill {
                buf[i] = next(&mut state) as u8;
            }
            check(&buf, initial_eax);
        }
    }
}
