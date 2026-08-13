//! Cut CU / Slice E: physical page bitmap allocator ownership.
//!
//! Freestanding, reloc-free bodies for:
//! - [`alloc_page`]
//! - [`free_page`]
//! - [`alloc_pages`]
//! - [`release_bitmap_page_mode_b`] (Mode B: BTS + `pages_free += delta`; no cursor)
//!
//! `page_start` / `page_end` arguments are **byte offsets from `map`** (FASM
//! trampoline converts absolute `page_start`/`page_end` ↔ offsets). This keeps
//! host differentials portable and blobs reloc-free.

/// Cut CU / Slice E differential PRNG seed (`'SLCE'`).
pub const PHYS_BITMAP_PRNG_SEED: u32 = 0x534C_4345;

/// Shared BTS free polarity → delta ∈ {0,1}.
///
/// # Safety
/// `map` must be writable at byte `page_index >> 3`.
#[inline(always)]
pub unsafe fn bitmap_bts_free_delta(page_index: u32, map: *mut u8) -> u32 {
    let byte_index = (page_index >> 3) as usize;
    let bit = (page_index & 7) as u8;
    let p = unsafe { map.add(byte_index) };
    let old_byte = unsafe { *p };
    let old = (old_byte >> bit) & 1;
    unsafe { *p = old_byte | (1 << bit) };
    u32::from(old == 0)
}

/// Mode B release: BTS + wrapping `pages_free += delta`; **no** cursor.
///
/// # Safety
/// `map` and `pages_free` must be valid writable pointers.
#[inline(always)]
pub unsafe fn release_bitmap_page_mode_b(
    page_index: u32,
    map: *mut u8,
    pages_free: *mut u32,
) -> u32 {
    let delta = unsafe { bitmap_bts_free_delta(page_index, map) };
    if delta != 0 {
        unsafe {
            *pages_free = (*pages_free).wrapping_add(delta);
        }
    }
    delta
}

/// FASM-faithful `alloc_page` (caller owns `cli` / pushfd).
///
/// `page_start` / `page_end` are byte offsets from `map`.
/// Returns physical page base or 0.
///
/// # Safety
/// Map covers `[page_start_off, page_end_off)`.
#[inline(always)]
pub unsafe fn alloc_page(
    map: *mut u8,
    page_start: *mut u32,
    page_end: *const u32,
    pages_free: *mut u32,
) -> u32 {
    if (unsafe { *pages_free } as i32) <= 1 {
        unsafe {
            *pages_free = 1;
        }
        return 0;
    }

    let end = unsafe { *page_end };
    let mut ebx = unsafe { *page_start };

    while ebx < end {
        let off = ebx as usize;
        let word = unsafe { core::ptr::read_unaligned(map.add(off) as *const u32) };
        if word != 0 {
            let bit = word.trailing_zeros();
            unsafe {
                *pages_free = (*pages_free).wrapping_sub(1);
            }
            if unsafe { *pages_free } == 0 {
                unsafe {
                    *pages_free = 1;
                }
                return 0;
            }
            let new_word = word & !(1u32 << bit);
            unsafe {
                core::ptr::write_unaligned(map.add(off) as *mut u32, new_word);
                *page_start = ebx;
            }
            let page_index = bit.wrapping_add(ebx.wrapping_mul(8));
            return page_index << 12;
        }
        ebx = ebx.wrapping_add(4);
    }
    0
}

/// FASM-faithful `free_page` (caller owns `cli` / pushfd).
///
/// `page_start` holds byte offset from `map`; may be lowered.
///
/// # Safety
/// Pointers must be valid.
#[inline(always)]
pub unsafe fn free_page(
    phys: u32,
    map: *mut u8,
    page_start: *mut u32,
    pages_free: *mut u32,
) {
    let page_index = phys >> 12;
    let delta = unsafe { bitmap_bts_free_delta(page_index, map) };
    if delta != 0 {
        unsafe {
            *pages_free = (*pages_free).wrapping_add(1);
        }
    }
    // dword offset from map: (page_index>>3) & ~3
    let dword_off = (page_index >> 3) & !3;
    let cursor = unsafe { *page_start };
    if cursor > dword_off {
        unsafe {
            *page_start = dword_off;
        }
    }
}

