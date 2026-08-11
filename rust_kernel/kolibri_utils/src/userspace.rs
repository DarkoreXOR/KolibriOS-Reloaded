//! Cut P: `is_region_userspace` — syscall user-memory region gate.
//! Cut BJ: `is_string_userspace` — NUL-terminated string userspace gate.
//!
//! Matches `kernel/kernel.asm` FASM leaves **bit-exactly**, including Cut P's
//! overflow-to-zero ZF quirk (`ADD` wraps to 0 → `JC` taken with `ZF=1`).
//!
//! Legacy return channel is **ZF** (reconstructed by the FASM trampoline from
//! this scalar). Rust returns `1` for FASM `ZF=1`, `0` for FASM `ZF=0`.

/// Kolibri `OS_BASE` from `kernel/const.inc` (same value as Cut O).
/// Not re-exported from `lib` — use [`crate::OS_BASE`].
const OS_BASE: u32 = 0x8000_0000;

/// `OS_BASE - 1` — first `cmp` immediate in the FASM body.
const OS_BASE_M1: u32 = 0x7FFF_FFFF;

/// FASM `is_string_userspace` max scan length (`cmp ecx, 0x10000`).
const STRING_USERSPACE_MAX: u32 = 0x1_0000;

/// Cut P differential PRNG seed (documented).
pub const IS_REGION_USERSPACE_PRNG_SEED: u32 = 0x4355_5450; // 'CUTP'

/// Cut BJ differential PRNG seed (`'CUBJ'`).
pub const IS_STRING_USERSPACE_PRNG_SEED: u32 = 0x4355_424A;

/// FASM-faithful userspace region check → legacy ZF sense as `0`/`1`.
///
/// Instruction-sequence model (see cut-p audit):
/// ```text
/// cmp base, OS_BASE-1 ; ja → ZF=0
/// add base, len       ; jc → ZF = (sum_mod == 0)
/// cmp sum, OS_BASE    ; ja → ZF=0
/// cmp eax, eax        ; ZF=1
/// ```
#[inline(always)]
pub fn is_region_userspace(base: u32, len: u32) -> u32 {
    // cmp eax, OS_BASE-1 / ja @fail → ZF=0
    if base > OS_BASE_M1 {
        return 0;
    }

    // add eax, len / jc @fail — ZF left from ADD when carry
    let (sum, carry) = base.overflowing_add(len);
    if carry {
        // Overflow-to-zero quirk: sum_mod==0 ⇒ ZF=1 (legacy false-accept)
        return u32::from(sum == 0);
    }

    // cmp eax, OS_BASE / ja @fail → ZF=0
    if sum > OS_BASE {
        return 0;
    }

    // cmp eax, eax → ZF=1
    1
}

/// Trampoline ZF reconstruction model: `cmp rust_ret, 1` then flag-neutral pops.
///
/// Returns `true` iff the trampoline leaves `ZF=1`.
#[inline(always)]
pub fn trampoline_zf_from_rust_return(rust_ret: u32) -> bool {
    rust_ret == 1
}

/// FASM-faithful NUL-terminated string userspace gate → legacy ZF sense as `0`/`1`.
///
/// Instruction-sequence model (`kernel/kernel.asm`):
/// ```text
/// mov edi, base
/// mov ecx, OS_BASE-1
/// sub ecx, edi      ; jb .done  → ZF from SUB (0 when base > OS_BASE-1)
/// inc ecx           ; ecx = OS_BASE - base
/// cmp ecx, 0x10000  ; jbe skip; mov ecx, 0x10000
/// xor eax, eax
/// repnz scasb       ; ZF=1 iff NUL found within ecx bytes
/// ```
///
/// # Safety
/// When `base as u32 <= OS_BASE-1`, `base` must be readable for
/// `min(OS_BASE - base_va, 0x10000)` bytes (or until the first NUL inclusive).
#[inline(always)]
pub unsafe fn is_string_userspace(base: *const u8) -> u32 {
    let base_va = base as u32;
    // sub ecx, edi / jb .done — ZF from SUB is 0 whenever CF=1 here
    if base_va > OS_BASE_M1 {
        return 0;
    }
    // inc ecx → OS_BASE - base
    let mut ecx = OS_BASE.wrapping_sub(base_va);
    if ecx > STRING_USERSPACE_MAX {
        ecx = STRING_USERSPACE_MAX;
    }
    let mut edi = base;
    // DF-agnostic forward scan (legacy assumes DF=0 for scasb)
    let mut i = 0u32;
    while i < ecx {
        // SAFETY: within the documented readable window / until NUL.
        if unsafe { *edi } == 0 {
            return 1;
        }
        edi = unsafe { edi.add(1) };
        i = i.wrapping_add(1);
    }
    0
}

