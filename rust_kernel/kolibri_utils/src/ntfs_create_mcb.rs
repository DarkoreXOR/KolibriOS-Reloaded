//! Cut AX: `createMcbEntry` — encode one NTFS MCB (data-run) entry.
//!
//! Matches `kernel/fs/ntfs.inc` FASM leaf semantics (width zig-zag / shl-1,
//! optional attribute + FRS extend with `sizeWithHeader` quirk, EDI at
//! terminator). No tables / `.rodata`. No `memcpy`/`memset` (reloc-free).
//!
//! Direction flag: Rust never touches DF. The FASM trampoline issues `cld`
//! when this leaf returns bit0 set (FRS slide path).

/// Offset of `sizeWithHeader` inside an NTFS attribute header.
pub const OFF_SIZE_WITH_HEADER: usize = 4;
/// Offset of `recordRealSize` inside an FRS.
pub const OFF_RECORD_REAL_SIZE: usize = 0x18;
/// Offset of `recordAllocatedSize` inside an FRS.
pub const OFF_RECORD_ALLOCATED_SIZE: usize = 0x1C;

/// PRNG seed for Cut AX host differential (`'CUTX'`).
pub const NTFS_CREATE_MCB_ENTRY_PRNG_SEED: u32 = 0x4355_5458;

/// Result of `createMcbEntry` on a synthetic fixture (host tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateMcbFixtureResult {
    /// Destination pointer advanced to the terminator byte (or unchanged).
    pub dest_off: usize,
    /// Attribute `sizeWithHeader` dword after the call.
    pub size_with_header: u32,
    /// FRS `recordRealSize` after the call.
    pub record_real_size: u32,
    /// Whether the FRS `std`/`cld` slide path ran (trampoline must `cld`).
    pub need_cld: bool,
    /// Whether the MCB header+bytes+terminator were written.
    pub wrote: bool,
}

/// FASM-faithful width of `fileDataStart` (signed zig-zag via `shl 1` / `not`).
#[inline(always)]
pub fn mcb_start_width(start: u32) -> u32 {
    let mut eax = start;
    let mut edx: u32 = 0;
    let cf = (eax & 0x8000_0000) != 0;
    eax = eax.wrapping_shl(1);
    if cf {
        eax = !eax;
    }
    loop {
        edx = edx.wrapping_add(1);
        eax >>= 8;
        if eax == 0 {
            break;
        }
    }
    edx
}

/// FASM-faithful width of `fileDataSize` (`shl 1`, no `not`).
#[inline(always)]
pub fn mcb_size_width(size: u32) -> u32 {
    let mut eax = size.wrapping_shl(1);
    let mut ecx: u32 = 0;
    loop {
        ecx = ecx.wrapping_add(1);
        eax >>= 8;
        if eax == 0 {
            break;
        }
    }
    ecx
}