/// FASM-faithful `alloc_pages` (caller owns `cli` / pushfd / DF).
///
/// Does **not** write `page_start`.
///
/// # Safety
/// Map window must be writable.
#[inline(always)]
pub unsafe fn alloc_pages(
    count: u32,
    map: *mut u8,
    page_start: *const u32,
    page_end: *const u32,
    pages_free: *mut u32,
) -> u32 {
    let need_bytes = count.wrapping_add(7) >> 3;
    let pf = unsafe { *pages_free };
    if (pf as i32).wrapping_sub(9) < 0 {
        return 0;
    }
    let avail = pf.wrapping_sub(9) >> 3;
    if (need_bytes as i32) > (avail as i32) {
        return 0;
    }
    if need_bytes == 0 {
        return 0;
    }

    let end = unsafe { *page_end };
    let mut ecx = unsafe { *page_start };

    while ecx < end {
        let mut edx = need_bytes;
        let edi = ecx;
        let mut cur = ecx;
        loop {
            if cur >= end {
                return 0;
            }
            let b = unsafe { *map.add(cur as usize) };
            if b != 0xFF {
                ecx = cur.wrapping_add(1);
                break;
            }
            edx = edx.wrapping_sub(1);
            if edx == 0 {
                let mut i = 0u32;
                while i < need_bytes {
                    unsafe {
                        core::ptr::write_volatile(map.add(edi as usize + i as usize), 0u8);
                    }
                    i = i.wrapping_add(1);
                }
                let charge = need_bytes << 3;
                unsafe {
                    *pages_free = (*pages_free).wrapping_sub(charge);
                }
                return edi << 15;
            }
            cur = cur.wrapping_add(1);
        }
    }
    0
}

#[inline(always)]
pub unsafe fn alloc_page_ptr(
    map: *mut u8,
    page_start: *mut u32,
    page_end: *const u32,
    pages_free: *mut u32,
) -> u32 {
    unsafe { alloc_page(map, page_start, page_end, pages_free) }
}

#[inline(always)]
pub unsafe fn free_page_ptr(
    phys: u32,
    map: *mut u8,
    page_start: *mut u32,
    pages_free: *mut u32,
) {
    unsafe { free_page(phys, map, page_start, pages_free) }
}

#[inline(always)]
pub unsafe fn alloc_pages_ptr(
    count: u32,
    map: *mut u8,
    page_start: *const u32,
    page_end: *const u32,
    pages_free: *mut u32,
) -> u32 {
    unsafe { alloc_pages(count, map, page_start, page_end, pages_free) }
}

