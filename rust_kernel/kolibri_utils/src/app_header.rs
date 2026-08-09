//! Cut O: `test_app_header` — MENUET binary header validation + `APP_HDR` fill.
//!
//! Matches `kernel/core/taskman.inc` FASM leaf semantics:
//! * accept `MENUET01` / `MENUET02` banner (bytes 0..8)
//! * do **not** validate the `version` dword at `+8`
//! * require `mem_size >= i_end`, `mem_size < OS_BASE`, `mem_size < pages_free<<12`
//! * on success fill `APP_HDR` fields used by `fs_execute`
//! * on mid-fail after magic: may have already written `eip` (partial mutate)
//!
//! `[pg_data.pages_free]` stays in the FASM trampoline so the Rust blob remains
//! reloc-free. `OS_BASE` is the locked constant `0x8000_0000`.

/// Kolibri `OS_BASE` from `kernel/const.inc`.
pub const OS_BASE: u32 = 0x8000_0000;

/// `APP_HEADER_01_.start` offset.
pub const HDR_OFF_START: usize = 12;
/// `APP_HEADER_01_.i_end` offset.
pub const HDR_OFF_I_END: usize = 16;
/// `APP_HEADER_01_.mem_size` offset.
pub const HDR_OFF_MEM_SIZE: usize = 20;
/// `APP_HEADER_01_.stack_top` offset.
pub const HDR_OFF_STACK_TOP: usize = 24;
/// `APP_HEADER_01_.i_param` offset.
pub const HDR_OFF_I_PARAM: usize = 28;
/// `APP_HEADER_01_.i_icon` offset.
pub const HDR_OFF_I_ICON: usize = 32;

/// `APP_HDR.cmdline` offset.
pub const APP_OFF_CMDLINE: usize = 0x00;
/// `APP_HDR.path` offset.
pub const APP_OFF_PATH: usize = 0x04;
/// `APP_HDR.eip` offset.
pub const APP_OFF_EIP: usize = 0x08;
/// `APP_HDR.esp` offset.
pub const APP_OFF_ESP: usize = 0x0C;
/// `APP_HDR._edata` offset.
pub const APP_OFF_EDATA: usize = 0x10;
/// `APP_HDR._emem` offset.
pub const APP_OFF_EMEM: usize = 0x14;

/// Cut O differential PRNG seed (documented).
pub const TEST_APP_HEADER_PRNG_SEED: u32 = 0x7E57_A0AD;

/// Little-endian `'MENU'` — immediate, not `.rodata`.
const MAGIC_MENU: u32 = 0x554E_454D;
/// Little-endian `'ET'`.
const MAGIC_ET: u16 = 0x5445;
/// Little-endian `'01'`.
const MAGIC_01: u16 = 0x3130;
/// Little-endian `'02'`.
const MAGIC_02: u16 = 0x3230;

#[inline(always)]
unsafe fn read_u32(base: *const u8, off: usize) -> u32 {
    unsafe { core::ptr::read_unaligned(base.add(off) as *const u32) }
}

#[inline(always)]
unsafe fn read_u16(base: *const u8, off: usize) -> u16 {
    unsafe { core::ptr::read_unaligned(base.add(off) as *const u16) }
}

#[inline(always)]
unsafe fn write_u32(base: *mut u8, off: usize, val: u32) {
    // Explicit store — avoid memset/bulk helpers in freestanding extract.
    unsafe { core::ptr::write_volatile(base.add(off) as *mut u32, val) }
}

