//! Cut K: `fat_next_short_name` — FAT 8.3 short-name collision mutate.
//!
//! Matches `kernel/fs/fat.inc` FASM leaf semantics (reverse `~` search,
//! digit increment / expand into trailing spaces / shrink prefix, CF
//! exhausted). No tables / `.rodata`.
//!
//! Freestanding FFI path uses explicit / volatile byte stores — never
//! `memcpy`/`memset`/`memmove` (those create reloc/GOT blockers; Cut I lesson).

/// 8.3 name length in bytes (8 basename + 3 extension).
pub const FAT_NAME_LEN: usize = 11;
/// Basename length (mutable region).
pub const FAT_BASENAME_LEN: usize = 8;

const _: () = assert!(FAT_NAME_LEN == 11);
const _: () = assert!(FAT_BASENAME_LEN == 8);

/// Result of [`fat_next_short_name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FatNextShortNameResult {
    /// `true` = OK (FASM CF=0); `false` = exhausted (FASM CF=1).
    pub ok: bool,
}

#[inline(always)]
unsafe fn load_b(p: *const u8) -> u8 {
    unsafe { *p }
}

#[inline(always)]
unsafe fn store_b(p: *mut u8, v: u8) {
    // Volatile prevents LLVM from coalescing fills into `memset` (GOT/reloc).
    unsafe { core::ptr::write_volatile(p, v) }
}

/// Fill basename bytes `[start, 8)` with ASCII `'0'` without `memset`.
#[inline(always)]
unsafe fn zerofill_to_end(name: *mut u8, start: usize) {
    // Basename is only 8 bytes — fully unrolled; no loop for LLVM to match.
    if start <= 0 {
        unsafe { store_b(name.add(0), b'0') };
    }
    if start <= 1 {
        unsafe { store_b(name.add(1), b'0') };
    }
    if start <= 2 {
        unsafe { store_b(name.add(2), b'0') };
    }
    if start <= 3 {
        unsafe { store_b(name.add(3), b'0') };
    }
    if start <= 4 {
        unsafe { store_b(name.add(4), b'0') };
    }
    if start <= 5 {
        unsafe { store_b(name.add(5), b'0') };
    }
    if start <= 6 {
        unsafe { store_b(name.add(6), b'0') };
    }
    if start <= 7 {
        unsafe { store_b(name.add(7), b'0') };
    }
}

/// Advance an 11-byte 8.3 name buffer to the next short-name collision
/// candidate (FASM `fat_next_short_name`).
///
/// Mutates basename bytes `0..8` in place. Extension bytes `8..11` are
/// never touched. On success returns `ok=true` (CF clear); when no further
/// candidate fits in 8 basename bytes, returns `ok=false` (CF set).
///
/// # Safety
/// `name` must be readable/writable for [`FAT_NAME_LEN`] bytes.
/// Pathological FASM OOB (`~` at index 0 with no expandable/incrementable
/// suffix) is not exercised; callers must not rely on it.
#[inline(always)]
pub unsafe fn fat_next_short_name(name: *mut u8) -> FatNextShortNameResult {
    // Reverse search for '~' in basename[0..8] (FASM `std`+`repnz scasb`).
    let mut tilde_idx: i32 = -1;
    let mut i: i32 = 7;
    while i >= 0 {
        if unsafe { load_b(name.add(i as usize)) } == b'~' {
            tilde_idx = i;
            break;
        }
        i -= 1;
    }

    if tilde_idx < 0 {
        // No tilde: insert "~1" at end of content.
        let mut pos: usize = 6;
        let b6 = unsafe { load_b(name.add(6)) };
        let b7 = unsafe { load_b(name.add(7)) };
        if b6 == b' ' && b7 == b' ' {
            let mut p: i32 = 6;
            loop {
                p -= 1;
                if unsafe { load_b(name.add(p as usize)) } != b' ' {
                    break;
                }
            }
            pos = (p + 1) as usize;
        }
        unsafe {
            store_b(name.add(pos), b'~');
            store_b(name.add(pos + 1), b'1');
        }
        return FatNextShortNameResult { ok: true };
    }

    // Walk from index 7 left to the tilde.
    let mut edi: i32 = 7;
    let mut space_count: u32 = 0;
    loop {
        let b = unsafe { load_b(name.add(edi as usize)) };
        if b == b'~' {
            if space_count == 0 {
                // .noplace / .err
                let before = edi - 1;
                if before == 0 {
                    return FatNextShortNameResult { ok: false };
                }
                if before < 0 {
                    // Pathological FASM OOB; not exercised.
                    return FatNextShortNameResult { ok: true };
                }
                let p = before as usize;
                unsafe {
                    store_b(name.add(p), b'~');
                    store_b(name.add(p + 1), b'1');
                    zerofill_to_end(name, p + 2);
                }
                return FatNextShortNameResult { ok: true };
            }
            // Expand: insert '1' after '~', shift digits right into spaces.
            let mut pos = (edi + 1) as usize;
            let mut al: u8 = b'1';
            loop {
                let old = unsafe { load_b(name.add(pos)) };
                unsafe { store_b(name.add(pos), al) };
                pos += 1;
                if old == b' ' {
                    break;
                }
                al = b'0';
            }
            return FatNextShortNameResult { ok: true };
        }
        if b == b' ' {
            edi -= 1;
            space_count = space_count.wrapping_add(1);
            continue;
        }
        if b != b'9' {
            unsafe {
                store_b(name.add(edi as usize), b.wrapping_add(1));
                zerofill_to_end(name, (edi as usize) + 1);
            }
            return FatNextShortNameResult { ok: true };
        }
        edi -= 1;
    }
}

