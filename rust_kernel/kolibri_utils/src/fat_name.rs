//! Cut K: `fat_next_short_name` — FAT 8.3 short-name collision mutate.
//! Cut U: `fat_gen_short_name` — UTF-8 LFN → 8.3 short-name generator.
//!
//! Matches `kernel/fs/fat.inc` FASM leaf semantics (reverse `~` search,
//! digit increment / expand into trailing spaces / shrink prefix, CF
//! exhausted; UTF-8→8.3 state machine with `fat_legal_chars` / multi-dot).
//! No `.rodata` in freestanding blobs — legality table is stack-materialized.
//!
//! Freestanding FFI path uses explicit / volatile byte stores — never
//! `memcpy`/`memset`/`memmove` (those create reloc/GOT blockers; Cut I lesson).

use crate::casefold::cp866_to_upper;

/// 8.3 name length in bytes (8 basename + 3 extension).
pub const FAT_NAME_LEN: usize = 11;
/// Basename length (mutable region).
pub const FAT_BASENAME_LEN: usize = 8;
/// FASM `stosd`×3 fill width (caller allocates 12 on stack).
pub const FAT_GEN_FILL_LEN: usize = 12;
/// PRNG seed for Cut U differential corpus.
pub const FAT_GEN_SHORT_NAME_PRNG_SEED: u32 = 0xF47_6E01;

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

/// Materialize FASM `fat_legal_chars` bit2 test without `.rodata`.
///
/// Returns true iff `(fat_legal_chars[c] & 2) != 0` (allowed in short names).
/// Values 0/1 in the FASM table reject; value 3 accepts.
#[inline(always)]
fn fat_short_char_ok(c: u8) -> bool {
    if c == b'!' {
        return true;
    }
    // '#' .. ')' = # $ % & ' ( )
    if c >= b'#' && c <= b')' {
        return true;
    }
    if c == b'-' || c == b'.' {
        return true;
    }
    if c >= b'0' && c <= b'9' {
        return true;
    }
    if c >= b'@' && c <= b'Z' {
        return true;
    }
    if c == b'^' || c == b'_' {
        return true;
    }
    if c >= b'`' && c <= b'z' {
        return true;
    }
    if c == b'{' || c == b'}' || c == b'~' {
        return true;
    }
    false
}

/// Generate a FAT 8.3 short name from a UTF-8 LFN (FASM `fat_gen_short_name`).
///
/// Fills 12 space bytes at `out` (matching `stosd`×3), then writes the 8.3
/// form into bytes `0..11`. When the conversion is lossy (BH bit0), calls
/// [`fat_next_short_name`] on the basename. High-bit UTF-8 bytes are skipped
/// as lossy (FASM `test al,al` / `js .space`).
///
/// # Safety
/// `src` must point to a readable NUL-terminated byte string.
/// `out` must be writable for [`FAT_GEN_FILL_LEN`] bytes (caller stack slot).
#[inline(always)]
pub unsafe fn fat_gen_short_name(src: *const u8, out: *mut u8) {
    // SAFETY: caller guarantees out is ≥12 writable bytes.
    unsafe {
        // FASM: mov eax,'    '; stosd×3
        let mut i = 0usize;
        while i < FAT_GEN_FILL_LEN {
            store_b(out.add(i), b' ');
            i += 1;
        }

        // Pointer indices relative to `out` (FASM EDI/ECX as offsets).
        let mut edi: usize = 0;
        let ecx: usize = 8; // extension start
        let mut bl: u8 = 8; // remaining slots in current field
        let mut bh: u8 = 0; // flags: bit0 lossy, bit2 had-first-dot
        let mut saved: usize = 0; // FASM stack slot from .firstdot
        let mut esi = src;

        loop {
            let al = load_b(esi);
            esi = esi.add(1);

            // test al, al / js .space  (SF from AL — high bit)
            if (al as i8) < 0 {
                bh |= 1;
                continue;
            }
            if al == 0 {
                break;
            }
            // test [fat_legal_chars+eax], 2 / jz .space
            if !fat_short_char_ok(al) {
                bh |= 1;
                continue;
            }
            if al == b'.' {
                if (bh & 2) == 0 {
                    // .firstdot
                    if bl == 8 {
                        bh |= 1;
                        continue;
                    }
                    saved = edi;
                    bh |= 2;
                    edi = ecx;
                    bl = 3;
                    continue;
                }
                // second+ dot: rewind prior extension into basename
                let mut ebx = saved.wrapping_add(edi).wrapping_sub(ecx);
                saved = ebx;
                if ebx >= ecx {
                    saved = ecx;
                    ebx = ecx;
                }
                if edi > ecx {
                    loop {
                        edi = edi.wrapping_sub(1);
                        let ch = load_b(out.add(edi));
                        ebx = ebx.wrapping_sub(1);
                        store_b(out.add(ebx), ch);
                        store_b(out.add(edi), b' ');
                        if edi <= ecx {
                            break;
                        }
                    }
                }
                bh = 3;
                edi = ecx;
                bl = 3;
                continue;
            }

            // dec bl / jns .store  (signed)
            let bl_dec = bl.wrapping_sub(1);
            if (bl_dec as i8) >= 0 {
                bl = bl_dec;
                let up = cp866_to_upper(al);
                store_b(out.add(edi), up);
                edi = edi.wrapping_add(1);
                continue;
            }
            // overflow: restore bl, mark lossy
            bl = bl_dec.wrapping_add(1);
            bh |= 1;
        }

        // FASM pops the saved firstdot/rewind slot when bh bit2 set (discard).
        let _ = saved;
        // lea edi, [ecx-8] — point at name start; then maybe fat_next
        if (bh & 1) != 0 {
            let _ = fat_next_short_name(out);
        }
    }
}

