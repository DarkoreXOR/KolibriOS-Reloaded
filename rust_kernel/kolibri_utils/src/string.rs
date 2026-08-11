//! String helpers matching FASM leaves in `kernel/core/string.inc`
//! and `strlen` in `kernel/fs/parse_fn.inc`.

/// Cut BB differential PRNG seed (`'CUBB'`).
pub const STRRCHR_PRNG_SEED: u32 = 0x4355_4242;

/// Cut BF differential PRNG seed (`'CUBF'`).
pub const STRNCPY_PRNG_SEED: u32 = 0x4355_4246;

/// Cut BH differential PRNG seed (`'CUBH'`).
pub const STRLEN_PRNG_SEED: u32 = 0x4355_4248;

/// C-string length matching FASM `strlen` (`parse_fn.inc`).
///
/// Mirrors:
/// ```text
///   or ecx, -1
///   mov edi, esi
///   xor eax, eax
///   repnz scasb
///   inc ecx
///   not ecx
/// ```
///
/// Returns the byte count before the terminating NUL (empty → 0).
/// Does not write memory; does not change DF.
///
/// # Safety
/// `s` must be a readable NUL-terminated C string.
#[inline(always)]
pub unsafe fn strlen(s: *const u8) -> u32 {
    // FASM: or ecx,-1 / repnz scasb / inc ecx / not ecx
    let mut ecx = 0u32.wrapping_sub(1); // -1
    let mut edi = s as usize;
    loop {
        // SAFETY: within readable C string through NUL.
        let b = unsafe { *(edi as *const u8) };
        edi = edi.wrapping_add(1);
        ecx = ecx.wrapping_sub(1);
        if b == 0 {
            break;
        }
    }
    ecx = ecx.wrapping_add(1);
    !ecx
}

/// Bounded padded copy matching FASM `strncpy` (`string.inc`).
///
/// Always writes exactly `n` bytes to `s1`:
/// * copy from `s2` including the terminating NUL when it falls within `n`;
/// * null-pad the remainder of the `n`-byte window;
/// * if no NUL appears in the first `n` source bytes, copy `n` bytes with
///   **no** terminator (C `strncpy` semantics).
///
/// `n == 0` → no write; returns `s1`.
///
/// Returns `s1` as `usize` (EAX on freestanding i686).
///
/// Implementation notes: single forward pass with `write_volatile` so LLVM
/// does not emit `memcpy`/`memset` (those introduce GOT/PLT relocs that the
/// reloc-free extractor rejects).
///
/// # Safety
/// `s2` must be readable for the bytes actually scanned (at most `n`, or
/// until NUL inclusive). `s1` must be writable for `n` bytes when `n > 0`.
#[inline(always)]
pub unsafe fn strncpy(s1: *mut u8, s2: *const u8, n: u32) -> usize {
    let mut i = 0u32;
    let mut saw_nul = false;
    while i < n {
        let b = if saw_nul {
            0u8
        } else {
            // SAFETY: within the n-byte / until-NUL contract.
            let c = unsafe { *s2.add(i as usize) };
            if c == 0 {
                saw_nul = true;
            }
            c
        };
        // Volatile store: prevents LLVM memset/memcpy outlining (reloc-free).
        unsafe {
            core::ptr::write_volatile(s1.add(i as usize), b);
        }
        i = i.wrapping_add(1);
    }
    s1 as usize
}

