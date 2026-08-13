//! Cut CV: `ext_SetFileInfo` — EXT plugin Path B leaf (sysfn70.6).
//!
//! Matches `kernel/fs/ext.inc` FASM body after `extfsWritingInit` has already
//! acquired `EXTFS.Lock` (or non-locally returned on RO). Reloc-free: f.70 /
//! EXTFS / path / inode buffer / DISK* and FASM helper addresses are injected
//! via [`ExtSetFileInfoCtx`].
//!
//! Lock acquire and RO non-local return stay in FASM. Rust must call
//! `ext_unlock` on every path. Helpers (`findInode`, `writeInode`, …) stay
//! FASM — only the leaf orchestration is Rust.

use crate::time::UNIXTIME_TO_KOS_OFFSET;

/// Cut CV differential / smoke marker (`'ESFI'`).
pub const EXT_SET_FILE_INFO_PRNG_SEED: u32 = 0x4553_4649;

/// Kolibri `ERROR_FILE_NOT_FOUND`.
pub const ERROR_FILE_NOT_FOUND: u32 = 5;
/// Kolibri `ERROR_ACCESS_DENIED`.
pub const ERROR_ACCESS_DENIED: u32 = 10;
/// Kolibri `ERROR_DEVICE`.
pub const ERROR_DEVICE: u32 = 11;

/// `EXT4_IMMUTABLE_FL` in `kernel/fs/ext.inc`.
pub const EXT4_IMMUTABLE_FL: u32 = 0x10;

/// `INODE.aTime` byte offset.
pub const INODE_ATIME_OFF: usize = 8;
/// `INODE.mTime` byte offset.
pub const INODE_MTIME_OFF: usize = 16;
/// `INODE.featureFlags` byte offset.
pub const INODE_FEATURE_FLAGS_OFF: usize = 32;

/// SetFileInfo BDFE atime offset within the 32-byte buffer (`[ebx+16]+16`).
pub const BDFE_ATIME_OFF: usize = 16;
/// SetFileInfo BDFE mtime offset (`+8` after atime).
pub const BDFE_MTIME_OFF: usize = 24;

/// Injected trampoline context (11 dwords = 44 bytes).
pub const EXT_SET_FILE_INFO_CTX_SIZE: usize = 44;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExtSetFileInfoCtx {
    /// f.70 parameter block (`ebx` at plugin entry).
    pub f70: *mut u8,
    /// `EXTFS` / `PARTITION` (`ebp`).
    pub extfs: *mut u8,
    /// UTF-8 path (`esi`).
    pub path: *mut u8,
    /// `&EXTFS.inodeBuffer`.
    pub inode_buf: *mut u8,
    /// `PARTITION.Disk` pointer value (for `disk_sync`).
    pub disk: *mut u8,
    /// FASM `findInode` address.
    pub find_inode: u32,
    /// FASM `fsCalculateTime` address (Cut G trampoline).
    pub calc_time: u32,
    /// FASM `writeInode` address.
    pub write_inode: u32,
    /// FASM `writeSuperblock` address.
    pub write_sb: u32,
    /// FASM `disk_sync` address.
    pub disk_sync: u32,
    /// FASM `ext_unlock` address.
    pub unlock: u32,
}

/// Register-ABI callback result for CF-bearing helpers.
#[derive(Clone, Copy)]
pub struct CfOut {
    pub cf: bool,
    pub eax: u32,
    pub esi: u32,
}

