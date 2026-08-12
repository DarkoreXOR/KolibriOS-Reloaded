//! Cut CI: `usb_td_to_virt` — TD physical address → linear address.
//!
//! Matches `kernel/bus/usb/memory.inc` FASM leaf:
//! ```text
//! walk page list at EAX; ECX = TD phys
//!   get_pg_addr(page) → match if td_phys in [page_phys, page_phys+0xFFF]
//!   next = [page + 0xFFC]
//! on hit: EAX = page_virt + (td_phys & 0xFFF); ECX &= 0xFFF
//! on miss: EAX = 0; ECX unchanged
//! ```
//!
//! Reloc-free: `page_tabs` + `os_base` are explicit (Cut AQ pattern).

use crate::get_pg_addr::{get_pg_addr, PAGE_SIZE};

/// Cut CI differential PRNG seed (`'UTDV'`).
pub const USB_TD_TO_VIRT_PRNG_SEED: u32 = 0x5554_4456;

/// Offset of the next-page link within a TD page (`0x1000 - 4`).
pub const TD_PAGE_NEXT_OFF: usize = PAGE_SIZE as usize - 4;

/// FASM-faithful TD phys → linear translate.
///
/// Returns `(eax, ecx)` where `ecx` is the post-call ECX (offset on hit,
/// original `td_phys` on miss).
///
/// # Safety
/// `first` must be 0 or a readable page-aligned TD page whose last dword is
/// the next-page link (or 0). `page_tabs` must satisfy Cut AQ requirements
/// for every page visited.
#[inline(always)]
pub unsafe fn usb_td_to_virt(
    first: u32,
    td_phys: u32,
    page_tabs: *const u32,
    os_base: u32,
) -> (u32, u32) {
    let mut page = first;
    loop {
        if page == 0 {
            return (0, td_phys);
        }
        // SAFETY: caller guarantees `page` is a readable TD page virt.
        let page_phys = unsafe { get_pg_addr(page, page_tabs, os_base) };
        // Match when td_phys ∈ [page_phys, page_phys + 0xFFF]
        // (FASM: sub page_phys, td_phys; jz / ja vs -0x1000).
        if td_phys.wrapping_sub(page_phys) < PAGE_SIZE {
            let offset = td_phys & (PAGE_SIZE - 1);
            return (page.wrapping_add(offset), offset);
        }
        // mov eax, [eax+0x1000-4]
        // SAFETY: last dword of the TD page is the next-page link.
        let next_ptr = (page as usize).wrapping_add(TD_PAGE_NEXT_OFF) as *const u32;
        page = unsafe { core::ptr::read_volatile(next_ptr) };
    }
}

/// Pointer-form wrapper for the FFI boundary (EAX only; trampoline fixes ECX).
///
/// # Safety
/// Same as [`usb_td_to_virt`].
#[inline(always)]
pub unsafe fn usb_td_to_virt_ptr(
    first: u32,
    td_phys: u32,
    page_tabs: *const u32,
    os_base: u32,
) -> u32 {
    let (eax, _) = unsafe { usb_td_to_virt(first, td_phys, page_tabs, os_base) };
    eax
}

