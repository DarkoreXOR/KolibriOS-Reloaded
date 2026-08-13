//! Stage-4 host-only physical page-bitmap oracle (research — not a production cut).
//!
//! Two independent engines are compared on identical synthetic state:
//!
//! 1. [`IndependentBitmap`] — set-theoretic free-page model (first free page
//!    index ≥ cursor page base). Not a line transcription of `memory.inc`.
//! 2. [`FasmBitmapEmu`] — instruction-faithful emulator of FASM `bsf` / `btr` /
//!    `bts` / `cmc` / `adc` control flow on a raw little-endian bitmap buffer.
//!
//! **Limitation:** this harness does not execute assembled FASM under Unicorn.
//! `release_pages` PTE/`invlpg`/`mutex` sides are out of scope; only the bitmap
//! half is modeled ([`IndependentBitmap::bitmap_release_phys_pages`]).
//!
//! Seed: `'PGBM'` (`0x5047_424D`). See `docs/migration/stage4-ownership-design.md` §19
//! and `docs/migration/stage4-release-bitmap-contract.md`.
//!
//! §19 contract primitive: [`IndependentBitmap::release_bitmap_page_without_cursor_update`]
//! / [`FasmBitmapEmu::release_bitmap_page_without_cursor_update`] — dedicated release
//! bitmap op (≠ `free_page`). Helper mutates map + returns delta; does **not** write
//! `pages_free` (Mode A: caller batches). Mode A (batched `pages_free` store) vs Mode B
//! (per-page helper + caller-side `pages_free += delta`) end-state equivalence is covered
//! by `pgbm_s19_*` tests.

#![cfg(test)]

/// PRNG seed for Stage-4 bitmap differential (`'PGBM'`).
pub const PG_BITMAP_PRNG_SEED: u32 = 0x5047_424D;

/// Default synthetic map size used by larger differential scenarios.
#[allow(dead_code)]
pub const DEFAULT_MAP_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapSnapshot {
    /// Little-endian bit map: bit=1 ⇒ page free (FASM `sys_pgmap` polarity).
    pub map: Vec<u8>,
    pub pages_free: u32,
    /// Byte offset of scan cursor (`page_start - sys_pgmap`), dword-aligned.
    pub page_start_off: usize,
    /// Exclusive end byte offset (`page_end - sys_pgmap`).
    pub page_end_off: usize,
}

impl BitmapSnapshot {
    pub fn fresh_all_free(map_bytes: usize) -> Self {
        assert!(map_bytes >= 4 && map_bytes % 4 == 0);
        let pages = (map_bytes as u32) * 8;
        Self {
            map: vec![0xFF; map_bytes],
            pages_free: pages,
            page_start_off: 0,
            page_end_off: map_bytes,
        }
    }

    pub fn total_pages(&self) -> u32 {
        (self.map.len() as u32).saturating_mul(8)
    }

