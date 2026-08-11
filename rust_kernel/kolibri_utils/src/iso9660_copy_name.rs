//! Cut BI: `iso9660_copy_name` — ISO9660 volume-name encoding dispatch.
//!
//! Matches `kernel/fs/iso9660.inc` FASM leaf **bit-exactly**, including:
//! - ASCII vs UCS-2 (`type_encoding`) source selection;
//! - `nameenc` ∈ {1=cp866, 2=utf16LE, 3=utf8}; other → NUL only;
//! - UCS-2 path `shr ecx,1` before the char loop;
//! - ASCII→UTF8 via `cp866toUTF8_string` control flow (ansi2uni + UTF16to8
//!   until SF or zero char — may read one source byte past `max_len` when the
//!   window has no embedded NUL, matching FASM);
//! - UCS-2→UTF8 fixed char-count loop with `ecx = chars*2` byte budget;
//! - terminator: always `byte[edi]=0`, then `word[edi]=0` when UCS-2.
//!
//! Composes inlined Cut A/AN/Q helpers (`cp866_encode` / `cp866_decode` /
//! `utf16_to_8`) — does **not** call FASM and does not introduce production
//! gates for ban-listed `uni2ansi_char` / deferred `cp866toUTF8_string`.
//!
//! No tables / `.rodata` beyond what those helpers already embed locally.
//! Freestanding FFI must stay reloc-free (no `memcpy`/`memset`).

use crate::unicode::{cp866_decode, cp866_encode};
use crate::utf16_to_8::utf16_to_8;

/// Cut BI differential PRNG seed (`'CUBI'`).
pub const ISO9660_COPY_NAME_PRNG_SEED: u32 = 0x4355_4249;

/// Observable result of [`iso9660_copy_name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Iso9660CopyNameResult {
    /// Final ESI (byte offset into source after the copy loops).
    pub esi: usize,
    /// Final EDI pointing at the terminator byte/word written.
    pub edi: usize,
}