/// Validate a MENUET app image header and fill `APP_HDR` on success.
///
/// # Returns
/// * On success: `header` cast to `u32` (matches FASM leaving `EAX` unchanged).
/// * On fail: `0` (matches FASM `xor eax, eax`).
///
/// # Safety
/// `header` must be readable through offset 32; `app_hdr` must be writable
/// through `APP_OFF_EMEM` (at least 0x18 bytes).
#[inline(always)]
pub unsafe fn test_app_header(header: *const u8, app_hdr: *mut u8, pages_free: u32) -> u32 {
    // FASM: cmp dword [eax], 'MENU'
    let menu = unsafe { read_u32(header, 0) };
    if menu != MAGIC_MENU {
        return 0;
    }
    // FASM: cmp word [eax+4], 'ET'
    let et = unsafe { read_u16(header, 4) };
    if et != MAGIC_ET {
        return 0;
    }
    // FASM: cmp [eax+6], word '01' / '02'
    let ver = unsafe { read_u16(header, 6) };
    if ver != MAGIC_01 && ver != MAGIC_02 {
        return 0;
    }

    // FASM writes eip *before* mem_size sanity checks (partial mutate on fail).
    let start = unsafe { read_u32(header, HDR_OFF_START) };
    unsafe { write_u32(app_hdr, APP_OFF_EIP, start) };

    let mem_size = unsafe { read_u32(header, HDR_OFF_MEM_SIZE) };
    let i_end = unsafe { read_u32(header, HDR_OFF_I_END) };

    // cmp edx, [i_end] / jb .fail
    if mem_size < i_end {
        return 0;
    }
    // cmp edx, OS_BASE / jae .fail
    if mem_size >= OS_BASE {
        return 0;
    }
    // ecx = pages_free << 12; cmp edx, ecx / jae .fail
    let pages_bytes = pages_free << 12;
    if mem_size >= pages_bytes {
        return 0;
    }

    let stack_top = unsafe { read_u32(header, HDR_OFF_STACK_TOP) };
    let i_param = unsafe { read_u32(header, HDR_OFF_I_PARAM) };
    let i_icon = unsafe { read_u32(header, HDR_OFF_I_ICON) };

    unsafe {
        write_u32(app_hdr, APP_OFF_EMEM, mem_size);
        write_u32(app_hdr, APP_OFF_ESP, stack_top);
        write_u32(app_hdr, APP_OFF_CMDLINE, i_param);
        write_u32(app_hdr, APP_OFF_PATH, i_icon);
        write_u32(app_hdr, APP_OFF_EDATA, i_end);
    }

    header as u32
}