/// FFI helper for stdcall trampoline.
///
/// # Safety
/// Same as [`fat_gen_short_name`].
#[inline(always)]
pub unsafe fn fat_gen_short_name_ptr(src: *const u8, out: *mut u8) {
    unsafe { fat_gen_short_name(src, out) }
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

    // -------- Cut U: fat_gen_short_name --------

    /// Host-side FASM `fat_legal_chars` bit2 oracle (table form).
    fn fasm_legal_bit2(c: u8) -> bool {
        let mut t = [0u8; 128];
        // rows matching fat.inc iglobal
        let r1 = [1u8, 3, 0, 3, 3, 3, 3, 3, 3, 3, 0, 1, 1, 3, 3, 0];
        let r2 = [3u8, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 1, 0, 1, 0, 0];
        let r3 = [3u8, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 1, 0, 1, 3, 3];
        let r4 = [3u8, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 3, 3, 0];
        t[32..48].copy_from_slice(&r1);
        t[48..64].copy_from_slice(&r2);
        for i in 64..80 {
            t[i] = 3;
        }
        t[80..96].copy_from_slice(&r3);
        for i in 96..112 {
            t[i] = 3;
        }
        t[112..128].copy_from_slice(&r4);
        if (c as usize) >= 128 {
            return false;
        }
        (t[c as usize] & 2) != 0
    }

    fn cp866_upper_u(ch: u8) -> u8 {
        crate::casefold::cp866_to_upper_fasm_oracle(ch)
    }

    /// Independent FASM-flow oracle for `fat_gen_short_name` (structurally
    /// different from production: uses table + explicit stack vec).
    fn fat_gen_fasm_oracle(src: &[u8]) -> [u8; FAT_GEN_FILL_LEN] {
        let mut buf = [b' '; FAT_GEN_FILL_LEN];
        let mut edi: usize = 0;
        let ecx: usize = 8;
        let mut bl: u8 = 8;
        let mut bh: u8 = 0;
        let mut stack: Vec<usize> = Vec::new();
        let mut i = 0usize;
        // Append implicit NUL if missing
        let mut bytes = src.to_vec();
        if bytes.last().copied() != Some(0) {
            bytes.push(0);
        }
        loop {
            let al = bytes[i];
            i += 1;
            if (al as i8) < 0 {
                bh |= 1;
                continue;
            }
            if al == 0 {
                break;
            }
            if !fasm_legal_bit2(al) {
                bh |= 1;
                continue;
            }
            if al == b'.' {
                if (bh & 2) == 0 {
                    if bl == 8 {
                        bh |= 1;
                        continue;
                    }
                    stack.push(edi);
                    bh |= 2;
                    edi = ecx;
                    bl = 3;
                    continue;
                }
                let mut ebx = stack.pop().unwrap().wrapping_add(edi).wrapping_sub(ecx);
                stack.push(ebx);
                if ebx >= ecx {
                    stack.pop();
                    stack.push(ecx);
                    ebx = ecx;
                }
                if edi > ecx {
                    loop {
                        edi -= 1;
                        let ch = buf[edi];
                        ebx -= 1;
                        buf[ebx] = ch;
                        buf[edi] = b' ';
                        if edi <= ecx {
                            break;
                        }
                    }
                }
                bh = 3;
                edi = ecx;
                bl = 3;
                continue;
            }
            let bl_dec = bl.wrapping_sub(1);
            if (bl_dec as i8) >= 0 {
                bl = bl_dec;
                buf[edi] = cp866_upper_u(al);
                edi += 1;
                continue;
            }
            bl = bl_dec.wrapping_add(1);
            bh |= 1;
        }
        if (bh & 2) != 0 {
            let _ = stack.pop();
        }
        if (bh & 1) != 0 {
            let mut name = [0u8; FAT_NAME_LEN];
            name.copy_from_slice(&buf[..FAT_NAME_LEN]);
            // Cut K pathological: all-space basename + insert-~1 walks EDI
            // before the buffer (FASM OOB). Skip oracle mutate; production
            // uses the same raw-pointer walk (not differentially comparable
            // under host bounds checks).
            if name[..FAT_BASENAME_LEN].iter().all(|&b| b == b' ') {
                return buf;
            }
            let _ = fasm_oracle(&mut name);
            buf[..FAT_NAME_LEN].copy_from_slice(&name);
        }
        buf
    }

    fn gen_run(src: &str) -> [u8; FAT_GEN_FILL_LEN] {
        let mut out = [0u8; FAT_GEN_FILL_LEN];
        let mut s = src.as_bytes().to_vec();
        s.push(0);
        unsafe { fat_gen_short_name(s.as_ptr(), out.as_mut_ptr()) };
        out
    }

    fn basename_all_spaces(out: &[u8; FAT_GEN_FILL_LEN]) -> bool {
        out[..FAT_BASENAME_LEN].iter().all(|&b| b == b' ')
    }

    #[test]
    fn fat_short_char_ok_matches_table() {
        for c in 0u8..=127 {
            assert_eq!(
                fat_short_char_ok(c),
                fasm_legal_bit2(c),
                "mismatch at c=0x{c:02X}"
            );
        }
    }

    #[test]
    fn gen_simple_file_txt() {
        assert_eq!(&gen_run("file.txt")[..11], b"FILE    TXT");
    }

    #[test]
    fn gen_hello_no_ext() {
        assert_eq!(&gen_run("HELLO")[..11], b"HELLO      ");
    }

    #[test]
    fn gen_long_lossy() {
        assert_eq!(&gen_run("longfilename.txt")[..11], b"LONGFI~1TXT");
    }

    #[test]
    fn gen_multi_dot() {
        assert_eq!(&gen_run("a.b.c")[..11], b"AB~1    C  ");
    }

    #[test]
    fn gen_leading_dot() {
        assert_eq!(&gen_run(".hidden")[..11], b"HIDDEN~1   ");
    }

    #[test]
    fn gen_space_lossy() {
        assert_eq!(&gen_run("test file.txt")[..11], b"TESTFI~1TXT");
    }

    #[test]
    fn gen_plus_lfn_only() {
        assert_eq!(&gen_run("ok+")[..11], b"OK~1       ");
    }

    #[test]
    fn gen_fills_twelve_spaces_initially_observable() {
        // Byte 11 is the 12th space from stosd×3; remains space if unused.
        let out = gen_run("A");
        assert_eq!(out[11], b' ');
        assert_eq!(&out[..11], b"A          ");
    }

    #[test]
    fn gen_named_vs_oracle() {
        let cases: &[&[u8]] = &[
            b"file.txt",
            b"HELLO",
            b"a.b.c",
            b"longfilename.txt",
            b".hidden",
            b"foo..bar",
            b"test file.txt",
            b"ok+",
            b"name~1.txt",
            b"x.y.z.w",
            b"UPPER.low",
            b"12345678.ABC",
            b"123456789.ABC",
            b"a",
            b"",
            b"foo.bar.baz.qux",
            b"file name",
            b"ok!.txt",
            b"a{b}.c",
            b"~~tilde~~.txt",
        ];
        for c in cases {
            let mut s = c.to_vec();
            s.push(0);
            let mut prod = [0u8; FAT_GEN_FILL_LEN];
            unsafe { fat_gen_short_name(s.as_ptr(), prod.as_mut_ptr()) };
            if basename_all_spaces(&prod) {
                continue;
            }
            let ora = fat_gen_fasm_oracle(c);
            assert_eq!(prod, ora, "mismatch on {:?}", core::str::from_utf8(c));
        }
    }

    #[test]
    fn gen_prng_differential_50k() {
        let mut state = FAT_GEN_SHORT_NAME_PRNG_SEED;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..50_000u32 {
            let len = (next() % 24) as usize;
            let mut s = Vec::with_capacity(len + 1);
            for _ in 0..len {
                let r = (next() % 70) as u8;
                let ch = match r {
                    0..=25 => b'a' + r,
                    26..=51 => b'A' + (r - 26),
                    52..=61 => b'0' + (r - 52),
                    62 => b'.',
                    63 => b' ',
                    64 => b'+',
                    65 => b'~',
                    66 => b'_',
                    67 => b'-',
                    68 => 0xC0, // high-bit UTF-8-ish
                    _ => b'!',
                };
                s.push(ch);
            }
            let mut s_nul = s.clone();
            s_nul.push(0);
            // Pre-filter: no storeable non-dot short chars ⇒ empty basename +
            // lossy ⇒ Cut K all-space OOB family.
            let has_store = s.iter().any(|&c| c != b'.' && fat_short_char_ok(c) && (c as i8) >= 0);
            if !has_store {
                continue;
            }
            let mut prod = [0u8; FAT_GEN_FILL_LEN];
            unsafe { fat_gen_short_name(s_nul.as_ptr(), prod.as_mut_ptr()) };
            if basename_all_spaces(&prod) {
                continue;
            }
            let ora = fat_gen_fasm_oracle(&s);
            assert_eq!(prod, ora, "PRNG mismatch src={:?}", s);
        }
    }
}