/// FASM-faithful `iso9660_copy_name`.
///
/// # Safety
/// `src` must be readable for the bytes the selected path consumes (up to
/// `max_len` bytes for ASCII paths, or `max_len` bytes of UCS-2 source after
/// the initial `shr`, plus the ASCII→UTF8 overread quirk). `dest` must be
/// writable for the encoded output plus a 1- or 2-byte terminator.
#[inline(always)]
pub unsafe fn iso9660_copy_name(
    src: *const u8,
    dest: *mut u8,
    max_len: u32,
    nameenc: u32,
    type_encoding: u32,
) -> Iso9660CopyNameResult {
    let mut esi = src as usize;
    let mut edi = dest as usize;
    let mut ecx = max_len;

    if type_encoding == 0 {
        // ASCII volume name
        if nameenc == 1 {
            // rep movsb
            while ecx != 0 {
                let b = unsafe { *(esi as *const u8) };
                unsafe { *(edi as *mut u8) = b };
                esi = esi.wrapping_add(1);
                edi = edi.wrapping_add(1);
                ecx = ecx.wrapping_sub(1);
            }
        } else if nameenc == 2 {
            // ascii → utf16LE via ansi2uni_char
            while ecx != 0 {
                let al = unsafe { *(esi as *const u8) };
                esi = esi.wrapping_add(1);
                let ax = cp866_decode(u32::from(al)) as u16;
                unsafe {
                    *(edi as *mut u8) = (ax & 0xFF) as u8;
                    *((edi as *mut u8).add(1)) = (ax >> 8) as u8;
                }
                edi = edi.wrapping_add(2);
                ecx = ecx.wrapping_sub(1);
            }
        } else if nameenc == 3 {
            // ascii → utf8 via cp866toUTF8_string
            // FASM: lodsb / ansi2uni / UTF16to8 / js exit / test eax / loop
            loop {
                let al = unsafe { *(esi as *const u8) };
                esi = esi.wrapping_add(1);
                let uni = cp866_decode(u32::from(al));
                let r = unsafe { utf16_to_8(uni as u16, edi as *mut u8, ecx) };
                ecx = r.ecx;
                if r.edi_delta != 0 {
                    edi = edi.wrapping_add(r.edi_delta as usize);
                }
                if r.sf != 0 {
                    break;
                }
                // test eax, eax — FASM tests the Unicode codepoint pushed before
                // UTF16to8 (original AX after ansi2uni), not the encode residue.
                if uni == 0 {
                    break;
                }
            }
        }
        // else: invalid nameenc → fall through to NUL only
    } else {
        // UCS-2 BE volume name
        ecx = ecx >> 1;
        if nameenc == 1 {
            // ucs2 → cp866 via uni2ansi_char
            while ecx != 0 {
                let b0 = unsafe { *(esi as *const u8) };
                let b1 = unsafe { *((esi as *const u8).add(1)) };
                esi = esi.wrapping_add(2);
                // lodsw + xchg al,ah → BE to host LE code unit in AX
                let ax = u16::from(b0) << 8 | u16::from(b1);
                let al = cp866_encode(u32::from(ax)) as u8;
                unsafe { *(edi as *mut u8) = al };
                edi = edi.wrapping_add(1);
                ecx = ecx.wrapping_sub(1);
            }
        } else if nameenc == 2 {
            // ucs2 BE → utf16LE (byte swap)
            while ecx != 0 {
                let b0 = unsafe { *(esi as *const u8) };
                let b1 = unsafe { *((esi as *const u8).add(1)) };
                esi = esi.wrapping_add(2);
                // lodsw + xchg al,ah + stosw
                unsafe {
                    *(edi as *mut u8) = b1;
                    *((edi as *mut u8).add(1)) = b0;
                }
                edi = edi.wrapping_add(2);
                ecx = ecx.wrapping_sub(1);
            }
        } else if nameenc == 3 {
            // ucs2 → utf8: ebx=chars, ecx=chars*2 byte budget
            let mut ebx = ecx;
            ecx = ecx << 1;
            while ebx != 0 {
                let b0 = unsafe { *(esi as *const u8) };
                let b1 = unsafe { *((esi as *const u8).add(1)) };
                esi = esi.wrapping_add(2);
                // lodsw + xchg ah,al
                let ax = u16::from(b0) << 8 | u16::from(b1);
                let r = unsafe { utf16_to_8(ax, edi as *mut u8, ecx) };
                ecx = r.ecx;
                if r.edi_delta != 0 {
                    edi = edi.wrapping_add(r.edi_delta as usize);
                }
                ebx = ebx.wrapping_sub(1);
            }
        }
    }

    // .end_copy_name — byte NUL always; UCS-2 overwrites as word NUL.
    // Write bytes (not *mut u16) so unaligned EDI is safe.
    unsafe {
        *(edi as *mut u8) = 0;
        if type_encoding != 0 {
            *((edi as *mut u8).add(1)) = 0;
        }
    }

    Iso9660CopyNameResult { esi, edi }
}

