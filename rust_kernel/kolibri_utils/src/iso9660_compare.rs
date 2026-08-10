//! Cut AJ: `iso9660_compare_name` — ISO9660 path-component name match.
//!
//! Matches `kernel/fs/iso9660.inc` FASM leaf semantics:
//! * Decode UTF-8 path component via `utf8to16`, upper via `utf16_to_upper`
//! * Compare against directory-record name (ASCII or UCS-2 BE via `type_encoding`)
//! * Honor `;` version separator and `name_len` end bound
//! * CF=0 match (ESI advances to `/` or NUL) / CF=1 miss (ESI unchanged)
//!
//! Composes Cut AB + Cut C pure helpers (`inline(always)`) so the freestanding
//! blob stays reloc-free (no FASM cross-calls / GOT).

use crate::casefold::utf16_to_upper;
use crate::utf8to16::utf8to16_ptr;

/// Cut AJ differential PRNG seed (`'CUTJ'`).
pub const ISO9660_COMPARE_NAME_PRNG_SEED: u32 = 0x4355_544A;

/// `ISO9660_DIRECTORY_RECORD.name_len` byte offset.
pub const ISO9660_DIR_OFF_NAME_LEN: usize = 32;
/// `ISO9660_DIRECTORY_RECORD.name` byte offset.
pub const ISO9660_DIR_OFF_NAME: usize = 33;

/// Result of [`iso9660_compare_name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Iso9660CompareNameResult {
    /// `true` = names match (FASM CF=0); `false` = miss (FASM CF=1).
    pub matched: bool,
    /// Bytes to advance ESI on match (0 on miss). Points at `/` or NUL.
    pub esi_advance: usize,
}

#[inline(always)]
unsafe fn load_b(p: *const u8) -> u8 {
    unsafe { *p }
}

#[inline(always)]
unsafe fn load_u16_unaligned(p: *const u8) -> u16 {
    unsafe { core::ptr::read_unaligned(p as *const u16) }
}

/// FASM-faithful `iso9660_compare_name`.
///
/// `type_encoding == 0` → ASCII directory name bytes; nonzero → UCS-2 BE.
///
/// # Safety
/// `utf8` must be readable through the path component (and its terminator).
/// `dir_record` must be readable for `name_len` bytes starting at offset 33
/// (and one extra byte/word for optional `;` peek when present in bounds).
#[inline(always)]
pub unsafe fn iso9660_compare_name(
    utf8: *const u8,
    dir_record: *const u8,
    type_encoding: u32,
) -> Iso9660CompareNameResult {
    let name_base = unsafe { dir_record.add(ISO9660_DIR_OFF_NAME) };
    let name_len = unsafe { load_b(dir_record.add(ISO9660_DIR_OFF_NAME_LEN)) } as usize;
    let name_end = unsafe { name_base.add(name_len) };

    let mut esi = utf8;
    let mut edi = name_base;
    let mut eax: u32 = 0;
    let ascii = type_encoding == 0;

    loop {
        // call utf8to16 ; call utf16toUpper ; mov edx, eax
        let mut esi_slot = esi;
        eax = unsafe { utf8to16_ptr(&mut esi_slot, eax) };
        esi = esi_slot;
        eax = u32::from(utf16_to_upper(eax as u16));
        let edx = eax;

        // mov ax, [edi] (+ ASCII shift/dec) ; xchg al, ah ; utf16toUpper
        let mut ax = unsafe { load_u16_unaligned(edi) };
        if ascii {
            // shl ax, 8 ; dec edi — net +1 after add edi,2
            ax = ax.wrapping_shl(8);
            edi = unsafe { edi.sub(1) };
        }
        ax = ax.swap_bytes(); // xchg al, ah
        eax = u32::from(utf16_to_upper(ax));

        if (eax as u16) != (edx as u16) {
            return Iso9660CompareNameResult {
                matched: false,
                esi_advance: 0,
            };
        }

        edi = unsafe { edi.add(2) };
        let next = unsafe { load_b(esi) };
        if next == b'/' || next == 0 {
            break;
        }
    }

    // End-of-name checks (.done)
    if ascii {
        if unsafe { load_b(edi) } == b';' {
            return matched_advance(utf8, esi);
        }
    } else if unsafe { load_u16_unaligned(edi) } == 0x3B00 {
        // BE ';' (0x003B stored as 0x3B00 on LE read of BE word)
        return matched_advance(utf8, esi);
    }

    if edi == name_end {
        matched_advance(utf8, esi)
    } else {
        Iso9660CompareNameResult {
            matched: false,
            esi_advance: 0,
        }
    }
}

