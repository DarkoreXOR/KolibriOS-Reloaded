//! Cut BL: `v86_get_lin_addr` — V86 address → linear via `page_tabs` PTE.
//!
//! Matches `kernel/core/v86.inc` FASM leaf semantics:
//! ```text
//!   push ecx edx
//!   mov  ecx, eax
//!   shr  ecx, 12
//!   mov  edx, [page_tabs + ecx*4]
//!   and  eax, 0xFFF
//!   and  edx, 0xFFFFF000
//!   or   eax, edx
//!   pop  edx ecx
//!   ret
//! ```
//!
//! Notes:
//! * Documented `esi=handle` is **unused** by the leaf (legacy comment quirk).
//! * `page_tabs` is trampoline-injected so the Rust blob stays reloc-free.
//! * Distinct from Cut AQ `get_pg_addr` (kernel VA−OS_BASE → phys page).

/// Cut BL differential PRNG seed (`'CUBL'`).
pub const V86_GET_LIN_ADDR_PRNG_SEED: u32 = 0x4355_424C;

/// Page offset mask (`and eax, 0xFFF`).
pub const PAGE_OFFSET_MASK: u32 = 0x0FFF;

/// PTE frame mask (`and edx, 0xFFFFF000`).
pub const PTE_FRAME_MASK: u32 = 0xFFFF_F000;

/// FASM-faithful V86 address → linear translation.
///
/// # Safety
/// `page_tabs` must be readable at index `v86_addr >> 12`.
#[inline(always)]
pub unsafe fn v86_get_lin_addr(v86_addr: u32, page_tabs: *const u32) -> u32 {
    // mov ecx, eax / shr ecx, 12
    let page = (v86_addr >> 12) as usize;
    // mov edx, [page_tabs + ecx*4]
    // SAFETY: caller guarantees page_tabs covers this PTE index.
    let pte = unsafe { *page_tabs.add(page) };
    // and eax, 0xFFF / and edx, 0xFFFFF000 / or eax, edx
    (v86_addr & PAGE_OFFSET_MASK) | (pte & PTE_FRAME_MASK)
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`v86_get_lin_addr`].
#[inline(always)]
pub unsafe fn v86_get_lin_addr_ptr(v86_addr: u32, page_tabs: *const u32) -> u32 {
    unsafe { v86_get_lin_addr(v86_addr, page_tabs) }
}

/// Independent FASM-flow oracle (register steps; not a call to Rust).
///
/// `ptes[idx]` supplies the dword that FASM would load from
/// `page_tabs + idx*4`. Missing entries read as `0`.
#[cfg(test)]
pub fn fasm_oracle_v86_get_lin_addr(v86_addr: u32, ptes: &[u32]) -> u32 {
    let page = (v86_addr >> 12) as usize;
    let pte = if page < ptes.len() { ptes[page] } else { 0 };
    (v86_addr & PAGE_OFFSET_MASK) | (pte & PTE_FRAME_MASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(v86_addr: u32, ptes: &[u32]) {
        let exp = fasm_oracle_v86_get_lin_addr(v86_addr, ptes);
        let got = unsafe { v86_get_lin_addr(v86_addr, ptes.as_ptr()) };
        assert_eq!(
            got, exp,
            "mismatch addr={v86_addr:#010x} got={got:#x} exp={exp:#x}"
        );
    }

    #[test]
    fn page_zero_offset_zero() {
        let ptes = [0x0012_3003u32];
        check(0x0000_0000, &ptes);
        assert_eq!(
            unsafe { v86_get_lin_addr(0, ptes.as_ptr()) },
            0x0012_3000
        );
    }

    #[test]
    fn page_zero_offset_max() {
        let ptes = [0x00AB_C067u32];
        check(0x0000_0FFF, &ptes);
        assert_eq!(
            unsafe { v86_get_lin_addr(0x0FFF, ptes.as_ptr()) },
            0x00AB_CFFF
        );
    }

    #[test]
    fn mid_page_strips_pte_flags() {
        // page 5, offset 0x123; PTE has dirty/accessed/present bits
        let mut ptes = [0u32; 8];
        ptes[5] = 0x00DE_AD0F;
        check(0x0000_5123, &ptes);
        assert_eq!(
            unsafe { v86_get_lin_addr(0x5123, ptes.as_ptr()) },
            0x00DE_A123
        );
    }

    #[test]
    fn unmapped_pte_zero() {
        let ptes = [0u32; 4];
        check(0x0000_2000, &ptes);
        assert_eq!(unsafe { v86_get_lin_addr(0x2000, ptes.as_ptr()) }, 0);
        check(0x0000_2ABC, &ptes);
        assert_eq!(
            unsafe { v86_get_lin_addr(0x2ABC, ptes.as_ptr()) },
            0x0ABC
        );
    }

    #[test]
    fn last_low_meg_bios_rom_style() {
        // V86 often maps 0xF0000 BIOS ROM page
        let mut ptes = [0u32; 0x100];
        ptes[0xF0] = 0x000F_0001;
        check(0x000F_0123, &ptes);
        assert_eq!(
            unsafe { v86_get_lin_addr(0xF0123, ptes.as_ptr()) },
            0x000F_0123
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
        // Compact synthetic table: 256 pages (1 MiB V86 window class).
        let mut ptes = [0u32; 256];
        let mut state = V86_GET_LIN_ADDR_PRNG_SEED;
        for slot in ptes.iter_mut() {
            *slot = xorshift32(&mut state);
        }
        for _ in 0..50_000 {
            let addr = xorshift32(&mut state) & 0x000F_FFFF; // stay in table
            check(addr, &ptes);
        }
    }
}
