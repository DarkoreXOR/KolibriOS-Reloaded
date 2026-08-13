//! Cut CQ: `exFAT_find_lfn` — UTF-8 path-component lookup in an exFAT directory.
//!
//! Matches `kernel/fs/exfat.inc` FASM leaf. Reloc-free: `exFAT*` field
//! pointers, stack `first`/`next`, and `exFAT_get_name` are injected via
//! [`ExFatFindLfnCtx`]. Cut AB `utf8to16`, Cut C `utf16toUpper` (`sub eax`),
//! and Cut AI NameHash are inlined (no cross-blob relocs).
//!
//! LFN **entry** assembly (0x85 / 0xC0 / 0xC1) stays in FASM `exFAT_get_name`
//! on the production kernel; host tests inject an independent fixture.

use crate::exfat_checksum::exfat_hash_calculate;
use crate::utf8to16::utf8to16_ptr;

/// Cut CQ differential PRNG seed (`'FLFN'`).
pub const EXFAT_FIND_LFN_PRNG_SEED: u32 = 0x464C_464E;

/// FASM `sub esp, 262*2` LFN UTF-16 units (including terminator slot).
pub const LFN_UTF16_UNITS: usize = 262;

/// Kolibri `ERROR_FILE_NOT_FOUND`.
pub const ERROR_FILE_NOT_FOUND: u32 = 5;

/// Injected trampoline context (13 dwords = 52 bytes).
pub const EXFAT_FIND_LFN_CTX_SIZE: usize = 52;

/// Slash UTF-16 used as path-component terminator.
const SLASH: u16 = 0x002F;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExFatFindLfnCtx {
    pub fs: *mut u8,
    pub secondary_dir_entry: *mut u32,
    pub need_hash: *mut u32,
    /// Points at `exFAT.LFN_reserve_place` (`dd` on i686 = `usize`).
    pub lfn_reserve_place: *mut usize,
    /// Points at `exFAT.path_in_UTF8`.
    pub path_in_utf8: *mut usize,
    pub current_hash: *mut u32,
    pub valid_data_length: *mut u32,
    pub first: u32,
    pub next: u32,
    pub get_name: u32,
    pub pair: *mut u32,
    pub esi_out: *mut u8,
    pub edi_out: *mut u8,
}

/// Register-ABI callback result (`first` / `next` / `get_name`).
#[derive(Clone, Copy)]
pub struct CallbackOut {
    pub cf: bool,
    pub eax: u32,
    pub esi: *mut u8,
    pub edi: *mut u8,
}

/// Host-side directory / get_name hooks (kernel uses ctx function pointers).
pub struct ExFatFindLfnHooks {
    pub first: unsafe fn(*mut u8, *mut u32, *mut u8) -> CallbackOut,
    pub next: unsafe fn(*mut u8, *mut u32, *mut u8) -> CallbackOut,
    pub get_name: unsafe fn(*mut u8, *mut u8, *mut u8) -> CallbackOut,
    pub state: *mut u8,
}

/// FASM `utf16toUpper`: `sub eax, 32` / `sub eax, 80` on the full register.
#[inline(always)]
pub fn utf16_to_upper_eax(eax: u32) -> u32 {
    let ax = eax as u16;
    if ax < b'a' as u16 {
        return eax;
    }
    if ax <= b'z' as u16 {
        return eax.wrapping_sub(32);
    }
    if ax < 0x0430 {
        return eax;
    }
    if ax < 0x0450 {
        return eax.wrapping_sub(32);
    }
    if ax >= 0x0460 {
        return eax;
    }
    eax.wrapping_sub(80)
}

#[inline(always)]
unsafe fn load_u32(p: *mut u32) -> u32 {
    unsafe { core::ptr::read_unaligned(p) }
}

#[inline(always)]
unsafe fn store_u32(p: *mut u32, v: u32) {
    unsafe { core::ptr::write_unaligned(p, v) }
}

#[inline(always)]
unsafe fn load_usize(p: *mut usize) -> usize {
    unsafe { core::ptr::read_unaligned(p) }
}

#[inline(always)]
unsafe fn store_usize(p: *mut usize, v: usize) {
    unsafe { core::ptr::write_unaligned(p, v) }
}

#[inline(always)]
unsafe fn call_dir_fn(
    fn_ptr: u32,
    pair: *mut u32,
    fs: *mut u8,
    edi: *mut u8,
    hooks: Option<&ExFatFindLfnHooks>,
    is_first: bool,
) -> CallbackOut {
    if let Some(h) = hooks {
        if is_first {
            return unsafe { (h.first)(h.state, pair, edi) };
        }
        return unsafe { (h.next)(h.state, pair, edi) };
    }
    unsafe { invoke_kernel_dir_fn(fn_ptr, pair, fs, edi) }
}

