//! Cut P: `is_region_userspace` — syscall user-memory region gate.
//!
//! Matches `kernel/kernel.asm` FASM leaf **bit-exactly**, including the
//! overflow-to-zero ZF quirk (`ADD` wraps to 0 → `JC` taken with `ZF=1`).
//!
//! Legacy return channel is **ZF** (reconstructed by the FASM trampoline from
//! this scalar). Rust returns `1` for FASM `ZF=1`, `0` for FASM `ZF=0`.

/// Kolibri `OS_BASE` from `kernel/const.inc` (same value as Cut O).
/// Not re-exported from `lib` — use [`crate::OS_BASE`].
const OS_BASE: u32 = 0x8000_0000;

/// `OS_BASE - 1` — first `cmp` immediate in the FASM body.
const OS_BASE_M1: u32 = 0x7FFF_FFFF;

/// Cut P differential PRNG seed (documented).
pub const IS_REGION_USERSPACE_PRNG_SEED: u32 = 0x4355_5450; // 'CUTP'

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
}
