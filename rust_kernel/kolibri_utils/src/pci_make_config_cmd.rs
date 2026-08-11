//! Cut BA: `pci_make_config_cmd` — PCI config-space address dword encode.
//!
//! Matches `kernel/bus/pci/pci32.inc` FASM leaf semantics:
//! ```text
//!   shl eax, 8          ; AH (bus) → bits 16..23
//!   mov ax, bx          ; BH=dev+func, BL=register → bits 15..0
//!   and eax, 0xffffff
//!   or  eax, 0x80000000 ; enable bit
//! ```
//! Result layout: `10000000 bbbbbbbb dddddfff rrrrrr00` (low two reg bits
//! retained from BL; callers dword-align later with `and al, 0xfc`).
//!
//! Pure bit math — no globals, MMIO, locks, or `.rodata`.

/// Cut BA differential PRNG seed (`'CUBA'`).
pub const PCI_MAKE_CONFIG_CMD_PRNG_SEED: u32 = 0x4355_4241;

/// FASM-faithful PCI mechanism-1 config address encode.
///
/// `bus`, `devfn`, and `reg` are truncated to 8 bits (matching AH / BH / BL).
#[inline(always)]
pub fn pci_make_config_cmd(bus: u32, devfn: u32, reg: u32) -> u32 {
    let bus = bus & 0xff;
    let devfn = devfn & 0xff;
    let reg = reg & 0xff;
    (bus << 16) | (devfn << 8) | reg | 0x8000_0000
}

/// Register-form helper: mirrors FASM inputs `EAX` (AH=bus) + `EBX` (BH/BL).
#[inline(always)]
pub fn pci_make_config_cmd_from_regs(eax_in: u32, ebx: u32) -> u32 {
    let bus = (eax_in >> 8) & 0xff;
    let devfn = (ebx >> 8) & 0xff;
    let reg = ebx & 0xff;
    pci_make_config_cmd(bus, devfn, reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (not derived from the Rust helper body).
    fn oracle(eax_in: u32, ebx: u32) -> u32 {
        // shl eax, 8
        let mut eax = eax_in << 8;
        // mov ax, bx
        eax = (eax & 0xffff_0000) | (ebx & 0xffff);
        // and eax, 0xffffff / or eax, 0x80000000
        (eax & 0x00ff_ffff) | 0x8000_0000
    }

    fn check_regs(eax_in: u32, ebx: u32) {
        let got = pci_make_config_cmd_from_regs(eax_in, ebx);
        let exp = oracle(eax_in, ebx);
        assert_eq!(
            got, exp,
            "mismatch eax_in={eax_in:#x} ebx={ebx:#x} got={got:#x} exp={exp:#x}"
        );
        let bus = (eax_in >> 8) & 0xff;
        let devfn = (ebx >> 8) & 0xff;
        let reg = ebx & 0xff;
        assert_eq!(pci_make_config_cmd(bus, devfn, reg), exp);
    }

    #[test]
    fn zero_bus_dev_reg() {
        // AH=0, BH=0, BL=0 → 0x80000000
        check_regs(0x0000_0000, 0x0000_0000);
        check_regs(0x0000_00aa, 0x0000_0000); // AL junk ignored after mov ax,bx path via AH
    }

    #[test]
    fn classic_bus_dev_reg() {
        // bus=1, devfn=0x18, reg=0x04 → 0x80011804
        check_regs(0x0000_0100, 0x0000_1804);
        assert_eq!(pci_make_config_cmd(1, 0x18, 0x04), 0x8001_1804);
    }

    #[test]
    fn max_bus_devfn_reg() {
        check_regs(0x0000_ff00, 0x0000_ffff);
        assert_eq!(pci_make_config_cmd(0xff, 0xff, 0xff), 0x80ff_ffff);
    }

    #[test]
    fn high_eax_bits_cleared() {
        // bits 16..31 of input become bits 24..39 after shl; and 0xffffff clears them
        check_regs(0xabcd_1200, 0x0000_3408);
        assert_eq!(pci_make_config_cmd(0x12, 0x34, 0x08), 0x8012_3408);
    }

    #[test]
    fn register_low_bits_preserved() {
        // BL=0x03 keeps low two bits (callers align later)
        check_regs(0x0000_0500, 0x0000_0a03);
        assert_eq!(pci_make_config_cmd(5, 0x0a, 3), 0x8005_0a03);
    }

    #[test]
    fn truncates_wide_args() {
        assert_eq!(
            pci_make_config_cmd(0x1_00, 0x2_11, 0x3_22),
            pci_make_config_cmd(0x00, 0x11, 0x22)
        );
    }

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn prng_50k_matches_oracle() {
        let mut state = PCI_MAKE_CONFIG_CMD_PRNG_SEED;
        for _ in 0..50_000 {
            let eax_in = xorshift32(&mut state);
            let ebx = xorshift32(&mut state);
            check_regs(eax_in, ebx);
        }
    }
}
