//! Cut BQ: `cp866toUTF8_string` — CP866 string → UTF-8 buffer streaming loop.
//!
//! Matches `kernel/fs/parse_fn.inc` FASM leaf **bit-exactly**, including:
//! - `lodsb` / `ansi2uni_char` / `UTF16to8` per-byte loop;
//! - overflow exit via SF (from `UTF16to8` `js`);
//! - NUL exit via `test eax,eax` on the **original** Unicode codepoint pushed
//!   before encode (not the encode residue);
//! - may read one source byte past a fixed window when no embedded NUL (FASM
//!   overread quirk shared with Cut BI nameenc=3 path).
//!
//! Composes inlined Cut AN + Q helpers — no cross-Rust-blob calls.

use crate::unicode::cp866_decode;
use crate::utf16_to_8::{pack_sf_zf_eax, utf16_to_8};

/// Cut BQ differential PRNG seed (`'CUPQ'`).
pub const CP866_TO_UTF8_STRING_PRNG_SEED: u32 = 0x4355_5051;

/// Observable exit state of one `cp866toUTF8_string` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cp866ToUtf8StringResult {
    /// Original Unicode codepoint tested by `test eax,eax` (not encode residue).
    pub eax: u32,
    pub esi: u32,
    pub edi: u32,
    pub ecx: u32,
    pub sf: u32,
    pub zf: u32,
}

/// FASM-faithful `cp866toUTF8_string`.
///
/// # Safety
/// `src` must be readable for consumed bytes (including the one-byte overread
/// quirk when the loop runs to SF without an embedded NUL). `dest` must be
/// writable for encoded output up to the byte budget.
#[inline(always)]
pub unsafe fn cp866_to_utf8_string(
    src: *const u8,
    dest: *mut u8,
    ecx_in: u32,
) -> Cp866ToUtf8StringResult {
    let mut esi = src as usize;
    let mut edi = dest as usize;
    let mut ecx = ecx_in;

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
            return Cp866ToUtf8StringResult {
                eax: uni,
                esi: esi as u32,
                edi: edi as u32,
                ecx,
                sf: 1,
                zf: 0,
            };
        }
        if uni == 0 {
            return Cp866ToUtf8StringResult {
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
pub unsafe fn cp866_to_utf8_string_ptr(
    src: *const u8,
    dest: *mut u8,
    ecx_in: u32,
    src_out: *mut u32,
    dest_out: *mut u32,
    ecx_out: *mut u32,
) -> u32 {
    let r = unsafe { cp866_to_utf8_string(src, dest, ecx_in) };
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
    use crate::unicode::cp866_decode;
    use crate::utf16_to_8::utf16_to_8;

    fn oracle_cp866_loop(bytes: &[u8], ecx_in: u32) -> (Cp866ToUtf8StringResult, Vec<u8>) {
        let mut buf = vec![0xA5u8; 512];
        let mut esi_off = 0usize;
        let mut edi_off = 0usize;
        let mut ecx = ecx_in;
        let mut i = 0usize;
        loop {
            let al = if i < bytes.len() {
                let b = bytes[i];
                i += 1;
                b
            } else {
                0
            };
            esi_off = i;
            let uni = cp866_decode(u32::from(al));
            let r = unsafe { utf16_to_8(uni as u16, buf.as_mut_ptr().add(edi_off), ecx) };
            ecx = r.ecx;
            if r.edi_delta != 0 {
                edi_off += r.edi_delta as usize;
            }
            if r.sf != 0 {
                return (
                    Cp866ToUtf8StringResult {
                        eax: uni,
                        esi: esi_off as u32,
                        edi: edi_off as u32,
                        ecx,
                        sf: 1,
                        zf: 0,
                    },
                    buf,
                );
            }
            if uni == 0 {
                return (
                    Cp866ToUtf8StringResult {
                        eax: 0,
                        esi: esi_off as u32,
                        edi: edi_off as u32,
                        ecx,
                        sf: 0,
                        zf: 1,
                    },
                    buf,
                );
            }
        }
    }

    fn run_case(bytes: &[u8], ecx_in: u32) {
        let (oracle, o_buf) = oracle_cp866_loop(bytes, ecx_in);
        let mut buf = vec![0xA5u8; 512];
        let got = unsafe { cp866_to_utf8_string(bytes.as_ptr(), buf.as_mut_ptr(), ecx_in) };
        assert_eq!(
            (got.eax, got.ecx, got.sf, got.zf),
            (oracle.eax, oracle.ecx, oracle.sf, oracle.zf),
            "bytes={bytes:?} ecx_in={ecx_in:#x}"
        );
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(got.esi, oracle.esi, "esi bytes={bytes:?}");
            assert_eq!(got.edi, oracle.edi, "edi bytes={bytes:?}");
        }
        let o_len = oracle.edi as usize;
        assert_eq!(&buf[..o_len], &o_buf[..o_len]);
    }

    #[test]
    fn bq_seed_constant() {
        assert_eq!(CP866_TO_UTF8_STRING_PRNG_SEED, 0x4355_5051);
    }

    #[test]
    fn bq_hello_nul() {
        run_case(b"Hi\0", 16);
    }

    #[test]
    fn bq_overflow_mid_ascii() {
        run_case(b"AA\0", 1);
    }

    #[test]
    fn bq_cyrillic_2byte() {
        // CP866 0xE0 -> U+0410 ('А') -> 2-byte UTF-8
        run_case(&[0xE0, 0], 4);
    }

    #[test]
    fn bq_prng_50k_cupq() {
        let mut s = CP866_TO_UTF8_STRING_PRNG_SEED;
        for _ in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let byte_count = (s % 8) + 1;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let ecx_in = s;
            let mut bytes = Vec::with_capacity(byte_count as usize + 1);
            for _ in 0..byte_count {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                bytes.push(s as u8);
            }
            bytes.push(0);
            run_case(&bytes, ecx_in);
        }
    }
}
