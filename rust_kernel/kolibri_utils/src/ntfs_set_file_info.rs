//! Cut CW: `ntfs_SetFileInfo` — NTFS plugin Path B leaf (sysfn70.6).
//!
//! Matches `kernel/fs/ntfs.inc` FASM body **after** `ntfs_lock`, `ntfs_find_lfn`,
//! fragment check, and INDX vs `$INDEX_ROOT` pointer fixup. Reloc-free: f.70 /
//! NTFS / `$I30` entry / record buffer / `LastRead` and FASM helper addresses
//! are injected via [`NtfsSetFileInfoCtx`].
//!
//! Lock acquire and path lookup stay in FASM. Rust mutates parent `$I30`
//! flags + three FILETIMEs, then calls `writeRecord` (USA + disk) and
//! `ntfsDone` (sync + unlock + EAX=0). Write/sync errors are ignored
//! (legacy quirk).

/// Cut CW differential / smoke marker (`'NSFI'`).
pub const NTFS_SET_FILE_INFO_PRNG_SEED: u32 = 0x4E53_4649;

/// Injected trampoline context (8 dwords = 32 bytes).
pub const NTFS_SET_FILE_INFO_CTX_SIZE: usize = 32;

/// `$I30` `fileCreated`.
pub const I30_FILE_CREATED: usize = 0x18;
/// `$I30` `fileModified`.
pub const I30_FILE_MODIFIED: usize = 0x20;
/// `$I30` `recordModified` — **not** written by SetFileInfo.
pub const I30_RECORD_MODIFIED: usize = 0x28;
/// `$I30` `fileAccessed`.
pub const I30_FILE_ACCESSED: usize = 0x30;
/// `$I30` `fileAllocatedSize` — not written.
pub const I30_FILE_ALLOCATED: usize = 0x38;
/// `$I30` `fileRealSize` — not written.
pub const I30_FILE_REAL_SIZE: usize = 0x40;
/// `$I30` `fileFlags`.
pub const I30_FILE_FLAGS: usize = 0x48;

/// Guest attrs bits applied (`R|H|S|A`).
pub const ATTR_MASK: u32 = 0x27;
/// Low-byte keep mask (`and byte …, -28h` = `0xD8`).
pub const FLAGS_KEEP_LOW: u8 = 0xD8;

/// SetFileInfo buffer: attrs @+0, flags dword @+4 ignored, ctime @+8.
pub const BDFE_CTIME_OFF: usize = 8;
/// atime @+16.
pub const BDFE_ATIME_OFF: usize = 16;
/// mtime @+24.
pub const BDFE_MTIME_OFF: usize = 24;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NtfsSetFileInfoCtx {
    /// f.70 parameter block (`ebx` at plugin entry).
    pub f70: *mut u8,
    /// `NTFS` / `PARTITION` (`ebp`).
    pub ntfs: *mut u8,
    /// Resolved `$I30` index entry.
    pub entry: *mut u8,
    /// `writeRecord` ebx (INDX buffer or FRS).
    pub record: *mut u8,
    /// `writeRecord` edx (`LastRead` partition sector).
    pub last_read: u32,
    /// FASM `ntfsCalculateTime` address (Cut AF trampoline).
    pub calc_time: u32,
    /// FASM `writeRecord` address.
    pub write_record: u32,
    /// FASM `ntfsDone` address (sync + unlock + EAX=0).
    pub done: u32,
}

#[cfg(all(target_arch = "x86", target_os = "none"))]
const _: () = assert!(core::mem::size_of::<NtfsSetFileInfoCtx>() == 32);

/// Host-side hooks (kernel uses ctx function pointers).
pub struct NtfsSetFileInfoHooks {
    pub calc_time: unsafe fn(*mut u8, *const u8) -> (u32, u32),
    pub write_record: unsafe fn(*mut u8, *mut u8, u32, *mut u8),
    pub done: unsafe fn(*mut u8, *mut u8) -> u32,
    pub state: *mut u8,
    /// When non-null, 32-byte SetFileInfo buffer (host tests; avoids u32 ptr trunc).
    pub bdfe: *const u8,
}

#[inline(always)]
unsafe fn load_u32(p: *const u8) -> u32 {
    unsafe { core::ptr::read_unaligned(p as *const u32) }
}

#[inline(always)]
unsafe fn store_u32(p: *mut u8, v: u32) {
    unsafe { core::ptr::write_unaligned(p as *mut u32, v) }
}