/// Pointer-friendly entry used by the stdcall FFI.
///
/// # Safety
/// Same as [`test_app_header`].
#[inline(always)]
pub unsafe fn test_app_header_ptr(header: *const u8, app_hdr: *mut u8, pages_free: u32) -> u32 {
    unsafe { test_app_header(header, app_hdr, pages_free) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-faithful oracle (mirrors taskman.inc:206–250).
    fn fasm_oracle(header: &[u8; 36], app_hdr: &mut [u8; 0x18], pages_free: u32) -> u32 {
        let menu = u32::from_le_bytes(header[0..4].try_into().unwrap());
        if menu != u32::from_le_bytes(*b"MENU") {
            return 0;
        }
        let et = u16::from_le_bytes(header[4..6].try_into().unwrap());
        if et != u16::from_le_bytes(*b"ET") {
            return 0;
        }
        let ver = u16::from_le_bytes(header[6..8].try_into().unwrap());
        if ver != u16::from_le_bytes(*b"01") && ver != u16::from_le_bytes(*b"02") {
            return 0;
        }

        let start = u32::from_le_bytes(header[12..16].try_into().unwrap());
        app_hdr[8..12].copy_from_slice(&start.to_le_bytes());

        let mem_size = u32::from_le_bytes(header[20..24].try_into().unwrap());
        let i_end = u32::from_le_bytes(header[16..20].try_into().unwrap());
        if mem_size < i_end {
            return 0;
        }
        if mem_size >= OS_BASE {
            return 0;
        }
        let pages_bytes = pages_free << 12;
        if mem_size >= pages_bytes {
            return 0;
        }

        let stack_top = u32::from_le_bytes(header[24..28].try_into().unwrap());
        let i_param = u32::from_le_bytes(header[28..32].try_into().unwrap());
        let i_icon = u32::from_le_bytes(header[32..36].try_into().unwrap());

        app_hdr[0x14..0x18].copy_from_slice(&mem_size.to_le_bytes());
        app_hdr[0x0C..0x10].copy_from_slice(&stack_top.to_le_bytes());
        app_hdr[0x00..0x04].copy_from_slice(&i_param.to_le_bytes());
        app_hdr[0x04..0x08].copy_from_slice(&i_icon.to_le_bytes());
        app_hdr[0x10..0x14].copy_from_slice(&i_end.to_le_bytes());

        // Success: FASM leaves EAX = header pointer. Host tests use sentinel 1.
        1
    }

    fn make_header(
        banner: &[u8; 8],
        version: u32,
        start: u32,
        i_end: u32,
        mem_size: u32,
        stack_top: u32,
        i_param: u32,
        i_icon: u32,
    ) -> [u8; 36] {
        let mut h = [0u8; 36];
        h[0..8].copy_from_slice(banner);
        h[8..12].copy_from_slice(&version.to_le_bytes());
        h[12..16].copy_from_slice(&start.to_le_bytes());
        h[16..20].copy_from_slice(&i_end.to_le_bytes());
        h[20..24].copy_from_slice(&mem_size.to_le_bytes());
        h[24..28].copy_from_slice(&stack_top.to_le_bytes());
        h[28..32].copy_from_slice(&i_param.to_le_bytes());
        h[32..36].copy_from_slice(&i_icon.to_le_bytes());
        h
    }

    fn check(header: &[u8; 36], pages_free: u32) {
        let mut rust_hdr = [0xA5u8; 0x18];
        let mut fasm_hdr = [0xA5u8; 0x18];
        let rust_rc = unsafe {
            test_app_header(header.as_ptr(), rust_hdr.as_mut_ptr(), pages_free)
        };
        let fasm_rc = fasm_oracle(header, &mut fasm_hdr, pages_free);

        let rust_ok = rust_rc != 0;
        let fasm_ok = fasm_rc != 0;
        assert_eq!(
            rust_ok, fasm_ok,
            "success mismatch pages_free={pages_free} header={header:?}"
        );
        assert_eq!(
            rust_hdr, fasm_hdr,
            "APP_HDR mismatch pages_free={pages_free}"
        );
        if rust_ok {
            assert_eq!(rust_rc, header.as_ptr() as u32);
        } else {
            assert_eq!(rust_rc, 0);
        }
    }

    #[test]
    fn menuet01_success_fills_all_fields() {
        let h = make_header(
            b"MENUET01",
            0xDEADBEEF, // version ignored
            0x1000,
            0x2000,
            0x3000,
            0x2FF0,
            0x11111111,
            0x22222222,
        );
        let mut app = [0u8; 0x18];
        let rc = unsafe { test_app_header(h.as_ptr(), app.as_mut_ptr(), 16) };
        assert_eq!(rc, h.as_ptr() as u32);
        assert_eq!(u32::from_le_bytes(app[8..12].try_into().unwrap()), 0x1000);
        assert_eq!(u32::from_le_bytes(app[0x14..0x18].try_into().unwrap()), 0x3000);
        assert_eq!(u32::from_le_bytes(app[0x0C..0x10].try_into().unwrap()), 0x2FF0);
        assert_eq!(u32::from_le_bytes(app[0..4].try_into().unwrap()), 0x11111111);
        assert_eq!(u32::from_le_bytes(app[4..8].try_into().unwrap()), 0x22222222);
        assert_eq!(u32::from_le_bytes(app[0x10..0x14].try_into().unwrap()), 0x2000);
        check(&h, 16);
    }

    #[test]
    fn menuet02_accepted() {
        let h = make_header(b"MENUET02", 1, 0x10, 0x20, 0x30, 0x28, 0, 0);
        check(&h, 1);
    }

    #[test]
    fn bad_magic_fails_without_mutate() {
        for bad in [b"XXXXET01", b"MENUxx01", b"MENUET00", b"MENUET03", b"menuet01"] {
            let mut banner = [0u8; 8];
            banner.copy_from_slice(bad);
            let h = make_header(&banner, 0, 1, 2, 3, 4, 5, 6);
            let mut app = [0xA5u8; 0x18];
            let before = app;
            let rc = unsafe { test_app_header(h.as_ptr(), app.as_mut_ptr(), 100) };
            assert_eq!(rc, 0);
            assert_eq!(app, before, "magic fail must not mutate APP_HDR");
            check(&h, 100);
        }
    }

    #[test]
    fn mem_size_lt_i_end_partial_eip() {
        let h = make_header(b"MENUET01", 0, 0xABCD1234, 0x5000, 0x4000, 1, 2, 3);
        let mut app = [0xA5u8; 0x18];
        let rc = unsafe { test_app_header(h.as_ptr(), app.as_mut_ptr(), 100) };
        assert_eq!(rc, 0);
        assert_eq!(
            u32::from_le_bytes(app[8..12].try_into().unwrap()),
            0xABCD1234
        );
        // other fields untouched
        assert_eq!(app[0], 0xA5);
        assert_eq!(app[0x14], 0xA5);
        check(&h, 100);
    }

    #[test]
    fn mem_size_at_os_base_fails() {
        let h = make_header(b"MENUET01", 0, 1, 2, OS_BASE, 3, 4, 5);
        check(&h, 0x1_0000);
        let h2 = make_header(b"MENUET01", 0, 1, 2, OS_BASE - 1, 3, 4, 5);
        // still may fail pages_free; use huge pages_free
        check(&h2, 0x8_0000);
    }

    #[test]
    fn pages_free_boundary() {
        // mem_size=0x3000 needs pages_free > 3 (3<<12=0x3000 → jae fail)
        let h = make_header(b"MENUET01", 0, 1, 0x1000, 0x3000, 2, 3, 4);
        check(&h, 3); // fail: mem_size >= 0x3000
        check(&h, 4); // ok: 4<<12 = 0x4000
    }

    #[test]
    fn mem_size_eq_i_end_ok() {
        let h = make_header(b"MENUET01", 0, 9, 0x2000, 0x2000, 8, 7, 6);
        check(&h, 16);
    }

    #[test]
    fn zero_pages_free_rejects_nonzero_mem() {
        let h = make_header(b"MENUET01", 0, 1, 0, 1, 0, 0, 0);
        check(&h, 0);
    }

    #[test]
    fn structured_grid() {
        let banners: [&[u8; 8]; 3] = [b"MENUET01", b"MENUET02", b"MENUET99"];
        let mems = [0u32, 1, 0xFFF, 0x1000, 0x7FFF_FFFF, OS_BASE - 1, OS_BASE, OS_BASE + 1];
        let i_ends = [0u32, 1, 0x1000, 0x2000];
        let pages = [0u32, 1, 2, 16, 0x1000, 0x8_0000];
        for banner in banners {
            for &mem in &mems {
                for &i_end in &i_ends {
                    for &pf in &pages {
                        let h = make_header(banner, 0x55, 0xAA, i_end, mem, 0xBB, 0xCC, 0xDD);
                        check(&h, pf);
                    }
                }
            }
        }
    }

    #[test]
    fn prng_200k() {
        // xorshift32
        let mut s = TEST_APP_HEADER_PRNG_SEED;
        for _ in 0..200_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let mut banner = *b"MENUET01";
            if (s & 7) == 0 {
                banner[0] = b'X';
            } else if (s & 7) == 1 {
                banner[6] = b'2';
            } else if (s & 7) == 2 {
                banner[6] = b'9';
            }
            let start = s.rotate_left(3);
            let i_end = s.wrapping_mul(3) & 0xFFFF;
            let mem_size = match s & 0x1F {
                0 => OS_BASE,
                1 => OS_BASE.wrapping_sub(1),
                2 => i_end.wrapping_sub(1),
                3 => i_end,
                _ => (s & 0x0FFF_FFFF) | 0x1000,
            };
            let pages_free = (s >> 8) & 0xFFFF;
            let h = make_header(
                &banner,
                s,
                start,
                i_end,
                mem_size,
                s.wrapping_add(1),
                s.wrapping_add(2),
                s.wrapping_add(3),
            );
            check(&h, pages_free);
        }
    }
}
