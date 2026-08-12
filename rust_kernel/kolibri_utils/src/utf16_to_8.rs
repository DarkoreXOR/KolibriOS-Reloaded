//! Cut Q: `UTF16to8` — single UTF-16 code unit → UTF-8 streaming encode.
//!
//! Matches `kernel/fs/parse_fn.inc` FASM leaf **bit-exactly**, including:
//! - pre-flight `ECX` decrements (one per output byte) with signed SF abort;
//! - partial `ECX` burn-down and zero stores on mid-encode failure;
//! - `ECX = INT_MIN` escape (`dec` → `INT_MAX`, SF=0, may encode);
//! - surrogate code units encoded as ordinary 3-byte UCS-2 units;
//! - 2/3-byte success `EAX` encoding residues.
//!
//! Legacy return channel is **SF** (reconstructed later by a FASM trampoline).
//! Rust exposes `sf` as `1` / `0` matching FASM `js .ret` taken / not taken.
//!
//! No tables / `.rodata`. Freestanding FFI uses unrolled byte stores — never
//! `memcpy`/`memset`/`memmove`.

/// Cut Q differential PRNG seed (documented).
pub const UTF16_TO_8_PRNG_SEED: u32 = 0x4355_5451; // 'CUTQ'

/// Observable result of one `UTF16to8` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16To8Result {
    /// Final `EAX` (zero-extended input, or FASM encoding residue).
    pub eax: u32,
    /// Final `ECX` bit pattern after all performed `dec`s.
    pub ecx: u32,
    /// Bytes written / `EDI` advance (`0` on any fail path).
    pub edi_delta: u32,
    /// `1` = FASM would take `js .ret` (SF=1); `0` = success (SF=0).
    pub sf: u32,
}

/// Pack `(sf, eax)` into one stdcall return dword for a future trampoline.
///
/// `EAX` residues always fit in the low 16 bits; SF occupies bit 31.
#[inline(always)]
pub fn pack_sf_eax(sf: u32, eax: u32) -> u32 {
    (sf << 31) | (eax & 0x7FFF_FFFF)
}

/// Unpack [`pack_sf_eax`].
#[inline(always)]
pub fn unpack_sf_eax(packed: u32) -> (u32, u32) {
    (packed >> 31, packed & 0x7FFF_FFFF)
}

/// FASM trampoline SF reconstruction model (Cut Q Step 3).
///
/// Mirrors:
/// ```text
/// mov ebx, eax          ; packed
/// and eax, 0x7FFFFFFF   ; residue
/// test ebx, ebx         ; SF = bit 31 of packed
/// ```
/// Returns `true` iff the trampoline leaves `SF=1`.
#[inline(always)]
pub fn trampoline_sf_from_packed(packed: u32) -> bool {
    (packed as i32) < 0
}

/// FASM trampoline EAX reconstruction model (Cut Q Step 3).
#[inline(always)]
pub fn trampoline_eax_from_packed(packed: u32) -> u32 {
    packed & 0x7FFF_FFFF
}

/// FASM `dec ecx` → (new_ecx, sf_from_result).
///
/// `DEC` sets SF from bit 31 of the result; does not modify CF.
#[inline(always)]
fn dec_ecx(ecx: u32) -> (u32, bool) {
    let r = ecx.wrapping_sub(1);
    let sf = (r & 0x8000_0000) != 0;
    (r, sf)
}

/// FASM 2-byte encode residue + output bytes (after `or` + `xchg al,ah`).
#[inline(always)]
fn encode_2byte(c: u32) -> (u32, [u8; 2]) {
    let mut eax = c << 2;
    let al = ((eax as u8) >> 2) as u32;
    eax = (eax & 0xFFFF_FF00) | al;
    eax |= 0xC080;
    // xchg al, ah
    let al = eax & 0xFF;
    let ah = (eax >> 8) & 0xFF;
    eax = (eax & 0xFFFF_0000) | (al << 8) | ah;
    let b0 = (eax & 0xFF) as u8;
    let b1 = ((eax >> 8) & 0xFF) as u8;
    (eax, [b0, b1])
}