#[inline(always)]
unsafe fn store_filetime(p: *mut u8, lo: u32, hi: u32) {
    unsafe {
        store_u32(p, lo);
        store_u32(p.add(4), hi);
    }
}

/// Apply FASM `fileFlags` low-byte mask: `and 0x27` / `and byte, 0xD8` / `or al`.
#[inline(always)]
pub fn apply_file_flags(old: u32, guest_attrs: u32) -> u32 {
    let keep = (old as u8) & FLAGS_KEEP_LOW;
    let bits = (guest_attrs & ATTR_MASK) as u8;
    (old & !0xFF) | u32::from(keep | bits)
}

#[inline(always)]
unsafe fn call_calc_time(
    fn_ptr: u32,
    bdfe: *const u8,
    hooks: Option<&NtfsSetFileInfoHooks>,
) -> (u32, u32) {
    if let Some(h) = hooks {
        return unsafe { (h.calc_time)(h.state, bdfe) };
    }
    unsafe { invoke_calc_time(fn_ptr, bdfe) }
}

#[inline(always)]
unsafe fn invoke_calc_time(fn_ptr: u32, bdfe: *const u8) -> (u32, u32) {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        let mut edx_r: u32;
        // ESI → BDFE; EDX:EAX FILETIME; ESI preserved by Cut AF stdcall.
        // Pin fn in EBX and BDFE in EDX; do not lateout a live fn/BDFE reg
        // before the `mov esi` (REG-017). FASM always injects a live fn ptr.
        unsafe {
            core::arch::asm!(
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",
                "mov esi, edx",
                "call ebx",
                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("ebx") fn_ptr,
                in("edx") bdfe as u32,
                lateout("eax") eax_r,
                lateout("edx") edx_r,
                lateout("ecx") _,
            );
        }
        (eax_r, edx_r)
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = bdfe;
        (0, 0)
    }
}

#[inline(always)]
unsafe fn call_write_record(
    fn_ptr: u32,
    record: *mut u8,
    last_read: u32,
    ntfs: *mut u8,
    hooks: Option<&NtfsSetFileInfoHooks>,
) {
    if let Some(h) = hooks {
        unsafe { (h.write_record)(h.state, record, last_read, ntfs) };
        return;
    }
    unsafe { invoke_write_record(fn_ptr, record, last_read, ntfs) }
}

#[inline(always)]
unsafe fn invoke_write_record(fn_ptr: u32, record: *mut u8, last_read: u32, ntfs: *mut u8) {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        // EBX=record, EDX=LastRead, EBP=PARTITION; DF=0. Return EAX ignored.
        // Pin fn in EDI; LLVM forbids ESI as an asm operand. (REG-017).
        unsafe {
            core::arch::asm!(
                "push ebx",
                "push ebp",
                "push esi",
                "mov ebx, edx",
                "mov edx, ecx",
                "mov ebp, eax",
                "call edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("edi") fn_ptr,
                in("edx") record as u32,
                in("ecx") last_read,
                in("eax") ntfs as u32,
                lateout("eax") _,
                lateout("ecx") _,
                lateout("edx") _,
            );
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, record, last_read, ntfs);
    }
}

#[inline(always)]
unsafe fn call_done(
    fn_ptr: u32,
    ntfs: *mut u8,
    hooks: Option<&NtfsSetFileInfoHooks>,
) -> u32 {
    if let Some(h) = hooks {
        return unsafe { (h.done)(h.state, ntfs) };
    }
    unsafe { invoke_done(fn_ptr, ntfs) }
}

#[inline(always)]
unsafe fn invoke_done(fn_ptr: u32, ntfs: *mut u8) -> u32 {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        unsafe {
            core::arch::asm!(
                "push ebx",
                "push ebp",
                "push esi",
                "mov ebp, eax",
                "call edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("edi") fn_ptr,
                in("eax") ntfs as u32,
                lateout("eax") eax_r,
                lateout("ecx") _,
                lateout("edx") _,
            );
        }
        eax_r
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = ntfs;
        0
    }
}

/// FASM-faithful `ntfs_SetFileInfo` body after lock/lookup/fixup.
///
/// Always calls `ntfsDone` (legacy has no Rust-side error path). WriteRecord
/// / `disk_sync` errors are ignored; return is `ntfsDone`'s EAX (0).
///
/// # Safety
/// `ctx` and all injected pointers/callbacks must be valid; NTFS lock held.
#[inline(always)]
pub unsafe fn ntfs_set_file_info(
    ctx: *mut NtfsSetFileInfoCtx,
    hooks: Option<NtfsSetFileInfoHooks>,
) -> u32 {
    unsafe { ntfs_set_file_info_inner(&*ctx, hooks.as_ref()) }
}