/// Pointer form for FFI: updates `*esi_inout` / `*edi_inout`.
///
/// # Safety
/// Inout pointers must be valid; buffers obey [`iso9660_copy_name`] contracts.
#[inline(always)]
pub unsafe fn iso9660_copy_name_ptr(
    esi_inout: *mut *mut u8,
    edi_inout: *mut *mut u8,
    max_len: u32,
    nameenc: u32,
    type_encoding: u32,
) {
    let src = unsafe { *esi_inout } as *const u8;
    let dest = unsafe { *edi_inout };
    let r = unsafe { iso9660_copy_name(src, dest, max_len, nameenc, type_encoding) };
    unsafe {
        *esi_inout = r.esi as *mut u8;
        *edi_inout = r.edi as *mut u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unicode::{cp866_decode, cp866_encode};
    use crate::utf16_to_8::utf16_to_8;

    /// Independent FASM-flow oracle (not a call to the SUT).
    fn fasm_oracle_iso9660_copy_name(
        src: &[u8],
        max_len: u32,
        nameenc: u32,
        type_encoding: u32,
    ) -> (Vec<u8>, usize, usize) {
        let mut dest = vec![0xA5u8; 256];
        let mut esi: usize = 0;
        let mut edi: usize = 0;
        let mut ecx = max_len;

        if type_encoding == 0 {
            if nameenc == 1 {
                while ecx != 0 {
                    dest[edi] = src[esi];
                    esi += 1;
                    edi += 1;
                    ecx -= 1;
                }
            } else if nameenc == 2 {
                while ecx != 0 {
                    let al = src[esi];
                    esi += 1;
                    let ax = cp866_decode(u32::from(al)) as u16;
                    dest[edi] = (ax & 0xFF) as u8;
                    dest[edi + 1] = (ax >> 8) as u8;
                    edi += 2;
                    ecx -= 1;
                }
            } else if nameenc == 3 {
                loop {
                    let al = src[esi];
                    esi += 1;
                    let uni = cp866_decode(u32::from(al));
                    let r = unsafe { utf16_to_8(uni as u16, dest.as_mut_ptr().add(edi), ecx) };
                    ecx = r.ecx;
                    if r.edi_delta != 0 {
                        // SAFETY: oracle owns dest; utf16_to_8 wrote edi_delta bytes.
                        let slice = unsafe {
                            core::slice::from_raw_parts(
                                dest.as_ptr().add(edi),
                                r.edi_delta as usize,
                            )
                        };
                        for (i, b) in slice.iter().enumerate() {
                            dest[edi + i] = *b;
                        }
                        edi += r.edi_delta as usize;
                    }
                    if r.sf != 0 || uni == 0 {
                        break;
                    }
                }
            }
        } else {
            ecx >>= 1;
            if nameenc == 1 {
                while ecx != 0 {
                    let ax = u16::from(src[esi]) << 8 | u16::from(src[esi + 1]);
                    esi += 2;
                    dest[edi] = cp866_encode(u32::from(ax)) as u8;
                    edi += 1;
                    ecx -= 1;
                }
            } else if nameenc == 2 {
                while ecx != 0 {
                    let b0 = src[esi];
                    let b1 = src[esi + 1];
                    esi += 2;
                    dest[edi] = b1;
                    dest[edi + 1] = b0;
                    edi += 2;
                    ecx -= 1;
                }
            } else if nameenc == 3 {
                let mut ebx = ecx;
                ecx <<= 1;
                while ebx != 0 {
                    let ax = u16::from(src[esi]) << 8 | u16::from(src[esi + 1]);
                    esi += 2;
                    let r = unsafe { utf16_to_8(ax, dest.as_mut_ptr().add(edi), ecx) };
                    ecx = r.ecx;
                    if r.edi_delta != 0 {
                        let slice = unsafe {
                            core::slice::from_raw_parts(
                                dest.as_ptr().add(edi),
                                r.edi_delta as usize,
                            )
                        };
                        for (i, b) in slice.iter().enumerate() {
                            dest[edi + i] = *b;
                        }
                        edi += r.edi_delta as usize;
                    }
                    ebx -= 1;
                }
            }
        }

        dest[edi] = 0;
        if type_encoding != 0 {
            dest[edi + 1] = 0;
        }
        (dest, esi, edi)
    }

    fn run_sut(
        src: &[u8],
        max_len: u32,
        nameenc: u32,
        type_encoding: u32,
    ) -> (Vec<u8>, Iso9660CopyNameResult) {
        let mut dest = vec![0xA5u8; 256];
        let r = unsafe {
            iso9660_copy_name(src.as_ptr(), dest.as_mut_ptr(), max_len, nameenc, type_encoding)
        };
        (dest, r)
    }

    fn assert_match(src: &[u8], max_len: u32, nameenc: u32, type_encoding: u32) {
        let (got_buf, got) = run_sut(src, max_len, nameenc, type_encoding);
        let (exp_buf, exp_esi, exp_edi) =
            fasm_oracle_iso9660_copy_name(src, max_len, nameenc, type_encoding);
        assert_eq!(got.esi - src.as_ptr() as usize, exp_esi, "esi delta");
        assert_eq!(got.edi - got_buf.as_ptr() as usize, exp_edi, "edi");
        let n = exp_edi + if type_encoding != 0 { 2 } else { 1 };
        assert_eq!(&got_buf[..n], &exp_buf[..n], "dest bytes");
    }

    #[test]
    fn ascii_cp866_passthrough() {
        let mut src = [0u8; 40];
        src[..5].copy_from_slice(b"KOLI\0");
        assert_match(&src, 4, 1, 0);
        let (buf, r) = run_sut(&src, 4, 1, 0);
        assert_eq!(&buf[..4], b"KOLI");
        assert_eq!(buf[4], 0);
        assert_eq!(r.edi - buf.as_ptr() as usize, 4);
    }

    #[test]
    fn ascii_to_utf16() {
        let mut src = [0u8; 40];
        src[..3].copy_from_slice(b"AB\0");
        assert_match(&src, 2, 2, 0);
        let (buf, r) = run_sut(&src, 2, 2, 0);
        assert_eq!(buf[0], b'A');
        assert_eq!(buf[1], 0);
        assert_eq!(buf[2], b'B');
        assert_eq!(buf[3], 0);
        assert_eq!(buf[4], 0);
        assert_eq!(r.edi - buf.as_ptr() as usize, 4);
    }

    #[test]
    fn ascii_to_utf8_stops_on_nul() {
        let mut src = [0u8; 40];
        src[..4].copy_from_slice(b"AB\0X");
        assert_match(&src, 32, 3, 0);
        let (buf, r) = run_sut(&src, 32, 3, 0);
        // UTF16to8 stores the NUL code unit, then .end_copy_name stores another.
        assert_eq!(&buf[..2], b"AB");
        assert_eq!(buf[2], 0);
        assert_eq!(buf[3], 0);
        assert_eq!(r.edi - buf.as_ptr() as usize, 3);
    }

    #[test]
    fn invalid_nameenc_nul_only_ascii() {
        let mut src = [0u8; 40];
        src[..4].copy_from_slice(b"ABCD");
        assert_match(&src, 32, 0, 0);
        let (buf, r) = run_sut(&src, 32, 99, 0);
        assert_eq!(buf[0], 0);
        assert_eq!(r.edi - buf.as_ptr() as usize, 0);
    }

    #[test]
    fn ucs2_to_cp866() {
        // BE 'A' 'B' → 0x0041 0x0042
        let mut src = [0u8; 40];
        src[0] = 0x00;
        src[1] = b'A';
        src[2] = 0x00;
        src[3] = b'B';
        assert_match(&src, 4, 1, 1);
        let (buf, r) = run_sut(&src, 4, 1, 1);
        assert_eq!(&buf[..2], b"AB");
        assert_eq!(buf[2], 0);
        assert_eq!(buf[3], 0); // word NUL
        assert_eq!(r.edi - buf.as_ptr() as usize, 2);
    }

    #[test]
    fn ucs2_to_utf16_swap() {
        let mut src = [0u8; 40];
        src[0] = 0x04;
        src[1] = 0x10; // U+0410 BE
        assert_match(&src, 2, 2, 1);
        let (buf, _) = run_sut(&src, 2, 2, 1);
        assert_eq!(buf[0], 0x10);
        assert_eq!(buf[1], 0x04);
        assert_eq!(buf[2], 0);
        assert_eq!(buf[3], 0);
    }

    #[test]
    fn ucs2_to_utf8() {
        let mut src = [0u8; 40];
        src[0] = 0x00;
        src[1] = b'A';
        assert_match(&src, 2, 3, 1);
        let (buf, r) = run_sut(&src, 2, 3, 1);
        assert_eq!(buf[0], b'A');
        assert_eq!(buf[1], 0);
        assert_eq!(buf[2], 0);
        assert_eq!(r.edi - buf.as_ptr() as usize, 1);
    }

    #[test]
    fn ptr_form_updates_inout() {
        let mut src = [0u8; 40];
        src[..3].copy_from_slice(b"Z\0X");
        let mut dest = [0xA5u8; 32];
        let mut esi = src.as_mut_ptr();
        let mut edi = dest.as_mut_ptr();
        unsafe { iso9660_copy_name_ptr(&mut esi, &mut edi, 1, 1, 0) };
        assert_eq!(dest[0], b'Z');
        assert_eq!(dest[1], 0);
        assert_eq!(edi, unsafe { dest.as_mut_ptr().add(1) });
        assert_eq!(esi, unsafe { src.as_mut_ptr().add(1) });
    }

    #[test]
    fn differential_prng_50k() {
        let mut state = ISO9660_COPY_NAME_PRNG_SEED;
        let mut next = || {
            state = state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            state
        };
        for _ in 0..50_000 {
            let type_encoding = next() & 1;
            let nameenc = (next() % 5) as u32; // 0..4 includes invalid
            let max_len = if type_encoding == 0 {
                (next() % 33) as u32 // 0..32
            } else {
                ((next() % 17) * 2) as u32 // even 0..32
            };
            let mut src = [0u8; 64];
            for b in src.iter_mut() {
                *b = (next() & 0xFF) as u8;
            }
            // Ensure readable pad past max_len for ASCII→UTF8 overread.
            assert_match(&src, max_len, nameenc, type_encoding);
        }
    }
}