/// Host-side hooks (kernel uses ctx function pointers).
pub struct ExtSetFileInfoHooks {
    pub find_inode: unsafe fn(*mut u8, *mut u8, *mut u8) -> CfOut,
    pub calc_time: unsafe fn(*mut u8, *const u8) -> u32,
    pub write_inode: unsafe fn(*mut u8, u32, *mut u8) -> CfOut,
    pub write_sb: unsafe fn(*mut u8),
    pub disk_sync: unsafe fn(*mut u8, *mut u8),
    pub unlock: unsafe fn(*mut u8),
    pub state: *mut u8,
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
unsafe fn call_find_inode(
    fn_ptr: u32,
    path: *mut u8,
    extfs: *mut u8,
    hooks: Option<&ExtSetFileInfoHooks>,
) -> CfOut {
    if let Some(h) = hooks {
        return unsafe { (h.find_inode)(h.state, path, extfs) };
    }
    unsafe { invoke_find_inode(fn_ptr, path, extfs) }
}

#[inline(always)]
unsafe fn invoke_find_inode(fn_ptr: u32, path: *mut u8, extfs: *mut u8) -> CfOut {
    if fn_ptr == 0 {
        return CfOut {
            cf: true,
            eax: ERROR_FILE_NOT_FOUND,
            esi: 0,
        };
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        let mut esi_r = path as u32;
        let mut cf_e: u32;
        // Pin extfs in EDI (not ECX): `lateout("ecx")` for CF must not destroy
        // a live EXTFS pointer that LLVM kept in ECX (REG-017/018 lesson).
        unsafe {
            core::arch::asm!(
                "cld",
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",
                "mov ebp, edi",
                "mov esi, edx",
                "call ebx",
                "sbb ecx, ecx",
                "mov edx, esi",
                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("ebx") fn_ptr,
                in("edi") extfs as u32,
                inout("edx") esi_r,
                lateout("eax") eax_r,
                lateout("ecx") cf_e,
            );
        }
        CfOut {
            cf: cf_e != 0,
            eax: eax_r,
            esi: esi_r,
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, path, extfs);
        CfOut {
            cf: true,
            eax: ERROR_FILE_NOT_FOUND,
            esi: 0,
        }
    }
}

#[inline(always)]
unsafe fn call_calc_time(
    fn_ptr: u32,
    bdfe: *const u8,
    hooks: Option<&ExtSetFileInfoHooks>,
) -> u32 {
    if let Some(h) = hooks {
        return unsafe { (h.calc_time)(h.state, bdfe) };
    }
    unsafe { invoke_calc_time(fn_ptr, bdfe) }
}

#[inline(always)]
unsafe fn invoke_calc_time(fn_ptr: u32, bdfe: *const u8) -> u32 {
    if fn_ptr == 0 {
        // Host unit tests inject hooks; kernel trampoline always sets calc_time.
        // Do not call `fs_calculate_time_ptr` here — that would pull Cut G into
        // this reloc-free section and bloat the blob.
        let _ = bdfe;
        return 0;
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        unsafe {
            core::arch::asm!(
                "cld",
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
                lateout("ecx") _,
            );
        }
        eax_r
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = bdfe;
        0
    }
}

#[inline(always)]
unsafe fn call_write_inode(
    fn_ptr: u32,
    inode_num: u32,
    inode_buf: *mut u8,
    extfs: *mut u8,
    hooks: Option<&ExtSetFileInfoHooks>,
) -> CfOut {
    if let Some(h) = hooks {
        return unsafe { (h.write_inode)(h.state, inode_num, inode_buf) };
    }
    unsafe { invoke_write_inode(fn_ptr, inode_num, inode_buf, extfs) }
}

#[inline(always)]
unsafe fn invoke_write_inode(
    fn_ptr: u32,
    inode_num: u32,
    inode_buf: *mut u8,
    extfs: *mut u8,
) -> CfOut {
    if fn_ptr == 0 {
        return CfOut {
            cf: true,
            eax: ERROR_DEVICE,
            esi: 0,
        };
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r = inode_num;
        let mut cf_e: u32;
        unsafe {
            // EXTFS / buf / fn via arbitrary regs — never `lateout` a reg that
            // still holds a live pointer (REG-017). CF in ECX only.
            core::arch::asm!(
                "cld",
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",
                "mov ebp, {extfs}",
                "mov ebx, {buf}",
                "call {f}",
                "sbb ecx, ecx",
                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                extfs = in(reg) extfs as u32,
                buf = in(reg) inode_buf as u32,
                f = in(reg) fn_ptr,
                inout("eax") eax_r,
                lateout("ecx") cf_e,
            );
        }
        CfOut {
            cf: cf_e != 0,
            eax: eax_r,
            esi: 0,
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, inode_num, inode_buf, extfs);
        CfOut {
            cf: true,
            eax: ERROR_DEVICE,
            esi: 0,
        }
    }
}

#[inline(always)]
unsafe fn call_write_sb(fn_ptr: u32, extfs: *mut u8, hooks: Option<&ExtSetFileInfoHooks>) {
    if let Some(h) = hooks {
        unsafe { (h.write_sb)(h.state) };
        return;
    }
    unsafe { invoke_write_sb(fn_ptr, extfs) }
}

#[inline(always)]
unsafe fn invoke_write_sb(fn_ptr: u32, extfs: *mut u8) {
    if fn_ptr == 0 {
        return;
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        unsafe {
            core::arch::asm!(
                "cld",
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",
                "mov ebp, ecx",
                "call ebx",
                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("ebx") fn_ptr,
                in("ecx") extfs as u32,
                lateout("eax") _,
                lateout("edx") _,
            );
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, extfs);
    }
}

#[inline(always)]
unsafe fn call_disk_sync(
    fn_ptr: u32,
    disk: *mut u8,
    hooks: Option<&ExtSetFileInfoHooks>,
) {
    if let Some(h) = hooks {
        unsafe { (h.disk_sync)(h.state, disk) };
        return;
    }
    unsafe { invoke_disk_sync(fn_ptr, disk) }
}

#[inline(always)]
unsafe fn invoke_disk_sync(fn_ptr: u32, disk: *mut u8) {
    if fn_ptr == 0 {
        return;
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        unsafe {
            core::arch::asm!(
                "cld",
                "push ebx",
                "push ebp",
                "push edi",
                "mov esi, edx",
                "call ebx",
                "pop edi",
                "pop ebp",
                "pop ebx",
                in("ebx") fn_ptr,
                in("edx") disk as u32,
                lateout("eax") _,
                lateout("ecx") _,
            );
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, disk);
    }
}

#[inline(always)]
unsafe fn call_unlock(fn_ptr: u32, extfs: *mut u8, hooks: Option<&ExtSetFileInfoHooks>) {
    if let Some(h) = hooks {
        unsafe { (h.unlock)(h.state) };
        return;
    }
    unsafe { invoke_unlock(fn_ptr, extfs) }
}

#[inline(always)]
unsafe fn invoke_unlock(fn_ptr: u32, extfs: *mut u8) {
    if fn_ptr == 0 {
        return;
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        unsafe {
            // `ext_unlock` is `lea ecx,[ebp+Lock]; jmp mutex_unlock` — plain call.
            core::arch::asm!(
                "cld",
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",
                "mov ebp, ecx",
                "call ebx",
                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("ebx") fn_ptr,
                in("ecx") extfs as u32,
                lateout("eax") _,
                lateout("edx") _,
            );
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, extfs);
    }
}

/// FASM-faithful `ext_SetFileInfo` body after `extfsWritingInit`.
///
/// Returns Kolibri status in EAX (0 = success). Always unlocks when `unlock`
/// is non-zero (production trampoline always injects `ext_unlock`).
///
/// # Safety
/// `ctx` and all injected pointers/callbacks must be valid for the call.
#[inline(always)]
pub unsafe fn ext_set_file_info(
    ctx: *mut ExtSetFileInfoCtx,
    hooks: Option<ExtSetFileInfoHooks>,
) -> u32 {
    unsafe { ext_set_file_info_inner(&*ctx, hooks.as_ref()) }
}

#[inline(always)]
unsafe fn ext_set_file_info_inner(
    ctx: &ExtSetFileInfoCtx,
    hooks: Option<&ExtSetFileInfoHooks>,
) -> u32 {
    // Reload from ctx after every callback — inline asm lateouts must not
    // leave a stale EXTFS pointer in a clobbered register.
    let path = ctx.path;
    let inode_buf = ctx.inode_buf;
    let disk = ctx.disk;

    let found = unsafe { call_find_inode(ctx.find_inode, path, ctx.extfs, hooks) };
    if found.cf {
        unsafe { call_unlock(ctx.unlock, ctx.extfs, hooks) };
        return found.eax;
    }
    let inode_num = found.esi;

    let flags = unsafe { load_u32(inode_buf.add(INODE_FEATURE_FLAGS_OFF)) };
    if (flags & EXT4_IMMUTABLE_FL) != 0 {
        unsafe { call_unlock(ctx.unlock, ctx.extfs, hooks) };
        return ERROR_ACCESS_DENIED;
    }

    let bdfe_base = unsafe { load_u32(ctx.f70.add(16)) as usize as *const u8 };
    let atime_bdfe = unsafe { bdfe_base.add(BDFE_ATIME_OFF) };
    let mtime_bdfe = unsafe { bdfe_base.add(BDFE_MTIME_OFF) };

    let atime_kos = unsafe { call_calc_time(ctx.calc_time, atime_bdfe, hooks) };
    let atime = atime_kos.wrapping_add(UNIXTIME_TO_KOS_OFFSET);
    unsafe { store_u32(inode_buf.add(INODE_ATIME_OFF), atime) };

    let mtime_kos = unsafe { call_calc_time(ctx.calc_time, mtime_bdfe, hooks) };
    let mtime = mtime_kos.wrapping_add(UNIXTIME_TO_KOS_OFFSET);
    unsafe { store_u32(inode_buf.add(INODE_MTIME_OFF), mtime) };

    let wr = unsafe {
        call_write_inode(ctx.write_inode, inode_num, inode_buf, ctx.extfs, hooks)
    };
    if wr.cf {
        unsafe { call_unlock(ctx.unlock, ctx.extfs, hooks) };
        return wr.eax;
    }

    unsafe { call_write_sb(ctx.write_sb, ctx.extfs, hooks) };
    unsafe { call_disk_sync(ctx.disk_sync, disk, hooks) };
    unsafe { call_unlock(ctx.unlock, ctx.extfs, hooks) };
    0
}

/// `stdcall` entry used by the FASM trampoline (`ret 4`).
///
/// # Safety
/// Same as [`ext_set_file_info`].
#[inline(always)]
pub unsafe fn ext_set_file_info_ptr(ctx: *mut ExtSetFileInfoCtx) -> u32 {
    unsafe { ext_set_file_info(ctx, None) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::BdfeTime;

    #[repr(C)]
    struct Trace {
        find: u32,
        calc: u32,
        write: u32,
        sb: u32,
        sync: u32,
        unlock: u32,
        find_cf: bool,
        find_eax: u32,
        find_ino: u32,
        write_cf: bool,
        write_eax: u32,
        last_ino: u32,
        last_buf: usize,
        last_disk: usize,
    }

    unsafe fn hook_find(state: *mut u8, _path: *mut u8, _extfs: *mut u8) -> CfOut {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.find = t.find.wrapping_add(1);
        CfOut {
            cf: t.find_cf,
            eax: t.find_eax,
            esi: t.find_ino,
        }
    }

    unsafe fn hook_calc(state: *mut u8, _bdfe: *const u8) -> u32 {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.calc = t.calc.wrapping_add(1);
        // KOS secs = Unix − UNIXTIME_TO_KOS_OFFSET (oracle fixture / epoch).
        if t.calc == 1 {
            return 1_341_411_731u32.wrapping_sub(UNIXTIME_TO_KOS_OFFSET);
        }
        if t.calc == 2 {
            return 1_542_963_930u32.wrapping_sub(UNIXTIME_TO_KOS_OFFSET);
        }
        0
    }

    unsafe fn hook_write(state: *mut u8, inode_num: u32, inode_buf: *mut u8) -> CfOut {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.write = t.write.wrapping_add(1);
        t.last_ino = inode_num;
        t.last_buf = inode_buf as usize;
        CfOut {
            cf: t.write_cf,
            eax: t.write_eax,
            esi: 0,
        }
    }

    unsafe fn hook_sb(state: *mut u8) {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.sb = t.sb.wrapping_add(1);
    }

    unsafe fn hook_sync(state: *mut u8, disk: *mut u8) {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.sync = t.sync.wrapping_add(1);
        t.last_disk = disk as usize;
    }

    unsafe fn hook_unlock(state: *mut u8) {
        let t = unsafe { &mut *(state as *mut Trace) };
        t.unlock = t.unlock.wrapping_add(1);
    }

    fn hooks(t: &mut Trace) -> ExtSetFileInfoHooks {
        ExtSetFileInfoHooks {
            find_inode: hook_find,
            calc_time: hook_calc,
            write_inode: hook_write,
            write_sb: hook_sb,
            disk_sync: hook_sync,
            unlock: hook_unlock,
            state: t as *mut Trace as *mut u8,
        }
    }

    fn bdfe_bytes(t: BdfeTime) -> [u8; 8] {
        t.to_bytes()
    }

    fn make_ctx(
        f70: &mut [u8],
        path: &mut [u8],
        inode: &mut [u8],
        disk: &mut [u8],
        extfs: &mut [u8],
    ) -> ExtSetFileInfoCtx {
        ExtSetFileInfoCtx {
            f70: f70.as_mut_ptr(),
            extfs: extfs.as_mut_ptr(),
            path: path.as_mut_ptr(),
            inode_buf: inode.as_mut_ptr(),
            disk: disk.as_mut_ptr(),
            find_inode: 1,
            calc_time: 0,
            write_inode: 1,
            write_sb: 1,
            disk_sync: 1,
            unlock: 1,
        }
    }

    #[test]
    fn ctx_layout_size() {
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(core::mem::size_of::<ExtSetFileInfoCtx>(), 44);
            assert_eq!(core::mem::align_of::<ExtSetFileInfoCtx>(), 4);
        }
    }

    #[test]
    fn success_writes_atime_mtime_and_sequences() {
        let mut t = Trace {
            find: 0,
            calc: 0,
            write: 0,
            sb: 0,
            sync: 0,
            unlock: 0,
            find_cf: false,
            find_eax: 0,
            find_ino: 12,
            write_cf: false,
            write_eax: 0,
            last_ino: 0,
            last_buf: 0,
            last_disk: 0,
        };
        let mut f70 = [0u8; 32];
        let mut bdfe = [0u8; 32];
        let at = bdfe_bytes(BdfeTime {
            sec: 11,
            min: 22,
            hour: 14,
            day: 4,
            month: 7,
            year: 2012,
        });
        let mt = bdfe_bytes(BdfeTime {
            sec: 30,
            min: 5,
            hour: 9,
            day: 23,
            month: 11,
            year: 2018,
        });
        bdfe[16..24].copy_from_slice(&at);
        bdfe[24..32].copy_from_slice(&mt);
        let bdfe_ptr = bdfe.as_ptr() as u32;
        f70[16..20].copy_from_slice(&bdfe_ptr.to_le_bytes());

        let mut path = *b"ROOT.TXT\0";
        let mut inode = [0u8; 160];
        let mut disk = [0u8; 4];
        let mut extfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut path, &mut inode, &mut disk, &mut extfs);

        let status = unsafe { ext_set_file_info(&mut ctx, Some(hooks(&mut t))) };
        assert_eq!(status, 0);
        assert_eq!(t.find, 1);
        assert_eq!(t.calc, 2);
        assert_eq!(t.write, 1);
        assert_eq!(t.sb, 1);
        assert_eq!(t.sync, 1);
        assert_eq!(t.unlock, 1);
        assert_eq!(t.last_ino, 12);
        assert_eq!(t.last_buf, inode.as_ptr() as usize);
        assert_eq!(t.last_disk, disk.as_ptr() as usize);

        let atime = u32::from_le_bytes(inode[8..12].try_into().unwrap());
        let mtime = u32::from_le_bytes(inode[16..20].try_into().unwrap());
        // Oracle fixture: Unix 1341411731 / 1542963930
        assert_eq!(atime, 1_341_411_731);
        assert_eq!(mtime, 1_542_963_930);
    }