#[inline(always)]
unsafe fn call_get_name(
    fn_ptr: u32,
    fs: *mut u8,
    edi: *mut u8,
    esi: *mut u8,
    hooks: Option<&ExFatFindLfnHooks>,
) -> CallbackOut {
    if let Some(h) = hooks {
        return unsafe { (h.get_name)(h.state, edi, esi) };
    }
    unsafe { invoke_kernel_get_name(fn_ptr, fs, edi, esi) }
}

#[inline(always)]
unsafe fn invoke_kernel_dir_fn(
    fn_ptr: u32,
    pair: *mut u32,
    fs: *mut u8,
    edi: *mut u8,
) -> CallbackOut {
    if fn_ptr == 0 {
        return CallbackOut {
            cf: true,
            eax: ERROR_FILE_NOT_FOUND,
            esi: core::ptr::null_mut(),
            edi,
        };
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax = pair as u32;
        let mut edi_r = edi as u32;
        let mut cf_e: u32;
        unsafe {
            // Capture CF with `sbb` *before* any pop. `setc cl` then `pop ecx`
            // discarded CF (REG-018). Do not `setc al` — EAX is the error code.
            // first/next are cdecl `call`/`ret` and clobber ECX/EDX/ESI (and
            // flags). Missing lateouts let LLVM reuse those regs on the next
            // directory entry (REG-019): one-shot smoke passed, live lookup hung.
            core::arch::asm!(
                "push ebx",
                "push ebp",
                "push esi",
                "mov ebx, ecx",
                "mov ebp, edx",
                "call ebx",
                "sbb ecx, ecx",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("ecx") fn_ptr,
                in("edx") fs as u32,
                inout("eax") eax,
                inout("edi") edi_r,
                lateout("ecx") cf_e,
                lateout("edx") _,
            );
        }
        CallbackOut {
            cf: cf_e != 0,
            eax,
            esi: core::ptr::null_mut(),
            edi: edi_r as *mut u8,
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fs, pair);
        CallbackOut {
            cf: true,
            eax: ERROR_FILE_NOT_FOUND,
            esi: core::ptr::null_mut(),
            edi,
        }
    }
}

#[inline(always)]
unsafe fn invoke_kernel_get_name(
    fn_ptr: u32,
    fs: *mut u8,
    edi: *mut u8,
    esi: *mut u8,
) -> CallbackOut {
    if fn_ptr == 0 {
        return CallbackOut {
            cf: true,
            eax: 0,
            esi,
            edi,
        };
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut esi_r = esi as u32;
        let mut edi_r = edi as u32;
        let mut cf_e: u32;
        unsafe {
            // Pin f/fs/esi_in to EBX/ECX/EDX. Do **not** use `in("esi")`
            // (LLVM internal). Do **not** `mov esi, esi_in` before
            // `mov ebp, fs` (REG-017). Capture CF with `sbb eax, eax` before
            // `pop` — never `setc` into a popped register (REG-018).
            // `call` clobbers ECX (stdcall). Without `lateout("ecx")` LLVM
            // reused FS across get_name iterations (REG-019).
            core::arch::asm!(
                "cld",
                "push ebx",
                "push ebp",
                "push esi",
                "mov ebp, ecx",
                "mov esi, edx",
                "call ebx",
                "sbb eax, eax",
                "mov edx, esi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("ebx") fn_ptr,
                in("ecx") fs as u32,
                inout("edx") esi_r,
                inout("edi") edi_r,
                lateout("eax") cf_e,
                lateout("ecx") _,
            );
        }
        CallbackOut {
            cf: cf_e != 0,
            eax: 0,
            esi: esi_r as *mut u8,
            edi: edi_r as *mut u8,
        }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = fs;
        CallbackOut {
            cf: true,
            eax: 0,
            esi,
            edi,
        }
    }
}

#[inline(always)]
unsafe fn step_utf16(esi: &mut *const u8, eax: u32) -> u32 {
    let mut p = *esi;
    let out = unsafe { utf8to16_ptr(&mut p, eax) };
    *esi = p;
    utf16_to_upper_eax(out)
}

/// FASM-faithful `exFAT_find_lfn`. Returns EAX (0 = success).
///
/// Writes `ctx.esi_out` / `ctx.edi_out`. Caller trampoline maps EAX=0 → CF=0.
///
/// # Safety
/// `ctx` and all injected pointers/callbacks must be valid for the lookup.
#[inline(always)]
pub unsafe fn exfat_find_lfn(
    ctx: *mut ExFatFindLfnCtx,
    hooks: Option<ExFatFindLfnHooks>,
) -> u32 {
    unsafe { exfat_find_lfn_inner(&mut *ctx, hooks.as_ref()) }
}