/// FASM 3-byte encode residue + output bytes (after final `shr eax,8`).
#[inline(always)]
fn encode_3byte(c: u32) -> (u32, [u8; 3]) {
    let mut eax = c << 4;
    let ax = ((eax as u16) >> 2) as u32;
    eax = (eax & 0xFFFF_0000) | ax;
    let al = ((eax as u8) >> 2) as u32;
    eax = (eax & 0xFFFF_FF00) | al;
    eax |= 0x00E0_8080;
    eax = eax.swap_bytes(); // bswap
    eax >>= 8;
    let b0 = (eax & 0xFF) as u8;
    eax >>= 8;
    let b1 = (eax & 0xFF) as u8;
    let b2 = ((eax >> 8) & 0xFF) as u8;
    (eax, [b0, b1, b2])
}

/// FASM-faithful `UTF16to8` over a destination buffer.
///
/// # Safety
/// On success paths, `dest` must be writable for 1, 2, or 3 bytes as required
/// by the encoding length of `ax`. Fail paths never write.
#[inline(always)]
pub unsafe fn utf16_to_8(ax: u16, dest: *mut u8, ecx_in: u32) -> Utf16To8Result {
    // movzx eax, ax
    let eax = ax as u32;
    let mut ecx = ecx_in;

    // dec ecx / js .ret
    let (n, sf) = dec_ecx(ecx);
    ecx = n;
    if sf {
        return Utf16To8Result {
            eax,
            ecx,
            edi_delta: 0,
            sf: 1,
        };
    }

    if eax < 0x80 {
        // stosb ; test eax, eax ; ret  (SF=0 for 0..0x7F)
        unsafe { core::ptr::write(dest, eax as u8) };
        return Utf16To8Result {
            eax,
            ecx,
            edi_delta: 1,
            sf: 0,
        };
    }

    // dec ecx / js .ret
    let (n, sf) = dec_ecx(ecx);
    ecx = n;
    if sf {
        return Utf16To8Result {
            eax,
            ecx,
            edi_delta: 0,
            sf: 1,
        };
    }

    if eax < 0x800 {
        let (res, bytes) = encode_2byte(eax);
        unsafe {
            core::ptr::write(dest, bytes[0]);
            core::ptr::write(dest.add(1), bytes[1]);
        }
        return Utf16To8Result {
            eax: res,
            ecx,
            edi_delta: 2,
            sf: 0,
        };
    }

    // dec ecx / js .ret
    let (n, sf) = dec_ecx(ecx);
    ecx = n;
    if sf {
        return Utf16To8Result {
            eax,
            ecx,
            edi_delta: 0,
            sf: 1,
        };
    }

    let (res, bytes) = encode_3byte(eax);
    unsafe {
        core::ptr::write(dest, bytes[0]);
        core::ptr::write(dest.add(1), bytes[1]);
        core::ptr::write(dest.add(2), bytes[2]);
    }
    Utf16To8Result {
        eax: res,
        ecx,
        edi_delta: 3,
        sf: 0,
    }
}

/// Pointer form for FFI: updates `*ecx_inout`, advances `*dest_inout` by
/// `edi_delta`, returns [`pack_sf_eax`].
///
/// # Safety
/// `dest_inout` / `ecx_inout` must be valid; `*dest_inout` writable on success.
#[inline(always)]
pub unsafe fn utf16_to_8_ptr(
    ch: u32,
    dest_inout: *mut *mut u8,
    ecx_inout: *mut u32,
) -> u32 {
    let dest = unsafe { *dest_inout };
    let ecx_in = unsafe { *ecx_inout };
    let r = unsafe { utf16_to_8(ch as u16, dest, ecx_in) };
    unsafe {
        *ecx_inout = r.ecx;
        if r.edi_delta != 0 {
            *dest_inout = dest.add(r.edi_delta as usize);
        }
    }
    pack_sf_eax(r.sf, r.eax)
}

/// Cut BP differential PRNG seed (`'CUPB'`).
pub const UTF16_TO_8_STRING_PRNG_SEED: u32 = 0x4355_5042;