/// Encode MCB into a host-owned fixture (attr at `attr_off` inside `frs`).
///
/// `dest_off` is the initial EDI offset into `frs`. Layout mirrors a single
/// buffer where the attribute header and run destination live inside the FRS
/// (as production callers do).
pub fn ntfs_create_mcb_entry_fixture(
    start: u32,
    size: u32,
    frs: &mut [u8],
    attr_off: usize,
    dest_off: usize,
) -> CreateMcbFixtureResult {
    let start_w = mcb_start_width(start);
    let size_w = mcb_size_width(size);

    let attr_size = read_u32(frs, attr_off + OFF_SIZE_WITH_HEADER);
    // lea eax,[edi+edx+1]; add eax,ecx; sub eax,esi; sub eax,[esi+sizeWithHeader]
    // CF=1 (borrow) ⇒ fits; CF=0 ⇒ need extend.
    let new_end = (dest_off as u32)
        .wrapping_add(start_w)
        .wrapping_add(1)
        .wrapping_add(size_w);
    let attr_end = (attr_off as u32).wrapping_add(attr_size);
    let (_diff, borrow) = new_end.overflowing_sub(attr_end);
    let need_extend = !borrow;

    let mut size_with_header = attr_size;
    let record_real = read_u32(frs, OFF_RECORD_REAL_SIZE);
    let record_alloc = read_u32(frs, OFF_RECORD_ALLOCATED_SIZE);
    let mut cur_dest = dest_off;
    let mut need_cld = false;

    if need_extend {
        // add word [esi+sizeWithHeader], 8
        let low = (size_with_header as u16).wrapping_add(8);
        size_with_header = (size_with_header & 0xFFFF_0000) | u32::from(low);
        write_u32(frs, attr_off + OFF_SIZE_WITH_HEADER, size_with_header);

        let new_real = record_real.wrapping_add(8);
        if new_real > record_alloc {
            // jc .end — early out; sizeWithHeader already bumped; edi unchanged
            return CreateMcbFixtureResult {
                dest_off,
                size_with_header,
                record_real_size: record_real,
                need_cld: false,
                wrote: false,
            };
        }
        write_u32(frs, OFF_RECORD_REAL_SIZE, new_real);

        // Reverse dword slide: make 8 bytes of room at dest.
        frs_slide_make_room(frs, dest_off, new_real as usize);
        need_cld = true;
    }

    // Write header + size bytes + start bytes + terminator at edi (no advance past term).
    let header = ((start_w & 0x0F) << 4) | (size_w & 0x0F);
    frs[cur_dest] = header as u8;
    cur_dest += 1;
    write_le_bytes(frs, cur_dest, size, size_w as usize);
    cur_dest += size_w as usize;
    write_le_bytes(frs, cur_dest, start, start_w as usize);
    cur_dest += start_w as usize;
    frs[cur_dest] = 0; // terminator; edi stays here

    CreateMcbFixtureResult {
        dest_off: cur_dest,
        size_with_header: read_u32(frs, attr_off + OFF_SIZE_WITH_HEADER),
        record_real_size: read_u32(frs, OFF_RECORD_REAL_SIZE),
        need_cld,
        wrote: true,
    }
}

/// Pointer FFI body used by `rust_ntfs_create_mcb_entry`.
///
/// Reloc-free discipline: raw pointers only — no slice indexing (avoids
/// `panic_bounds_check` / GOT). Width loops and LE byte stores use shifts.
///
/// # Safety
/// `attr` / `frs` / `dest` must be live writable regions matching a real FRS
/// layout; `out_dest` must be a writable pointer slot.
#[inline(always)]
pub unsafe fn ntfs_create_mcb_entry_ptr(
    start: u32,
    size: u32,
    dest: *mut u8,
    attr: *mut u8,
    frs: *mut u8,
    out_dest: *mut *mut u8,
) -> u32 {
    let start_w = mcb_start_width(start);
    let size_w = mcb_size_width(size);

    let attr_size = unsafe { read_u32_ptr(attr.add(OFF_SIZE_WITH_HEADER)) };
    let dest_u = dest as u32;
    let attr_u = attr as u32;
    let new_end = dest_u
        .wrapping_add(start_w)
        .wrapping_add(1)
        .wrapping_add(size_w);
    let attr_end = attr_u.wrapping_add(attr_size);
    let (_diff, borrow) = new_end.overflowing_sub(attr_end);
    let need_extend = !borrow;

    let mut edi = dest;
    let mut need_cld: u32 = 0;

    if need_extend {
        let sw = unsafe { read_u32_ptr(attr.add(OFF_SIZE_WITH_HEADER)) };
        let low = (sw as u16).wrapping_add(8);
        let new_sw = (sw & 0xFFFF_0000) | u32::from(low);
        unsafe { write_u32_ptr(attr.add(OFF_SIZE_WITH_HEADER), new_sw) };

        let record_real = unsafe { read_u32_ptr(frs.add(OFF_RECORD_REAL_SIZE)) };
        let record_alloc = unsafe { read_u32_ptr(frs.add(OFF_RECORD_ALLOCATED_SIZE)) };
        let new_real = record_real.wrapping_add(8);
        if new_real > record_alloc {
            unsafe { *out_dest = dest };
            return 0;
        }
        unsafe { write_u32_ptr(frs.add(OFF_RECORD_REAL_SIZE), new_real) };

        unsafe {
            frs_slide_make_room_ptr(frs, dest, new_real);
        }
        need_cld = 1;
    }

    unsafe {
        *edi = (((start_w & 0x0F) << 4) | (size_w & 0x0F)) as u8;
        edi = edi.add(1);
        write_le_bytes_ptr(edi, size, size_w);
        edi = edi.add(size_w as usize);
        write_le_bytes_ptr(edi, start, start_w);
        edi = edi.add(start_w as usize);
        *edi = 0;
        *out_dest = edi;
    }
    need_cld
}