#[inline(always)]
unsafe fn ntfs_set_file_info_inner(
    ctx: &NtfsSetFileInfoCtx,
    hooks: Option<&NtfsSetFileInfoHooks>,
) -> u32 {
    let entry = ctx.entry;
    let record = ctx.record;
    let last_read = ctx.last_read;
    let ntfs = ctx.ntfs;

    let bdfe_base = match hooks {
        Some(h) if !h.bdfe.is_null() => h.bdfe,
        _ => unsafe { load_u32(ctx.f70.add(16)) as usize as *const u8 },
    };
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    unsafe {
        core::arch::asm!("cld");
    }

    let guest_attrs = unsafe { load_u32(bdfe_base) };
    let old_flags = unsafe { load_u32(entry.add(I30_FILE_FLAGS)) };
    unsafe { store_u32(entry.add(I30_FILE_FLAGS), apply_file_flags(old_flags, guest_attrs)) };

    let (c_lo, c_hi) =
        unsafe { call_calc_time(ctx.calc_time, bdfe_base.add(BDFE_CTIME_OFF), hooks) };
    unsafe { store_filetime(entry.add(I30_FILE_CREATED), c_lo, c_hi) };

    let (a_lo, a_hi) =
        unsafe { call_calc_time(ctx.calc_time, bdfe_base.add(BDFE_ATIME_OFF), hooks) };
    unsafe { store_filetime(entry.add(I30_FILE_ACCESSED), a_lo, a_hi) };

    let (m_lo, m_hi) =
        unsafe { call_calc_time(ctx.calc_time, bdfe_base.add(BDFE_MTIME_OFF), hooks) };
    unsafe { store_filetime(entry.add(I30_FILE_MODIFIED), m_lo, m_hi) };

    unsafe { call_write_record(ctx.write_record, record, last_read, ntfs, hooks) };
    unsafe { call_done(ctx.done, ntfs, hooks) }
}