/// Observable exit state of one `UTF16to8_string` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16To8StringResult {
    pub eax: u32,
    pub esi: u32,
    pub edi: u32,
    pub ecx: u32,
    pub sf: u32,
    pub zf: u32,
}

#[inline(always)]
pub fn pack_sf_zf_eax(sf: u32, zf: u32, eax: u32) -> u32 {
    (sf << 31) | (zf << 30) | (eax & 0x3FFF_FFFF)
}

#[inline(always)]
pub fn unpack_sf_zf_eax(packed: u32) -> (u32, u32, u32) {
    ((packed >> 31) & 1, (packed >> 30) & 1, packed & 0x3FFF_FFFF)
}

#[inline(always)]
pub fn trampoline_zf_from_packed(packed: u32) -> bool {
    (packed & 0x3FFF_FFFF) == 0 && (packed & 0x4000_0000) != 0
}

#[inline(always)]
fn utf16_to_8_one(ch: u16, dest: *mut u8, ecx: u32) -> Utf16To8Result {
    unsafe { utf16_to_8(ch, dest, ecx) }
}

/// FASM-faithful `UTF16to8_string` (host tests + inlined by `rust_utf16_to_8_string`).
#[inline(always)]
pub unsafe fn utf16_to_8_string(
    src: *const u8,
    dest: *mut u8,
    ecx_in: u32,
) -> Utf16To8StringResult {
    let mut esi = src as usize;
    let mut edi = dest as usize;
    let mut ecx = ecx_in;
    let mut eax = 0u32;

    loop {
        let ax = unsafe { core::ptr::read_unaligned(esi as *const u16) };
        esi += 2;
        eax = (eax & 0xFFFF_0000) | u32::from(ax);

        let r = utf16_to_8_one(ax, edi as *mut u8, ecx);
        if r.sf != 0 {
            return Utf16To8StringResult {
                eax,
                esi: esi as u32,
                edi: if r.edi_delta != 0 {
                    (edi + r.edi_delta as usize) as u32
                } else {
                    edi as u32
                },
                ecx: r.ecx,
                sf: 1,
                zf: 0,
            };
        }
        ecx = r.ecx;
        if r.edi_delta != 0 {
            edi += r.edi_delta as usize;
        }
        if eax == 0 {
            return Utf16To8StringResult {
                eax: 0,
                esi: esi as u32,
                edi: edi as u32,
                ecx,
                sf: 0,
                zf: 1,
            };
        }
    }
}