#[inline(always)]
pub unsafe fn release_bitmap_page_mode_b_ptr(
    page_index: u32,
    map: *mut u8,
    pages_free: *mut u32,
) -> u32 {
    unsafe { release_bitmap_page_mode_b(page_index, map, pages_free) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pg_bitmap_oracle::{BitmapSnapshot, FasmBitmapEmu, IndependentBitmap};

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    struct HostCtx {
        map: Vec<u8>,
        page_start: u32,
        page_end: u32,
        pages_free: u32,
    }

    impl HostCtx {
        fn from_snap(snap: &BitmapSnapshot) -> Self {
            Self {
                map: snap.map.clone(),
                page_start: snap.page_start_off as u32,
                page_end: snap.page_end_off as u32,
                pages_free: snap.pages_free,
            }
        }

        fn to_snap(&self) -> BitmapSnapshot {
            BitmapSnapshot {
                map: self.map.clone(),
                pages_free: self.pages_free,
                page_start_off: self.page_start as usize,
                page_end_off: self.page_end as usize,
            }
        }

        unsafe fn alloc_page(&mut self) -> u32 {
            unsafe {
                alloc_page(
                    self.map.as_mut_ptr(),
                    &mut self.page_start,
                    &self.page_end,
                    &mut self.pages_free,
                )
            }
        }

        unsafe fn free_page(&mut self, phys: u32) {
            unsafe {
                free_page(
                    phys,
                    self.map.as_mut_ptr(),
                    &mut self.page_start,
                    &mut self.pages_free,
                )
            }
        }

        unsafe fn alloc_pages(&mut self, count: u32) -> u32 {
            unsafe {
                alloc_pages(
                    count,
                    self.map.as_mut_ptr(),
                    &self.page_start,
                    &self.page_end,
                    &mut self.pages_free,
                )
            }
        }

        unsafe fn release_mode_b(&mut self, page_index: u32) -> u32 {
            unsafe {
                release_bitmap_page_mode_b(page_index, self.map.as_mut_ptr(), &mut self.pages_free)
            }
        }
    }

    #[test]
    fn slce_alloc_free_roundtrip() {
        let mut snap = BitmapSnapshot::fresh_all_free(64);
        // Force first free page away from phys 0 (EAX=0 is also OOM).
        snap.map[0] = 0;
        snap.map[1] = 0xFF;
        snap.pages_free = 8;
        snap.page_start_off = 0;
        let mut ctx = HostCtx::from_snap(&snap);
        let phys = unsafe { ctx.alloc_page() };
        assert_eq!(phys, 8 << 12);
        assert_eq!(ctx.pages_free, 7);
        let ps_after_alloc = ctx.page_start;
        unsafe { ctx.free_page(phys) };
        assert_eq!(ctx.pages_free, 8);
        assert!(ctx.page_start <= ps_after_alloc);
    }

    #[test]
    fn slce_oom_force_one() {
        let mut snap = BitmapSnapshot::fresh_all_free(16);
        snap.pages_free = 1;
        let mut ctx = HostCtx::from_snap(&snap);
        assert_eq!(unsafe { ctx.alloc_page() }, 0);
        assert_eq!(ctx.pages_free, 1);
    }

    #[test]
    fn slce_alloc_pages_ff_run_cursor_unchanged() {
        let mut snap = BitmapSnapshot::fresh_all_free(64);
        snap.map[0] = 0; // skip phys-0 ambiguity
        snap.pages_free = 64;
        snap.page_start_off = 1;
        let mut ctx = HostCtx::from_snap(&snap);
        let ps = ctx.page_start;
        let phys = unsafe { ctx.alloc_pages(8) };
        assert_eq!(phys, 1 << 15);
        assert_eq!(ctx.page_start, ps);
        assert_eq!(ctx.pages_free, 56);
        assert_eq!(ctx.map[1], 0);
    }

    #[test]
    fn slce_mode_b_updates_pages_free_not_cursor() {
        let mut snap = BitmapSnapshot::fresh_all_free(16);
        snap.map[0] &= !(1 << 5);
        snap.pages_free = 10;
        snap.page_start_off = 8;
        let mut ctx = HostCtx::from_snap(&snap);
        let ps = ctx.page_start;
        assert_eq!(unsafe { ctx.release_mode_b(5) }, 1);
        assert_eq!(ctx.pages_free, 11);
        assert_eq!(ctx.page_start, ps);
        assert_eq!(unsafe { ctx.release_mode_b(5) }, 0);
        assert_eq!(ctx.pages_free, 11);
    }

    #[test]
    fn slce_free_ne_release_cursor() {
        let mut snap = BitmapSnapshot::fresh_all_free(64);
        snap.map[0] &= !1;
        snap.pages_free = 49;
        snap.page_start_off = 16;
        let mut free_ctx = HostCtx::from_snap(&snap);
        let mut rel_ctx = HostCtx::from_snap(&snap);
        unsafe { free_ctx.free_page(0) };
        let _ = unsafe { rel_ctx.release_mode_b(0) };
        assert!(free_ctx.page_start < 16);
        assert_eq!(rel_ctx.page_start, 16);
        assert_eq!(free_ctx.pages_free, rel_ctx.pages_free);
    }

    #[test]
    fn slce_vs_fasm_emu_50000() {
        let mut rng = PHYS_BITMAP_PRNG_SEED;
        for case in 0..50_000u32 {
            let map_bytes = 64usize;
            let mut snap = BitmapSnapshot::fresh_all_free(map_bytes);
            for b in &mut snap.map {
                *b = (xorshift32(&mut rng) & 0xFF) as u8;
            }
            let mut free = 0u32;
            for i in 0..snap.total_pages() {
                let byte = (i as usize) / 8;
                let bit = (i as usize) % 8;
                if (snap.map[byte] >> bit) & 1 == 1 {
                    free += 1;
                }
            }
            snap.pages_free = free.max(2);
            snap.page_start_off = ((xorshift32(&mut rng) as usize) % (map_bytes / 4)) * 4;

            let op = xorshift32(&mut rng) % 5;
            let mut fasm = FasmBitmapEmu::new(snap.clone());
            let mut ind = IndependentBitmap::from_snapshot(&snap);
            let mut ctx = HostCtx::from_snap(&snap);

            match op {
                0 => {
                    let a = fasm.alloc_page().unwrap_or(0);
                    let b = ind.alloc_page().unwrap_or(0);
                    let c = unsafe { ctx.alloc_page() };
                    assert_eq!(a, b, "case {case} alloc ind");
                    assert_eq!(c, a, "case {case} alloc rust");
                    assert_eq!(ctx.to_snap().pages_free, fasm.snap.pages_free);
                    assert_eq!(ctx.to_snap().page_start_off, fasm.snap.page_start_off);
                    assert_eq!(ctx.to_snap().map, fasm.snap.map);
                }
                1 => {
                    let phys = (xorshift32(&mut rng) % snap.total_pages()) << 12;
                    fasm.free_page(phys);
                    ind.free_page(phys);
                    unsafe { ctx.free_page(phys) };
                    assert_eq!(ctx.to_snap().pages_free, fasm.snap.pages_free, "case {case}");
                    assert_eq!(ctx.to_snap().page_start_off, fasm.snap.page_start_off);
                    assert_eq!(ctx.to_snap().map, fasm.snap.map);
                }
                2 => {
                    let n = xorshift32(&mut rng) % 17;
                    let a = fasm.alloc_pages(n).unwrap_or(0);
                    let b = ind.alloc_pages(n).unwrap_or(0);
                    let c = unsafe { ctx.alloc_pages(n) };
                    assert_eq!(a, b, "case {case} ap ind");
                    assert_eq!(c, a, "case {case} ap rust");
                    assert_eq!(ctx.to_snap().pages_free, fasm.snap.pages_free);
                    assert_eq!(ctx.to_snap().page_start_off, fasm.snap.page_start_off);
                    assert_eq!(ctx.to_snap().map, fasm.snap.map);
                }
                3 => {
                    let page = xorshift32(&mut rng) % snap.total_pages();
                    let ps_before = fasm.snap.page_start_off;
                    let df = fasm.release_bitmap_page_without_cursor_update(page);
                    if df != 0 {
                        fasm.snap.pages_free = fasm.snap.pages_free.wrapping_add(df);
                    }
                    let _ = ind.release_bitmap_page_without_cursor_update(page);
                    let dr = unsafe { ctx.release_mode_b(page) };
                    assert_eq!(dr, df, "case {case} mode b delta");
                    assert_eq!(ctx.pages_free, fasm.snap.pages_free, "case {case} mode b pf");
                    assert_eq!(ctx.page_start as usize, ps_before, "case {case} mode b ps");
                    assert_eq!(ctx.map, fasm.snap.map);
                }
                _ => {
                    if let Some(phys) = fasm.alloc_page() {
                        ctx = HostCtx::from_snap(&fasm.snap);
                        if xorshift32(&mut rng) & 1 == 0 {
                            fasm.free_page(phys);
                            unsafe { ctx.free_page(phys) };
                        } else {
                            let page = phys >> 12;
                            let d = fasm.release_bitmap_page_without_cursor_update(page);
                            if d != 0 {
                                fasm.snap.pages_free = fasm.snap.pages_free.wrapping_add(d);
                            }
                            let _ = unsafe { ctx.release_mode_b(page) };
                        }
                        assert_eq!(ctx.to_snap().map, fasm.snap.map, "case {case} mixed map");
                        assert_eq!(ctx.pages_free, fasm.snap.pages_free, "case {case} mixed pf");
                        assert_eq!(
                            ctx.page_start as usize,
                            fasm.snap.page_start_off,
                            "case {case} mixed ps"
                        );
                    }
                }
            }
            let _ = ind;
        }
    }
}
