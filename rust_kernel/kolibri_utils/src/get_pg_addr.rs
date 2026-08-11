//! Cut AQ: `get_pg_addr` — kernel linear address → physical page.
//!
//! Matches `kernel/core/memory.inc` FASM leaf semantics:
//! ```text
//! sub eax, OS_BASE
//! cmp eax, 0x400000 / jb .low
//! shr eax, 12
//! mov eax, [page_tabs + (eax + (OS_BASE shr 12))*4]
//! .low:
//! and eax, -PAGE_SIZE
//! ```
//!
//! First 4 MiB above `OS_BASE` are identity-mapped (phys = linear − OS_BASE).
//! Above that, PTE fetch via `page_tabs`. `page_tabs` and `OS_BASE` are
//! passed explicitly so the Rust blob stays reloc-free (Cut AA pattern).

/// Production `OS_BASE` (`kernel/const.inc`).
pub const OS_BASE: u32 = 0x8000_0000;

/// Production `page_tabs` VA (`kernel/const.inc`).
pub const PAGE_TABS: u32 = 0xFDC0_0000;

/// `PAGE_SIZE` / page mask (`-PAGE_SIZE` in FASM).
pub const PAGE_SIZE: u32 = 4096;

/// Identity-mapped kernel window size above `OS_BASE`.
pub const IDENTITY_WINDOW: u32 = 0x40_0000;

/// Cut AQ differential PRNG seed (`'CUTQ'`).
pub const GET_PG_ADDR_PRNG_SEED: u32 = 0x4355_5451;

/// FASM-faithful linear → physical page translation.
///
/// # Safety
/// When `linear.wrapping_sub(os_base) >= IDENTITY_WINDOW`, `page_tabs` must be
/// readable at index `(offset >> 12).wrapping_add(os_base >> 12)`.
#[inline(always)]
pub unsafe fn get_pg_addr(linear: u32, page_tabs: *const u32, os_base: u32) -> u32 {
    // sub eax, OS_BASE
    let mut eax = linear.wrapping_sub(os_base);
    // cmp eax, 0x400000 / jb @f
    if eax >= IDENTITY_WINDOW {
        // shr eax, 12
        eax >>= 12;
        // mov eax, [page_tabs + (eax+(OS_BASE shr 12))*4]
        let idx = eax.wrapping_add(os_base >> 12);
        // SAFETY: caller guarantees page_tabs covers this PTE index.
        eax = unsafe { *page_tabs.add(idx as usize) };
    }
    // and eax, -PAGE_SIZE
    eax & !(PAGE_SIZE - 1)
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`get_pg_addr`].
#[inline(always)]
pub unsafe fn get_pg_addr_ptr(linear: u32, page_tabs: *const u32, os_base: u32) -> u32 {
    unsafe { get_pg_addr(linear, page_tabs, os_base) }
}

