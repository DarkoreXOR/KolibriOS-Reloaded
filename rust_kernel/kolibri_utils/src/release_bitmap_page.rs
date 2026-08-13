//! Cut CT: `release_bitmap_page_without_cursor_update` — bitmap BTS + delta.
//!
//! Matches `kernel/core/memory.inc` FASM helper:
//! ```text
//! bts dword [sys_pgmap], eax   ; CF = OLD (1 = already free)
//! cmc                          ; CF = !OLD
//! mov eax, 0
//! adc eax, 0                   ; EAX = delta ∈ {0,1}
//! ret
//! ```
//!
//! Does **not** write `pages_free` or `page_start`. Does not touch PTE /
//! `invlpg` / mutex / CR3. `map` is trampoline-injected so the blob stays
//! reloc-free (Cut AA pattern).

/// Cut CT differential PRNG seed (`'RBPB'`).
pub const RELEASE_BITMAP_PAGE_PRNG_SEED: u32 = 0x5250_4242;

/// FASM-faithful bitmap release without cursor / pages_free mutation.
///
/// # Safety
/// `map` must be writable at byte `page_index >> 3` (unchecked, like FASM `bts`).
#[inline(always)]
pub unsafe fn release_bitmap_page_without_cursor_update(
    page_index: u32,
    map: *mut u8,
) -> u32 {
    let byte_index = (page_index >> 3) as usize;
    let bit = (page_index & 7) as u8;
    // SAFETY: caller guarantees map covers this bit (production: sys_pgmap).
    let p = unsafe { map.add(byte_index) };
    let old_byte = unsafe { *p };
    let old = (old_byte >> bit) & 1;
    unsafe { *p = old_byte | (1 << bit) };
    // bts CF=OLD; cmc; adc eax,0 → delta = !OLD
    u32::from(old == 0)
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`release_bitmap_page_without_cursor_update`].
#[inline(always)]
pub unsafe fn release_bitmap_page_without_cursor_update_ptr(
    page_index: u32,
    map: *mut u8,
) -> u32 {
    unsafe { release_bitmap_page_without_cursor_update(page_index, map) }
}

