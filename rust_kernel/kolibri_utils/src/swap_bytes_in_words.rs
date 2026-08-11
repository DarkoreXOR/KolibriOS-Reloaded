//! Cut BG: `swap_bytes_in_words` — in-place endian swap of 16-bit words.
//!
//! Matches `kernel/blkdev/ahci.inc` FASM leaf semantics:
//! ```text
//!   xor ecx, ecx
//!   mov ebx, [base]
//! .loop:
//!   cmp ecx, [len]
//!   jae .loop_end
//!   mov ax, word [ebx + ecx*2]
//!   xchg ah, al
//!   mov word [ebx + ecx*2], ax
//!   inc ecx
//!   jmp .loop
//! ```
//!
//! `len` is a **word count** (not byte count). Pure buffer rewrite — no
//! globals, locks, `.rodata`, or external calls.

/// Cut BG differential PRNG seed (`'CUBG'`).
pub const SWAP_BYTES_IN_WORDS_PRNG_SEED: u32 = 0x4355_4247;

/// FASM-faithful in-place byte-swap of `len` little-endian `u16` words at `base`.
///
/// # Safety
/// `base` must be writable for `len * 2` bytes when `len > 0`.
#[inline(always)]
pub unsafe fn swap_bytes_in_words(base: *mut u16, len: u32) {
    let mut ecx = 0u32;
    while ecx < len {
        // mov ax, word [ebx + ecx*2] / xchg ah, al / store
        let p = base.add(ecx as usize);
        let w = core::ptr::read_volatile(p);
        let swapped = w.swap_bytes();
        core::ptr::write_volatile(p, swapped);
        ecx = ecx.wrapping_add(1);
    }
}

/// Pointer-free helper for differential tests (slice form).
#[inline(always)]
pub fn swap_bytes_in_words_slice(words: &mut [u16]) {
    // Safety: slice covers exactly `words.len()` writable u16s.
    unsafe {
        swap_bytes_in_words(words.as_mut_ptr(), words.len() as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not derived from the Rust helper body).
    fn oracle(words: &mut [u16]) {
        let mut ecx = 0u32;
        let len = words.len() as u32;
        while ecx < len {
            let w = words[ecx as usize];
            // xchg ah, al on little-endian word
            let lo = (w & 0xff) as u8;
            let hi = ((w >> 8) & 0xff) as u8;
            words[ecx as usize] = (u16::from(lo) << 8) | u16::from(hi);
            ecx = ecx.wrapping_add(1);
        }
    }

    fn check(mut input: Vec<u16>) {
        let mut expected = input.clone();
        oracle(&mut expected);
        swap_bytes_in_words_slice(&mut input);
        assert_eq!(input, expected, "mismatch after swap");
    }

    #[test]
    fn len_zero_noop() {
        let v: Vec<u16> = vec![];
        check(v.clone());
        let mut one = vec![0x1234];
        // len=0 via raw call must leave buffer alone
        unsafe {
            swap_bytes_in_words(one.as_mut_ptr(), 0);
        }
        assert_eq!(one, vec![0x1234]);
    }

    #[test]
    fn single_word() {
        check(vec![0x1234]);
        check(vec![0x00ff]);
        check(vec![0xff00]);
        check(vec![0x0000]);
        check(vec![0xffff]);
    }

    #[test]
    fn ata_model_word_count() {
        // IDENTIFY words 27..46 inclusive → 20 words
        let mut v = Vec::with_capacity(20);
        for i in 0u16..20 {
            v.push(0x4100 + i); // 'A' + index in high/low mix
        }
        check(v);
    }

    #[test]
    fn double_swap_is_identity() {
        let mut v: Vec<u16> = (0..64).map(|i| (i * 0x0101) ^ 0x5a5a).collect();
        let orig = v.clone();
        swap_bytes_in_words_slice(&mut v);
        swap_bytes_in_words_slice(&mut v);
        assert_eq!(v, orig);
    }

    #[test]
    fn prng_50k() {
        let mut state = SWAP_BYTES_IN_WORDS_PRNG_SEED;
        fn next(state: &mut u32) -> u32 {
            // xorshift32
            let mut x = *state;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *state = x;
            x
        }
        for _ in 0..50_000 {
            let len = (next(&mut state) % 65) as usize; // 0..64
            let mut v = Vec::with_capacity(len);
            for _ in 0..len {
                v.push(next(&mut state) as u16);
            }
            check(v);
        }
    }
}
