//! Cut AV: `ahci_find_cmdslot` — AHCI free command-list slot lookup.
//!
//! Matches `kernel/blkdev/ahci.inc` FASM leaf semantics:
//! * `slots = SACT | CI` (bitset of busy command slots)
//! * `ncs = (HBA_MEM.cap >> 8) & 0x0F` — **4-bit mask quirk** (drops AHCI NCS bit 4)
//! * Linear scan `i = 0 .. ncs-1` (exclusive upper bound via FASM `cmp/jae`)
//! * First clear bit → return index; else `0xFFFF_FFFF` (`-1`)
//!
//! Note: AHCI CAP.NCS is a 0-based slot count field; FASM uses the masked field
//! directly as the exclusive loop bound (does **not** add 1). Preserve exactly.
//!
//! MMIO loads stay in the FASM trampoline so the Rust blob stays reloc-free
//! (pure arithmetic). No locks / `.rodata` / external calls.

/// Cut AV differential PRNG seed (`'CUTV'`).
pub const AHCI_FIND_CMDSLOT_PRNG_SEED: u32 = 0x4355_5456;

/// FASM-faithful free command-slot scan.
///
/// `slots` is `SACT | CI`. `ncs` is already `(CAP >> 8) & 0x0F`.
/// Returns the first free slot index, or `u32::MAX` (`-1`).
#[inline(always)]
pub fn ahci_find_cmdslot(slots: u32, ncs: u32) -> u32 {
    // xor ecx, ecx / .for1: cmp ecx, edx / jae .for1_end
    let mut i = 0u32;
    let mut bits = slots;
    while i < ncs {
        // bt ebx, 0 / jc .cont1 → free when bit0 clear
        if (bits & 1) == 0 {
            return i;
        }
        // shr ebx, 1 / inc ecx
        bits >>= 1;
        i = i.wrapping_add(1);
    }
    u32::MAX
}

/// Pointer-free wrapper kept for FFI naming symmetry with other cuts.
#[inline(always)]
pub fn ahci_find_cmdslot_from_regs(slots: u32, ncs: u32) -> u32 {
    ahci_find_cmdslot(slots, ncs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not derived from the Rust helper body).
    fn oracle(mut slots: u32, ncs: u32) -> u32 {
        let mut ecx = 0u32;
        loop {
            // cmp ecx, edx / jae .for1_end
            if ecx >= ncs {
                return u32::MAX;
            }
            // bt ebx, 0 / jc .cont1
            if (slots & 1) == 0 {
                return ecx;
            }
            slots >>= 1;
            ecx = ecx.wrapping_add(1);
        }
    }

    fn check(slots: u32, ncs: u32) {
        let got = ahci_find_cmdslot(slots, ncs);
        let exp = oracle(slots, ncs);
        assert_eq!(
            got, exp,
            "mismatch slots={slots:#x} ncs={ncs} got={got:#x} exp={exp:#x}"
        );
    }

    #[test]
    fn ncs_zero_always_miss() {
        check(0, 0);
        check(0xffff_ffff, 0);
    }

    #[test]
    fn empty_slots_first_free() {
        check(0, 1);
        check(0, 15);
        assert_eq!(ahci_find_cmdslot(0, 8), 0);
    }

    #[test]
    fn first_middle_last_free() {
        // bit0 busy → free at 1
        check(0b1, 8);
        assert_eq!(ahci_find_cmdslot(0b1, 8), 1);
        // bits 0..2 busy → free at 3
        check(0b111, 8);
        assert_eq!(ahci_find_cmdslot(0b111, 8), 3);
        // only last of ncs=4 free (bits 0..2 set)
        check(0b0111, 4);
        assert_eq!(ahci_find_cmdslot(0b0111, 4), 3);
    }

    #[test]
    fn all_occupied_miss() {
        check(0b1111, 4);
        assert_eq!(ahci_find_cmdslot(0b1111, 4), u32::MAX);
        // bits 0..14 set with ncs=15 → miss (slot 15 never inspected)
        assert_eq!(ahci_find_cmdslot(0x7fff, 15), u32::MAX);
        check(0xffff_ffff, 15);
    }

    #[test]
    fn ncs_mask_quirk_bound() {
        // FASM and edx, 0xf — callers pass already-masked ncs.
        // ncs=15 checks indices 0..14 only (15 iterations max).
        assert_eq!(ahci_find_cmdslot(0x7fff, 15), u32::MAX); // bits 0..14 set
        assert_eq!(ahci_find_cmdslot(0x3fff, 15), 14); // bit14 clear
        // Slot 15 would be free in a 32-slot HBA, but ncs=15 never inspects it.
        assert_eq!(ahci_find_cmdslot(0x7fff, 15), u32::MAX);
    }

    #[test]
    fn named_vectors() {
        let cases = [
            (0u32, 0u32, u32::MAX),
            (0, 1, 0),
            (1, 1, u32::MAX),
            (0b10, 2, 0),
            (0b01, 2, 1),
            (0b11, 2, u32::MAX),
            (0b1111_1110, 8, 0),
            (0b0111_1111, 8, 7),
            (0xff, 8, u32::MAX),
            (0, 15, 0),
            (0x7fff, 15, u32::MAX),
        ];
        for (slots, ncs, exp) in cases {
            assert_eq!(ahci_find_cmdslot(slots, ncs), exp);
            check(slots, ncs);
        }
    }

    #[test]
    fn prng_50k_cutv() {
        let mut state = AHCI_FIND_CMDSLOT_PRNG_SEED;
        for _ in 0..50_000 {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let slots = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let ncs = state & 0xf; // match FASM mask domain
            check(slots, ncs);
        }
    }
}