#[inline(always)]
unsafe fn exfat_find_lfn_inner(ctx: &mut ExFatFindLfnCtx, hooks: Option<&ExFatFindLfnHooks>) -> u32 {
    let path = ctx.esi_out;
    let fs = ctx.fs;
    let pair = ctx.pair;
    let orig_edi = ctx.edi_out;

    unsafe {
        store_u32(ctx.secondary_dir_entry, 1);
        store_u32(ctx.need_hash, 1);
    }

    let first = unsafe { call_dir_fn(ctx.first, pair, fs, orig_edi, hooks, true) };
    if first.cf {
        ctx.esi_out = path;
        ctx.edi_out = first.edi;
        return first.eax;
    }
    let mut edi = first.edi;

    // FASM `sub esp, 262*2` — uninitialized. A `[0u16; 262]` zero-init
    // outlines `memset` + GOT (reloc-free extract rejects both).
    let mut lfn_raw = core::mem::MaybeUninit::<[u16; LFN_UTF16_UNITS]>::uninit();
    let lfn = lfn_raw.as_mut_ptr() as *mut u16;
    let lfn_bytes = lfn as *mut u8;
    unsafe {
        store_usize(ctx.lfn_reserve_place, lfn_bytes as usize);
        store_usize(ctx.path_in_utf8, path as usize);
    }

    let mut esi_path = path as *const u8;
    let mut eax = lfn_bytes as u32;
    let mut dst = lfn;
    let mut filled = 0usize;
    loop {
        eax = unsafe { step_utf16(&mut esi_path, eax) };
        unsafe {
            core::ptr::write(dst, eax as u16);
        }
        dst = unsafe { dst.add(1) };
        filled = filled.saturating_add(1);
        let ax = eax as u16;
        if ax == 0 || ax == SLASH || filled >= LFN_UTF16_UNITS {
            break;
        }
    }
    let byte_len = (dst as usize)
        .wrapping_sub(lfn as usize)
        .wrapping_sub(2) as u32;
    let hash = unsafe { exfat_hash_calculate(lfn_bytes as *const u8, byte_len) };
    unsafe {
        store_u32(ctx.current_hash, u32::from(hash));
    }

    let mut esi_gn = path;

    loop {
        let gn = unsafe { call_get_name(ctx.get_name, fs, edi, esi_gn, hooks) };
        esi_gn = gn.esi;
        edi = gn.edi;
        let secondary = unsafe { load_u32(ctx.secondary_dir_entry) };
        if gn.cf || secondary != 0 {
            if unsafe { load_u32(ctx.valid_data_length) } == 0 {
                ctx.esi_out = path;
                ctx.edi_out = edi;
                return ERROR_FILE_NOT_FOUND;
            }
            let nxt = unsafe { call_dir_fn(ctx.next, pair, fs, edi, hooks, false) };
            if nxt.cf {
                ctx.esi_out = path;
                ctx.edi_out = nxt.edi;
                return nxt.eax;
            }
            edi = nxt.edi;
            continue;
        }

        let mut pe = unsafe { load_usize(ctx.path_in_utf8) } as *const u8;
        let mut lfn_di = lfn as *const u16;
        eax = 0;
        loop {
            eax = unsafe { step_utf16(&mut pe, eax) };
            let dx = eax;
            let unit = unsafe { core::ptr::read_unaligned(lfn_di) };
            eax = (eax & 0xFFFF_0000) | u32::from(unit);
            eax = utf16_to_upper_eax(eax);
            if (eax as u16) != (dx as u16) {
                if (dx as u16) == SLASH && (eax as u16) == 0 {
                    return unsafe { finish_found(ctx, pe as *mut u8, edi, hooks) };
                }
                if unsafe { load_u32(ctx.valid_data_length) } == 0 {
                    ctx.esi_out = path;
                    ctx.edi_out = edi;
                    return ERROR_FILE_NOT_FOUND;
                }
                let nxt = unsafe { call_dir_fn(ctx.next, pair, fs, edi, hooks, false) };
                if nxt.cf {
                    ctx.esi_out = path;
                    ctx.edi_out = nxt.edi;
                    return nxt.eax;
                }
                edi = nxt.edi;
                break;
            }
            lfn_di = unsafe { lfn_di.add(1) };
            if (eax as u16) == 0 {
                let back = (pe as usize).wrapping_sub(1) as *mut u8;
                return unsafe { finish_found(ctx, back, edi, hooks) };
            }
        }
    }
}