/// Independent FASM-flow oracle (register steps; not a call to Rust).
///
/// `ptes[idx]` supplies the dword that FASM would load from
/// `page_tabs + idx*4`. Missing entries read as `0` (tests must plant PTEs).
#[cfg(test)]
pub fn fasm_oracle_get_pg_addr(linear: u32, ptes: &[u32], os_base: u32) -> u32 {
    let mut eax = linear.wrapping_sub(os_base);
    if eax >= IDENTITY_WINDOW {
        eax >>= 12;
        let idx = eax.wrapping_add(os_base >> 12) as usize;
        eax = if idx < ptes.len() { ptes[idx] } else { 0 };
    }
    eax & !(PAGE_SIZE - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    unsafe fn rust_call(linear: u32, ptes: &[u32], os_base: u32) -> u32 {
        unsafe { get_pg_addr(linear, ptes.as_ptr(), os_base) }
    }

    #[test]
    fn low_path_identity() {
        let empty: [u32; 0] = [];
        // OS_BASE → phys 0
        assert_eq!(
            unsafe { rust_call(OS_BASE, &empty, OS_BASE) },
            0
        );
        assert_eq!(fasm_oracle_get_pg_addr(OS_BASE, &empty, OS_BASE), 0);

        // OS_BASE + 0x1234 → page 0x1000
        assert_eq!(
            unsafe { rust_call(OS_BASE + 0x1234, &empty, OS_BASE) },
            0x1000
        );
        assert_eq!(
            fasm_oracle_get_pg_addr(OS_BASE + 0x1234, &empty, OS_BASE),
            0x1000
        );

        // Last byte of identity window
        let last = OS_BASE + IDENTITY_WINDOW - 1;
        assert_eq!(
            unsafe { rust_call(last, &empty, OS_BASE) },
            IDENTITY_WINDOW - PAGE_SIZE
        );
        assert_eq!(
            fasm_oracle_get_pg_addr(last, &empty, OS_BASE),
            IDENTITY_WINDOW - PAGE_SIZE
        );
    }

    #[test]
    fn high_path_pte_fetch() {
        // Use os_base=0 so indices stay small for a compact fixture.
        // linear = 0x401000 → offset >= 4MB → idx = 0x401
        let mut ptes = vec![0u32; 0x500];
        ptes[0x401] = 0x00AB_CD03; // phys page + present flags
        // 0x00ABCD03 & ~0xFFF = 0x00ABC000
        assert_eq!(
            unsafe { rust_call(0x401_000, &ptes, 0) },
            0x00AB_C000
        );
        assert_eq!(fasm_oracle_get_pg_addr(0x401_000, &ptes, 0), 0x00AB_C000);

        // Boundary: exactly IDENTITY_WINDOW uses high path
        ptes[0x400] = 0x1234_5067;
        assert_eq!(
            unsafe { rust_call(0x400_000, &ptes, 0) },
            0x1234_5000
        );
        assert_eq!(fasm_oracle_get_pg_addr(0x400_000, &ptes, 0), 0x1234_5000);
    }

    #[test]
    fn high_path_production_os_base_index() {
        // linear = OS_BASE + 4MB → idx = 0x80000 + 0x400 = 0x80400
        let idx = 0x80400usize;
        let mut ptes = vec![0u32; idx + 1];
        ptes[idx] = 0xDEAD_BEEF;
        let linear = OS_BASE + IDENTITY_WINDOW;
        assert_eq!(
            unsafe { rust_call(linear, &ptes, OS_BASE) },
            0xDEAD_B000
        );
        assert_eq!(
            fasm_oracle_get_pg_addr(linear, &ptes, OS_BASE),
            0xDEAD_B000
        );
    }

    #[test]
    fn page_mask_strips_flags() {
        let mut ptes = vec![0u32; 0x500];
        // Must use high path (linear >= 4MB when os_base=0).
        ptes[0x405] = 0x0010_0FFF; // all low flags set
        assert_eq!(unsafe { rust_call(0x405_000, &ptes, 0) }, 0x0010_0000);
        assert_eq!(fasm_oracle_get_pg_addr(0x405_000, &ptes, 0), 0x0010_0000);
    }

    #[test]
    fn low_path_exhaustive_page_offsets() {
        let empty: [u32; 0] = [];
        // Every page start in the identity window
        let mut off = 0u32;
        while off < IDENTITY_WINDOW {
            let linear = OS_BASE.wrapping_add(off);
            let got = unsafe { rust_call(linear, &empty, OS_BASE) };
            let expect = fasm_oracle_get_pg_addr(linear, &empty, OS_BASE);
            assert_eq!(got, expect, "off={off:#x}");
            assert_eq!(got, off & !(PAGE_SIZE - 1));
            off = off.wrapping_add(PAGE_SIZE);
        }
        // Intra-page offsets sample
        for add in [0u32, 1, 0x7FF, 0xFFF] {
            let mut off = 0u32;
            while off < IDENTITY_WINDOW {
                let linear = OS_BASE.wrapping_add(off).wrapping_add(add);
                if linear.wrapping_sub(OS_BASE) >= IDENTITY_WINDOW {
                    break;
                }
                let got = unsafe { rust_call(linear, &empty, OS_BASE) };
                let expect = fasm_oracle_get_pg_addr(linear, &empty, OS_BASE);
                assert_eq!(got, expect, "off={off:#x} add={add:#x}");
                off = off.wrapping_add(PAGE_SIZE * 16);
            }
        }
    }

    #[test]
    fn wrap_under_os_base_matches_oracle() {
        // Legacy wrapping: linear < OS_BASE still follows unsigned sub/shr path.
        let mut ptes = vec![0u32; 0x100_010 + 1];
        let linear = 0x0001_0000u32;
        let idx = linear
            .wrapping_sub(OS_BASE)
            .wrapping_shr(12)
            .wrapping_add(OS_BASE >> 12) as usize;
        ptes[idx] = 0x0BAD_F00D;
        assert_eq!(
            unsafe { rust_call(linear, &ptes, OS_BASE) },
            0x0BAD_F000
        );
        assert_eq!(
            fasm_oracle_get_pg_addr(linear, &ptes, OS_BASE),
            0x0BAD_F000
        );
    }

    #[test]
    fn prng_50k_seed_cutq() {
        let mut state = GET_PG_ADDR_PRNG_SEED;
        // Compact table with os_base=0; plant random PTEs for high indices.
        let mut ptes = vec![0u32; 0x1000];
        for i in 0..ptes.len() {
            ptes[i] = xorshift32(&mut state) | 1;
        }
        for _ in 0..50_000 {
            let linear = xorshift32(&mut state);
            // Keep index in table when high path: idx = linear>>12 for os_base=0
            let linear = if linear.wrapping_sub(0) >= IDENTITY_WINDOW {
                let idx = (linear >> 12) as usize % ptes.len();
                (idx as u32) << 12
            } else {
                linear % IDENTITY_WINDOW
            };
            let got = unsafe { rust_call(linear, &ptes, 0) };
            let expect = fasm_oracle_get_pg_addr(linear, &ptes, 0);
            assert_eq!(got, expect, "linear={linear:#x}");
        }
    }

    #[test]
    fn prng_production_os_base_low_and_boundary() {
        let empty: [u32; 0] = [];
        let mut state = GET_PG_ADDR_PRNG_SEED ^ 0xA5A5_A5A5;
        for _ in 0..10_000 {
            let off = xorshift32(&mut state) % IDENTITY_WINDOW;
            let linear = OS_BASE.wrapping_add(off);
            let got = unsafe { rust_call(linear, &empty, OS_BASE) };
            let expect = fasm_oracle_get_pg_addr(linear, &empty, OS_BASE);
            assert_eq!(got, expect);
        }
        // High-path samples around first non-identity page
        let idx0 = 0x80400usize;
        let mut ptes = vec![0u32; idx0 + 0x20];
        for i in 0..0x20 {
            ptes[idx0 + i] = (0x1000_0000 + (i as u32) * PAGE_SIZE) | 0x27;
        }
        for i in 0u32..0x20 {
            let linear = OS_BASE + IDENTITY_WINDOW + i * PAGE_SIZE;
            let got = unsafe { rust_call(linear, &ptes, OS_BASE) };
            let expect = fasm_oracle_get_pg_addr(linear, &ptes, OS_BASE);
            assert_eq!(got, expect, "i={i}");
        }
    }
}