/// `stdcall` entry used by the FASM trampoline (`ret 4`).
///
/// # Safety
/// Same as [`ntfs_set_file_info`].
#[inline(always)]
pub unsafe fn ntfs_set_file_info_ptr(ctx: *mut NtfsSetFileInfoCtx) -> u32 {
    unsafe { ntfs_set_file_info(ctx, None) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{ntfs_calculate_time, ntfs_calculate_time_ptr, BdfeTime};

    #[repr(C)]
    struct Trace {
        calc: u32,
        write: u32,
        done: u32,
        last_record: usize,
        last_read: u32,
        last_ntfs: usize,
        done_ntfs: usize,
        write_eax: u32,
        done_eax: u32,
        calc_ptrs: [usize; 4],
    }

    unsafe fn hook_calc(state: *mut u8, bdfe: *const u8) -> (u32, u32) {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.calc = t.calc.wrapping_add(1);
        if (t.calc as usize) <= t.calc_ptrs.len() {
            t.calc_ptrs[(t.calc - 1) as usize] = bdfe as usize;
        }
        unsafe { ntfs_calculate_time_ptr(bdfe) }
    }

    unsafe fn hook_write(state: *mut u8, record: *mut u8, last_read: u32, ntfs: *mut u8) {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.write = t.write.wrapping_add(1);
        t.last_record = record as usize;
        t.last_read = last_read;
        t.last_ntfs = ntfs as usize;
        let _ = t.write_eax;
    }

    unsafe fn hook_done(state: *mut u8, ntfs: *mut u8) -> u32 {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.done = t.done.wrapping_add(1);
        t.done_ntfs = ntfs as usize;
        t.done_eax
    }

    fn hooks(t: &mut Trace, bdfe: *const u8) -> NtfsSetFileInfoHooks {
        NtfsSetFileInfoHooks {
            calc_time: hook_calc,
            write_record: hook_write,
            done: hook_done,
            state: t as *mut Trace as *mut u8,
            bdfe,
        }
    }

    fn bdfe_bytes(t: BdfeTime) -> [u8; 8] {
        t.to_bytes()
    }

    fn soak_ctime() -> BdfeTime {
        BdfeTime {
            sec: 2,
            min: 27,
            hour: 12,
            day: 14,
            month: 8,
            year: 2026,
        }
    }

    fn soak_atime() -> BdfeTime {
        BdfeTime {
            sec: 11,
            min: 22,
            hour: 14,
            day: 4,
            month: 7,
            year: 2012,
        }
    }

    fn soak_mtime() -> BdfeTime {
        BdfeTime {
            sec: 30,
            min: 5,
            hour: 9,
            day: 23,
            month: 11,
            year: 2018,
        }
    }

    fn make_ctx(
        f70: &mut [u8],
        entry: &mut [u8],
        record: &mut [u8],
        ntfs: &mut [u8],
        last_read: u32,
    ) -> NtfsSetFileInfoCtx {
        NtfsSetFileInfoCtx {
            f70: f70.as_mut_ptr(),
            ntfs: ntfs.as_mut_ptr(),
            entry: entry.as_mut_ptr(),
            record: record.as_mut_ptr(),
            last_read,
            calc_time: 1,
            write_record: 1,
            done: 1,
        }
    }

    fn load_ft(buf: &[u8], off: usize) -> u64 {
        let lo = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as u64;
        let hi = u32::from_le_bytes(buf[off + 4..off + 8].try_into().unwrap()) as u64;
        lo | (hi << 32)
    }

    #[test]
    fn ctx_layout_size() {
        assert_eq!(NTFS_SET_FILE_INFO_CTX_SIZE, 32);
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(core::mem::size_of::<NtfsSetFileInfoCtx>(), 32);
            assert_eq!(core::mem::align_of::<NtfsSetFileInfoCtx>(), 4);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, f70), 0);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, ntfs), 4);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, entry), 8);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, record), 12);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, last_read), 16);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, calc_time), 20);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, write_record), 24);
            assert_eq!(core::mem::offset_of!(NtfsSetFileInfoCtx, done), 28);
        }
    }

    #[test]
    fn flag_mask_preserves_directory_bit28() {
        let old = 0x1000_0020;
        let out = apply_file_flags(old, 0x13);
        assert_eq!(out & 0xFF, 0x03);
        assert_eq!(out & 0x1000_0000, 0x1000_0000);
        assert_eq!(apply_file_flags(0x20, 0x10) & 0xFF, 0);
        assert_eq!(apply_file_flags(0x27, 0) & 0xFF, 0);
        assert_eq!(apply_file_flags(0x00, 0x27) & 0xFF, 0x27);
    }

    #[test]
    fn success_writes_i30_times_and_sequences() {
        let mut t = Trace {
            calc: 0,
            write: 0,
            done: 0,
            last_record: 0,
            last_read: 0,
            last_ntfs: 0,
            done_ntfs: 0,
            write_eax: 1,
            done_eax: 0,
            calc_ptrs: [0; 4],
        };
        let mut f70 = [0u8; 32];
        let mut bdfe = [0u8; 32];
        bdfe[0..4].copy_from_slice(&0x21u32.to_le_bytes());
        bdfe[8..16].copy_from_slice(&bdfe_bytes(soak_ctime()));
        bdfe[16..24].copy_from_slice(&bdfe_bytes(soak_atime()));
        bdfe[24..32].copy_from_slice(&bdfe_bytes(soak_mtime()));
        let bdfe_ptr = bdfe.as_ptr() as u32;
        f70[16..20].copy_from_slice(&bdfe_ptr.to_le_bytes());

        let mut entry = [0u8; 0x60];
        entry[I30_FILE_FLAGS..I30_FILE_FLAGS + 4]
            .copy_from_slice(&0x1000_0020u32.to_le_bytes());
        entry[I30_RECORD_MODIFIED..I30_RECORD_MODIFIED + 8].copy_from_slice(&[0xAA; 8]);
        entry[I30_FILE_REAL_SIZE..I30_FILE_REAL_SIZE + 8].copy_from_slice(&57u64.to_le_bytes());
        let mut record = [0u8; 16];
        record[0] = b'I';
        let mut ntfs = [0u8; 8];
        let mut ctx = make_ctx(&mut f70, &mut entry, &mut record, &mut ntfs, 0x1234);

        let status = unsafe { ntfs_set_file_info(&mut ctx, Some(hooks(&mut t, bdfe.as_ptr()))) };
        assert_eq!(status, 0);
        assert_eq!(t.calc, 3);
        assert_eq!(t.write, 1);
        assert_eq!(t.done, 1);
        assert_eq!(t.last_record, record.as_ptr() as usize);
        assert_eq!(t.last_read, 0x1234);
        assert_eq!(t.last_ntfs, ntfs.as_ptr() as usize);
        assert_eq!(t.done_ntfs, ntfs.as_ptr() as usize);
        let base = bdfe.as_ptr() as usize;
        assert_eq!(t.calc_ptrs[0], base + 8);
        assert_eq!(t.calc_ptrs[1], base + 16);
        assert_eq!(t.calc_ptrs[2], base + 24);

        let (c_lo, c_hi) = ntfs_calculate_time(soak_ctime());
        let (a_lo, a_hi) = ntfs_calculate_time(soak_atime());
        let (m_lo, m_hi) = ntfs_calculate_time(soak_mtime());
        assert_eq!(load_ft(&entry, I30_FILE_CREATED), pack(c_lo, c_hi));
        assert_eq!(load_ft(&entry, I30_FILE_ACCESSED), pack(a_lo, a_hi));
        assert_eq!(load_ft(&entry, I30_FILE_MODIFIED), pack(m_lo, m_hi));
        assert_eq!(load_ft(&entry, I30_FILE_ACCESSED), 129858853310000000);
        assert_eq!(load_ft(&entry, I30_FILE_MODIFIED), 131874375300000000);

        let flags = u32::from_le_bytes(entry[I30_FILE_FLAGS..I30_FILE_FLAGS + 4].try_into().unwrap());
        assert_eq!(flags, 0x1000_0021);
        assert_eq!(&entry[I30_RECORD_MODIFIED..I30_RECORD_MODIFIED + 8], &[0xAA; 8]);
        assert_eq!(load_ft(&entry, I30_FILE_REAL_SIZE), 57);
    }

    fn pack(lo: u32, hi: u32) -> u64 {
        (lo as u64) | ((hi as u64) << 32)
    }

    #[test]
    fn write_error_still_calls_done_and_returns_zero() {
        let mut t = Trace {
            calc: 0,
            write: 0,
            done: 0,
            last_record: 0,
            last_read: 0,
            last_ntfs: 0,
            done_ntfs: 0,
            write_eax: 11,
            done_eax: 0,
            calc_ptrs: [0; 4],
        };
        let mut f70 = [0u8; 32];
        let mut bdfe = [0u8; 32];
        let bdfe_ptr = bdfe.as_ptr() as u32;
        f70[16..20].copy_from_slice(&bdfe_ptr.to_le_bytes());
        let mut entry = [0u8; 0x60];
        let mut record = [0u8; 4];
        let mut ntfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut entry, &mut record, &mut ntfs, 7);
        let status = unsafe { ntfs_set_file_info(&mut ctx, Some(hooks(&mut t, bdfe.as_ptr()))) };
        assert_eq!(status, 0);
        assert_eq!(t.write, 1);
        assert_eq!(t.done, 1);
    }

    #[test]
    fn idempotent_second_write_same_values() {
        let mut t = Trace {
            calc: 0,
            write: 0,
            done: 0,
            last_record: 0,
            last_read: 0,
            last_ntfs: 0,
            done_ntfs: 0,
            write_eax: 0,
            done_eax: 0,
            calc_ptrs: [0; 4],
        };
        let mut f70 = [0u8; 32];
        let mut bdfe = [0u8; 32];
        bdfe[16..24].copy_from_slice(&bdfe_bytes(soak_atime()));
        bdfe[24..32].copy_from_slice(&bdfe_bytes(soak_mtime()));
        let bdfe_ptr = bdfe.as_ptr() as u32;
        f70[16..20].copy_from_slice(&bdfe_ptr.to_le_bytes());
        let mut entry = [0u8; 0x60];
        let mut record = [0u8; 4];
        let mut ntfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut entry, &mut record, &mut ntfs, 1);
        unsafe { ntfs_set_file_info(&mut ctx, Some(hooks(&mut t, bdfe.as_ptr()))) };
        let first_a = load_ft(&entry, I30_FILE_ACCESSED);
        let first_m = load_ft(&entry, I30_FILE_MODIFIED);
        t.calc = 0;
        t.write = 0;
        t.done = 0;
        unsafe { ntfs_set_file_info(&mut ctx, Some(hooks(&mut t, bdfe.as_ptr()))) };
        assert_eq!(load_ft(&entry, I30_FILE_ACCESSED), first_a);
        assert_eq!(load_ft(&entry, I30_FILE_MODIFIED), first_m);
        assert_eq!(t.calc, 3);
        assert_eq!(t.write, 1);
        assert_eq!(t.done, 1);
    }

    #[test]
    fn epoch_boundary_filetime() {
        let (lo, hi) = ntfs_calculate_time(BdfeTime {
            sec: 0,
            min: 0,
            hour: 0,
            day: 1,
            month: 1,
            year: 2001,
        });
        assert_eq!(lo, 3365781504);
        assert_eq!(hi, 29389701);
    }
}