/// Last occurrence of byte `c` in C string `s`, matching FASM `strrchr`.
///
/// Mirrors `string.inc`: forward scan to NUL (length including NUL), then
/// reverse scan for `(c as u8)`; miss → `0`. Only the low 8 bits of `c`
/// participate (`scasb` / `AL`).
///
/// Returns the match address as `usize` (`0` = NULL). On freestanding i686
/// this is a dword suitable for EAX; host tests keep full pointer width.
///
/// # Safety
/// `s` must be a readable NUL-terminated C string.
#[inline(always)]
pub unsafe fn strrchr(s: *const u8, c: u32) -> usize {
    let needle = c as u8;
    let mut p = s as usize;
    // Forward find NUL; `len` counts bytes including the terminator.
    let mut len: u32 = 0;
    loop {
        // SAFETY: caller guarantees readable C string through NUL.
        let b = unsafe { *(p as *const u8) };
        p = p.wrapping_add(1);
        len = len.wrapping_add(1);
        if b == 0 {
            break;
        }
    }
    // `p` is one past NUL — step back onto the terminator (FASM `dec edi`).
    p = p.wrapping_sub(1);
    let mut ecx = len;
    while ecx != 0 {
        // SAFETY: still within [s, NUL] inclusive.
        if unsafe { *(p as *const u8) } == needle {
            return p;
        }
        ecx = ecx.wrapping_sub(1);
        if ecx == 0 {
            break;
        }
        p = p.wrapping_sub(1);
    }
    0
}

/// Compare up to `n` bytes of two C strings, matching FASM `strncmp`.
///
/// Returns −1 / 0 / +1 from an **unsigned** byte compare, the same as
/// FASM `cmpsb` + `seta`/`setb`/`movsx`.
///
/// `n == 0` → 0. `n == 0xFFFF_FFFF` (−1) means “until NUL or mismatch”
/// for practical string lengths (counter only exhausts after 2³² steps).
///
/// Inlined into the FFI entry so `.text.rust_strncmp` stays reloc-free.
///
/// # Safety
/// `s1` and `s2` must be readable for the bytes actually compared
/// (at most `n` bytes, or until a `0` byte is seen on an equal path).
#[inline(always)]
pub unsafe fn strncmp(s1: *const u8, s2: *const u8, mut n: u32) -> i32 {
    if n == 0 {
        return 0;
    }
    let mut p1 = s1;
    let mut p2 = s2;
    loop {
        // SAFETY: caller guarantees these bytes are readable for this compare.
        let a = unsafe { *p1 };
        let b = unsafe { *p2 };
        if a != b {
            // Unsigned byte order (matches cmpsb / seta / setb).
            return if a > b { 1 } else { -1 };
        }
        if a == 0 {
            return 0;
        }
        // SAFETY: still within the n-byte / until-NUL contract.
        p1 = unsafe { p1.add(1) };
        p2 = unsafe { p2.add(1) };
        n = n.wrapping_sub(1);
        if n == 0 {
            return 0;
        }
    }
}

/// Host-side FASM-faithful oracle (mirrors `string.inc` control flow).
#[cfg(test)]
pub fn strncmp_fasm_oracle(s1: &[u8], s2: &[u8], n: u32) -> i32 {
    if n == 0 {
        return 0;
    }
    let mut i = 0u32;
    loop {
        let a = s1[i as usize];
        let b = s2[i as usize];
        if a != b {
            return if a > b { 1 } else { -1 };
        }
        if a == 0 {
            return 0;
        }
        i = i.wrapping_add(1);
        let remaining = n.wrapping_sub(i);
        if remaining == 0 {
            return 0;
        }
        // Guard host slices (production paths must not overrun).
        assert!((i as usize) < s1.len() && (i as usize) < s2.len());
    }
}

/// Independent FASM-flow oracle for `strncpy` (mirrors `string.inc` scasb/movsb/stosb).
#[cfg(test)]
pub fn strncpy_fasm_oracle(s2: &[u8], n: u32) -> Vec<u8> {
    let mut out = vec![0u8; n as usize];
    if n == 0 {
        return out;
    }
    // FASM: ecx=n; edi=s2; edx=n; repne scasb for 0.
    let mut ecx = n;
    let mut scanned = 0u32;
    let mut found_nul = false;
    while ecx != 0 {
        assert!((scanned as usize) < s2.len(), "oracle overrun (missing NUL or n too large)");
        let b = s2[scanned as usize];
        scanned = scanned.wrapping_add(1);
        ecx = ecx.wrapping_sub(1);
        if b == 0 {
            found_nul = true;
            break;
        }
    }
    // sub edx,ecx → bytes to copy (incl NUL if found); else n when ecx==0.
    let copy_len = if found_nul {
        scanned // includes NUL
    } else {
        n
    };
    let pad = n.wrapping_sub(copy_len);
    for i in 0..copy_len as usize {
        out[i] = s2[i];
    }
    for i in 0..pad as usize {
        out[copy_len as usize + i] = 0;
    }
    out
}