    pub fn digest(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for &b in &self.map {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        for w in [
            self.pages_free,
            self.page_start_off as u32,
            self.page_end_off as u32,
        ] {
            h ^= u64::from(w);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

/// Independent set-theoretic model (page-index first-fit over a free-bit vector).
#[derive(Clone, Debug)]
pub struct IndependentBitmap {
    /// Packed free bits: bit=1 ⇒ free (same polarity as FASM map).
    words: Vec<u64>,
    pub(crate) pages_free: u32,
    /// First page index of the dword pointed by `page_start`.
    cursor_page_base: u32,
    total_pages: u32,
}

impl IndependentBitmap {
    pub fn from_snapshot(s: &BitmapSnapshot) -> Self {
        let total_pages = s.total_pages();
        let nwords = ((total_pages as usize) + 63) / 64;
        let mut words = vec![0u64; nwords];
        for i in 0..total_pages {
            if bit_is_free(&s.map, i) {
                words[(i / 64) as usize] |= 1u64 << (i % 64);
            }
        }
        Self {
            words,
            pages_free: s.pages_free,
            cursor_page_base: ((s.page_start_off as u32) / 4) * 32,
            total_pages,
        }
    }

    pub fn to_snapshot(&self, page_end_off: usize) -> BitmapSnapshot {
        let map_bytes = (self.total_pages as usize).div_ceil(8);
        let mut map = vec![0u8; map_bytes];
        for i in 0..self.total_pages {
            if self.is_free(i) {
                set_free_bit(&mut map, i, true);
            }
        }
        BitmapSnapshot {
            map,
            pages_free: self.pages_free,
            page_start_off: ((self.cursor_page_base / 32) * 4) as usize,
            page_end_off,
        }
    }

    fn is_free(&self, page: u32) -> bool {
        if page >= self.total_pages {
            return false;
        }
        (self.words[(page / 64) as usize] >> (page % 64)) & 1 == 1
    }

    fn set_free(&mut self, page: u32, free: bool) {
        if page >= self.total_pages {
            return;
        }
        let w = (page / 64) as usize;
        let b = page % 64;
        if free {
            self.words[w] |= 1u64 << b;
        } else {
            self.words[w] &= !(1u64 << b);
        }
    }

    pub fn alloc_page(&mut self) -> Option<u32> {
        if self.pages_free <= 1 {
            self.pages_free = 1;
            return None;
        }
        // Page-index scan (not dword-BSF transcription).
        let mut page = self.cursor_page_base;
        while page < self.total_pages {
            if self.is_free(page) {
                self.pages_free = self.pages_free.wrapping_sub(1);
                if self.pages_free == 0 {
                    self.pages_free = 1;
                    return None;
                }
                self.set_free(page, false);
                self.cursor_page_base = (page / 32) * 32;
                return Some(page << 12);
            }
            page += 1;
        }
        None
    }

    pub fn free_page(&mut self, phys: u32) {
        let page = phys >> 12;
        if page >= self.total_pages {
            return;
        }
        let was_free = self.is_free(page);
        self.set_free(page, true);
        if !was_free {
            self.pages_free = self.pages_free.wrapping_add(1);
        }
        let dword_base = (page / 32) * 32;
        if dword_base < self.cursor_page_base {
            self.cursor_page_base = dword_base;
        }
    }

    pub fn alloc_pages(&mut self, count: u32) -> Option<u32> {
        let need_bytes = count.wrapping_add(7) >> 3;
        if self.pages_free < 9 {
            return None;
        }
        let avail_bytes = (self.pages_free - 9) >> 3;
        if need_bytes > avail_bytes {
            return None;
        }
        if need_bytes == 0 {
            return None;
        }
        let start_page = self.cursor_page_base;
        let mut byte = (start_page / 8) as usize;
        let end_byte = (self.total_pages / 8) as usize;
        let need = need_bytes as usize;
        while byte < end_byte {
            if byte + need > end_byte {
                return None;
            }
            let mut fail_at: Option<usize> = None;
            for k in 0..need {
                let p0 = ((byte + k) * 8) as u32;
                let mut byte_free = true;
                for j in 0..8u32 {
                    if !self.is_free(p0 + j) {
                        byte_free = false;
                        break;
                    }
                }
                if !byte_free {
                    fail_at = Some(byte + k);
                    break;
                }
            }
            if let Some(pos) = fail_at {
                byte = pos + 1;
                continue;
            }
            for k in 0..need {
                let p0 = ((byte + k) * 8) as u32;
                for j in 0..8u32 {
                    self.set_free(p0 + j, false);
                }
            }
            self.pages_free = self.pages_free.wrapping_sub(need_bytes << 3);
            return Some((byte as u32) << 15);
        }
        None
    }

    /// Bitmap half of `release_pages`: free phys pages; **do not** update cursor.
    pub fn bitmap_release_phys_pages(&mut self, phys_pages: &[u32]) {
        let saved_cursor = self.cursor_page_base;
        for &phys in phys_pages {
            let page = phys >> 12;
            if page >= self.total_pages {
                continue;
            }
            let was_free = self.is_free(page);
            self.set_free(page, true);
            if !was_free {
                self.pages_free = self.pages_free.wrapping_add(1);
            }
        }
        self.cursor_page_base = saved_cursor;
    }

    /// Independent §19 primitive: `release_bitmap_page_without_cursor_update`.
    ///
    /// Input: page index (`phys >> 12`). Output: delta ∈ {0,1}.
    /// Does **not** update the cursor (≠ [`Self::free_page`]).
    /// Does **not** write `pages_free` (Mode A: caller batches `add ebp,eax`).
    pub fn release_bitmap_page_without_cursor_update(&mut self, page_index: u32) -> u32 {
        if page_index >= self.total_pages {
            return 0;
        }
        let was_free = self.is_free(page_index);
        self.set_free(page_index, true);
        u32::from(!was_free)
    }
}

/// Instruction-faithful FASM emulator (raw map + BSF/BTR/BTS).
#[derive(Clone, Debug)]
pub struct FasmBitmapEmu {
    pub snap: BitmapSnapshot,
}

impl FasmBitmapEmu {
    pub fn new(snap: BitmapSnapshot) -> Self {
        Self { snap }
    }

    pub fn alloc_page(&mut self) -> Option<u32> {
        let s = &mut self.snap;
        if s.pages_free <= 1 {
            s.pages_free = 1;
            return None;
        }
        let mut ebx = s.page_start_off;
        let ecx = s.page_end_off;
        while ebx < ecx {
            let word = read_u32(&s.map, ebx);
            if word != 0 {
                let bit = word.trailing_zeros();
                s.pages_free = s.pages_free.wrapping_sub(1);
                if s.pages_free == 0 {
                    s.pages_free = 1;
                    return None;
                }
                let new_word = word & !(1u32 << bit);
                write_u32(&mut s.map, ebx, new_word);
                s.page_start_off = ebx;
                let page_index = bit + (ebx as u32) * 8;
                return Some(page_index << 12);
            }
            ebx += 4;
        }
        None
    }

    pub fn free_page(&mut self, phys: u32) {
        let s = &mut self.snap;
        let mut eax = phys >> 12;
        let byte_index = (eax as usize) >> 3;
        if byte_index >= s.map.len() {
            return;
        }
        let bit = (eax & 7) as u8;
        let old = (s.map[byte_index] >> bit) & 1;
        s.map[byte_index] |= 1 << bit;
        // bts CF=old; cmc; adc → +1 iff old==0
        if old == 0 {
            s.pages_free = s.pages_free.wrapping_add(1);
        }
        eax >>= 3;
        eax &= !3;
        let dword_off = eax as usize;
        if dword_off < s.page_start_off {
            s.page_start_off = dword_off;
        }
    }

    pub fn alloc_pages(&mut self, count: u32) -> Option<u32> {
        let s = &mut self.snap;
        let count_bytes = count.wrapping_add(7) >> 3;
        if s.pages_free < 9 {
            return None;
        }
        let avail = (s.pages_free - 9) >> 3;
        if count_bytes > avail {
            return None;
        }
        if count_bytes == 0 {
            return None;
        }
        let mut ecx = s.page_start_off;
        let end = s.page_end_off;
        while ecx < end {
            let mut edx = count_bytes;
            let edi = ecx;
            let mut cur = ecx;
            let mut failed: Option<usize> = None;
            loop {
                if cur >= end {
                    // Mid-run / start past end → FASM falls into `.fail`.
                    return None;
                }
                if s.map[cur] != 0xFF {
                    failed = Some(cur);
                    break;
                }
                edx = edx.wrapping_sub(1);
                if edx == 0 {
                    let run = (cur - edi) + 1;
                    debug_assert_eq!(run, count_bytes as usize);
                    for b in edi..edi + run {
                        s.map[b] = 0;
                    }
                    s.pages_free = s.pages_free.wrapping_sub(count_bytes << 3);
                    return Some((edi as u32) << 15);
                }
                cur += 1;
            }
            // `.next: inc ecx` from the failing byte.
            ecx = failed.expect("non-FF failure") + 1;
        }
        None
    }

    pub fn bitmap_release_phys_pages(&mut self, phys_pages: &[u32]) {
        let s = &mut self.snap;
        let mut ebp = s.pages_free;
        let mut _ebx = s.page_start_off;
        for &phys in phys_pages {
            let mut eax = phys >> 12;
            let byte_index = (eax as usize) >> 3;
            if byte_index >= s.map.len() {
                continue;
            }
            let bit = (eax & 7) as u8;
            let old = (s.map[byte_index] >> bit) & 1;
            s.map[byte_index] |= 1 << bit;
            if old == 0 {
                ebp = ebp.wrapping_add(1);
            }
            eax >>= 3;
            eax &= !3;
            let dword_off = eax as usize;
            if dword_off < _ebx {
                _ebx = dword_off;
            }
        }
        s.pages_free = ebp;
        // page_start intentionally unchanged (FASM never stores ebx).
    }

    /// Mode A (legacy `release_pages` bitmap batching): accumulate into local
    /// `ebp`, write `pages_free` once; never store `page_start`.
    pub fn mode_a_release_page_indices(&mut self, page_indices: &[u32]) {
        let phys: Vec<u32> = page_indices.iter().map(|&i| i << 12).collect();
        self.bitmap_release_phys_pages(&phys);
    }

    /// FASM-faithful §19 helper (bitmap + delta only; no cursor / pages_free).
    ///
    /// Models `bts` / `cmc` / `adc eax,0` on the map. Caller owns Mode-A
    /// `pages_free` batching (`add ebp,eax`). Does **not** execute assembled
    /// FASM under Unicorn — instruction-faithful host emu only.
    pub fn release_bitmap_page_without_cursor_update(&mut self, page_index: u32) -> u32 {
        let s = &mut self.snap;
        let byte_index = (page_index as usize) >> 3;
        if byte_index >= s.map.len() {
            return 0;
        }
        let bit = (page_index & 7) as u8;
        let old = (s.map[byte_index] >> bit) & 1;
        s.map[byte_index] |= 1 << bit;
        // bts CF=OLD; cmc; adc → delta = !OLD; pages_free untouched
        u32::from(old == 0)
    }
}

fn bit_is_free(map: &[u8], page: u32) -> bool {
    let i = page as usize;
    let byte = i / 8;
    let bit = i % 8;
    if byte >= map.len() {
        return false;
    }
    ((map[byte] >> bit) & 1) == 1
}

fn set_free_bit(map: &mut [u8], page: u32, free: bool) {
    let i = page as usize;
    let byte = i / 8;
    let bit = i % 8;
    if byte >= map.len() {
        return;
    }
    if free {
        map[byte] |= 1 << bit;
    } else {
        map[byte] &= !(1 << bit);
    }
}

fn read_u32(map: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(map[off..off + 4].try_into().unwrap())
}

fn write_u32(map: &mut [u8], off: usize, v: u32) {
    map[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn sync_compare(label: &str, a: &BitmapSnapshot, b: &BitmapSnapshot) {
    assert_eq!(a.pages_free, b.pages_free, "{label}: pages_free");
    assert_eq!(a.page_start_off, b.page_start_off, "{label}: page_start_off");
    assert_eq!(a.page_end_off, b.page_end_off, "{label}: page_end_off");
    assert_eq!(
        a.map, b.map,
        "{label}: map digest a={:#x} b={:#x}",
        a.digest(),
        b.digest()
    );
}

fn run_both_alloc(snap: &BitmapSnapshot) -> (Option<u32>, BitmapSnapshot) {
    let mut ind = IndependentBitmap::from_snapshot(snap);
    let mut fasm = FasmBitmapEmu::new(snap.clone());
    let ra = ind.alloc_page();
    let rb = fasm.alloc_page();
    assert_eq!(ra, rb, "alloc_page result");
    let sa = ind.to_snapshot(snap.page_end_off);
    sync_compare("alloc_page", &sa, &fasm.snap);
    (ra, sa)
}

fn run_both_free(snap: &BitmapSnapshot, phys: u32) -> BitmapSnapshot {
    let mut ind = IndependentBitmap::from_snapshot(snap);
    let mut fasm = FasmBitmapEmu::new(snap.clone());
    ind.free_page(phys);
    fasm.free_page(phys);
    let sa = ind.to_snapshot(snap.page_end_off);
    sync_compare(&format!("free_page phys={phys:#x}"), &sa, &fasm.snap);
    sa
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
fn pgbm_oom_pages_free_le_1() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.pages_free = 1;
    let (r, sa) = run_both_alloc(&s);
    assert_eq!(r, None);
    assert_eq!(sa.pages_free, 1);
    assert_eq!(sa.map, s.map);

    s.pages_free = 0;
    let (r, sa) = run_both_alloc(&s);
    assert_eq!(r, None);
    assert_eq!(sa.pages_free, 1);
}

#[test]
fn pgbm_alloc_last_safe_page_leaves_pages_free_1() {
    let mut s = BitmapSnapshot::fresh_all_free(16);
    s.map.fill(0);
    set_free_bit(&mut s.map, 5, true);
    s.pages_free = 2;
    s.page_start_off = 0;
    let (r, sa) = run_both_alloc(&s);
    assert_eq!(r, Some(5 << 12));
    assert_eq!(sa.pages_free, 1);
    assert!(!bit_is_free(&sa.map, 5));
}

#[test]
fn pgbm_scan_miss_preserves_pages_free() {
    let mut s = BitmapSnapshot::fresh_all_free(32);
    s.map.fill(0);
    s.pages_free = 5;
    let (r, sa) = run_both_alloc(&s);
    assert_eq!(r, None);
    assert_eq!(sa.pages_free, 5);
}

#[test]
fn pgbm_bts_polarity_and_double_free() {
    let mut s = BitmapSnapshot::fresh_all_free(32);
    s.map.fill(0);
    s.pages_free = 0;
    let sa = run_both_free(&s, 3 << 12);
    assert!(bit_is_free(&sa.map, 3));
    assert_eq!(sa.pages_free, 1);
    let sb = run_both_free(&sa, 3 << 12);
    assert!(bit_is_free(&sb.map, 3));
    assert_eq!(sb.pages_free, 1);
}

#[test]
fn pgbm_free_lowers_page_start() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    s.pages_free = 0;
    s.page_start_off = 16;
    let sa = run_both_free(&s, 2 << 12);
    assert_eq!(sa.page_start_off, 0);
}

#[test]
fn pgbm_alloc_pages_ff_run() {
    let s = BitmapSnapshot::fresh_all_free(64);
    let mut ind = IndependentBitmap::from_snapshot(&s);
    let mut fasm = FasmBitmapEmu::new(s.clone());
    let ra = ind.alloc_pages(16);
    let rb = fasm.alloc_pages(16);
    assert_eq!(ra, rb);
    assert_eq!(ra, Some(0));
    let sa = ind.to_snapshot(s.page_end_off);
    sync_compare("alloc_pages 16", &sa, &fasm.snap);
    assert_eq!(sa.pages_free, 512 - 16);
    assert_eq!(sa.page_start_off, 0);
    assert_eq!(sa.map[0], 0);
    assert_eq!(sa.map[1], 0);
    assert_eq!(sa.map[2], 0xFF);
}

#[test]
fn pgbm_alloc_pages_zero_and_guard() {
    let s = BitmapSnapshot::fresh_all_free(64);
    let mut ind = IndependentBitmap::from_snapshot(&s);
    let mut fasm = FasmBitmapEmu::new(s.clone());
    assert_eq!(ind.alloc_pages(0), None);
    assert_eq!(fasm.alloc_pages(0), None);

    let mut s2 = s;
    s2.pages_free = 8;
    let mut ind = IndependentBitmap::from_snapshot(&s2);
    let mut fasm = FasmBitmapEmu::new(s2);
    assert_eq!(ind.alloc_pages(8), None);
    assert_eq!(fasm.alloc_pages(8), None);
}

#[test]
fn pgbm_release_does_not_update_page_start() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    s.pages_free = 0;
    s.page_start_off = 32;
    let pages = [1u32 << 12, 2 << 12];
    let mut ind = IndependentBitmap::from_snapshot(&s);
    let mut fasm = FasmBitmapEmu::new(s.clone());
    ind.bitmap_release_phys_pages(&pages);
    fasm.bitmap_release_phys_pages(&pages);
    let sa = ind.to_snapshot(s.page_end_off);
    sync_compare("release_pages bitmap", &sa, &fasm.snap);
    assert_eq!(sa.page_start_off, 32);
    assert_eq!(sa.pages_free, 2);

    let via_free = run_both_free(&s, 1 << 12);
    let via_free = run_both_free(&via_free, 2 << 12);
    assert_eq!(via_free.page_start_off, 0);
    assert_ne!(via_free.page_start_off, sa.page_start_off);
}

#[test]
fn pgbm_cursor_skips_pages_below_page_start() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    set_free_bit(&mut s.map, 1, true);
    set_free_bit(&mut s.map, 40, true);
    s.pages_free = 2;
    s.page_start_off = 4;
    let (r, sa) = run_both_alloc(&s);
    assert_eq!(r, Some(40 << 12));
    assert!(bit_is_free(&sa.map, 1));
}

#[test]
fn pgbm_prng_50000_differential() {
    let mut rng = PG_BITMAP_PRNG_SEED;
    // 512 B ⇒ 4096 pages: enough coverage without multi-second rebuilds.
    let map_bytes = 512;
    let mut snap = BitmapSnapshot::fresh_all_free(map_bytes);
    for b in &mut snap.map {
        *b = (xorshift32(&mut rng) & 0xFF) as u8;
    }
    let mut free = 0u32;
    for i in 0..snap.total_pages() {
        if bit_is_free(&snap.map, i) {
            free += 1;
        }
    }
    snap.pages_free = free;
    snap.page_start_off = ((xorshift32(&mut rng) as usize) % (map_bytes / 4)) * 4;
    snap.page_end_off = map_bytes;

    let mut ind = IndependentBitmap::from_snapshot(&snap);
    let mut fasm = FasmBitmapEmu::new(snap);

    for case in 0..50_000u32 {
        let op = xorshift32(&mut rng) % 5;
        match op {
            0 => {
                let ra = ind.alloc_page();
                let rb = fasm.alloc_page();
                assert_eq!(ra, rb, "case {case} alloc_page");
            }
            1 => {
                let phys = (xorshift32(&mut rng) % fasm.snap.total_pages()) << 12;
                ind.free_page(phys);
                fasm.free_page(phys);
            }
            2 => {
                let n = xorshift32(&mut rng) % 64;
                let ra = ind.alloc_pages(n);
                let rb = fasm.alloc_pages(n);
                assert_eq!(ra, rb, "case {case} alloc_pages n={n}");
            }
            3 => {
                let n = 1 + (xorshift32(&mut rng) % 8);
                let mut pages = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    pages.push((xorshift32(&mut rng) % fasm.snap.total_pages()) << 12);
                }
                ind.bitmap_release_phys_pages(&pages);
                fasm.bitmap_release_phys_pages(&pages);
            }
            _ => {
                let off = ((xorshift32(&mut rng) as usize) % (map_bytes / 4)) * 4;
                fasm.snap.page_start_off = off;
                ind.cursor_page_base = ((off as u32) / 4) * 32;
            }
        }
        if case % 1024 == 0 {
            sync_compare(
                &format!("case {case} checkpoint"),
                &ind.to_snapshot(map_bytes),
                &fasm.snap,
            );
        }
    }
    sync_compare("final", &ind.to_snapshot(map_bytes), &fasm.snap);
}

fn run_both_s19(snap: &BitmapSnapshot, page_index: u32) -> (u32, u32, BitmapSnapshot) {
    let mut ind = IndependentBitmap::from_snapshot(snap);
    let mut fasm = FasmBitmapEmu::new(snap.clone());
    let cursor_before = snap.page_start_off;
    let pf_before = snap.pages_free;
    let da = ind.release_bitmap_page_without_cursor_update(page_index);
    let db = fasm.release_bitmap_page_without_cursor_update(page_index);
    assert_eq!(da, db, "s19 delta page={page_index}");
    let sa = ind.to_snapshot(snap.page_end_off);
    sync_compare(&format!("s19 page={page_index}"), &sa, &fasm.snap);
    assert_eq!(sa.page_start_off, cursor_before, "s19 page_start invariant");
    assert_eq!(sa.pages_free, pf_before, "s19 pages_free untouched by helper");
    (da, db, sa)
}

#[test]
fn pgbm_s19_allocated_and_already_free() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    s.pages_free = 0;
    s.page_start_off = 16;
    let (d1, _, sa) = run_both_s19(&s, 3);
    assert_eq!(d1, 1);
    assert!(bit_is_free(&sa.map, 3));
    assert_eq!(sa.page_start_off, 16);
    let (d2, _, sb) = run_both_s19(&sa, 3);
    assert_eq!(d2, 0);
    assert_eq!(sb.pages_free, sa.pages_free);
    assert_eq!(sb.page_start_off, 16);
}

#[test]
fn pgbm_s19_cursor_relative_pages() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    s.pages_free = 0;
    s.page_start_off = 16; // dword covering pages 128..159
    for page in [2u32, 128, 200] {
        let (d, _, sa) = run_both_s19(&s, page);
        assert_eq!(d, 1, "page {page}");
        assert_eq!(sa.page_start_off, 16);
        s = sa;
    }
}