#[inline(always)]
unsafe fn read_u32_ptr(p: *const u8) -> u32 {
    unsafe {
        let b0 = *p as u32;
        let b1 = *p.add(1) as u32;
        let b2 = *p.add(2) as u32;
        let b3 = *p.add(3) as u32;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }
}

#[inline(always)]
unsafe fn write_u32_ptr(p: *mut u8, v: u32) {
    unsafe {
        *p = v as u8;
        *p.add(1) = (v >> 8) as u8;
        *p.add(2) = (v >> 16) as u8;
        *p.add(3) = (v >> 24) as u8;
    }
}

/// Write `n` little-endian bytes of `v` (no slice index / panic).
#[inline(always)]
unsafe fn write_le_bytes_ptr(p: *mut u8, v: u32, n: u32) {
    let mut i = 0u32;
    let mut x = v;
    while i < n {
        unsafe {
            *p.add(i as usize) = x as u8;
        }
        x >>= 8;
        i = i.wrapping_add(1);
    }
}

/// Mirror FASM `std`/`rep movsd`/`cld` slide that inserts 8 bytes at `dest`.
#[inline(always)]
unsafe fn frs_slide_make_room_ptr(frs: *mut u8, dest: *mut u8, new_real: u32) {
    let frs_u = frs as u32;
    let dest_off = (dest as u32).wrapping_sub(frs_u);
    let mut src = unsafe { frs.add(new_real.wrapping_sub(12) as usize) };
    let mut dst = unsafe { frs.add(new_real.wrapping_sub(4) as usize) };
    let count = new_real.wrapping_sub(dest_off).wrapping_sub(8) / 4;
    let mut i = 0u32;
    while i < count {
        let v = unsafe { read_u32_ptr(src) };
        unsafe { write_u32_ptr(dst, v) };
        src = unsafe { src.sub(4) };
        dst = unsafe { dst.sub(4) };
        i = i.wrapping_add(1);
    }
}

// ---- Host / fixture helpers (may use slices; not on the freestanding FFI path) ----

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    let b0 = v as u8;
    let b1 = (v >> 8) as u8;
    let b2 = (v >> 16) as u8;
    let b3 = (v >> 24) as u8;
    buf[off] = b0;
    buf[off + 1] = b1;
    buf[off + 2] = b2;
    buf[off + 3] = b3;
}

fn write_le_bytes(buf: &mut [u8], off: usize, v: u32, n: usize) {
    let mut x = v;
    let mut i = 0usize;
    while i < n {
        buf[off + i] = x as u8;
        x >>= 8;
        i += 1;
    }
}

fn frs_slide_make_room(frs: &mut [u8], dest_off: usize, new_real: usize) {
    let mut src = new_real.wrapping_sub(12);
    let mut dst = new_real.wrapping_sub(4);
    let count = new_real.wrapping_sub(dest_off).wrapping_sub(8) / 4;
    let mut i = 0usize;
    while i < count {
        let v = read_u32(frs, src);
        write_u32(frs, dst, v);
        src = src.wrapping_sub(4);
        dst = dst.wrapping_sub(4);
        i += 1;
    }
}

