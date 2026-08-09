//! String helpers matching FASM `strncmp` in `kernel/core/string.inc`.

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

#[cfg(test)]
mod tests {
    use super::{strncmp, strncmp_fasm_oracle};

    fn rust_vs(s1: &[u8], s2: &[u8], n: u32) -> i32 {
        unsafe { strncmp(s1.as_ptr(), s2.as_ptr(), n) }
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
}