#[test]
fn pgbm_s19_first_last_and_dword_boundary() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    s.pages_free = 0;
    s.page_start_off = 8;
    let last = s.total_pages() - 1;
    for page in [0u32, 31, 32, last] {
        let (d, _, sa) = run_both_s19(&s, page);
        assert_eq!(d, 1, "boundary page {page}");
        assert_eq!(sa.page_start_off, 8);
        s = sa;
    }
}

#[test]
fn pgbm_s19_pages_free_wrapping() {
    // Helper leaves pages_free untouched; Mode-A caller wraps via add ebp,eax.
    let mut s = BitmapSnapshot::fresh_all_free(16);
    s.map.fill(0);
    s.pages_free = u32::MAX;
    s.page_start_off = 0;
    let (d, _, sa) = run_both_s19(&s, 1);
    assert_eq!(d, 1);
    assert_eq!(sa.pages_free, u32::MAX);
    assert_eq!(sa.pages_free.wrapping_add(d), 0);
    assert_eq!(sa.page_start_off, 0);
}

#[test]
fn pgbm_s19_ne_free_page_for_cursor() {
    let mut s = BitmapSnapshot::fresh_all_free(64);
    s.map.fill(0);
    s.pages_free = 0;
    s.page_start_off = 32;
    let (_, _, via_s19) = run_both_s19(&s, 1);
    assert_eq!(via_s19.page_start_off, 32);
    let via_free = run_both_free(&s, 1 << 12);
    assert_eq!(via_free.page_start_off, 0);
    assert_ne!(via_s19.page_start_off, via_free.page_start_off);
}