/// Independent FASM-flow oracle (separate control flow from production).
#[cfg(test)]
pub fn fasm_oracle_create_mcb_entry(
    start: u32,
    size: u32,
    frs: &mut [u8],
    attr_off: usize,
    dest_off: usize,
) -> CreateMcbFixtureResult {
    // Re-derived from FASM instruction stream — not a call into production.
    let mut edx: u32 = 0;
    let mut eax = start;
    let cf = (eax & 0x8000_0000) != 0;
    eax <<= 1;
    if cf {
        eax = !eax;
    }
    loop {
        edx += 1;
        eax >>= 8;
        if eax == 0 {
            break;
        }
    }
    let start_w = edx;

    eax = size << 1;
    let mut ecx: u32 = 0;
    loop {
        ecx += 1;
        eax >>= 8;
        if eax == 0 {
            break;
        }
    }
    let size_w = ecx;

    // lea eax,[edi+edx+1]; add eax,ecx; sub eax,esi; sub eax,[esi+sizeWithHeader]
    eax = (dest_off as u32)
        .wrapping_add(start_w)
        .wrapping_add(1)
        .wrapping_add(size_w);
    eax = eax.wrapping_sub(attr_off as u32);
    let sw0 = read_u32(frs, attr_off + OFF_SIZE_WITH_HEADER);
    let (eax2, cf2) = eax.overflowing_sub(sw0);
    let _ = eax2;

    let mut size_with_header = sw0;
    let mut record_real = read_u32(frs, OFF_RECORD_REAL_SIZE);
    let record_alloc = read_u32(frs, OFF_RECORD_ALLOCATED_SIZE);
    let mut need_cld = false;
    let mut cur_dest = dest_off;

    if !cf2 {
        // extend path
        let low = (size_with_header as u16).wrapping_add(8);
        size_with_header = (size_with_header & 0xFFFF_0000) | u32::from(low);
        write_u32(frs, attr_off + OFF_SIZE_WITH_HEADER, size_with_header);

        eax = record_real.wrapping_add(8);
        // cmp [alloc], eax ; jc .end — CF if alloc < eax
        if record_alloc < eax {
            return CreateMcbFixtureResult {
                dest_off,
                size_with_header,
                record_real_size: record_real,
                need_cld: false,
                wrote: false,
            };
        }
        write_u32(frs, OFF_RECORD_REAL_SIZE, eax);
        record_real = eax;
        frs_slide_make_room(frs, dest_off, record_real as usize);
        need_cld = true;
    }

    let header = ((start_w & 0x0F) << 4) | (size_w & 0x0F);
    frs[cur_dest] = header as u8;
    cur_dest += 1;
    write_le_bytes(frs, cur_dest, size, size_w as usize);
    cur_dest += size_w as usize;
    write_le_bytes(frs, cur_dest, start, start_w as usize);
    cur_dest += start_w as usize;
    frs[cur_dest] = 0;

    CreateMcbFixtureResult {
        dest_off: cur_dest,
        size_with_header: read_u32(frs, attr_off + OFF_SIZE_WITH_HEADER),
        record_real_size: read_u32(frs, OFF_RECORD_REAL_SIZE),
        need_cld,
        wrote: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfs_mcb::ntfs_decode_mcb_entry;

    fn make_frs(alloc: u32, real: u32, attr_off: usize, attr_size: u32, fill: u8) -> Vec<u8> {
        let mut frs = vec![fill; alloc as usize];
        write_u32(&mut frs, OFF_RECORD_REAL_SIZE, real);
        write_u32(&mut frs, OFF_RECORD_ALLOCATED_SIZE, alloc);
        write_u32(&mut frs, attr_off + OFF_SIZE_WITH_HEADER, attr_size);
        // Plant a recognizable tail past dest so slide is observable.
        for i in (attr_off + attr_size as usize)..(real as usize) {
            frs[i] = (0x40 + (i & 0x3F)) as u8;
        }
        frs
    }

    fn check(start: u32, size: u32, attr_off: usize, dest_off: usize, attr_size: u32, real: u32, alloc: u32) {
        let mut a = make_frs(alloc, real, attr_off, attr_size, 0xA5);
        let mut b = a.clone();
        let ra = ntfs_create_mcb_entry_fixture(start, size, &mut a, attr_off, dest_off);
        let rb = fasm_oracle_create_mcb_entry(start, size, &mut b, attr_off, dest_off);
        assert_eq!(ra, rb, "result start={start:#x} size={size:#x}");
        assert_eq!(a, b, "buffer start={start:#x} size={size:#x}");
    }

    #[test]
    fn widths_named() {
        assert_eq!(mcb_start_width(0), 1);
        assert_eq!(mcb_start_width(0x7F), 1);
        assert_eq!(mcb_start_width(0x80), 2);
        assert_eq!(mcb_start_width(0xFFFF_FFFF), 1); // -1
        assert_eq!(mcb_size_width(0), 1);
        assert_eq!(mcb_size_width(0x7F), 1);
        assert_eq!(mcb_size_width(0x80), 2);
    }

    #[test]
    fn no_extend_simple() {
        // attr_off=0x40, size=0x50 → attr covers to 0x90; dest at 0x80 fits small run
        check(2, 5, 0x40, 0x80, 0x50, 0x100, 0x200);
    }

    #[test]
    fn extend_and_slide() {
        // dest past attr end → need +8 extend + FRS slide
        check(2, 5, 0x40, 0x8E, 0x50, 0x100, 0x200);
    }

    #[test]
    fn frs_full_early_out_size_quirk() {
        // dest past attr end → need +8 extend; alloc == real → cannot grow;
        // sizeWithHeader still +8; no write; edi unchanged.
        let attr_off = 0x40;
        let dest_off = 0x8E;
        let mut a = make_frs(0x100, 0x100, attr_off, 0x50, 0xA5);
        let mut b = a.clone();
        let ra = ntfs_create_mcb_entry_fixture(2, 5, &mut a, attr_off, dest_off);
        let rb = fasm_oracle_create_mcb_entry(2, 5, &mut b, attr_off, dest_off);
        assert_eq!(ra, rb);
        assert!(!ra.wrote);
        assert_eq!(ra.dest_off, dest_off);
        assert_eq!(ra.size_with_header & 0xFFFF, 0x58); // 0x50+8
        assert_eq!(a, b);
    }

    #[test]
    fn negative_start_roundtrip_decode() {
        let attr_off = 0x40;
        let dest_off = 0x80;
        let mut frs = make_frs(0x200, 0x100, attr_off, 0x50, 0x00);
        let r = ntfs_create_mcb_entry_fixture(0xFFFF_FFFE, 3, &mut frs, attr_off, dest_off);
        assert!(r.wrote);
        let entry_len = r.dest_off - dest_off + 1; // include terminator
        let entry = &frs[dest_off..dest_off + entry_len];
        let mut buf = [0xA5u8; 16];
        let d = ntfs_decode_mcb_entry(&entry[..entry.len() - 1], &mut buf);
        assert!(d.more);
        assert_eq!(&buf[0..8], &[3, 0, 0, 0, 0, 0, 0, 0]);
        let got = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        assert_eq!(got as u32, 0xFFFF_FFFE);
    }

    #[test]
    fn ptr_wrapper_matches_fixture() {
        let attr_off = 0x40;
        let dest_off = 0x88;
        let mut frs = make_frs(0x200, 0x100, attr_off, 0x50, 0x5A);
        let mut frs2 = frs.clone();
        let expected = ntfs_create_mcb_entry_fixture(0x1234, 0x10, &mut frs, attr_off, dest_off);

        let mut out: *mut u8 = core::ptr::null_mut();
        let need_cld = unsafe {
            ntfs_create_mcb_entry_ptr(
                0x1234,
                0x10,
                frs2.as_mut_ptr().add(dest_off),
                frs2.as_mut_ptr().add(attr_off),
                frs2.as_mut_ptr(),
                &mut out,
            )
        };
        assert_eq!(need_cld != 0, expected.need_cld);
        assert_eq!(out as usize, frs2.as_ptr() as usize + expected.dest_off);
        assert_eq!(frs, frs2);
    }

    #[test]
    fn prng_differential_50k() {
        let mut state = NTFS_CREATE_MCB_ENTRY_PRNG_SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..50_000 {
            let start = next();
            let size = next() & 0x00FF_FFFF; // keep widths ≤4 typically
            let attr_off = 0x40;
            let attr_size = 0x40 + (next() & 0x3F);
            let dest_off = attr_off + 0x30 + (next() as usize & 0x1F);
            let real = 0x120 + (next() & 0x3F);
            let alloc = real + (next() & 0x7F);
            if dest_off + 20 >= real as usize {
                continue;
            }
            if attr_off + attr_size as usize > real as usize {
                continue;
            }
            check(start, size, attr_off, dest_off, attr_size, real, alloc.max(real + 8));
        }
    }
}
