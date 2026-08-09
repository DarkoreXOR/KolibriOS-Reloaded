//! Cut Y: `fix_coff_relocs` — apply COFF DIR32/REL32 relocations in-place.
//!
//! Matches `kernel/core/dll.inc` FASM `proc fix_coff_relocs stdcall`:
//! * walk `COFF_HEADER.nSections` section headers starting at `coff+20`
//! * for each reloc of type 6 (DIR32) or 20 (REL32), patch a dword
//! * other reloc types are skipped
//!
//! No tables / `.rodata`. Freestanding path uses only raw pointer arithmetic.

/// Cut Y differential PRNG seed (`'CUTY'`).
pub const FIX_COFF_RELOCS_PRNG_SEED: u32 = 0x4355_5459;

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

/// `COFF_RELOC.SymIndex` offset.
pub const OFF_RELOC_SYM_INDEX: usize = 4;

/// `COFF_RELOC.Type` offset (u16).
pub const OFF_RELOC_TYPE: usize = 8;

/// `sizeof.COFF_SYM` — FASM: `SymIndex*2 + SymIndex*8` via `lea`.
pub const COFF_SYM_SIZE: usize = 18;

/// `COFF_SYM.Value` offset.
pub const OFF_SYM_VALUE: usize = 8;

/// IMAGE_REL_I386_DIR32.
pub const RELOC_TYPE_DIR32: u16 = 6;

/// IMAGE_REL_I386_REL32.
pub const RELOC_TYPE_REL32: u16 = 20;

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