#[test]
fn pgbm_s19_mode_a_vs_mode_b_end_state() {
    // Mode A: batched release_pages bitmap half (one pages_free store).
    // Mode B: N× §19 helpers + caller-side pages_free += delta (same end state).
    let mut rng = PG_BITMAP_PRNG_SEED ^ 0xA5A5_5A5A;
    for trial in 0..200u32 {
        let map_bytes = 64;
        let mut snap = BitmapSnapshot::fresh_all_free(map_bytes);
        for b in &mut snap.map {
            *b = (xorshift32(&mut rng) & 0xFF) as u8;
        }
        let mut free = 0u32;
        for i in 0..snap.total_pages() {
            if bit_is_free(&snap.map, i) {
                free += 1;
            }
        }
        snap.pages_free = free;
        snap.page_start_off = ((xorshift32(&mut rng) as usize) % (map_bytes / 4)) * 4;

        let n = 1 + (xorshift32(&mut rng) % 12);
        let mut indices = Vec::with_capacity(n as usize);
        for _ in 0..n {
            indices.push(xorshift32(&mut rng) % snap.total_pages());
        }

        let mut mode_a = FasmBitmapEmu::new(snap.clone());
        mode_a.mode_a_release_page_indices(&indices);

        let mut mode_b = FasmBitmapEmu::new(snap.clone());
        let mut ind_b = IndependentBitmap::from_snapshot(&snap);
        let mut deltas_f = Vec::new();
        let mut deltas_i = Vec::new();
        for &idx in &indices {
            let df = mode_b.release_bitmap_page_without_cursor_update(idx);
            let di = ind_b.release_bitmap_page_without_cursor_update(idx);
            mode_b.snap.pages_free = mode_b.snap.pages_free.wrapping_add(df);
            ind_b.pages_free = ind_b.pages_free.wrapping_add(di);
            deltas_f.push(df);
            deltas_i.push(di);
        }
        assert_eq!(deltas_f, deltas_i, "trial {trial} per-page deltas");
        sync_compare(
            &format!("trial {trial} modeB ind vs fasm"),
            &ind_b.to_snapshot(map_bytes),
            &mode_b.snap,
        );
        sync_compare(
            &format!("trial {trial} modeA vs modeB"),
            &mode_a.snap,
            &mode_b.snap,
        );
        assert_eq!(mode_a.snap.page_start_off, snap.page_start_off);
        assert_eq!(mode_b.snap.page_start_off, snap.page_start_off);
    }
}