#[inline(always)]
pub unsafe fn utf16_to_8_string_ptr(
    src: *const u8,
    dest: *mut u8,
    ecx_in: u32,
    src_out: *mut u32,
    dest_out: *mut u32,
    ecx_out: *mut u32,
) -> u32 {
    let r = unsafe { utf16_to_8_string(src, dest, ecx_in) };
    unsafe {
        *src_out = r.esi;
        *dest_out = r.edi;
        *ecx_out = r.ecx;
    }
    pack_sf_zf_eax(r.sf, r.zf, r.eax)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bp_seed_constant() {
        assert_eq!(UTF16_TO_8_STRING_PRNG_SEED, 0x4355_5042);
    }

    #[test]
    fn bp_direct_hello() {
        let words: [u16; 6] = [
            b'H' as u16,
            b'e' as u16,
            b'l' as u16,
            b'l' as u16,
            b'o' as u16,
            0,
        ];
        let src: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut buf = vec![0u8; 32];
        let r = unsafe { utf16_to_8_string(src.as_ptr(), buf.as_mut_ptr(), 16) };
        assert_eq!(r.zf, 1);
        assert_eq!(&buf[..6], b"Hello\0");
    }

    fn string_oracle(words: &[u16], ecx_in: u32) -> (Utf16To8StringResult, Vec<u8>) {
        let mut buf = vec![0xA5u8; 512];
        let mut esi_off = 0usize;
        let mut edi_off = 0usize;
        let mut ecx = ecx_in;
        let mut eax = 0u32;
        while esi_off < words.len() {
            let ax = words[esi_off];
            esi_off += 1;
            eax = (eax & 0xFFFF_0000) | u32::from(ax);
            let r = unsafe { utf16_to_8(ax, buf.as_mut_ptr().add(edi_off), ecx) };
            if r.sf != 0 {
                return (
                    Utf16To8StringResult {
                        eax,
                        esi: (esi_off * 2) as u32,
                        edi: if r.edi_delta != 0 {
                            (edi_off + r.edi_delta as usize) as u32
                        } else {
                            edi_off as u32
                        },
                        ecx: r.ecx,
                        sf: 1,
                        zf: 0,
                    },
                    buf,
                );
            }
            ecx = r.ecx;
            if r.edi_delta != 0 {
                edi_off += r.edi_delta as usize;
            }
            if eax == 0 {
                return (
                    Utf16To8StringResult {
                        eax: 0,
                        esi: (esi_off * 2) as u32,
                        edi: edi_off as u32,
                        ecx,
                        sf: 0,
                        zf: 1,
                    },
                    buf,
                );
            }
        }
        panic!("oracle needs terminator");
    }

    fn run_string_words(words: &[u16], ecx_in: u32) {
        let (oracle, o_buf) = string_oracle(words, ecx_in);
        let mut buf = vec![0xA5u8; 512];
        let src: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut buf = vec![0xA5u8; 512];
        let got = unsafe { utf16_to_8_string(src.as_ptr(), buf.as_mut_ptr(), ecx_in) };
        assert_eq!(
            (got.eax, got.ecx, got.sf, got.zf),
            (oracle.eax, oracle.ecx, oracle.sf, oracle.zf),
            "words={words:?} ecx_in={ecx_in:#x}"
        );
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(got.esi, oracle.esi, "esi words={words:?}");
            assert_eq!(got.edi, oracle.edi, "edi words={words:?}");
        }
        let o_len = oracle.edi as usize;
        assert_eq!(&buf[..o_len], &o_buf[..o_len]);
    }

    #[test]
    fn bp_overflow_mid_ascii() {
        run_string_words(&[0x41, 0x41, 0], 1);
    }

    #[test]
    fn bp_prng_50k_cupb() {
        let mut s = UTF16_TO_8_STRING_PRNG_SEED;
        for _ in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let word_count = (s % 8) + 1;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let ecx_in = s;
            let mut words = Vec::with_capacity(word_count as usize + 1);
            for _ in 0..word_count {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                words.push(s as u16);
            }
            words.push(0);
            run_string_words(&words, ecx_in);
        }
    }

    /// Independent FASM instruction-sequence oracle (Step 1 truth table).
    /// Kept separate from [`utf16_to_8`] so differential tests are meaningful.
    fn fasm_oracle(ax: u16, ecx_in: u32) -> (Utf16To8Result, [u8; 4]) {
        let mut buf = [0u8; 4];
        let mut eax = ax as u32;
        let mut ecx = ecx_in;
        let mut edi = 0usize;

        // dec / js
        ecx = ecx.wrapping_sub(1);
        if (ecx & 0x8000_0000) != 0 {
            return (
                Utf16To8Result {
                    eax,
                    ecx,
                    edi_delta: 0,
                    sf: 1,
                },
                buf,
            );
        }

        if eax < 0x80 {
            buf[edi] = eax as u8;
            edi += 1;
            // test eax,eax → SF=0 for movzx ASCII
            return (
                Utf16To8Result {
                    eax,
                    ecx,
                    edi_delta: edi as u32,
                    sf: 0,
                },
                buf,
            );
        }

        ecx = ecx.wrapping_sub(1);
        if (ecx & 0x8000_0000) != 0 {
            return (
                Utf16To8Result {
                    eax,
                    ecx,
                    edi_delta: 0,
                    sf: 1,
                },
                buf,
            );
        }

        if eax < 0x800 {
            // shl eax,2 ; shr al,2 ; or 0xC080 ; xchg al,ah ; stosw
            eax <<= 2;
            let al = ((eax & 0xFF) as u8) >> 2;
            eax = (eax & 0xFFFF_FF00) | u32::from(al);
            eax |= 0xC080;
            let al = (eax & 0xFF) as u8;
            let ah = ((eax >> 8) & 0xFF) as u8;
            eax = (eax & 0xFFFF_0000) | (u32::from(al) << 8) | u32::from(ah);
            buf[edi] = (eax & 0xFF) as u8;
            buf[edi + 1] = ((eax >> 8) & 0xFF) as u8;
            edi += 2;
            return (
                Utf16To8Result {
                    eax,
                    ecx,
                    edi_delta: edi as u32,
                    sf: 0,
                },
                buf,
            );
        }

        ecx = ecx.wrapping_sub(1);
        if (ecx & 0x8000_0000) != 0 {
            return (
                Utf16To8Result {
                    eax,
                    ecx,
                    edi_delta: 0,
                    sf: 1,
                },
                buf,
            );
        }

        // shl 4 ; shr ax,2 ; shr al,2 ; or 0xE08080 ; bswap ; shr 8 ; stosb ; shr 8 ; stosw
        eax <<= 4;
        let axv = ((eax & 0xFFFF) as u16) >> 2;
        eax = (eax & 0xFFFF_0000) | u32::from(axv);
        let al = ((eax & 0xFF) as u8) >> 2;
        eax = (eax & 0xFFFF_FF00) | u32::from(al);
        eax |= 0x00E0_8080;
        eax = eax.swap_bytes();
        eax >>= 8;
        buf[edi] = (eax & 0xFF) as u8;
        edi += 1;
        eax >>= 8;
        buf[edi] = (eax & 0xFF) as u8;
        buf[edi + 1] = ((eax >> 8) & 0xFF) as u8;
        edi += 2;
        (
            Utf16To8Result {
                eax,
                ecx,
                edi_delta: edi as u32,
                sf: 0,
            },
            buf,
        )
    }

    fn run_case(ax: u16, ecx_in: u32) {
        let (oracle, o_bytes) = fasm_oracle(ax, ecx_in);
        let mut buf = [0xA5u8; 8];
        let r = unsafe { utf16_to_8(ax, buf.as_mut_ptr(), ecx_in) };
        assert_eq!(r, oracle, "state AX={ax:#06x} ECXin={ecx_in:#x}");
        if r.edi_delta == 0 {
            assert!(
                buf.iter().all(|&b| b == 0xA5),
                "fail path must not write AX={ax:#06x} ECXin={ecx_in:#x}"
            );
        } else {
            for i in 0..r.edi_delta as usize {
                assert_eq!(
                    buf[i], o_bytes[i],
                    "byte[{i}] AX={ax:#06x} ECXin={ecx_in:#x}"
                );
            }
            for i in r.edi_delta as usize..buf.len() {
                assert_eq!(buf[i], 0xA5, "overflow write AX={ax:#06x}");
            }
        }
        let packed = pack_sf_eax(r.sf, r.eax);
        let (sf2, eax2) = unpack_sf_eax(packed);
        assert_eq!(sf2, r.sf);
        assert_eq!(eax2, r.eax & 0x7FFF_FFFF);
    }

    #[test]
    fn required_boundary_ax_ecx_matrix() {
        const AXS: &[u16] = &[
            0x0000, 0x0001, 0x007F, 0x0080, 0x07FF, 0x0800, 0xD800, 0xDBFF, 0xDC00, 0xDFFF,
            0xFFFF,
        ];
        const ECXS: &[u32] = &[
            0,
            1,
            2,
            3,
            4,
            u32::MAX,          // -1
            (-2i32) as u32,    // -2
            i32::MIN as u32,   // INT_MIN
            (i32::MIN as u32).wrapping_add(1),
            i32::MAX as u32,   // INT_MAX
        ];
        for &ax in AXS {
            for &ecx in ECXS {
                run_case(ax, ecx);
            }
        }
    }

    #[test]
    fn known_eax_residues() {
        let mut b = [0u8; 4];
        let r = unsafe { utf16_to_8(0x0080, b.as_mut_ptr(), 2) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.eax, 0x0000_80C2);
        assert_eq!(&b[..2], &[0xC2, 0x80]);

        let r = unsafe { utf16_to_8(0x07FF, b.as_mut_ptr(), 2) };
        assert_eq!(r.eax, 0x0000_BFDF);
        assert_eq!(&b[..2], &[0xDF, 0xBF]);

        let r = unsafe { utf16_to_8(0x0800, b.as_mut_ptr(), 3) };
        assert_eq!(r.eax, 0x0000_80A0);
        assert_eq!(&b[..3], &[0xE0, 0xA0, 0x80]);

        let r = unsafe { utf16_to_8(0xD800, b.as_mut_ptr(), 3) };
        assert_eq!(r.eax, 0x0000_80A0);
        assert_eq!(&b[..3], &[0xED, 0xA0, 0x80]);

        let r = unsafe { utf16_to_8(0xFFFF, b.as_mut_ptr(), 3) };
        assert_eq!(r.eax, 0x0000_BFBF);
        assert_eq!(&b[..3], &[0xEF, 0xBF, 0xBF]);
    }

    #[test]
    fn exhaustion_points() {
        // 1-byte need, ECX=0 → fail after #1
        let mut b = [0xA5u8; 4];
        let r = unsafe { utf16_to_8(0x0041, b.as_mut_ptr(), 0) };
        assert_eq!(r, Utf16To8Result { eax: 0x41, ecx: u32::MAX, edi_delta: 0, sf: 1 });
        assert_eq!(b, [0xA5; 4]);

        // 2-byte need, ECX=1 → fail after #2
        let r = unsafe { utf16_to_8(0x0080, b.as_mut_ptr(), 1) };
        assert_eq!(r, Utf16To8Result { eax: 0x80, ecx: u32::MAX, edi_delta: 0, sf: 1 });

        // 2-byte ok
        let r = unsafe { utf16_to_8(0x0080, b.as_mut_ptr(), 2) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.edi_delta, 2);
        assert_eq!(r.ecx, 0);

        // 3-byte, ECX=1 → fail after #2
        let mut b = [0xA5u8; 4];
        let r = unsafe { utf16_to_8(0x0800, b.as_mut_ptr(), 1) };
        assert_eq!(r.sf, 1);
        assert_eq!(r.ecx, u32::MAX);
        assert_eq!(r.edi_delta, 0);

        // 3-byte, ECX=2 → fail after #3
        let r = unsafe { utf16_to_8(0x0800, b.as_mut_ptr(), 2) };
        assert_eq!(r.sf, 1);
        assert_eq!(r.ecx, u32::MAX);
        assert_eq!(r.edi_delta, 0);
        assert_eq!(b, [0xA5; 4]);

        // 3-byte ok
        let r = unsafe { utf16_to_8(0x0800, b.as_mut_ptr(), 3) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.edi_delta, 3);
        assert_eq!(r.ecx, 0);
    }

    #[test]
    fn negative_ecx_decrements_once() {
        let mut b = [0xA5u8; 4];
        let r = unsafe { utf16_to_8(0x0041, b.as_mut_ptr(), (-1i32) as u32) };
        assert_eq!(r.sf, 1);
        assert_eq!(r.ecx, (-2i32) as u32);
        assert_eq!(r.edi_delta, 0);
        assert_eq!(b, [0xA5; 4]);

        let r = unsafe { utf16_to_8(0x0800, b.as_mut_ptr(), (-2i32) as u32) };
        assert_eq!(r.sf, 1);
        assert_eq!(r.ecx, (-3i32) as u32);
    }

    #[test]
    fn int_min_ecx_encodes() {
        let mut b = [0u8; 4];
        let r = unsafe { utf16_to_8(0x0041, b.as_mut_ptr(), i32::MIN as u32) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.edi_delta, 1);
        assert_eq!(r.ecx, i32::MAX as u32);
        assert_eq!(b[0], 0x41);

        let r = unsafe { utf16_to_8(0x0080, b.as_mut_ptr(), i32::MIN as u32) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.edi_delta, 2);
        assert_eq!(r.ecx, (i32::MAX as u32).wrapping_sub(1));

        let r = unsafe { utf16_to_8(0x0800, b.as_mut_ptr(), i32::MIN as u32) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.edi_delta, 3);
        assert_eq!(r.ecx, (i32::MAX as u32).wrapping_sub(2));
    }

    #[test]
    fn surrogate_independent_3byte() {
        let cases: &[(u16, &[u8])] = &[
            (0xD800, &[0xED, 0xA0, 0x80]),
            (0xDBFF, &[0xED, 0xAF, 0xBF]),
            (0xDC00, &[0xED, 0xB0, 0x80]),
            (0xDFFF, &[0xED, 0xBF, 0xBF]),
        ];
        for &(ax, expect) in cases {
            let mut b = [0u8; 4];
            let r = unsafe { utf16_to_8(ax, b.as_mut_ptr(), 3) };
            assert_eq!(r.sf, 0, "{ax:#x}");
            assert_eq!(&b[..3], expect, "{ax:#x}");
        }
    }

    #[test]
    fn exhaustive_ax_over_ecx_budgets() {
        const ECXS: &[u32] = &[
            0,
            1,
            2,
            3,
            4,
            u32::MAX,
            (-2i32) as u32,
            i32::MIN as u32,
            (i32::MIN as u32).wrapping_add(1),
            i32::MAX as u32,
        ];
        for ax in 0u32..=0xFFFF {
            for &ecx in ECXS {
                run_case(ax as u16, ecx);
            }
        }
    }

    #[test]
    fn prng_corpus() {
        let mut state = UTF16_TO_8_PRNG_SEED;
        for _ in 0..200_000 {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let ax = state as u16;
            let ecx = {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state
            };
            run_case(ax, ecx);
        }
    }

    #[test]
    fn ffi_ptr_advances_dest_and_ecx() {
        let mut buf = [0u8; 8];
        let mut dest = buf.as_mut_ptr();
        let mut ecx = 4u32;
        let packed = unsafe { utf16_to_8_ptr(0x0800, &mut dest, &mut ecx) };
        let (sf, eax) = unpack_sf_eax(packed);
        assert_eq!(sf, 0);
        assert_eq!(eax, 0x80A0);
        assert_eq!(ecx, 1);
        assert_eq!(dest as usize - buf.as_ptr() as usize, 3);
        assert_eq!(&buf[..3], &[0xE0, 0xA0, 0x80]);
    }

    #[test]
    fn trampoline_sf_eax_reconstruction_model() {
        // Success: SF=0, residue in low bits
        let p = pack_sf_eax(0, 0x80C2);
        assert!(!trampoline_sf_from_packed(p));
        assert_eq!(trampoline_eax_from_packed(p), 0x80C2);

        // Fail: SF=1, residue = code unit
        let p = pack_sf_eax(1, 0x41);
        assert!(trampoline_sf_from_packed(p));
        assert_eq!(trampoline_eax_from_packed(p), 0x41);

        // Surrogate success residue
        let p = pack_sf_eax(0, 0x80A0);
        assert!(!trampoline_sf_from_packed(p));
        assert_eq!(trampoline_eax_from_packed(p), 0x80A0);

        // Round-trip vs utf16_to_8_ptr packing
        let mut buf = [0u8; 4];
        let mut dest = buf.as_mut_ptr();
        let mut ecx = 0u32;
        let packed = unsafe { utf16_to_8_ptr(0x41, &mut dest, &mut ecx) };
        assert!(trampoline_sf_from_packed(packed));
        assert_eq!(trampoline_eax_from_packed(packed), 0x41);
        assert_eq!(ecx, u32::MAX);
    }

    #[test]
    fn ascii_nul_zf_side_channel_eax_zero() {
        let mut b = [0xFFu8; 2];
        let r = unsafe { utf16_to_8(0, b.as_mut_ptr(), 1) };
        assert_eq!(r.sf, 0);
        assert_eq!(r.eax, 0);
        assert_eq!(b[0], 0);
    }
}
