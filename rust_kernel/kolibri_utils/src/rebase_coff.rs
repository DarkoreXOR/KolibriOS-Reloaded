//! Cut CH: `rebase_coff` — preferred-base DIR32 rebase in-place.
//!
//! Matches `kernel/core/dll.inc` FASM `proc rebase_coff stdcall`:
//! * walk `COFF_HEADER.nSections` section headers starting at `coff+20`
//! * for each reloc of type 6 (DIR32) only, patch a dword
//! * other reloc types are skipped
//! * addend is **`delta`** (not symbol Value); `sym` is unused
//! * patch address = `reloc.VA + sec.VA + delta` → `add [addr], delta`
//!
//! No tables / `.rodata`. Freestanding path uses only raw pointer arithmetic.
//!
//! Legacy FASM enters `.fix_sec` before testing `nSections` (do-while quirk).
//! Rust uses a while-guard (clean no-op on 0). Production callers never pass 0.

/// Cut CH differential PRNG seed (`'RBCF'`).
pub const REBASE_COFF_PRNG_SEED: u32 = 0x5242_4346;

/// `sizeof.COFF_HEADER` — sections array begins immediately after.
pub const COFF_HEADER_SIZE: usize = 20;

/// Offset of `COFF_HEADER.nSections` (u16).
pub const OFF_N_SECTIONS: usize = 2;

/// `sizeof.COFF_SECTION`.
pub const COFF_SECTION_SIZE: usize = 40;

/// `COFF_SECTION.VirtualAddress` offset.
pub const OFF_SEC_VIRTUAL_ADDRESS: usize = 12;

/// `COFF_SECTION.PtrReloc` offset (file/image-relative).
pub const OFF_SEC_PTR_RELOC: usize = 24;

/// `COFF_SECTION.NumReloc` offset (u16).
pub const OFF_SEC_NUM_RELOC: usize = 32;

/// `sizeof.COFF_RELOC`.
pub const COFF_RELOC_SIZE: usize = 10;

/// `COFF_RELOC.VirtualAddress` offset.
pub const OFF_RELOC_VA: usize = 0;

/// `COFF_RELOC.Type` offset (u16).
pub const OFF_RELOC_TYPE: usize = 8;

/// IMAGE_REL_I386_DIR32.
pub const RELOC_TYPE_DIR32: u16 = 6;

#[inline(always)]
unsafe fn read_u16(p: *const u8) -> u16 {
    unsafe { u16::from_le_bytes([*p, *p.add(1)]) }
}

#[inline(always)]
unsafe fn read_u32(p: *const u8) -> u32 {
    unsafe { u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) }
}

#[inline(always)]
unsafe fn patch_add_u32(addr: u32, addend: u32) {
    let p = addr as *mut u32;
    unsafe {
        let cur = core::ptr::read_unaligned(p);
        core::ptr::write_unaligned(p, cur.wrapping_add(addend));
    }
}