/// Independent FASM-flow oracle for `strrchr` (not derived from the Rust body).
#[cfg(test)]
pub fn strrchr_fasm_oracle(s: &[u8], c: u32) -> Option<usize> {
    let needle = (c & 0xff) as u8;
    // Forward: ecx=-1; repne scasb for 0; not ecx → length including NUL.
    let mut i = 0usize;
    loop {
        assert!(i < s.len(), "oracle overrun (missing NUL)");
        let b = s[i];
        i += 1;
        if b == 0 {
            break;
        }
    }
    let len = i; // includes NUL
    // Reverse from NUL index (len-1), for `len` steps.
    let mut idx = len - 1;
    let mut ecx = len;
    while ecx != 0 {
        if s[idx] == needle {
            return Some(idx);
        }
        ecx -= 1;
        if ecx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        strlen, strncmp, strncmp_fasm_oracle, strncpy, strncpy_fasm_oracle, strrchr,
        strrchr_fasm_oracle, STRLEN_PRNG_SEED, STRNCPY_PRNG_SEED, STRRCHR_PRNG_SEED,
    };

    fn check_strncpy(s2: &[u8], n: u32) {
        let expect = strncpy_fasm_oracle(s2, n);
        let mut got = vec![0xA5u8; n as usize];
        let ret = unsafe { strncpy(got.as_mut_ptr(), s2.as_ptr(), n) };
        assert_eq!(ret, got.as_ptr() as usize);
        assert_eq!(got, expect, "mismatch s2={s2:?} n={n}");
    }

    #[test]
    fn strncpy_n_zero_no_write() {
        let mut dst = [0xA5u8; 4];
        let ret = unsafe { strncpy(dst.as_mut_ptr(), b"hi\0".as_ptr(), 0) };
        assert_eq!(ret, dst.as_ptr() as usize);
        assert_eq!(dst, [0xA5; 4]);
        check_strncpy(b"hi\0", 0);
    }

    #[test]
    fn strncpy_pad_after_early_nul() {
        check_strncpy(b"ab\0", 5);
        check_strncpy(b"\0", 4);
        check_strncpy(b"x\0", 1);
        check_strncpy(b"x\0", 2);
    }

    #[test]
    fn strncpy_trunc_no_nul_in_n() {
        // Source longer than n and readable for n bytes (no NUL in window).
        check_strncpy(b"abcdef", 3);
        check_strncpy(b"abcdef", 6);
        check_strncpy(b"abc\0def", 3); // stops at n before NUL
    }

    #[test]
    fn strncpy_exact_fit_with_nul() {
        check_strncpy(b"abc\0", 4);
        check_strncpy(b"abc\0", 8);
    }

    #[test]
    fn strncpy_shmem_name_window() {
        // Live caller: shmem_open copies name into 31-byte field.
        check_strncpy(b"short\0", 31);
        check_strncpy(b"0123456789012345678901234567890\0", 31); // 31 chars + NUL → trunc 31
    }

    #[test]
    fn strncpy_prng_50k_matches_oracle() {
        let mut state = STRNCPY_PRNG_SEED;
        let mut src = [0u8; 96];
        for _ in 0..50_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let n = (state % 48) as u32;
            let mode = state % 3;
            let readable: &[u8] = match mode {
                0 => {
                    // C string with NUL at or before n.
                    let body = if n == 0 {
                        0
                    } else {
                        (state as usize) % (n as usize)
                    };
                    for i in 0..body {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        src[i] = (state as u8).wrapping_add(1).max(1);
                    }
                    src[body] = 0;
                    &src[..=body]
                }
                1 => {
                    // Exactly n non-NUL bytes (truncation / no terminator).
                    for i in 0..(n as usize) {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        src[i] = (state as u8).wrapping_add(1).max(1);
                    }
                    &src[..n as usize]
                }
                _ => {
                    // C string longer than n (NUL after the window).
                    let total = (n as usize) + 1 + ((state as usize) % 8);
                    for i in 0..total.saturating_sub(1) {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        src[i] = (state as u8).wrapping_add(1).max(1);
                    }
                    src[total - 1] = 0;
                    &src[..total]
                }
            };
            check_strncpy(readable, n);
        }
    }

    fn rust_vs(s1: &[u8], s2: &[u8], n: u32) -> i32 {
        unsafe { strncmp(s1.as_ptr(), s2.as_ptr(), n) }
    }

    fn rust_strrchr_offset(s: &[u8], c: u32) -> Option<usize> {
        let base = s.as_ptr() as usize;
        let p = unsafe { strrchr(s.as_ptr(), c) };
        if p == 0 {
            None
        } else {
            Some(p - base)
        }
    }

    fn check_strrchr(s: &[u8], c: u32) {
        assert!(s.ends_with(&[0]), "fixture must be NUL-terminated");
        let got = rust_strrchr_offset(s, c);
        let exp = strrchr_fasm_oracle(s, c);
        assert_eq!(got, exp, "mismatch s={s:?} c={c:#x}");
    }

    #[test]
    fn strrchr_empty_and_nul_needle() {
        check_strrchr(b"\0", 0);
        check_strrchr(b"\0", b'/' as u32);
        check_strrchr(b"\0", 0x100); // only low byte → NUL
    }

    #[test]
    fn strrchr_path_last_slash() {
        check_strrchr(b"/sys/app\0", b'/' as u32);
        check_strrchr(b"app\0", b'/' as u32);
        check_strrchr(b"/a/b/c\0", b'/' as u32);
        check_strrchr(b"///\0", b'/' as u32);
    }

    #[test]
    fn strrchr_first_mid_last() {
        check_strrchr(b"abc\0", b'a' as u32);
        check_strrchr(b"abc\0", b'b' as u32);
        check_strrchr(b"abc\0", b'c' as u32);
        check_strrchr(b"abca\0", b'a' as u32);
        check_strrchr(b"xxx\0", b'y' as u32);
    }

    #[test]
    fn strrchr_wide_c_truncates_to_byte() {
        check_strrchr(b"x/y\0", 0x1234_002F); // '/'
        check_strrchr(b"x/y\0", 0xFFFFFF00u32); // NUL
    }

    #[test]
    fn strrchr_prng_50k_matches_oracle() {
        let mut state = STRRCHR_PRNG_SEED;
        let mut buf = [0u8; 96];
        for _ in 0..50_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let len = (state % 64) as usize;
            for i in 0..len {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                buf[i] = (state as u8).wrapping_add(1).max(1); // no early NUL
            }
            buf[len] = 0;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let c = state;
            check_strrchr(&buf[..=len], c);
        }
    }

    #[test]
    fn named_vectors_match_oracle() {
        let cases: &[(&[u8], &[u8], u32)] = &[
            (b"\0", b"\0", 0),
            (b"\0", b"\0", 1),
            (b"a\0", b"a\0", 1),
            (b"a\0", b"b\0", 1),
            (b"b\0", b"a\0", 1),
            (b"abc\0", b"abc\0", 3),
            (b"abc\0", b"abd\0", 3),
            (b"ab\0", b"abc\0", 3),
            (b"abc\0", b"ab\0", 3),
            (b"abc\0", b"abc\0", 100),
            (b"\0", b"a\0", 1),
            (b"a\0", b"\0", 1),
            // unsigned: 0xFF > 0x00
            (&[0xFF, 0], &[0x00, 0], 1),
            (&[0x00, 0], &[0xFF, 0], 1),
            // n limits before difference
            (b"ax\0", b"ay\0", 1),
            // n = -1 unlimited
            (b"kernel\0", b"kernel\0", u32::MAX),
            (b"kernel\0", b"kernex\0", u32::MAX),
            (b"kern\0", b"kernel\0", u32::MAX),
        ];
        for &(s1, s2, n) in cases {
            let got = rust_vs(s1, s2, n);
            let expect = strncmp_fasm_oracle(s1, s2, n);
            assert_eq!(got, expect, "s1={s1:?} s2={s2:?} n={n}");
            assert!(got == -1 || got == 0 || got == 1);
        }
    }

    #[test]
    fn n_zero_always_equal() {
        assert_eq!(rust_vs(b"abc\0", b"xyz\0", 0), 0);
        assert_eq!(strncmp_fasm_oracle(b"abc\0", b"xyz\0", 0), 0);
    }

    #[test]
    fn sign_values_are_exact() {
        assert_eq!(rust_vs(b"a\0", b"b\0", 1), -1);
        assert_eq!(rust_vs(b"b\0", b"a\0", 1), 1);
        assert_eq!(rust_vs(b"a\0", b"a\0", 1), 0);
    }

    #[test]
    fn deterministic_prng_corpus_vs_oracle() {
        // Fixed-seed xorshift32 — reproducible across hosts.
        let mut state = 0xC0FF_EE01u32;
        let mut scratch_a = [0u8; 64];
        let mut scratch_b = [0u8; 64];
        for _ in 0..10_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let len = (state % 48) as usize + 1;
            for i in 0..len {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                scratch_a[i] = (state as u8).wrapping_add(1).max(1); // avoid early NUL
                scratch_b[i] = scratch_a[i];
            }
            scratch_a[len] = 0;
            scratch_b[len] = 0;
            // Occasionally introduce a difference.
            if state & 1 != 0 {
                let idx = (state as usize) % len;
                scratch_b[idx] = scratch_b[idx].wrapping_add(1).max(1);
            }
            let n_choice = match state % 5 {
                0 => 0u32,
                1 => 1,
                2 => len as u32,
                3 => (len as u32).saturating_add(8),
                _ => u32::MAX,
            };
            let a = &scratch_a[..=len];
            let b = &scratch_b[..=len];
            assert_eq!(
                rust_vs(a, b, n_choice),
                strncmp_fasm_oracle(a, b, n_choice),
                "len={len} n={n_choice} a={a:?} b={b:?}"
            );
        }
    }

    // --- Cut BH: strlen ---

    /// Independent FASM-flow oracle for `strlen` (`parse_fn.inc`).
    fn strlen_fasm_oracle(s: &[u8]) -> u32 {
        assert!(!s.is_empty() && s[s.len() - 1] == 0);
        let mut ecx = 0u32.wrapping_sub(1);
        let mut i = 0usize;
        loop {
            let b = s[i];
            i += 1;
            ecx = ecx.wrapping_sub(1);
            if b == 0 {
                break;
            }
        }
        ecx = ecx.wrapping_add(1);
        !ecx
    }

    fn check_strlen(s: &[u8]) {
        assert!(!s.is_empty() && s[s.len() - 1] == 0);
        let got = unsafe { strlen(s.as_ptr()) };
        let expect = strlen_fasm_oracle(s);
        assert_eq!(got, expect, "s={s:?} got={got} expect={expect}");
        // Sanity: length excludes the terminator.
        assert_eq!(got as usize, s.len() - 1);
    }

    #[test]
    fn strlen_empty_and_short() {
        check_strlen(b"\0");
        check_strlen(b"a\0");
        check_strlen(b"ab\0");
        check_strlen(b"abc\0");
        check_strlen(b"hello\0");
    }

    #[test]
    fn strlen_binary_and_high_bytes() {
        check_strlen(&[0xFF, 0]);
        check_strlen(&[1, 2, 3, 0]);
        check_strlen(&[0x80, 0x7F, 0x00]);
        check_strlen(b"/sys/lib/libname.obj\0");
    }

    #[test]
    fn strlen_prng_50k_matches_oracle() {
        let mut state = STRLEN_PRNG_SEED;
        let mut buf = [0u8; 128];
        for _ in 0..50_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let len = (state % 96) as usize;
            for i in 0..len {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                buf[i] = (state as u8).wrapping_add(1).max(1); // no early NUL
            }
            buf[len] = 0;
            check_strlen(&buf[..=len]);
        }
    }
}