/// Independent BTS oracle (map-only; no pages_free / page_start).
#[cfg(test)]
pub fn oracle_release_bitmap_delta(map: &mut [u8], page_index: u32) -> u32 {
    let byte_index = (page_index >> 3) as usize;
    if byte_index >= map.len() {
        return 0;
    }
    let bit = (page_index & 7) as u8;
    let old = (map[byte_index] >> bit) & 1;
    map[byte_index] |= 1 << bit;
    u32::from(old == 0)
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

    #[test]
    fn rbpb_allocated_then_already_free() {
        let mut map = [0u8; 16];
        let d1 = unsafe { release_bitmap_page_without_cursor_update(5, map.as_mut_ptr()) };
        assert_eq!(d1, 1);
        assert_eq!(map[0] & (1 << 5), 1 << 5);
        let d2 = unsafe { release_bitmap_page_without_cursor_update(5, map.as_mut_ptr()) };
        assert_eq!(d2, 0);
    }

    #[test]
    fn rbpb_pages_free_canary_untouched() {
        // Helper must not receive pages_free — prove map-only API leaves a
        // side canary alone when the caller never passes it.
        let mut map = [0u8; 8];
        let mut pages_free = 0xDEAD_BEEFu32;
        let mut page_start = 0x5151_4151u32;
        let before_pf = pages_free;
        let before_ps = page_start;
        let _ = unsafe { release_bitmap_page_without_cursor_update(3, map.as_mut_ptr()) };
        assert_eq!(pages_free, before_pf);
        assert_eq!(page_start, before_ps);
        // Silence unused-mut if optimizer folds (keep canaries live).
        core::hint::black_box((&mut pages_free, &mut page_start));
    }

    #[test]
    fn rbpb_page_start_cursor_relative() {
        let mut map = [0u8; 64];
        let page_start_canary = 0x5151_4151u32;
        for page in [2u32, 31, 32, 100, 200] {
            let ps = page_start_canary;
            let d = unsafe { release_bitmap_page_without_cursor_update(page, map.as_mut_ptr()) };
            assert_eq!(d, 1, "page {page}");
            assert_eq!(ps, page_start_canary);
            let d2 = unsafe { release_bitmap_page_without_cursor_update(page, map.as_mut_ptr()) };
            assert_eq!(d2, 0);
            assert_eq!(ps, page_start_canary);
        }
    }

    #[test]
    fn rbpb_vs_oracle_prng_50000() {
        let mut rng = RELEASE_BITMAP_PAGE_PRNG_SEED;
        let mut map_a = vec![0u8; 512];
        let mut map_b = vec![0u8; 512];
        for b in map_a.iter_mut() {
            *b = (xorshift32(&mut rng) & 0xFF) as u8;
        }
        map_b.copy_from_slice(&map_a);
        for case in 0..50_000u32 {
            let page = xorshift32(&mut rng) % (512 * 8);
            let da = unsafe { release_bitmap_page_without_cursor_update(page, map_a.as_mut_ptr()) };
            let db = oracle_release_bitmap_delta(&mut map_b, page);
            assert_eq!(da, db, "case {case} page={page}");
            assert_eq!(map_a, map_b, "case {case} map");
        }
    }

    #[test]
    fn rbpb_vs_pgbm_independent_and_fasm() {
        use crate::pg_bitmap_oracle::{BitmapSnapshot, FasmBitmapEmu, IndependentBitmap};

        let mut rng = RELEASE_BITMAP_PAGE_PRNG_SEED ^ 0x1111_2222;
        for trial in 0..500u32 {
            let map_bytes = 64;
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
            snap.pages_free = free;
            snap.page_start_off = ((xorshift32(&mut rng) as usize) % (map_bytes / 4)) * 4;

            let page = xorshift32(&mut rng) % snap.total_pages();
            let pf_before = snap.pages_free;
            let ps_before = snap.page_start_off;

            let mut ind = IndependentBitmap::from_snapshot(&snap);
            let mut fasm = FasmBitmapEmu::new(snap.clone());
            let mut rust_map = snap.map.clone();

            let di = ind.release_bitmap_page_without_cursor_update(page);
            let df = fasm.release_bitmap_page_without_cursor_update(page);
            let dr = unsafe {
                release_bitmap_page_without_cursor_update(page, rust_map.as_mut_ptr())
            };
            assert_eq!(di, df, "trial {trial} ind vs fasm");
            assert_eq!(dr, di, "trial {trial} rust vs ind");
            assert_eq!(rust_map, fasm.snap.map, "trial {trial} rust map");
            assert_eq!(fasm.snap.pages_free, pf_before, "trial {trial} pages_free");
            assert_eq!(fasm.snap.page_start_off, ps_before, "trial {trial} page_start");
            assert_eq!(ind.to_snapshot(map_bytes).pages_free, pf_before);
            assert_eq!(ind.to_snapshot(map_bytes).page_start_off, ps_before);
        }
    }

    #[test]
    fn rbpb_dword_word_boundaries() {
        let mut map = [0u8; 16];
        for page in [0u32, 7, 8, 15, 16, 31, 32, 63] {
            let d = unsafe { release_bitmap_page_without_cursor_update(page, map.as_mut_ptr()) };
            assert_eq!(d, 1, "first release page {page}");
        }
    }

    #[test]
    fn rbpb_wrapping_delta_contract() {
        // Helper returns delta only; caller's add ebp,eax wraps. Prove delta=1
        // when releasing an allocated page even if a virtual pages_free is MAX.
        let mut map = [0u8; 8];
        let pages_free = u32::MAX;
        let d = unsafe { release_bitmap_page_without_cursor_update(1, map.as_mut_ptr()) };
        assert_eq!(d, 1);
        let batched = pages_free.wrapping_add(d);
        assert_eq!(batched, 0);
        // Helper itself did not observe pages_free.
        assert_eq!(pages_free, u32::MAX);
    }
}