/// FASM-faithful COFF reloc application.
///
/// # Safety
/// `coff` must point at a readable `COFF_HEADER` followed by `nSections`
/// section headers; each section's reloc table (`PtrReloc` relative to `coff`)
/// must be readable; `sym` must cover every referenced `SymIndex`; every
/// computed patch address `VA + sec.VA + delta` must be a writable dword.
#[inline(always)]
pub unsafe fn fix_coff_relocs(coff: *mut u8, sym: *const u8, delta: u32) {
    let mut n_sec = unsafe { read_u16(coff.add(OFF_N_SECTIONS)) } as u32;
    let mut sec = unsafe { coff.add(COFF_HEADER_SIZE) };
    while n_sec != 0 {
        let ptr_reloc = unsafe { read_u32(sec.add(OFF_SEC_PTR_RELOC)) };
        let mut reloc = unsafe { coff.add(ptr_reloc as usize) };
        let mut num_reloc = unsafe { read_u16(sec.add(OFF_SEC_NUM_RELOC)) } as u32;
        let sec_va = unsafe { read_u32(sec.add(OFF_SEC_VIRTUAL_ADDRESS)) };
        while num_reloc != 0 {
            let sym_index = unsafe { read_u32(reloc.add(OFF_RELOC_SYM_INDEX)) };
            // FASM: add ebx,ebx / lea ebx,[ebx+ebx*8] → *18
            let sym_ptr = unsafe { sym.add((sym_index as usize) * COFF_SYM_SIZE) };
            let mut value = unsafe { read_u32(sym_ptr.add(OFF_SYM_VALUE)) };
            let rtype = unsafe { read_u16(reloc.add(OFF_RELOC_TYPE)) };
            if rtype == RELOC_TYPE_DIR32 {
                let mut eax = unsafe { read_u32(reloc.add(OFF_RELOC_VA)) };
                eax = eax.wrapping_add(sec_va);
                eax = eax.wrapping_add(delta);
                unsafe { patch_add_u32(eax, value) };
            } else if rtype == RELOC_TYPE_REL32 {
                let mut eax = unsafe { read_u32(reloc.add(OFF_RELOC_VA)) };
                eax = eax.wrapping_add(sec_va);
                value = value.wrapping_sub(eax).wrapping_sub(4);
                eax = eax.wrapping_add(delta);
                unsafe { patch_add_u32(eax, value) };
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
/// Same as [`fix_coff_relocs`].
#[inline(always)]
pub unsafe fn fix_coff_relocs_ptr(coff: *mut u8, sym: *const u8, delta: u32) {
    unsafe { fix_coff_relocs(coff, sym, delta) }
}

/// Buffered FASM-flow oracle / host differential helper.
///
/// Interprets all computed patch addresses as **offsets into `image`**.
/// Test corpora must place `VA + sec.VA + delta` inside `image`.
/// Control flow matches production [`fix_coff_relocs`] exactly.
pub fn fix_coff_relocs_buffered(image: &mut [u8], coff_off: usize, sym_off: usize, delta: u32) {
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
            let sym_index = ru32(image, reloc + OFF_RELOC_SYM_INDEX);
            let sym_ptr = sym_off.wrapping_add((sym_index as usize) * COFF_SYM_SIZE);
            let mut value = ru32(image, sym_ptr + OFF_SYM_VALUE);
            let rtype = ru16(image, reloc + OFF_RELOC_TYPE);
            if rtype == RELOC_TYPE_DIR32 {
                let mut eax = ru32(image, reloc + OFF_RELOC_VA);
                eax = eax.wrapping_add(sec_va);
                eax = eax.wrapping_add(delta);
                patch(image, eax, value);
            } else if rtype == RELOC_TYPE_REL32 {
                let mut eax = ru32(image, reloc + OFF_RELOC_VA);
                eax = eax.wrapping_add(sec_va);
                value = value.wrapping_sub(eax).wrapping_sub(4);
                eax = eax.wrapping_add(delta);
                patch(image, eax, value);
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
fn oracle_buffered(image: &mut [u8], coff_off: usize, sym_off: usize, delta: u32) {
    // Intentionally re-derived from dll.inc, not a call to fix_coff_relocs_buffered.
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

    let mut n_sec = ru16(image, coff_off + 2) as u32;
    let mut sec = coff_off + 20;
    while n_sec != 0 {
        let edi = coff_off.wrapping_add(ru32(image, sec + 24) as usize);
        let mut reloc = edi;
        let mut ecx = ru16(image, sec + 32) as u32;
        let sec_va = ru32(image, sec + 12);
        while ecx != 0 {
            let sym_index = ru32(image, reloc + 4);
            let ebx = sym_off.wrapping_add((sym_index as usize) * 18);
            let mut edx = ru32(image, ebx + 8);
            let ty = ru16(image, reloc + 8);
            if ty == 6 {
                let mut eax = ru32(image, reloc);
                eax = eax.wrapping_add(sec_va);
                eax = eax.wrapping_add(delta);
                patch(image, eax, edx);
            } else if ty == 20 {
                let mut eax = ru32(image, reloc);
                eax = eax.wrapping_add(sec_va);
                edx = edx.wrapping_sub(eax).wrapping_sub(4);
                eax = eax.wrapping_add(delta);
                patch(image, eax, edx);
            }
            reloc = reloc.wrapping_add(10);
            ecx = ecx.wrapping_sub(1);
        }
        sec = sec.wrapping_add(40);
        n_sec = n_sec.wrapping_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layout helper: one header + N sections + reloc blob + sym table + data area.
    /// Returns (coff_off=0, sym_off, data_base_off).
    struct Img {
        buf: Vec<u8>,
        coff: usize,
        sym: usize,
        data: usize,
    }

    impl Img {
        fn new(n_sec: u16, n_sym: u32, data_bytes: usize) -> Self {
            // [header 20][sections 40*n][relocs reserved 256][syms 18*n][data]
            let coff = 0usize;
            let sec_end = 20 + 40 * n_sec as usize;
            let reloc_area = sec_end;
            let _reloc_cap = 256usize;
            let sym = reloc_area + 256;
            let data = sym + 18 * n_sym as usize;
            let len = data + data_bytes;
            let mut buf = vec![0u8; len];
            buf[2] = (n_sec & 0xff) as u8;
            buf[3] = (n_sec >> 8) as u8;
            // nSymbols
            buf[12..16].copy_from_slice(&n_sym.to_le_bytes());
            Self {
                buf,
                coff,
                sym,
                data,
            }
        }

        fn set_sec(&mut self, idx: usize, va: u32, ptr_reloc: u32, num_reloc: u16) {
            let s = self.coff + 20 + idx * 40;
            self.buf[s + 12..s + 16].copy_from_slice(&va.to_le_bytes());
            self.buf[s + 24..s + 28].copy_from_slice(&ptr_reloc.to_le_bytes());
            self.buf[s + 32..s + 34].copy_from_slice(&num_reloc.to_le_bytes());
        }

        fn write_reloc(&mut self, off_from_coff: u32, va: u32, sym_index: u32, ty: u16) {
            let o = self.coff + off_from_coff as usize;
            self.buf[o..o + 4].copy_from_slice(&va.to_le_bytes());
            self.buf[o + 4..o + 8].copy_from_slice(&sym_index.to_le_bytes());
            self.buf[o + 8..o + 10].copy_from_slice(&ty.to_le_bytes());
        }

        fn set_sym_value(&mut self, idx: u32, value: u32) {
            let o = self.sym + (idx as usize) * 18 + 8;
            self.buf[o..o + 4].copy_from_slice(&value.to_le_bytes());
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
            fix_coff_relocs_buffered(&mut rust_img, self.coff, self.sym, delta);
            oracle_buffered(&mut ora_img, self.coff, self.sym, delta);
            (rust_img, ora_img)
        }
    }

    #[test]
    fn empty_sections_noop() {
        // Rust: nSections=0 is a clean no-op (while-guard).
        // Legacy FASM enters .fix_sec before testing n_sec (do-while quirk) —
        // not exercised here; no production caller passes 0.
        let mut img = Img::new(0, 0, 16);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        assert_eq!(a, img.buf);
    }

    #[test]
    fn zero_relocs_noop() {
        let mut img = Img::new(1, 1, 16);
        // PtrReloc relative to coff; NumReloc=0
        let reloc_off = (20 + 40) as u32; // start of reloc area
        img.set_sec(0, 0, reloc_off, 0);
        img.set_data_u32(0, 0xAABBCCDD);
        let before = img.get_data_u32(0);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        let got = u32::from_le_bytes([
            a[img.data],
            a[img.data + 1],
            a[img.data + 2],
            a[img.data + 3],
        ]);
        assert_eq!(got, before);
        assert_eq!(got, 0xAABBCCDD);
    }

    #[test]
    fn dir32_adds_symbol_value() {
        let mut img = Img::new(1, 1, 64);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        // Patch at data+0 → VA = data offset (delta=0, sec_va=0)
        let va = img.data as u32;
        img.write_reloc(reloc_off, va, 0, RELOC_TYPE_DIR32);
        img.set_sym_value(0, 0x1000);
        img.set_data_u32(0, 0x5);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        let got = u32::from_le_bytes([a[img.data], a[img.data + 1], a[img.data + 2], a[img.data + 3]]);
        assert_eq!(got, 0x5u32.wrapping_add(0x1000));
    }

    #[test]
    fn rel32_pc_relative() {
        let mut img = Img::new(1, 1, 64);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        let va = img.data as u32;
        img.write_reloc(reloc_off, va, 0, RELOC_TYPE_REL32);
        img.set_sym_value(0, 0x2000);
        img.set_data_u32(0, 0);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        // addend += Value - (VA+secVA) - 4
        let expected = 0u32
            .wrapping_add(0x2000u32.wrapping_sub(va).wrapping_sub(4));
        let got = u32::from_le_bytes([a[img.data], a[img.data + 1], a[img.data + 2], a[img.data + 3]]);
        assert_eq!(got, expected);
    }

    #[test]
    fn unknown_type_skipped() {
        let mut img = Img::new(1, 1, 64);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        let va = img.data as u32;
        img.write_reloc(reloc_off, va, 0, 99); // neither 6 nor 20
        img.set_sym_value(0, 0x1234);
        img.set_data_u32(0, 0x55);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        let got = u32::from_le_bytes([a[img.data], a[img.data + 1], a[img.data + 2], a[img.data + 3]]);
        assert_eq!(got, 0x55);
    }

    #[test]
    fn delta_shifts_patch_address() {
        let mut img = Img::new(1, 1, 64);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        // Want patch at data+8: VA + delta = data+8 → VA=data, delta=8
        let va = img.data as u32;
        img.write_reloc(reloc_off, va, 0, RELOC_TYPE_DIR32);
        img.set_sym_value(0, 0x11);
        img.set_data_u32(8, 0x22);
        let (a, b) = img.run_pair(8);
        assert_eq!(a, b);
        let got = u32::from_le_bytes([
            a[img.data + 8],
            a[img.data + 9],
            a[img.data + 10],
            a[img.data + 11],
        ]);
        assert_eq!(got, 0x22u32.wrapping_add(0x11));
    }

    #[test]
    fn multi_section_and_sym_index() {
        let mut img = Img::new(2, 3, 128);
        let reloc_area = (20 + 80) as u32;
        // sec0: 1 DIR32 → data+0 via sym 1
        img.set_sec(0, 0, reloc_area, 1);
        img.write_reloc(reloc_area, img.data as u32, 1, RELOC_TYPE_DIR32);
        // sec1: 1 REL32 → data+16 via sym 2; sec_va=0
        img.set_sec(1, 0, reloc_area + 10, 1);
        img.write_reloc(reloc_area + 10, (img.data + 16) as u32, 2, RELOC_TYPE_REL32);
        img.set_sym_value(1, 0xAAA);
        img.set_sym_value(2, 0x10000);
        img.set_data_u32(0, 1);
        img.set_data_u32(16, 2);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
    }

    #[test]
    fn wrapping_addends() {
        let mut img = Img::new(1, 1, 64);
        let reloc_off = (20 + 40) as u32;
        img.set_sec(0, 0, reloc_off, 1);
        let va = img.data as u32;
        img.write_reloc(reloc_off, va, 0, RELOC_TYPE_DIR32);
        img.set_sym_value(0, 0xFFFF_FFF0);
        img.set_data_u32(0, 0x20);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        let got = u32::from_le_bytes([a[img.data], a[img.data + 1], a[img.data + 2], a[img.data + 3]]);
        assert_eq!(got, 0x20u32.wrapping_add(0xFFFF_FFF0));
    }

    #[test]
    fn sec_va_included() {
        let mut img = Img::new(1, 1, 64);
        let reloc_off = (20 + 40) as u32;
        // sec_va=8, reloc.VA = data-8 → patch at data
        let sec_va = 8u32;
        img.set_sec(0, sec_va, reloc_off, 1);
        let va = (img.data as u32).wrapping_sub(sec_va);
        img.write_reloc(reloc_off, va, 0, RELOC_TYPE_DIR32);
        img.set_sym_value(0, 7);
        img.set_data_u32(0, 3);
        let (a, b) = img.run_pair(0);
        assert_eq!(a, b);
        let got = u32::from_le_bytes([a[img.data], a[img.data + 1], a[img.data + 2], a[img.data + 3]]);
        assert_eq!(got, 10);
    }

    /// Deterministic LCG PRNG corpus vs independent oracle.
    #[test]
    fn prng_corpus_50k() {
        let mut state = FIX_COFF_RELOCS_PRNG_SEED;
        let mut lcg = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            state
        };
        for _ in 0..50_000 {
            let n_sec = (lcg() % 3 + 1) as u16;
            let n_sym = lcg() % 4 + 1;
            let mut img = Img::new(n_sec, n_sym, 256);
            let reloc_base = (20 + 40 * n_sec as usize) as u32;
            let mut reloc_cursor = reloc_base;
            for si in 0..n_sec as usize {
                let nrel = (lcg() % 3) as u16;
                let sec_va = (lcg() % 32) & !3;
                img.set_sec(si, sec_va, reloc_cursor, nrel);
                for _ in 0..nrel {
                    let sym_i = lcg() % n_sym;
                    let ty = match lcg() % 5 {
                        0 | 1 => RELOC_TYPE_DIR32,
                        2 | 3 => RELOC_TYPE_REL32,
                        _ => (lcg() % 40 + 1) as u16, // often unknown
                    };
                    // Keep patch slots inside data[0..240]
                    let slot = (lcg() % 60) * 4;
                    let patch_abs = (img.data as u32).wrapping_add(slot);
                    // VA = patch_abs - sec_va - delta_later; pick delta 0..16
                    // We'll fix delta after; use VA = patch_abs - sec_va for delta=0 path
                    // vary delta separately:
                    let _ = ty;
                    let va = patch_abs.wrapping_sub(sec_va);
                    img.write_reloc(reloc_cursor, va, sym_i, ty);
                    reloc_cursor += 10;
                }
            }
            for s in 0..n_sym {
                img.set_sym_value(s, lcg());
            }
            for d in 0..64 {
                img.set_data_u32(d * 4, lcg());
            }
            // delta must keep patch in-bounds: we constructed VA+sec_va = data+slot
            // so delta must be 0 for in-bounds (or small and slots leave room)
            let delta = 0u32;
            let (a, b) = img.run_pair(delta);
            assert_eq!(a, b, "PRNG mismatch state={state:#x}");
        }
    }

    #[test]
    fn constants_match_fasm_layouts() {
        assert_eq!(COFF_HEADER_SIZE, 20);
        assert_eq!(COFF_SECTION_SIZE, 40);
        assert_eq!(COFF_RELOC_SIZE, 10);
        assert_eq!(COFF_SYM_SIZE, 18);
        assert_eq!(RELOC_TYPE_DIR32, 6);
        assert_eq!(RELOC_TYPE_REL32, 20);
    }
}