#[inline(always)]
fn matched_advance(start: *const u8, esi: *const u8) -> Iso9660CompareNameResult {
    Iso9660CompareNameResult {
        matched: true,
        esi_advance: (esi as usize).wrapping_sub(start as usize),
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// Returns `0` = match (CF clear) / `1` = miss (CF set). On match updates
/// `*esi_inout` to the advanced ESI; on miss leaves it unchanged.
///
/// # Safety
/// Same as [`iso9660_compare_name`]; `esi_inout` must be valid.
#[inline(always)]
pub unsafe fn iso9660_compare_name_ptr(
    esi_inout: *mut *const u8,
    dir_record: *const u8,
    type_encoding: u32,
) -> u32 {
    let start = unsafe { *esi_inout };
    let r = unsafe { iso9660_compare_name(start, dir_record, type_encoding) };
    if r.matched {
        unsafe {
            *esi_inout = start.add(r.esi_advance);
        }
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casefold::utf16_to_upper_fasm_oracle;

    struct DirRecordBuf {
        bytes: Vec<u8>,
    }

    impl DirRecordBuf {
        fn new(name: &[u8]) -> Self {
            let mut bytes = vec![0u8; ISO9660_DIR_OFF_NAME + name.len() + 4];
            let size = (ISO9660_DIR_OFF_NAME + name.len()) as u8;
            bytes[0] = size;
            bytes[ISO9660_DIR_OFF_NAME_LEN] = name.len() as u8;
            bytes[ISO9660_DIR_OFF_NAME..ISO9660_DIR_OFF_NAME + name.len()].copy_from_slice(name);
            Self { bytes }
        }

        fn as_ptr(&self) -> *const u8 {
            self.bytes.as_ptr()
        }
    }

    fn make_dir_record(name: &[u8]) -> DirRecordBuf {
        DirRecordBuf::new(name)
    }

    /// Independent FASM-flow utf8to16 (duplicate of Cut AB test oracle; not SUT).
    fn utf8to16_oracle(input: &[u8], mut eax: u32, mut esi: usize) -> (u32, usize) {
        loop {
            let al0 = input[esi];
            esi += 1;
            eax = (eax & 0xffff_ff00) | u32::from(al0);
            if (eax as u8) & 0x80 == 0 {
                return (eax & 0xffff_00ff, esi);
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
                    return (eax & 0xffff_00ff, esi);
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
                let ax_out = ax_in.wrapping_shl(3);
                eax = (eax & 0xffff_0000) | u32::from(ax_out);
                if !cf_ax {
                    let ax_fin = (eax as u16) >> 5;
                    eax = (eax & 0xffff_0000) | u32::from(ax_fin);
                    return (eax, esi);
                }
                eax = eax.wrapping_shl(3);
                let al4 = input[esi];
                esi += 1;
                eax = (eax & 0xffff_ff00) | u32::from(al4);
                if (eax as u8) & 0x80 == 0 {
                    return (eax & 0xffff_00ff, esi);
                }
                let al = eax as u8;
                let cf = (al >> 6) & 1 == 1;
                let al = al.wrapping_shl(2);
                eax = (eax & 0xffff_ff00) | u32::from(al);
                if cf {
                    continue;
                }
                eax >>= 2;
                return (eax, esi);
            }
        }
    }

    /// Independent FASM-flow oracle for `iso9660_compare_name` (not a call to SUT).
    fn fasm_oracle_iso9660_compare_name(
        utf8: &[u8],
        dir: &[u8],
        type_encoding: u32,
    ) -> Iso9660CompareNameResult {
        let name_len = dir[ISO9660_DIR_OFF_NAME_LEN] as usize;
        let name_base = ISO9660_DIR_OFF_NAME;
        let name_end = name_base + name_len;
        let ascii = type_encoding == 0;

        let mut esi: usize = 0;
        let mut edi: usize = name_base;
        let mut eax: u32 = 0;

        loop {
            let (ax1, esi1) = utf8to16_oracle(utf8, eax, esi);
            esi = esi1;
            // Mirror production Rust utf16 trampoline (zero-extend AX).
            eax = u32::from(utf16_to_upper_fasm_oracle(ax1 as u16));
            let edx = eax;

            let mut ax = u16::from_le_bytes([dir[edi], dir[edi + 1]]);
            if ascii {
                ax = ax.wrapping_shl(8);
                edi = edi.wrapping_sub(1);
            }
            ax = ax.swap_bytes();
            eax = u32::from(utf16_to_upper_fasm_oracle(ax));

            if (eax as u16) != (edx as u16) {
                return Iso9660CompareNameResult {
                    matched: false,
                    esi_advance: 0,
                };
            }

            edi = edi.wrapping_add(2);
            let next = utf8[esi];
            if next == b'/' || next == 0 {
                break;
            }
        }

        if ascii {
            if dir.get(edi).copied().unwrap_or(0) == b';' {
                return Iso9660CompareNameResult {
                    matched: true,
                    esi_advance: esi,
                };
            }
        } else {
            let w = u16::from_le_bytes([
                dir.get(edi).copied().unwrap_or(0),
                dir.get(edi + 1).copied().unwrap_or(0),
            ]);
            if w == 0x3B00 {
                return Iso9660CompareNameResult {
                    matched: true,
                    esi_advance: esi,
                };
            }
        }

        if edi == name_end {
            Iso9660CompareNameResult {
                matched: true,
                esi_advance: esi,
            }
        } else {
            Iso9660CompareNameResult {
                matched: false,
                esi_advance: 0,
            }
        }
    }

    fn run_one(utf8: &[u8], name: &[u8], enc: u32) -> Iso9660CompareNameResult {
        let rec = make_dir_record(name);
        unsafe { iso9660_compare_name(utf8.as_ptr(), rec.as_ptr(), enc) }
    }

    #[test]
    fn ascii_match_exact() {
        let utf8 = b"ab\0";
        let r = run_one(utf8, b"AB", 0);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(b"AB").bytes, 0);
        assert_eq!(r, o);
        assert!(r.matched);
        assert_eq!(r.esi_advance, 2);
    }

    #[test]
    fn ascii_match_with_version() {
        let utf8 = b"file\0";
        let name = b"FILE;1";
        let r = run_one(utf8, name, 0);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(name).bytes, 0);
        assert_eq!(r, o);
        assert!(r.matched);
        assert_eq!(r.esi_advance, 4);
    }

    #[test]
    fn ascii_match_path_separator() {
        let utf8 = b"dir/next\0";
        let r = run_one(utf8, b"DIR", 0);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(b"DIR").bytes, 0);
        assert_eq!(r, o);
        assert!(r.matched);
        assert_eq!(r.esi_advance, 3); // points at '/'
        assert_eq!(utf8[r.esi_advance], b'/');
    }

    #[test]
    fn ascii_mismatch() {
        let utf8 = b"ab\0";
        let r = run_one(utf8, b"AC", 0);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(b"AC").bytes, 0);
        assert_eq!(r, o);
        assert!(!r.matched);
    }

    #[test]
    fn ascii_too_short_vs_dir() {
        let utf8 = b"a\0";
        let r = run_one(utf8, b"AB", 0);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(b"AB").bytes, 0);
        assert_eq!(r, o);
        assert!(!r.matched);
    }

    #[test]
    fn ucs2_be_match() {
        // UCS-2 BE for "AB" = 00 41 00 42
        let name = [0x00u8, 0x41, 0x00, 0x42];
        let utf8 = b"ab\0";
        let r = run_one(utf8, &name, 1);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(&name).bytes, 1);
        assert_eq!(r, o);
        assert!(r.matched);
    }

    #[test]
    fn joliet_iso_fixture_hello_and_readme() {
        // Exact Joliet root names from images/iso9660-image.iso (UCS-2 BE).
        let hello_name = [
            0x00u8, b'h', 0x00, b'e', 0x00, b'l', 0x00, b'l', 0x00, b'o',
        ];
        let readme_name = [
            0x00u8, b'R', 0x00, b'E', 0x00, b'A', 0x00, b'D', 0x00, b'M', 0x00, b'E', 0x00,
            b'.', 0x00, b'm', 0x00, b'd',
        ];
        let hello = make_dir_record(&hello_name);
        let readme = make_dir_record(&readme_name);

        let path_hello = b"hello\0";
        let r = unsafe { iso9660_compare_name(path_hello.as_ptr(), hello.as_ptr(), 1) };
        assert!(r.matched, "Joliet hello must match");
        assert_eq!(r.esi_advance, 5);

        let path_readme = b"README.md\0";
        let r = unsafe { iso9660_compare_name(path_readme.as_ptr(), readme.as_ptr(), 1) };
        assert!(r.matched, "Joliet README.md must match");
        assert_eq!(r.esi_advance, 9);

        let r = unsafe { iso9660_compare_name(path_hello.as_ptr(), readme.as_ptr(), 1) };
        assert!(!r.matched);
    }

    #[test]
    fn ucs2_be_version_separator() {
        // "A" + BE ';' (U+003B) = 00 41 00 3B — LE word at ';' is 0x3B00
        let name = [0x00u8, 0x41, 0x00, 0x3B];
        let utf8 = b"a\0";
        let r = run_one(utf8, &name, 1);
        let o = fasm_oracle_iso9660_compare_name(utf8, &make_dir_record(&name).bytes, 1);
        assert_eq!(r, o);
        assert!(r.matched);
    }

    #[test]
    fn ptr_updates_esi_on_match_only() {
        let utf8 = b"ab\0";
        let rec = make_dir_record(b"AB");
        let mut p = utf8.as_ptr();
        let rc = unsafe { iso9660_compare_name_ptr(&mut p, rec.as_ptr(), 0) };
        assert_eq!(rc, 0);
        assert_eq!(p, unsafe { utf8.as_ptr().add(2) });

        let mut p2 = utf8.as_ptr();
        let rec2 = make_dir_record(b"AC");
        let rc2 = unsafe { iso9660_compare_name_ptr(&mut p2, rec2.as_ptr(), 0) };
        assert_eq!(rc2, 1);
        assert_eq!(p2, utf8.as_ptr());
    }

    #[test]
    fn prng_differential_50k() {
        let mut state = ISO9660_COMPARE_NAME_PRNG_SEED;
        let mut next = || {
            state = state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            state
        };

        for _ in 0..50_000 {
            let enc = next() & 1;
            let nlen = (next() % 8) as usize + 1;
            let mut name = Vec::with_capacity(if enc == 0 { nlen } else { nlen * 2 });
            let mut utf8 = Vec::new();

            if enc == 0 {
                for _ in 0..nlen {
                    let c = b'A' + (next() % 26) as u8;
                    name.push(c);
                    // randomly lower in path
                    let path_c = if next() & 1 == 0 {
                        c
                    } else {
                        c + 32
                    };
                    utf8.push(path_c);
                }
                if next() & 3 == 0 {
                    name.push(b';');
                    name.push(b'1');
                }
            } else {
                for _ in 0..nlen {
                    let c = b'A' + (next() % 26) as u8;
                    name.push(0x00);
                    name.push(c);
                    let path_c = if next() & 1 == 0 { c } else { c + 32 };
                    utf8.push(path_c);
                }
                if next() & 3 == 0 {
                    name.push(0x00);
                    name.push(0x3B); // BE ';'
                }
            }

            match next() % 5 {
                0 => utf8.push(0),
                1 => {
                    utf8.push(b'/');
                    utf8.extend_from_slice(b"x\0");
                }
                2 => {
                    // mismatch: flip last path char when possible
                    if let Some(last) = utf8.last_mut() {
                        *last = last.wrapping_add(1);
                        if *last == 0 {
                            *last = b'Z';
                        }
                    }
                    utf8.push(0);
                }
                3 => {
                    // truncate path
                    if utf8.len() > 1 {
                        utf8.pop();
                    }
                    utf8.push(0);
                }
                _ => {
                    utf8.push(b'Z');
                    utf8.push(0);
                }
            }

            let rec = make_dir_record(&name);
            let got = unsafe { iso9660_compare_name(utf8.as_ptr(), rec.as_ptr(), enc) };
            let exp = fasm_oracle_iso9660_compare_name(&utf8, &rec.bytes, enc);
            assert_eq!(got, exp, "utf8={utf8:?} name={name:?} enc={enc}");
        }
    }
}
