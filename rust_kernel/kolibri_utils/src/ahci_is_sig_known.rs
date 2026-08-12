//! Cut BM: `ahci_is_sig_known` — AHCI port PxSIG known-device recognition.
//!
//! Matches `kernel/blkdev/ahci.inc` FASM leaf semantics:
//! * Compare `EAX` against four SATA signature constants
//! * **ZF=1** when known (callers use `jz`); **ZF=0** when unknown
//! * Legacy destroys **flags only** (all GPRs preserved)
//!
//! Rust returns `1` = known, `0` = unknown; the FASM trampoline maps that to
//! legacy ZF via `cmp eax, 1` before `ret`.

/// SATA PxSIG values (`kernel/blkdev/ahci.inc`).
pub const SATA_SIG_ATA: u32 = 0x0000_0101;
pub const SATA_SIG_ATAPI: u32 = 0xEB14_0101;
pub const SATA_SIG_SEMB: u32 = 0xC33C_0101;
pub const SATA_SIG_PM: u32 = 0x9669_0101;

/// Cut BM differential PRNG seed (`'CUTM'`).
pub const AHCI_IS_SIG_KNOWN_PRNG_SEED: u32 = 0x4355_544D;

/// FASM-faithful known-signature test.
///
/// Returns `1` when `sig` matches a known SATA device signature, else `0`.
#[inline(always)]
pub fn ahci_is_sig_known(sig: u32) -> u32 {
    if sig == SATA_SIG_ATA
        || sig == SATA_SIG_ATAPI
        || sig == SATA_SIG_SEMB
        || sig == SATA_SIG_PM
    {
        1
    } else {
        0
    }
}

/// Pointer-free wrapper kept for FFI naming symmetry with other cuts.
#[inline(always)]
pub fn ahci_is_sig_known_from_reg(sig: u32) -> u32 {
    ahci_is_sig_known(sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (four `cmp`/`je` chain ending at `.known:`).
    fn oracle(sig: u32) -> u32 {
        if sig == SATA_SIG_ATA
            || sig == SATA_SIG_ATAPI
            || sig == SATA_SIG_SEMB
            || sig == SATA_SIG_PM
        {
            1
        } else {
            0
        }
    }

    fn check(sig: u32) {
        let got = ahci_is_sig_known(sig);
        let exp = oracle(sig);
        assert_eq!(got, exp, "sig={sig:#x} got={got} exp={exp}");
    }

    #[test]
    fn all_four_known_signatures() {
        check(SATA_SIG_ATA);
        check(SATA_SIG_ATAPI);
        check(SATA_SIG_SEMB);
        check(SATA_SIG_PM);
    }

    #[test]
    fn near_miss_and_zero_unknown() {
        check(0);
        check(0xFFFF_FFFF);
        check(SATA_SIG_ATA ^ 1);
        check(SATA_SIG_ATAPI ^ 0x100);
        check(SATA_SIG_SEMB + 1);
        check(SATA_SIG_PM.wrapping_sub(1));
    }

    #[test]
    fn prng_corpus_50k() {
        let mut state = AHCI_IS_SIG_KNOWN_PRNG_SEED;
        for _ in 0..50_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            check(state);
        }
    }
}