/// FFI helper: returns `0` OK / `1` fail for trampoline CF mapping.
///
/// # Safety
/// Same as [`fat_next_short_name`].
#[inline(always)]
pub unsafe fn fat_next_short_name_ptr(name: *mut u8) -> u32 {
    if unsafe { fat_next_short_name(name) }.ok {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Separately coded FASM-faithful oracle (different structure from
    /// production) for differential testing.
    fn fasm_oracle(buf: &mut [u8; FAT_NAME_LEN]) -> bool {
        let mut tilde: Option<usize> = None;
        for i in (0..FAT_BASENAME_LEN).rev() {
            if buf[i] == b'~' {
                tilde = Some(i);
                break;
            }
        }

        if tilde.is_none() {
            let mut pos = 6usize;
            if buf[6] == b' ' && buf[7] == b' ' {
                let mut p = 6isize;
                loop {
                    p -= 1;
                    if buf[p as usize] != b' ' {
                        break;
                    }
                }
                pos = (p + 1) as usize;
            }
            buf[pos] = b'~';
            buf[pos + 1] = b'1';
            return true;
        }

        let mut edi = 7isize;
        let mut spaces: u32 = 0;
        loop {
            let b = buf[edi as usize];
            if b == b'~' {
                if spaces == 0 {
                    let before = edi - 1;
                    if before == 0 {
                        return false;
                    }
                    assert!(before > 0, "pathological '~' at 0 not in oracle corpus");
                    let p = before as usize;
                    buf[p] = b'~';
                    buf[p + 1] = b'1';
                    for cur in (p + 2)..FAT_BASENAME_LEN {
                        buf[cur] = b'0';
                    }
                    return true;
                }
                let mut pos = (edi + 1) as usize;
                let mut al = b'1';
                loop {
                    let old = buf[pos];
                    buf[pos] = al;
                    pos += 1;
                    if old == b' ' {
                        break;
                    }
                    al = b'0';
                }
                return true;
            }
            if b == b' ' {
                edi -= 1;
                spaces = spaces.wrapping_add(1);
                continue;
            }
            if b != b'9' {
                buf[edi as usize] = b.wrapping_add(1);
                for p in ((edi as usize) + 1)..FAT_BASENAME_LEN {
                    buf[p] = b'0';
                }
                return true;
            }
            edi -= 1;
        }
    }

    fn run(input: &[u8; FAT_NAME_LEN]) -> (bool, [u8; FAT_NAME_LEN]) {
        let mut a = *input;
        let ok = unsafe { fat_next_short_name(a.as_mut_ptr()) }.ok;
        (ok, a)
    }

    fn name11(s: &str) -> [u8; FAT_NAME_LEN] {
        assert_eq!(s.len(), FAT_NAME_LEN);
        let mut b = [0u8; FAT_NAME_LEN];
        b.copy_from_slice(s.as_bytes());
        b
    }

    #[test]
    fn insert_tilde_full_basename() {
        let (ok, out) = run(&name11("FILENAMETXT"));
        assert!(ok);
        assert_eq!(&out, b"FILENA~1TXT");
    }

    #[test]
    fn insert_tilde_short_content() {
        let (ok, out) = run(&name11("FILE    TXT"));
        assert!(ok);
        assert_eq!(&out, b"FILE~1  TXT");
    }

    #[test]
    fn increment_digit() {
        let (ok, out) = run(&name11("FILE~1  TXT"));
        assert!(ok);
        assert_eq!(&out, b"FILE~200TXT");
    }

    #[test]
    fn increment_with_zerofill() {
        let (ok, out) = run(&name11("FILE~19 TXT"));
        assert!(ok);
        assert_eq!(&out, b"FILE~200TXT");
    }

    #[test]
    fn nine_carry_expand_into_space() {
        let (ok, out) = run(&name11("FILE~9  TXT"));
        assert!(ok);
        assert_eq!(&out, b"FILE~10 TXT");
    }

    #[test]
    fn nine_carry_expand_double() {
        let (ok, out) = run(&name11("FILE~99 TXT"));
        assert!(ok);
        assert_eq!(&out, b"FILE~100TXT");
    }

    #[test]
    fn noplace_shrink() {
        let (ok, out) = run(&name11("FILE~999TXT"));
        assert!(ok);
        assert_eq!(&out, b"FIL~1000TXT");
    }

    #[test]
    fn err_when_tilde_at_index_1() {
        let input = name11("X~999999TXT");
        let (ok, out) = run(&input);
        assert!(!ok);
        assert_eq!(&out, &input);
    }

    #[test]
    fn extension_untouched_on_mutate() {
        let (ok, out) = run(&name11("ABCDEFGHRST"));
        assert!(ok);
        assert_eq!(&out[8..], b"RST");
    }

    #[test]
    fn named_vs_oracle() {
        let cases: &[[u8; FAT_NAME_LEN]] = &[
            *b"FILENAMETXT",
            *b"FILE    TXT",
            *b"FILE~1  TXT",
            *b"FILE~9  TXT",
            *b"FILE~99 TXT",
            *b"FILE~999TXT",
            *b"X~999999TXT",
            *b"ABCDEFGHRST",
            *b"A~1     BIN",
            *b"~~~~1   EXE",
            *b"TEST~8  DAT",
            *b"HELLO~90ASM",
            *b"Z~99999 COM",
            *b"AB~9999 SYS",
            *b"ABC~999 SYS",
            *b"ABCD~99 SYS",
            *b"ABCDE~9 SYS",
            *b"ABCDEF~ SYS",
            *b"FN~0001 TXT",
            *b"FN~0009 TXT",
        ];
        for c in cases {
            let mut prod = *c;
            let mut ora = *c;
            let ok_p = unsafe { fat_next_short_name(prod.as_mut_ptr()) }.ok;
            let ok_o = fasm_oracle(&mut ora);
            assert_eq!(ok_p, ok_o, "CF mismatch on {:?}", core::str::from_utf8(c));
            assert_eq!(prod, ora, "buf mismatch on {:?}", core::str::from_utf8(c));
        }
    }

    #[test]
    fn ptr_helper_ok_fail() {
        let mut ok_buf = name11("FILE~1  TXT");
        assert_eq!(unsafe { fat_next_short_name_ptr(ok_buf.as_mut_ptr()) }, 0);
        let mut err_buf = name11("X~999999TXT");
        assert_eq!(unsafe { fat_next_short_name_ptr(err_buf.as_mut_ptr()) }, 1);
    }

    #[test]
    fn prng_differential_200k() {
        let mut state = 0xC07B_10EBu32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };

        for _ in 0..200_000u32 {
            let mut buf = [0u8; FAT_NAME_LEN];
            for i in 0..FAT_NAME_LEN {
                let r = (next() % 40) as u8;
                buf[i] = match r {
                    0..=25 => b'A' + r,
                    26..=35 => b'0' + (r - 26),
                    36 => b'~',
                    37 => b' ',
                    _ => b'X',
                };
            }
            for i in 8..11 {
                buf[i] = b'A' + ((next() % 26) as u8);
            }
            if would_hit_fasm_oob(&buf) {
                continue;
            }

            let mut prod = buf;
            let mut ora = buf;
            let ok_p = unsafe { fat_next_short_name(prod.as_mut_ptr()) }.ok;
            let ok_o = fasm_oracle(&mut ora);
            assert_eq!(ok_p, ok_o, "CF mismatch PRNG buf={:?}", buf);
            assert_eq!(prod, ora, "buf mismatch PRNG");
            assert_eq!(&prod[8..], &buf[8..]);
        }
    }

    fn would_hit_fasm_oob(buf: &[u8; FAT_NAME_LEN]) -> bool {
        let mut tilde = None;
        for i in (0..8).rev() {
            if buf[i] == b'~' {
                tilde = Some(i);
                break;
            }
        }
        if tilde != Some(0) {
            return false;
        }
        let mut edi = 7isize;
        let mut spaces = 0u32;
        loop {
            let b = buf[edi as usize];
            if b == b'~' {
                return spaces == 0;
            }
            if b == b' ' {
                spaces += 1;
                edi -= 1;
                continue;
            }
            if b != b'9' {
                return false;
            }
            edi -= 1;
        }
    }

    #[test]
    fn sequential_collision_chain() {
        let mut buf = name11("LONGNAMEEXE");
        assert!(unsafe { fat_next_short_name(buf.as_mut_ptr()) }.ok);
        assert_eq!(&buf, b"LONGNA~1EXE");
        for expect in [b"LONGNA~2EXE", b"LONGNA~3EXE", b"LONGNA~4EXE"] {
            assert!(unsafe { fat_next_short_name(buf.as_mut_ptr()) }.ok);
            assert_eq!(&buf, expect);
        }
    }
}