#[inline(always)]
unsafe fn finish_found(
    ctx: &mut ExFatFindLfnCtx,
    esi: *mut u8,
    edi: *mut u8,
    hooks: Option<&ExFatFindLfnHooks>,
) -> u32 {
    ctx.esi_out = esi;
    ctx.edi_out = edi;
    if unsafe { load_u32(ctx.secondary_dir_entry) } != 0 {
        let nxt = unsafe { call_dir_fn(ctx.next, ctx.pair, ctx.fs, edi, hooks, false) };
        if nxt.cf {
            ctx.esi_out = esi;
            ctx.edi_out = nxt.edi;
            return nxt.eax;
        }
        ctx.edi_out = nxt.edi;
    }
    0
}

/// Pointer-form FFI helper.
///
/// # Safety
/// Same as [`exfat_find_lfn`].
#[inline(always)]
pub unsafe fn exfat_find_lfn_ptr(ctx: *mut ExFatFindLfnCtx) -> u32 {
    unsafe { exfat_find_lfn(ctx, None) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR_ENTRY: usize = 32;
    const MAX_C1: usize = 17;
    const NAME_CHARS_PER_C1: usize = 15;

    /// Host `exFAT*` subset used by the independent get_name fixture.
    #[repr(C)]
    struct HostFs {
        secondary_dir_entry: u32,
        need_hash: u32,
        lfn_reserve_place: usize,
        path_in_utf8: usize,
        current_hash: u32,
        valid_data_length: u32,
        hash_flag: u32,
        buffer_curr_sector: u32,
        buff_file_dirsect: u32,
        buff_file_dir_pos: u32,
        fname_extdir_offset: u32,
        longname_sector1: u32,
        longname_sector2: u32,
        file_dir_entry: [u8; 32],
        str_ext_dir_entry: [u8; 32],
        fname_ext_dir_entry: [u8; 32 * MAX_C1],
        volume_label: [u8; 12],
        dir: Vec<u8>,
        pos: usize,
        pair: [u32; 2],
    }

    struct OracleOut {
        cf: bool,
        eax: u32,
        esi: usize,
        #[allow(dead_code)]
        edi_off: usize,
    }

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    /// Independent NameHash (byte walk, not Cut AI's skip-flag helper).
    fn oracle_namehash(utf16_bytes: &[u8]) -> u16 {
        let mut sum: u16 = 0;
        for &b in utf16_bytes {
            let rotated = if (sum & 1) != 0 { 0x8000u16 } else { 0 };
            sum = rotated.wrapping_add(sum >> 1).wrapping_add(u16::from(b));
        }
        sum
    }

    /// Independent UTF-8 decode (RFC 3629 subset; ASCII/valid 2–3 byte).
    fn oracle_utf8_next(bytes: &[u8], i: &mut usize) -> u16 {
        if *i >= bytes.len() {
            return 0;
        }
        let b0 = bytes[*i];
        *i += 1;
        if b0 < 0x80 {
            return u16::from(b0);
        }
        if b0 & 0xE0 == 0xC0 && *i < bytes.len() {
            let b1 = bytes[*i];
            *i += 1;
            return (u16::from(b0 & 0x1F) << 6) | u16::from(b1 & 0x3F);
        }
        if b0 & 0xF0 == 0xE0 && *i + 1 < bytes.len() {
            let b1 = bytes[*i];
            let b2 = bytes[*i + 1];
            *i += 2;
            return (u16::from(b0 & 0x0F) << 12)
                | (u16::from(b1 & 0x3F) << 6)
                | u16::from(b2 & 0x3F);
        }
        0
    }

    fn oracle_upper(ch: u16) -> u16 {
        if (b'a' as u16..=b'z' as u16).contains(&ch) {
            return ch - 32;
        }
        if (0x0430..0x0450).contains(&ch) {
            return ch - 32;
        }
        if (0x0450..0x0460).contains(&ch) {
            return ch - 80;
        }
        ch
    }

    fn oracle_path_component(path: &[u8]) -> (Vec<u16>, usize, u16) {
        let mut i = 0usize;
        let mut units = Vec::new();
        loop {
            let ch = oracle_upper(oracle_utf8_next(path, &mut i));
            units.push(ch);
            if ch == 0 || ch == SLASH {
                break;
            }
        }
        let hash_bytes: Vec<u8> = units[..units.len() - 1]
            .iter()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let hash = oracle_namehash(&hash_bytes);
        (units, i, hash)
    }

    /// Independent 0x85/0xC0/0xC1 get_name (FASM control flow, not production).
    unsafe fn oracle_get_name(fs: &mut HostFs, edi_off: usize, mut esi_off: usize) -> (bool, usize) {
        if edi_off + DIR_ENTRY > fs.dir.len() {
            return (true, esi_off);
        }
        let ent: [u8; 32] = fs.dir[edi_off..edi_off + DIR_ENTRY]
            .try_into()
            .unwrap();
        let t = ent[0];
        if t == 0 {
            return (true, esi_off);
        }
        if t == 0x85 {
            fs.buff_file_dirsect = fs.buffer_curr_sector;
            fs.buff_file_dir_pos = edi_off as u32;
            fs.fname_extdir_offset = 0;
            fs.longname_sector1 = 0;
            fs.longname_sector2 = 0;
            fs.hash_flag = 0;
            fs.secondary_dir_entry = u32::from(ent[1]).wrapping_sub(1);
            fs.file_dir_entry = ent;
            esi_off = fs.lfn_reserve_place;
            if fs.lfn_reserve_place != 0 {
                unsafe {
                    core::ptr::write_unaligned(
                        (fs.lfn_reserve_place as *mut u16).add(260),
                        0,
                    );
                }
            }
            return (true, esi_off);
        }
        if t == 0xC0 {
            if fs.need_hash != 0 {
                let h = u16::from_le_bytes([ent[4], ent[5]]);
                if u32::from(h) != fs.current_hash {
                    fs.hash_flag = 1;
                }
            }
            fs.str_ext_dir_entry = ent;
            esi_off = fs.lfn_reserve_place;
            if fs.lfn_reserve_place != 0 {
                unsafe {
                    core::ptr::write_unaligned(
                        (fs.lfn_reserve_place as *mut u16).add(260),
                        0,
                    );
                }
            }
            return (true, esi_off);
        }
        if t == 0xC1 {
            if fs.hash_flag != 0 {
                return (true, esi_off);
            }
            if fs.fname_extdir_offset >= (32 * MAX_C1) as u32 {
                return (true, esi_off);
            }
            let o = fs.fname_extdir_offset as usize;
            fs.fname_ext_dir_entry[o..o + 32].copy_from_slice(&ent);
            fs.fname_extdir_offset = fs.fname_extdir_offset.wrapping_add(32);
            let dest = esi_off as *mut u8;
            if !dest.is_null() {
                unsafe {
                    core::ptr::copy_nonoverlapping(ent.as_ptr().add(2), dest, 30);
                    core::ptr::write_unaligned(dest.add(30) as *mut u16, 0);
                }
            }
            let sde = fs.secondary_dir_entry.wrapping_sub(1);
            fs.secondary_dir_entry = sde;
            if sde != 0 {
                return (true, esi_off.wrapping_add(30));
            }
            return (false, esi_off);
        }
        (true, esi_off)
    }

    fn oracle_find_lfn(path: &[u8], dir: &[u8]) -> OracleOut {
        let mut fs = HostFs {
            secondary_dir_entry: 1,
            need_hash: 1,
            lfn_reserve_place: 0,
            path_in_utf8: 0,
            current_hash: 0,
            valid_data_length: 0xFFFF_FFFF,
            hash_flag: 0,
            buffer_curr_sector: 1,
            buff_file_dirsect: 0,
            buff_file_dir_pos: 0,
            fname_extdir_offset: 0,
            longname_sector1: 0,
            longname_sector2: 0,
            file_dir_entry: [0; 32],
            str_ext_dir_entry: [0; 32],
            fname_ext_dir_entry: [0; 32 * MAX_C1],
            volume_label: [0; 12],
            dir: dir.to_vec(),
            pos: 0,
            pair: [2, 0],
        };
        let mut lfn = [0u16; LFN_UTF16_UNITS];
        fs.lfn_reserve_place = lfn.as_mut_ptr() as usize;
        let (_units, _end, hash) = oracle_path_component(path);
        fs.current_hash = u32::from(hash);

        if fs.dir.is_empty() {
            return OracleOut {
                cf: true,
                eax: ERROR_FILE_NOT_FOUND,
                esi: 0,
                edi_off: 0,
            };
        }
        let mut edi_off = 0usize;
        let mut esi_off = fs.lfn_reserve_place;
        loop {
            let (cf, esi_new) = unsafe { oracle_get_name(&mut fs, edi_off, esi_off) };
            esi_off = esi_new;
            if cf || fs.secondary_dir_entry != 0 {
                if fs.valid_data_length == 0 {
                    return OracleOut {
                        cf: true,
                        eax: ERROR_FILE_NOT_FOUND,
                        esi: 0,
                        edi_off,
                    };
                }
                edi_off += DIR_ENTRY;
                if edi_off + DIR_ENTRY > fs.dir.len() {
                    return OracleOut {
                        cf: true,
                        eax: ERROR_FILE_NOT_FOUND,
                        esi: 0,
                        edi_off,
                    };
                }
                continue;
            }
            let mut pi = 0usize;
            let mut li = 0usize;
            loop {
                let dx = oracle_upper(oracle_utf8_next(path, &mut pi));
                let ax = oracle_upper(lfn[li]);
                if ax != dx {
                    if dx == SLASH && ax == 0 {
                        return OracleOut {
                            cf: false,
                            eax: 0,
                            esi: pi,
                            edi_off,
                        };
                    }
                    edi_off += DIR_ENTRY;
                    if edi_off + DIR_ENTRY > fs.dir.len() {
                        return OracleOut {
                            cf: true,
                            eax: ERROR_FILE_NOT_FOUND,
                            esi: 0,
                            edi_off,
                        };
                    }
                    break;
                }
                li += 1;
                if ax == 0 {
                    return OracleOut {
                        cf: false,
                        eax: 0,
                        esi: pi.saturating_sub(1),
                        edi_off,
                    };
                }
            }
        }
    }

    fn pack_entries(name_utf16: &[u16], hash_override: Option<u16>) -> Vec<u8> {
        let n_c1 = ((name_utf16.len() + NAME_CHARS_PER_C1 - 1) / NAME_CHARS_PER_C1).max(1);
        let secondary = 1 + n_c1;
        let bytes: Vec<u8> = name_utf16.iter().flat_map(|u| u.to_le_bytes()).collect();
        let hash = hash_override.unwrap_or_else(|| oracle_namehash(&bytes));
        let mut out = Vec::new();
        let mut e85 = [0u8; 32];
        e85[0] = 0x85;
        e85[1] = secondary as u8;
        out.extend_from_slice(&e85);
        let mut e80 = [0u8; 32];
        e80[0] = 0xC0;
        e80[4..6].copy_from_slice(&hash.to_le_bytes());
        out.extend_from_slice(&e80);
        for chunk in 0..n_c1 {
            let mut e = [0u8; 32];
            e[0] = 0xC1;
            let start = chunk * NAME_CHARS_PER_C1;
            for i in 0..NAME_CHARS_PER_C1 {
                let u = *name_utf16.get(start + i).unwrap_or(&0);
                e[2 + i * 2] = u as u8;
                e[3 + i * 2] = (u >> 8) as u8;
            }
            out.extend_from_slice(&e);
        }
        out
    }

    unsafe fn host_first(state: *mut u8, _pair: *mut u32, _edi: *mut u8) -> CallbackOut {
        let fs = unsafe { &mut *(state as *mut HostFs) };
        if fs.dir.len() < DIR_ENTRY {
            return CallbackOut {
                cf: true,
                eax: ERROR_FILE_NOT_FOUND,
                esi: core::ptr::null_mut(),
                edi: core::ptr::null_mut(),
            };
        }
        fs.pos = 0;
        fs.valid_data_length = fs.valid_data_length.wrapping_sub(512);
        CallbackOut {
            cf: false,
            eax: 0,
            esi: core::ptr::null_mut(),
            edi: fs.dir.as_mut_ptr(),
        }
    }

    unsafe fn host_next(state: *mut u8, _pair: *mut u32, edi: *mut u8) -> CallbackOut {
        let fs = unsafe { &mut *(state as *mut HostFs) };
        let next = (edi as usize).wrapping_add(DIR_ENTRY);
        let start = fs.dir.as_mut_ptr() as usize;
        let end = start + fs.dir.len();
        if next + DIR_ENTRY > end {
            return CallbackOut {
                cf: true,
                eax: ERROR_FILE_NOT_FOUND,
                esi: core::ptr::null_mut(),
                edi: edi as *mut u8,
            };
        }
        fs.pos = next - start;
        CallbackOut {
            cf: false,
            eax: 0,
            esi: core::ptr::null_mut(),
            edi: next as *mut u8,
        }
    }

    unsafe fn host_get_name(state: *mut u8, edi: *mut u8, esi: *mut u8) -> CallbackOut {
        let fs = unsafe { &mut *(state as *mut HostFs) };
        let start = fs.dir.as_ptr() as usize;
        let edi_off = (edi as usize).wrapping_sub(start);
        let (cf, esi_new) = unsafe { oracle_get_name(fs, edi_off, esi as usize) };
        CallbackOut {
            cf,
            eax: 0,
            esi: esi_new as *mut u8,
            edi,
        }
    }

    fn run_prod(path: &[u8], dir: &[u8]) -> (u32, u32, u32) {
        let mut fs = HostFs {
            secondary_dir_entry: 0,
            need_hash: 0,
            lfn_reserve_place: 0,
            path_in_utf8: 0,
            current_hash: 0,
            valid_data_length: 0xFFFF_FFFF,
            hash_flag: 0,
            buffer_curr_sector: 1,
            buff_file_dirsect: 0,
            buff_file_dir_pos: 0,
            fname_extdir_offset: 0,
            longname_sector1: 0,
            longname_sector2: 0,
            file_dir_entry: [0; 32],
            str_ext_dir_entry: [0; 32],
            fname_ext_dir_entry: [0; 32 * MAX_C1],
            volume_label: [0; 12],
            dir: dir.to_vec(),
            pos: 0,
            pair: [2, 0],
        };
        let mut path_buf = path.to_vec();
        if path_buf.last() != Some(&0) {
            path_buf.push(0);
        }
        let fs_ptr = &mut fs as *mut HostFs;
        let mut ctx = unsafe {
            ExFatFindLfnCtx {
                fs: fs_ptr as *mut u8,
                secondary_dir_entry: core::ptr::addr_of_mut!((*fs_ptr).secondary_dir_entry),
                need_hash: core::ptr::addr_of_mut!((*fs_ptr).need_hash),
                lfn_reserve_place: core::ptr::addr_of_mut!((*fs_ptr).lfn_reserve_place),
                path_in_utf8: core::ptr::addr_of_mut!((*fs_ptr).path_in_utf8),
                current_hash: core::ptr::addr_of_mut!((*fs_ptr).current_hash),
                valid_data_length: core::ptr::addr_of_mut!((*fs_ptr).valid_data_length),
                first: 1,
                next: 1,
                get_name: 1,
                pair: core::ptr::addr_of_mut!((*fs_ptr).pair).cast::<u32>(),
                esi_out: path_buf.as_mut_ptr(),
                edi_out: core::ptr::null_mut(),
            }
        };
        let hooks = ExFatFindLfnHooks {
            first: host_first,
            next: host_next,
            get_name: host_get_name,
            state: fs_ptr as *mut u8,
        };
        let eax = unsafe { exfat_find_lfn(&mut ctx, Some(hooks)) };
        let esi_delta = (ctx.esi_out as usize).wrapping_sub(path_buf.as_ptr() as usize);
        let dir_base = unsafe { (*fs_ptr).dir.as_ptr() as usize };
        let edi_off = if ctx.edi_out.is_null() {
            0
        } else {
            (ctx.edi_out as usize).wrapping_sub(dir_base)
        };
        (eax, esi_delta as u32, edi_off as u32)
    }

    fn ascii_units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn flfn_ctx_size() {
        #[cfg(target_pointer_width = "32")]
        assert_eq!(core::mem::size_of::<ExFatFindLfnCtx>(), EXFAT_FIND_LFN_CTX_SIZE);
    }

    #[test]
    fn flfn_ascii_match() {
        let dir = pack_entries(&ascii_units("FOO"), None);
        let (eax, esi, _edi) = run_prod(b"FOO\0", &dir);
        assert_eq!(eax, 0);
        assert_eq!(esi, 3);
        let o = oracle_find_lfn(b"FOO\0", &dir);
        assert!(!o.cf);
        assert_eq!(o.esi, 3);
    }

    #[test]
    fn flfn_path_slash() {
        let dir = pack_entries(&ascii_units("FOO"), None);
        let (eax, esi, _) = run_prod(b"FOO/BAR\0", &dir);
        assert_eq!(eax, 0);
        assert_eq!(esi, 4);
        let o = oracle_find_lfn(b"FOO/BAR\0", &dir);
        assert!(!o.cf);
        assert_eq!(o.esi, 4);
    }

    #[test]
    fn flfn_not_found() {
        let dir = pack_entries(&ascii_units("FOO"), None);
        let (eax, _, _) = run_prod(b"BAR\0", &dir);
        assert_eq!(eax, ERROR_FILE_NOT_FOUND);
        let o = oracle_find_lfn(b"BAR\0", &dir);
        assert!(o.cf);
        assert_eq!(o.eax, ERROR_FILE_NOT_FOUND);
    }

    #[test]
    fn flfn_hash_mismatch() {
        let dir = pack_entries(&ascii_units("FOO"), Some(0xDEAD));
        let (eax, _, _) = run_prod(b"FOO\0", &dir);
        assert_eq!(eax, ERROR_FILE_NOT_FOUND);
        let o = oracle_find_lfn(b"FOO\0", &dir);
        assert!(o.cf);
    }

    #[test]
    fn flfn_deleted_skip() {
        let mut dir = vec![0u8; 32];
        dir[0] = 0x05;
        dir.extend_from_slice(&pack_entries(&ascii_units("FOO"), None));
        let (eax, esi, _) = run_prod(b"FOO\0", &dir);
        assert_eq!(eax, 0);
        assert_eq!(esi, 3);
    }

    #[test]
    fn flfn_fragmented_lfn() {
        let name: Vec<u16> = (0..40).map(|i| u16::from(b'A') + (i % 26)).collect();
        let dir = pack_entries(&name, None);
        let mut path: Vec<u8> = name.iter().map(|u| *u as u8).collect();
        path.push(0);
        let (eax, _, _) = run_prod(&path, &dir);
        assert_eq!(eax, 0);
        let o = oracle_find_lfn(&path, &dir);
        assert!(!o.cf);
    }

    #[test]
    fn flfn_malformed_incomplete() {
        let mut dir = pack_entries(&ascii_units("FOO"), None);
        dir.truncate(64);
        let (eax, _, _) = run_prod(b"FOO\0", &dir);
        assert_eq!(eax, ERROR_FILE_NOT_FOUND);
    }

    #[test]
    fn flfn_type_zero_skip() {
        // FASM `cmp byte [edi], 0` / `.no` is CF=1 skip, not directory end.
        let mut dir = vec![0u8; 32];
        dir.extend_from_slice(&pack_entries(&ascii_units("FOO"), None));
        let (eax, esi, _) = run_prod(b"FOO\0", &dir);
        assert_eq!(eax, 0);
        assert_eq!(esi, 3);
    }

    #[test]
    fn flfn_max_length() {
        let name: Vec<u16> = (0..255).map(|i| u16::from(b'A') + (i % 26) as u16).collect();
        let dir = pack_entries(&name, None);
        let mut path: Vec<u8> = name.iter().map(|u| *u as u8).collect();
        path.push(0);
        let (eax, _, _) = run_prod(&path, &dir);
        assert_eq!(eax, 0);
    }

    #[test]
    fn flfn_unicode_cyrillic() {
        let name = [0x0430u16, 0x0431, 0x0432];
        let dir = pack_entries(&name, None);
        let path = "абв".as_bytes();
        let mut pb = path.to_vec();
        pb.push(0);
        let (eax, _, _) = run_prod(&pb, &dir);
        let o = oracle_find_lfn(&pb, &dir);
        assert_eq!(eax == 0, !o.cf);
    }

    #[test]
    fn flfn_mixed_valid_invalid() {
        let mut dir = pack_entries(&ascii_units("AAA"), None);
        dir.extend_from_slice(&[0x41u8; 32]);
        dir.extend_from_slice(&pack_entries(&ascii_units("FOO"), None));
        let (eax, esi, _) = run_prod(b"FOO\0", &dir);
        assert_eq!(eax, 0);
        assert_eq!(esi, 3);
    }

    #[test]
    fn flfn_empty_dir() {
        let (eax, _, _) = run_prod(b"FOO\0", &[]);
        assert_eq!(eax, ERROR_FILE_NOT_FOUND);
    }

    #[test]
    fn flfn_casefold_ascii() {
        let dir = pack_entries(&ascii_units("FOO"), None);
        let (eax, _, _) = run_prod(b"foo\0", &dir);
        assert_eq!(eax, 0);
    }

    #[test]
    fn flfn_canary() {
        let mut dir = pack_entries(&ascii_units("Z"), None);
        dir.insert(0, 0xA5);
        dir.remove(0);
        let before = dir.clone();
        let _ = run_prod(b"Z\0", &dir);
        assert_eq!(dir, before);
    }

    #[test]
    fn flfn_prng_50000() {
        let mut s = EXFAT_FIND_LFN_PRNG_SEED;
        for _ in 0..50_000 {
            let nlen = 1 + (xorshift32(&mut s) % 16) as usize;
            let name: Vec<u16> = (0..nlen)
                .map(|_| u16::from(b'A') + (xorshift32(&mut s) % 26) as u16)
                .collect();
            let mut dir = Vec::new();
            let junk = xorshift32(&mut s) % 3;
            for _ in 0..junk {
                match xorshift32(&mut s) % 3 {
                    0 => {
                        let mut d = [0u8; 32];
                        d[0] = 0x05;
                        dir.extend_from_slice(&d);
                    }
                    1 => dir.extend_from_slice(&pack_entries(
                        &ascii_units("XXX"),
                        Some(0x1111),
                    )),
                    _ => dir.extend_from_slice(&pack_entries(&ascii_units("ZZZ"), None)),
                }
            }
            dir.extend_from_slice(&pack_entries(&name, None));
            if xorshift32(&mut s) & 1 != 0 {
                dir.extend_from_slice(&pack_entries(&ascii_units("QQQ"), None));
            }
            let mut path: Vec<u8> = name.iter().map(|u| *u as u8).collect();
            if xorshift32(&mut s) & 7 == 0 {
                path.extend_from_slice(b"/X");
            }
            path.push(0);
            let (eax, esi, _) = run_prod(&path, &dir);
            let o = oracle_find_lfn(&path, &dir);
            assert_eq!(eax == 0, !o.cf, "match polarity");
            if eax == 0 {
                assert_eq!(esi as usize, o.esi, "esi");
            } else {
                assert_eq!(eax, o.eax);
            }
        }
    }
}