#[test]
fn pgbm_s19_prng_50000() {
    let mut rng = PG_BITMAP_PRNG_SEED;
    let map_bytes = 512;
    let mut snap = BitmapSnapshot::fresh_all_free(map_bytes);
    for b in &mut snap.map {
        *b = (xorshift32(&mut rng) & 0xFF) as u8;
    }
    let mut free = 0u32;
    for i in 0..snap.total_pages() {
        if bit_is_free(&snap.map, i) {
            free += 1;
        }
    }
    snap.pages_free = free;
    snap.page_start_off = ((xorshift32(&mut rng) as usize) % (map_bytes / 4)) * 4;

    let mut ind = IndependentBitmap::from_snapshot(&snap);
    let mut fasm = FasmBitmapEmu::new(snap);

    for case in 0..50_000u32 {
        let page = xorshift32(&mut rng) % fasm.snap.total_pages();
        let cursor_before = fasm.snap.page_start_off;
        let pf_before = fasm.snap.pages_free;
        let da = ind.release_bitmap_page_without_cursor_update(page);
        let db = fasm.release_bitmap_page_without_cursor_update(page);
        assert_eq!(da, db, "s19 prng case {case} delta");
        assert_eq!(fasm.snap.page_start_off, cursor_before);
        assert_eq!(fasm.snap.pages_free, pf_before, "helper must not write pages_free");
        // Mix: occasionally prove free_page still lowers cursor independently.
        if case % 17 == 0 {
            let p2 = xorshift32(&mut rng) % fasm.snap.total_pages();
            // Allocate bit first so free_page has something to do on a clone path —
            // only compare s19 path for sync; free_page on both would diverge cursors
            // from s19-only runs. Skip free_page in this loop.
            let _ = p2;
        }
        if case % 2048 == 0 {
            sync_compare(
                &format!("s19 prng case {case}"),
                &ind.to_snapshot(map_bytes),
                &fasm.snap,
            );
        }
    }
    sync_compare("s19 prng final", &ind.to_snapshot(map_bytes), &fasm.snap);
}