/// FASM-faithful preferred-base DIR32 rebase.
///
/// # Safety
/// `coff` must point at a readable `COFF_HEADER` followed by `nSections`
/// section headers; each section's reloc table (`PtrReloc` relative to `coff`)
/// must be readable; every computed patch address `VA + sec.VA + delta` must
/// be a writable dword. `sym` is unused (ABI parity with FASM/`load_library`).
#[inline(always)]
pub unsafe fn rebase_coff(coff: *mut u8, _sym: *const u8, delta: u32) {
    let mut n_sec = unsafe { read_u16(coff.add(OFF_N_SECTIONS)) } as u32;
    let mut sec = unsafe { coff.add(COFF_HEADER_SIZE) };
    while n_sec != 0 {
        let ptr_reloc = unsafe { read_u32(sec.add(OFF_SEC_PTR_RELOC)) };
        let mut reloc = unsafe { coff.add(ptr_reloc as usize) };
        let mut num_reloc = unsafe { read_u16(sec.add(OFF_SEC_NUM_RELOC)) } as u32;
        let sec_va = unsafe { read_u32(sec.add(OFF_SEC_VIRTUAL_ADDRESS)) };
        while num_reloc != 0 {
            let rtype = unsafe { read_u16(reloc.add(OFF_RELOC_TYPE)) };
            if rtype == RELOC_TYPE_DIR32 {
                let mut eax = unsafe { read_u32(reloc.add(OFF_RELOC_VA)) };
                eax = eax.wrapping_add(sec_va);
                // FASM: add [eax + edx], edx  with edx = delta
                unsafe { patch_add_u32(eax.wrapping_add(delta), delta) };
            }
            reloc = unsafe { reloc.add(COFF_RELOC_SIZE) };
            num_reloc = num_reloc.wrapping_sub(1);
        }
        sec = unsafe { sec.add(COFF_SECTION_SIZE) };
        n_sec = n_sec.wrapping_sub(1);
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`rebase_coff`].
#[inline(always)]
pub unsafe fn rebase_coff_ptr(coff: *mut u8, sym: *const u8, delta: u32) {
    unsafe { rebase_coff(coff, sym, delta) }
}

/// Buffered FASM-flow helper / host differential path.
///
/// Interprets all computed patch addresses as **offsets into `image`**.
/// Test corpora must place `VA + sec.VA + delta` inside `image`.
/// Control flow matches production [`rebase_coff`] exactly.
pub fn rebase_coff_buffered(image: &mut [u8], coff_off: usize, delta: u32) {
    fn ru16(img: &[u8], off: usize) -> u16 {
        u16::from_le_bytes([img[off], img[off + 1]])
    }
    fn ru32(img: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([img[off], img[off + 1], img[off + 2], img[off + 3]])
    }
    fn patch(img: &mut [u8], addr: u32, addend: u32) {
        let off = addr as usize;
        let cur = ru32(img, off);
        let bytes = cur.wrapping_add(addend).to_le_bytes();
        img[off..off + 4].copy_from_slice(&bytes);
    }

    let mut n_sec = ru16(image, coff_off + OFF_N_SECTIONS) as u32;
    let mut sec = coff_off + COFF_HEADER_SIZE;
    while n_sec != 0 {
        let ptr_reloc = ru32(image, sec + OFF_SEC_PTR_RELOC);
        let mut reloc = coff_off.wrapping_add(ptr_reloc as usize);
        let mut num_reloc = ru16(image, sec + OFF_SEC_NUM_RELOC) as u32;
        let sec_va = ru32(image, sec + OFF_SEC_VIRTUAL_ADDRESS);
        while num_reloc != 0 {
            let rtype = ru16(image, reloc + OFF_RELOC_TYPE);
            if rtype == RELOC_TYPE_DIR32 {
                let mut eax = ru32(image, reloc + OFF_RELOC_VA);
                eax = eax.wrapping_add(sec_va);
                patch(image, eax.wrapping_add(delta), delta);
            }
            reloc = reloc.wrapping_add(COFF_RELOC_SIZE);
            num_reloc = num_reloc.wrapping_sub(1);
        }
        sec = sec.wrapping_add(COFF_SECTION_SIZE);
        n_sec = n_sec.wrapping_sub(1);
    }
}

/// Independent FASM-flow oracle (duplicate control flow for differential).
#[cfg(test)]
fn rebase_coff_oracle(image: &mut [u8], coff_off: usize, delta: u32) {
    // Intentionally re-derived from dll.inc, not a call to rebase_coff_buffered.
    fn ru16(img: &[u8], off: usize) -> u16 {
        u16::from_le_bytes([img[off], img[off + 1]])
    }
    fn ru32(img: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([img[off], img[off + 1], img[off + 2], img[off + 3]])
    }
    fn patch(img: &mut [u8], addr: u32, addend: u32) {
        let off = addr as usize;
        let cur = u32::from_le_bytes([img[off], img[off + 1], img[off + 2], img[off + 3]]);
        let bytes = cur.wrapping_add(addend).to_le_bytes();
        img[off..off + 4].copy_from_slice(&bytes);
    }

    // FASM: edx = delta held for whole walk
    let edx = delta;
    let mut n_sec = ru16(image, coff_off + 2) as u32;
    let mut sec = coff_off + 20;
    // while-guard (Rust); FASM do-while enters once even when n_sec==0
    while n_sec != 0 {
        let mut edi = coff_off.wrapping_add(ru32(image, sec + 24) as usize);
        let mut ecx = ru16(image, sec + 32) as u32;
        let sec_va = ru32(image, sec + 12);
        while ecx != 0 {
            let ty = ru16(image, edi + 8);
            if ty == 6 {
                let mut eax = ru32(image, edi);
                eax = eax.wrapping_add(sec_va);
                // add [eax + edx], edx
                patch(image, eax.wrapping_add(edx), edx);
            }
            edi = edi.wrapping_add(10);
            ecx = ecx.wrapping_sub(1);
        }
        sec = sec.wrapping_add(40);
        n_sec = n_sec.wrapping_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layout: header + N sections + reloc area + data area.
    struct Img {
        buf: Vec<u8>,
        coff: usize,
        data: usize,
    }

    impl Img {
        fn new(n_sec: u16, data_bytes: usize) -> Self {
            let coff = 0usize;
            let sec_end = 20 + 40 * n_sec as usize;
            let reloc_area = sec_end;
            let data = reloc_area + 256;
            let len = data + data_bytes;
            let mut buf = vec![0u8; len];
            buf[2] = (n_sec & 0xff) as u8;
            buf[3] = (n_sec >> 8) as u8;
            Self { buf, coff, data }
        }

        fn set_sec(&mut self, idx: usize, va: u32, ptr_reloc: u32, num_reloc: u16) {
            let s = self.coff + 20 + idx * 40;
            self.buf[s + 12..s + 16].copy_from_slice(&va.to_le_bytes());
            self.buf[s + 24..s + 28].copy_from_slice(&ptr_reloc.to_le_bytes());
            self.buf[s + 32..s + 34].copy_from_slice(&num_reloc.to_le_bytes());
        }

        fn write_reloc(&mut self, off_from_coff: u32, va: u32, ty: u16) {
            let o = self.coff + off_from_coff as usize;
            self.buf[o..o + 4].copy_from_slice(&va.to_le_bytes());
            // SymIndex unused by rebase — leave 0
            self.buf[o + 8..o + 10].copy_from_slice(&ty.to_le_bytes());
        }

        fn set_data_u32(&mut self, off: usize, v: u32) {
            let o = self.data + off;
            self.buf[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }

        fn get_data_u32(&self, off: usize) -> u32 {
            let o = self.data + off;
            u32::from_le_bytes([
                self.buf[o],
                self.buf[o + 1],
                self.buf[o + 2],
                self.buf[o + 3],
            ])
        }

        fn run_pair(&mut self, delta: u32) -> (Vec<u8>, Vec<u8>) {
            let mut rust_img = self.buf.clone();
            let mut ora_img = self.buf.clone();
            rebase_coff_buffered(&mut rust_img, self.coff, delta);
            rebase_coff_oracle(&mut ora_img, self.coff, delta);
            (rust_img, ora_img)
        }
    }

    #[test]
    fn rbcf_empty_sections_noop() {
        let mut img = Img::new(0, 16);
        let (a, b) = img.run_pair(0x1000);
        assert_eq!(a, b);
        assert_eq!(a, img.buf);
    }

    #[test]
    fn rbcf_zero_relocs_noop() {
        let mut img = Img::new(1, 16);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 0);
        img.set_data_u32(0, 0x1111_2222);
        let (a, b) = img.run_pair(0x40);
        assert_eq!(a, b);
        assert_eq!(img.get_data_u32(0), 0x1111_2222);
        // also via buffered on original
        let mut c = img.buf.clone();
        rebase_coff_buffered(&mut c, img.coff, 0x40);
        assert_eq!(c, img.buf);
    }

    #[test]
    fn rbcf_dir32_adds_delta() {
        // Layout: reloc.VA = data+0x20 - delta; patch lands at data+0x20.
        let mut img = Img::new(1, 64);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        let delta = 0x20u32;
        let patch_off = img.data + 0x20;
        let va = (patch_off as u32).wrapping_sub(delta);
        img.write_reloc(reloc_off, va, RELOC_TYPE_DIR32);
        img.set_data_u32(0x20, 0x100);
        let (a, b) = img.run_pair(delta);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[patch_off..patch_off + 4].try_into().unwrap()),
            0x120
        );

        // delta=0: VA = patch address; addend 0 → unchanged
        let mut img = Img::new(1, 32);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        img.write_reloc(reloc_off, img.data as u32, RELOC_TYPE_DIR32);
        img.set_data_u32(0, 0x10);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[img.data..img.data + 4].try_into().unwrap()),
            0x10
        );
    }

    #[test]
    fn rbcf_skips_non_dir32() {
        let mut img = Img::new(1, 32);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        let va = img.data as u32;
        img.write_reloc(reloc_off, va, 20); // REL32 — must skip
        img.set_data_u32(0, 0xABCD_0000);
        let (a, b) = img.run_pair(0x10);
        assert_eq!(a, b);
        let mut work = img.buf.clone();
        rebase_coff_buffered(&mut work, img.coff, 0x10);
        assert_eq!(
            u32::from_le_bytes(work[img.data..img.data + 4].try_into().unwrap()),
            0xABCD_0000
        );
    }

    #[test]
    fn rbcf_multi_section() {
        let mut img = Img::new(2, 64);
        let reloc0 = (20 + 80) as u32;
        let reloc1 = reloc0 + 10;
        let delta = 4u32;
        img.set_sec(0, 0, reloc0, 1);
        img.set_sec(1, 0, reloc1, 1);
        // Patch targets at data+4 and data+12 → VA = target - delta
        let p0 = (img.data + 4) as u32;
        let p1 = (img.data + 12) as u32;
        img.write_reloc(reloc0, p0.wrapping_sub(delta), 6);
        img.write_reloc(reloc1, p1.wrapping_sub(delta), 6);
        img.set_data_u32(4, 10);
        img.set_data_u32(12, 20);
        let (a, b) = img.run_pair(delta);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[img.data + 4..img.data + 8].try_into().unwrap()),
            14
        );
        assert_eq!(
            u32::from_le_bytes(a[img.data + 12..img.data + 16].try_into().unwrap()),
            24
        );
    }

    #[test]
    fn rbcf_sec_va_added() {
        let mut img = Img::new(1, 64);
        let reloc_off = (20 + 40) as u32;
        // sec.VA = 8; reloc.VA = data; delta = 0 → patch at data+8
        img.set_sec(0, 8, reloc_off, 1);
        img.write_reloc(reloc_off, img.data as u32, 6);
        img.set_data_u32(8, 0x50);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[img.data + 8..img.data + 12].try_into().unwrap()),
            0x50
        );
        // with delta=4 → patch data+12; VA = data so addr = data+8+4
        img.set_data_u32(12, 0x70);
        let (a, b) = img.run_pair(4);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[img.data + 12..img.data + 16].try_into().unwrap()),
            0x74
        );
    }

    #[test]
    fn rbcf_unused_sym_ignored() {
        // Production API accepts sym; buffered path has no sym — prove oracle match
        // with VA = patch - delta so the addend lands on a known dword.
        let mut img = Img::new(1, 0x120);
        let reloc_off = (20 + 40) as u32;
        let delta = 0x100u32;
        img.set_sec(0, 0, reloc_off, 1);
        let patch_off = img.data + 0x100;
        let va = (patch_off as u32).wrapping_sub(delta);
        img.write_reloc(reloc_off, va, 6);
        img.set_data_u32(0x100, 7);
        let (a, b) = img.run_pair(delta);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[patch_off..patch_off + 4].try_into().unwrap()),
            0x107
        );
    }

    #[test]
    fn rbcf_wrapping_add() {
        let mut img = Img::new(1, 0x30);
        let reloc_off = (20 + 40) as u32;
        let delta = 0x20u32;
        img.set_sec(0, 0, reloc_off, 1);
        let patch_off = img.data + 0x20;
        let va = (patch_off as u32).wrapping_sub(delta);
        img.write_reloc(reloc_off, va, 6);
        img.set_data_u32(0x20, 0xFFFF_FFF0);
        let (a, b) = img.run_pair(delta);
        assert_eq!(a, b);
        assert_eq!(
            u32::from_le_bytes(a[patch_off..patch_off + 4].try_into().unwrap()),
            0xFFFF_FFF0u32.wrapping_add(0x20)
        );
    }

    /// Arena PRNG: 50_000 cases vs independent oracle.
    #[test]
    fn rbcf_prng_50000_vs_oracle() {
        let mut rng = REBASE_COFF_PRNG_SEED;
        fn next(r: &mut u32) -> u32 {
            // xorshift32
            let mut x = *r;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *r = x;
            x
        }

        for _ in 0..50_000 {
            let n_sec = (next(&mut rng) % 3 + 1) as u16; // 1..3
            let delta = (next(&mut rng) % 16) * 4; // 0..60 step 4
            let data_bytes = 512usize;
            let mut img = Img::new(n_sec, data_bytes);
            let reloc_base = (20 + 40 * n_sec as usize) as u32;
            let mut reloc_cursor = reloc_base;
            for s in 0..n_sec as usize {
                let num = (next(&mut rng) % 3) as u16; // 0..2
                let sec_va = (next(&mut rng) % 8) * 4;
                img.set_sec(s, sec_va, reloc_cursor, num);
                for _ in 0..num {
                    let ty = if next(&mut rng) & 1 == 0 { 6u16 } else { 20 };
                    // Choose patch slot inside data[0..data_bytes-4], then
                    // VA = patch - sec_va - delta so patch addr stays in-bounds.
                    let slot = (next(&mut rng) % ((data_bytes as u32 - 4) / 4)) * 4;
                    let patch = img.data as u32 + slot;
                    let va = patch
                        .wrapping_sub(sec_va)
                        .wrapping_sub(delta);
                    img.write_reloc(reloc_cursor, va, ty);
                    reloc_cursor += 10;
                }
            }
            // Seed data area with PRNG bytes
            for i in (0..data_bytes).step_by(4) {
                img.set_data_u32(i, next(&mut rng));
            }
            let (a, b) = img.run_pair(delta);
            assert_eq!(a, b, "oracle mismatch seed-derived case");
        }
    }
}