/// Host/oracle helper: scan `bytes` as if they resided at linear `base_va`.
///
/// Reads at most `bytes.len()` bytes; if the FASM window exceeds the slice and
/// no NUL was seen, returns `0` (same as exhausting the window without NUL).
#[inline(always)]
pub fn is_string_userspace_at(base_va: u32, bytes: &[u8]) -> u32 {
    if base_va > OS_BASE_M1 {
        return 0;
    }
    let mut ecx = OS_BASE.wrapping_sub(base_va);
    if ecx > STRING_USERSPACE_MAX {
        ecx = STRING_USERSPACE_MAX;
    }
    let limit = core::cmp::min(ecx as usize, bytes.len());
    let mut i = 0usize;
    while i < limit {
        if bytes[i] == 0 {
            return 1;
        }
        i += 1;
    }
    // Exhausted FASM window (or provided bytes) without NUL → ZF=0
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent oracle mirroring the FASM control-flow / flag outcomes.
    fn fasm_oracle_zf(base: u32, len: u32) -> u32 {
        if base > OS_BASE_M1 {
            return 0; // ja: ZF=0
        }
        let (sum, carry) = base.overflowing_add(len);
        if carry {
            return u32::from(sum == 0); // jc: ZF from ADD
        }
        if sum > OS_BASE {
            return 0; // ja: ZF=0
        }
        1 // cmp eax,eax
    }

    fn check(base: u32, len: u32, expect_zf: u32) {
        let got = is_region_userspace(base, len);
        let oracle = fasm_oracle_zf(base, len);
        assert_eq!(oracle, expect_zf, "oracle mismatch base={base:#x} len={len:#x}");
        assert_eq!(got, expect_zf, "impl mismatch base={base:#x} len={len:#x}");
        assert_eq!(
            trampoline_zf_from_rust_return(got),
            expect_zf == 1,
            "trampoline ZF model base={base:#x} len={len:#x}"
        );
    }

    #[test]
    fn required_vectors() {
        check(0, 0, 1);
        check(0, 1, 1);
        check(0x7FFF_FFFF, 0, 1);
        check(0x7FFF_FFFF, 1, 1); // end == OS_BASE
        check(0x7FFF_FFFF, 2, 0); // end == OS_BASE+1
        check(0x8000_0000, 0, 0);
        check(0x8000_0000, 1, 0);

        // Overflow to zero → ZF=1 (quirk)
        check(1, 0xFFFF_FFFF, 1);
        check(0x4000_0000, 0xC000_0000, 1);

        // Overflow non-zero → ZF=0
        check(2, 0xFFFF_FFFF, 0);
        check(0x4000_0000, 0xC000_0001, 0);
    }

    #[test]
    fn end_equals_os_base_accepted() {
        check(0, OS_BASE, 1);
        check(0x7FFF_FFFE, 2, 1);
    }

    #[test]
    fn boundary_grid() {
        let bases = [
            0u32,
            1,
            2,
            0x7FFF_FFFE,
            0x7FFF_FFFF,
            0x8000_0000,
            0x8000_0001,
            0xFFFF_FFFE,
            0xFFFF_FFFF,
        ];
        let lens = [
            0u32,
            1,
            2,
            OS_BASE - 1,
            OS_BASE,
            OS_BASE + 1,
            0xC000_0000,
            0xFFFF_FFFE,
            0xFFFF_FFFF,
        ];
        for &b in &bases {
            for &l in &lens {
                let expect = fasm_oracle_zf(b, l);
                check(b, l, expect);
            }
        }
    }

    #[test]
    fn overflow_to_zero_disagree_with_naive_reject() {
        // Documented "overflow ⇒ reject" would want 0; FASM ZF is 1.
        assert_eq!(is_region_userspace(1, 0xFFFF_FFFF), 1);
        assert_eq!(is_region_userspace(0x4000_0000, 0xC000_0000), 1);
    }

    #[test]
    fn prng_corpus_matches_oracle() {
        let mut state = IS_REGION_USERSPACE_PRNG_SEED;
        let mut n_ov0 = 0u32;
        for _ in 0..200_000u32 {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let base = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let len = state;
            let got = is_region_userspace(base, len);
            let exp = fasm_oracle_zf(base, len);
            assert_eq!(got, exp, "prng base={base:#x} len={len:#x}");
            if base <= OS_BASE_M1 {
                let (sum, carry) = base.overflowing_add(len);
                if carry && sum == 0 {
                    n_ov0 += 1;
                    assert_eq!(got, 1);
                }
            }
        }
        // With 200k trials, overflow-to-zero is rare (~1/2^32 per pair when
        // conditioned); force-inject a few known vectors into the tally path.
        let _ = n_ov0;
        for &(b, l) in &[
            (1u32, 0xFFFF_FFFF),
            (0x4000_0000, 0xC000_0000),
            (0x1234_5678, 0xEDCB_A988),
        ] {
            assert_eq!(is_region_userspace(b, l), 1);
        }
    }

    #[test]
    fn trampoline_zf_model_both_polarities() {
        assert!(trampoline_zf_from_rust_return(1));
        assert!(!trampoline_zf_from_rust_return(0));
        // Non-canonical returns: cmp eax,1 only sets ZF when exactly 1
        assert!(!trampoline_zf_from_rust_return(2));
        assert!(!trampoline_zf_from_rust_return(0xFFFF_FFFF));
    }

    /// Independent FASM-flow oracle for `is_string_userspace` (not a call to SUT).
    fn fasm_string_oracle(base_va: u32, bytes: &[u8]) -> u32 {
        if base_va > OS_BASE_M1 {
            return 0;
        }
        let mut ecx = OS_BASE.wrapping_sub(base_va);
        if ecx > STRING_USERSPACE_MAX {
            ecx = STRING_USERSPACE_MAX;
        }
        let limit = core::cmp::min(ecx as usize, bytes.len());
        for &b in &bytes[..limit] {
            if b == 0 {
                return 1;
            }
        }
        0
    }

    fn check_string(base_va: u32, bytes: &[u8], expect: u32) {
        let oracle = fasm_string_oracle(base_va, bytes);
        assert_eq!(oracle, expect, "oracle base={base_va:#x}");
        let got = is_string_userspace_at(base_va, bytes);
        assert_eq!(got, expect, "impl base={base_va:#x}");
        assert_eq!(
            trampoline_zf_from_rust_return(got),
            expect == 1,
            "trampoline ZF base={base_va:#x}"
        );
        // Pointer path when bytes live at a real allocation and base_va matches
        // the pointer's low address used in host tests (base_va == ptr as u32
        // only when we place the buffer intentionally — covered below).
    }

    #[test]
    fn string_reject_bases_no_deref() {
        check_string(OS_BASE, &[], 0);
        check_string(OS_BASE + 1, &[0], 0);
        check_string(0xFFFF_FFFF, &[0], 0);
        check_string(0x8000_0001, b"hi\0", 0);
    }

    #[test]
    fn string_empty_and_short_accept() {
        check_string(0, b"\0", 1);
        check_string(0, b"a\0", 1);
        check_string(0x1000, b"hello\0", 1);
        check_string(0x7FFF_FFFE, b"x\0", 1);
        check_string(0x7FFF_FFFF, b"\0", 1); // last userspace byte is NUL
    }

    #[test]
    fn string_no_nul_within_window_reject() {
        check_string(0, b"abc", 0); // slice ends before NUL; window has no NUL
        let no_nul = [0xFFu8; 32];
        check_string(0, &no_nul, 0);
    }

    #[test]
    fn string_64k_cap() {
        // Window capped at 0x10000 even when OS_BASE - base is larger.
        let mut buf = vec![0x41u8; 0x10000];
        // No NUL in first 64K → reject
        check_string(0, &buf, 0);
        // NUL exactly at last scanned byte (index 0xFFFF)
        buf[0xFFFF] = 0;
        check_string(0, &buf, 1);
        // NUL just past the 64K window must not be seen
        let mut buf2 = vec![0x41u8; 0x10001];
        buf2[0x10000] = 0;
        check_string(0, &buf2, 0);
    }

    #[test]
    fn string_near_os_base_window() {
        // base = OS_BASE-4 → ecx = 4 after inc
        check_string(OS_BASE - 4, b"abc\0", 1);
        check_string(OS_BASE - 4, b"abcd", 0); // 4 non-NUL → reject
        check_string(OS_BASE - 1, b"\0", 1);
        check_string(OS_BASE - 1, b"Z", 0);
    }

    #[test]
    fn string_ptr_path_matches_at() {
        let s = b"libname.obj\0";
        let got_at = is_string_userspace_at(0x2000, s);
        assert_eq!(got_at, 1);
        // On 64-bit hosts, heap pointers often truncate above OS_BASE when cast
        // to u32 — only exercise the unsafe path when the truncated VA is a
        // legitimate userspace address.
        let va = s.as_ptr() as usize as u32;
        if va <= OS_BASE_M1 {
            let got_ptr = unsafe { is_string_userspace(s.as_ptr()) };
            assert_eq!(got_ptr, 1);
            assert!(trampoline_zf_from_rust_return(got_ptr));
        }
    }

    #[test]
    fn string_prng_50k_matches_oracle() {
        let mut state = IS_STRING_USERSPACE_PRNG_SEED;
        for _ in 0..50_000u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let base = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let kind = state % 5;
            let bytes: Vec<u8> = match kind {
                0 => vec![0],
                1 => {
                    let mut v = vec![0u8; (state % 64) as usize + 1];
                    let last = v.len() - 1;
                    for b in &mut v[..last] {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        *b = (state as u8) | 1; // non-NUL
                    }
                    v[last] = 0;
                    v
                }
                2 => {
                    let n = (state % 128) as usize + 1;
                    let mut v = vec![0u8; n];
                    for b in &mut v {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        *b = (state as u8) | 1;
                    }
                    v
                }
                3 => vec![],
                _ => {
                    // Force high base often
                    let mut v = vec![0u8; 8];
                    v[7] = 0;
                    v
                }
            };
            let base_va = if kind == 4 {
                OS_BASE.wrapping_add(state % 16)
            } else if kind == 3 {
                OS_BASE_M1.wrapping_sub(state % 8)
            } else {
                base % 0x1000_0000
            };
            let got = is_string_userspace_at(base_va, &bytes);
            let exp = fasm_string_oracle(base_va, &bytes);
            assert_eq!(got, exp, "prng base={base_va:#x} kind={kind}");
        }
    }
}