    #[test]
    fn missing_path_unlocks_no_writes() {
        let mut t = Trace {
            find: 0,
            calc: 0,
            write: 0,
            sb: 0,
            sync: 0,
            unlock: 0,
            find_cf: true,
            find_eax: ERROR_FILE_NOT_FOUND,
            find_ino: 0,
            write_cf: false,
            write_eax: 0,
            last_ino: 0,
            last_buf: 0,
            last_disk: 0,
        };
        let mut f70 = [0u8; 32];
        let mut path = *b"NOPE\0";
        let mut inode = [0u8; 160];
        let mut disk = [0u8; 4];
        let mut extfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut path, &mut inode, &mut disk, &mut extfs);
        let status = unsafe { ext_set_file_info(&mut ctx, Some(hooks(&mut t))) };
        assert_eq!(status, ERROR_FILE_NOT_FOUND);
        assert_eq!(t.find, 1);
        assert_eq!(t.calc, 0);
        assert_eq!(t.write, 0);
        assert_eq!(t.sb, 0);
        assert_eq!(t.sync, 0);
        assert_eq!(t.unlock, 1);
    }

    #[test]
    fn immutable_denied_no_sb_sync() {
        let mut t = Trace {
            find: 0,
            calc: 0,
            write: 0,
            sb: 0,
            sync: 0,
            unlock: 0,
            find_cf: false,
            find_eax: 0,
            find_ino: 7,
            write_cf: false,
            write_eax: 0,
            last_ino: 0,
            last_buf: 0,
            last_disk: 0,
        };
        let mut f70 = [0u8; 32];
        let mut path = *b"X\0";
        let mut inode = [0u8; 160];
        inode[32..36].copy_from_slice(&EXT4_IMMUTABLE_FL.to_le_bytes());
        let mut disk = [0u8; 4];
        let mut extfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut path, &mut inode, &mut disk, &mut extfs);
        let status = unsafe { ext_set_file_info(&mut ctx, Some(hooks(&mut t))) };
        assert_eq!(status, ERROR_ACCESS_DENIED);
        assert_eq!(t.write, 0);
        assert_eq!(t.sb, 0);
        assert_eq!(t.sync, 0);
        assert_eq!(t.unlock, 1);
    }

    #[test]
    fn write_inode_fail_no_sb_sync() {
        let mut t = Trace {
            find: 0,
            calc: 0,
            write: 0,
            sb: 0,
            sync: 0,
            unlock: 0,
            find_cf: false,
            find_eax: 0,
            find_ino: 3,
            write_cf: true,
            write_eax: ERROR_DEVICE,
            last_ino: 0,
            last_buf: 0,
            last_disk: 0,
        };
        let mut f70 = [0u8; 32];
        let mut bdfe = [0u8; 32];
        let zero = bdfe_bytes(BdfeTime {
            sec: 0,
            min: 0,
            hour: 0,
            day: 1,
            month: 1,
            year: 2001,
        });
        bdfe[16..24].copy_from_slice(&zero);
        bdfe[24..32].copy_from_slice(&zero);
        let bdfe_ptr = bdfe.as_ptr() as u32;
        f70[16..20].copy_from_slice(&bdfe_ptr.to_le_bytes());
        let mut path = *b"X\0";
        let mut inode = [0u8; 160];
        let mut disk = [0u8; 4];
        let mut extfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut path, &mut inode, &mut disk, &mut extfs);
        let status = unsafe { ext_set_file_info(&mut ctx, Some(hooks(&mut t))) };
        assert_eq!(status, ERROR_DEVICE);
        assert_eq!(t.write, 1);
        assert_eq!(t.sb, 0);
        assert_eq!(t.sync, 0);
        assert_eq!(t.unlock, 1);
    }

    #[test]
    fn wrapping_add_offset() {
        // kos near u32::MAX → wrapping unix store matches FASM `add`.
        let kos = 0xffff_ffff_u32;
        assert_eq!(
            kos.wrapping_add(UNIXTIME_TO_KOS_OFFSET),
            kos.wrapping_add(978_307_200)
        );
    }

    #[test]
    fn idempotent_same_value() {
        let mut t = Trace {
            find: 0,
            calc: 0,
            write: 0,
            sb: 0,
            sync: 0,
            unlock: 0,
            find_cf: false,
            find_eax: 0,
            find_ino: 12,
            write_cf: false,
            write_eax: 0,
            last_ino: 0,
            last_buf: 0,
            last_disk: 0,
        };
        let mut f70 = [0u8; 32];
        let mut bdfe = [0u8; 32];
        let at = bdfe_bytes(BdfeTime {
            sec: 11,
            min: 22,
            hour: 14,
            day: 4,
            month: 7,
            year: 2012,
        });
        bdfe[16..24].copy_from_slice(&at);
        bdfe[24..32].copy_from_slice(&at);
        let bdfe_ptr = bdfe.as_ptr() as u32;
        f70[16..20].copy_from_slice(&bdfe_ptr.to_le_bytes());
        let mut path = *b"ROOT.TXT\0";
        let mut inode = [0u8; 160];
        // Pre-seed same unix times
        inode[8..12].copy_from_slice(&1_341_411_731u32.to_le_bytes());
        inode[16..20].copy_from_slice(&1_341_411_731u32.to_le_bytes());
        let mut disk = [0u8; 4];
        let mut extfs = [0u8; 4];
        let mut ctx = make_ctx(&mut f70, &mut path, &mut inode, &mut disk, &mut extfs);
        let status = unsafe { ext_set_file_info(&mut ctx, Some(hooks(&mut t))) };
        assert_eq!(status, 0);
        assert_eq!(t.sb, 1);
        assert_eq!(t.sync, 1);
        let atime = u32::from_le_bytes(inode[8..12].try_into().unwrap());
        assert_eq!(atime, 1_341_411_731);
    }
}