/// Independent FASM-flow oracle (register steps; not a call to Rust).
///
/// `page_phys_of(virt)` supplies what `get_pg_addr` would return.
/// `next_of(virt)` supplies `[virt + 0xFFC]`.
#[cfg(test)]
pub fn fasm_oracle_usb_td_to_virt(
    first: u32,
    td_phys: u32,
    mut page_phys_of: impl FnMut(u32) -> u32,
    mut next_of: impl FnMut(u32) -> u32,
) -> (u32, u32) {
    let mut eax = first;
    let mut ecx = td_phys;
    loop {
        if eax == 0 {
            return (0, ecx);
        }
        let saved = eax;
        eax = page_phys_of(saved);
        let diff = eax.wrapping_sub(ecx);
        if diff == 0 || diff > (-(PAGE_SIZE as i32) as u32) {
            eax = saved;
            ecx &= PAGE_SIZE - 1;
            eax = eax.wrapping_add(ecx);
            return (eax, ecx);
        }
        eax = next_of(saved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    /// Logical arena: fake u32 virts + side tables (host-safe on x64).
    /// Exercises oracle + a pure twin of the Rust match/walk without casting
    /// host pointers to u32 (which truncates on 64-bit).
    struct LogicalArena {
        /// virt → (phys, next_virt)
        map: HashMap<u32, (u32, u32)>,
        first: u32,
    }

    impl LogicalArena {
        fn new(n: usize, phys_base: u32, virt_base: u32) -> Self {
            let mut map = HashMap::new();
            for i in 0..n {
                let virt = virt_base + (i as u32) * PAGE_SIZE;
                let phys = phys_base + (i as u32) * PAGE_SIZE;
                let next = if i + 1 < n {
                    virt_base + ((i + 1) as u32) * PAGE_SIZE
                } else {
                    0
                };
                map.insert(virt, (phys, next));
            }
            Self {
                map,
                first: virt_base,
            }
        }

        fn phys_of(&self, virt: u32) -> u32 {
            self.map.get(&virt).map(|x| x.0).unwrap_or(0)
        }

        fn next_of(&self, virt: u32) -> u32 {
            self.map.get(&virt).map(|x| x.1).unwrap_or(0)
        }

        /// Pure twin of [`usb_td_to_virt`] using side tables (not host ptrs).
        fn rust_twin(&self, first: u32, td_phys: u32) -> (u32, u32) {
            let mut page = first;
            loop {
                if page == 0 {
                    return (0, td_phys);
                }
                let page_phys = self.phys_of(page);
                if td_phys.wrapping_sub(page_phys) < PAGE_SIZE {
                    let offset = td_phys & (PAGE_SIZE - 1);
                    return (page.wrapping_add(offset), offset);
                }
                page = self.next_of(page);
            }
        }

        fn oracle(&self, first: u32, td_phys: u32) -> (u32, u32) {
            fasm_oracle_usb_td_to_virt(
                first,
                td_phys,
                |v| self.phys_of(v),
                |v| self.next_of(v),
            )
        }
    }

    #[test]
    fn utdv_empty_head() {
        let (a, c) = LogicalArena {
            map: HashMap::new(),
            first: 0,
        }
        .rust_twin(0, 0x1234);
        assert_eq!((a, c), (0, 0x1234));
        assert_eq!(
            fasm_oracle_usb_td_to_virt(0, 0x1234, |_| 0, |_| 0),
            (0, 0x1234)
        );
    }

    #[test]
    fn utdv_single_page_hit_offset0() {
        let arena = LogicalArena::new(1, 0x1000, 0x10000);
        let td_phys = arena.phys_of(arena.first);
        let got = arena.rust_twin(arena.first, td_phys);
        assert_eq!(got, (arena.first, 0));
        assert_eq!(got, arena.oracle(arena.first, td_phys));
    }

    #[test]
    fn utdv_single_page_hit_mid_offset() {
        let arena = LogicalArena::new(1, 0x1000, 0x10000);
        let off = 0xABC;
        let td_phys = arena.phys_of(arena.first) + off;
        let got = arena.rust_twin(arena.first, td_phys);
        assert_eq!(got, (arena.first + off, off));
        assert_eq!(got, arena.oracle(arena.first, td_phys));
    }

    #[test]
    fn utdv_single_page_hit_last_byte() {
        let arena = LogicalArena::new(1, 0x1000, 0x10000);
        let off = 0xFFF;
        let td_phys = arena.phys_of(arena.first) + off;
        let got = arena.rust_twin(arena.first, td_phys);
        assert_eq!(got, (arena.first + off, off));
        assert_eq!(got, arena.oracle(arena.first, td_phys));
    }

    #[test]
    fn utdv_single_page_miss_outside() {
        let arena = LogicalArena::new(1, 0x1000, 0x10000);
        let td_phys = arena.phys_of(arena.first) + PAGE_SIZE;
        let got = arena.rust_twin(arena.first, td_phys);
        assert_eq!(got, (0, td_phys));
        assert_eq!(got, arena.oracle(arena.first, td_phys));
    }

    #[test]
    fn utdv_hit_on_second_page() {
        let arena = LogicalArena::new(3, 0x2000, 0x20000);
        let target = arena.first + PAGE_SIZE;
        let off = 0x40;
        let td_phys = arena.phys_of(target) + off;
        let got = arena.rust_twin(arena.first, td_phys);
        assert_eq!(got, (target + off, off));
        assert_eq!(got, arena.oracle(arena.first, td_phys));
    }

    #[test]
    fn utdv_miss_after_full_walk() {
        let arena = LogicalArena::new(2, 0x3000, 0x30000);
        let td_phys = 0xDEAD_0000;
        let got = arena.rust_twin(arena.first, td_phys);
        assert_eq!(got, (0, td_phys));
        assert_eq!(got, arena.oracle(arena.first, td_phys));
    }

    #[test]
    fn utdv_oracle_unsigned_window_matches_fasm_cmp() {
        let page_phys = 0x1000u32;
        for off in [0u32, 1, 0x7FF, 0xFFE, 0xFFF] {
            let td = page_phys + off;
            let diff = page_phys.wrapping_sub(td);
            let fasm_hit = diff == 0 || diff > (-(PAGE_SIZE as i32) as u32);
            let rust_hit = td.wrapping_sub(page_phys) < PAGE_SIZE;
            assert_eq!(fasm_hit, rust_hit, "off={off}");
        }
        let td_miss = page_phys + PAGE_SIZE;
        let diff = page_phys.wrapping_sub(td_miss);
        assert!(!(diff == 0 || diff > (-(PAGE_SIZE as i32) as u32)));
        assert!(!(td_miss.wrapping_sub(page_phys) < PAGE_SIZE));
    }

    #[test]
    fn utdv_prng_50000() {
        let mut state = USB_TD_TO_VIRT_PRNG_SEED;
        for case in 0..50_000u32 {
            let n_pages = 1 + (xorshift32(&mut state) % 4) as usize;
            let phys_base = 0x1000 + (xorshift32(&mut state) & 0xFF) * PAGE_SIZE;
            let virt_base = 0x10000 + (xorshift32(&mut state) & 0x3F) * PAGE_SIZE * 4;
            let arena = LogicalArena::new(n_pages, phys_base, virt_base);
            let mode = xorshift32(&mut state) % 5;
            let (first, td_phys) = match mode {
                0 => {
                    let pi = (xorshift32(&mut state) as usize) % n_pages;
                    let virt = virt_base + (pi as u32) * PAGE_SIZE;
                    let off = xorshift32(&mut state) & 0xFFF;
                    (arena.first, arena.phys_of(virt) + off)
                }
                1 => (
                    arena.first,
                    arena.phys_of(virt_base + ((n_pages - 1) as u32) * PAGE_SIZE) + PAGE_SIZE,
                ),
                2 => (arena.first, xorshift32(&mut state) | 0x8000_0000),
                3 => (arena.first, arena.phys_of(arena.first)),
                _ => {
                    let td = xorshift32(&mut state);
                    let got = arena.rust_twin(0, td);
                    let exp = arena.oracle(0, td);
                    assert_eq!(got, exp, "case {case} empty");
                    assert_eq!(got, (0, td));
                    continue;
                }
            };
            let got = arena.rust_twin(first, td_phys);
            let exp = arena.oracle(first, td_phys);
            assert_eq!(got, exp, "case {case} mode {mode}");
        }
    }

    /// Live pointer-path smoke when a low-32-bit page can be allocated.
    #[cfg(windows)]
    #[test]
    fn utdv_live_pointer_path_if_low_alloc() {
        #[link(name = "kernel32")]
        extern "system" {
            fn VirtualAlloc(
                lp_address: *mut core::ffi::c_void,
                dw_size: usize,
                fl_allocation_type: u32,
                fl_protect: u32,
            ) -> *mut core::ffi::c_void;
            fn VirtualFree(
                lp_address: *mut core::ffi::c_void,
                dw_size: usize,
                dw_free_type: u32,
            ) -> i32;
        }
        const MEM_COMMIT: u32 = 0x1000;
        const MEM_RESERVE: u32 = 0x2000;
        const MEM_RELEASE: u32 = 0x8000;
        const PAGE_READWRITE: u32 = 0x04;

        // Hint into low 2GiB so virt fits in u32 and is readable as such.
        let hint = 0x2000_0000usize as *mut core::ffi::c_void;
        let p = unsafe {
            VirtualAlloc(
                hint,
                0x2000,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if p.is_null() || (p as u64) > u32::MAX as u64 {
            // Environment cannot provide a u32-reachable page — skip.
            return;
        }
        let page0 = p as u32;
        let page1 = page0 + PAGE_SIZE;
        unsafe {
            core::ptr::write_bytes(p as *mut u8, 0, 0x2000);
            core::ptr::write_unaligned(
                (page0 as *mut u8).add(TD_PAGE_NEXT_OFF) as *mut u32,
                page1,
            );
            core::ptr::write_unaligned(
                (page1 as *mut u8).add(TD_PAGE_NEXT_OFF) as *mut u32,
                0,
            );
        }
        // Identity: os_base = page0, phys = virt - os_base (within 4MiB window).
        let os_base = page0;
        let phys0 = 0u32;
        let phys1 = PAGE_SIZE;
        let empty: [u32; 0] = [];
        let (a, c) = unsafe { usb_td_to_virt(page0, phys0 + 0x55, empty.as_ptr(), os_base) };
        assert_eq!((a, c), (page0 + 0x55, 0x55));
        let (a, c) = unsafe { usb_td_to_virt(page0, phys1 + 0x10, empty.as_ptr(), os_base) };
        assert_eq!((a, c), (page1 + 0x10, 0x10));
        let (a, c) = unsafe { usb_td_to_virt(page0, 0xDEAD0000, empty.as_ptr(), os_base) };
        assert_eq!((a, c), (0, 0xDEAD0000));
        unsafe {
            VirtualFree(p, 0, MEM_RELEASE);
        }
    }
}
