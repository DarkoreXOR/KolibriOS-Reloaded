//! FFI boundary for FASM trampolines (32-bit x86 only).
//!
//! Only ABI-stable primitives cross this boundary. All `unsafe` for raw pointers
//! is confined here. See `docs/migration/cut-a-implementation.md`.
//!
//! Calling convention: `stdcall` (callee pops args). Partial CRC is an explicit
//! argument because a Rust prologue must not rely on live `EAX` from FASM.

use crate::app_header::test_app_header_ptr;
use crate::casefold::{cp866_to_upper, utf16_to_upper};
use crate::checksum::{checksum_1, checksum_2};
use crate::coff_reloc::fix_coff_relocs_ptr;
use crate::crc::crc32_update;
use crate::exfat_checksum::{calculate_set_checksum_field_ptr, exfat_hash_calculate_ptr};
use crate::fat_name::{fat_gen_short_name_ptr, fat_next_short_name_ptr};
use crate::font::anti_aliasing;
use crate::geometry::block_clip_ptr;
use crate::io_access::set_io_access_rights_ptr;
use crate::ipv4_route::ipv4_route_ptr;
use crate::iso9660_compare::iso9660_compare_name_ptr;
use crate::mouse::mouse_acceleration;
use crate::ntfs_bootsec::ntfs_test_bootsec_ptr;
use crate::ntfs_mcb::ntfs_decode_mcb_entry_ptr;
use crate::ntfs_usa::ntfs_restore_usa_ptr;
use crate::partition::{is_partition_table_entry_ptr, is_protective_mbr_ptr};
use crate::pid_to_slot::pid_to_slot_ptr;
use crate::string::strncmp;
use crate::tcp::{tcp_set_persist_ptr, tcp_xmit_timer_ptr};
use crate::time::{fs_calculate_time_ptr, fs_time2bdfe_ptr, ntfs_calculate_time_ptr, ntfs_datetime_to_bdfe_ptr};
use crate::unicode::{cp866_encode, utf16_encode, utf8_decode};
use crate::userspace::is_region_userspace;
use crate::utf16_to_8::utf16_to_8_ptr;
use crate::utf8to16::utf8to16_ptr;
use crate::window::check_window_position_ptr;
use crate::xfs_extent::xfs_extent_unpack_ptr;
use crate::xfs_hash_lookup::{pack_eax_zf, xfs_get_addr_by_hash_ptr};
use crate::PHASE_C_PROBE_MAGIC;

/// `stdcall` rust_phase_c_probe() -> eax == [`PHASE_C_PROBE_MAGIC`].
///
/// Phase C integration probe — deterministic, freestanding, zero dependencies.
/// Removable once hybrid link is proven. No arguments, no memory access.
#[no_mangle]
pub extern "stdcall" fn rust_phase_c_probe() -> u32 {
    PHASE_C_PROBE_MAGIC
}

/// `stdcall` rust_crc_32(partial, poly, buffer, length) -> eax
///
/// Dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
///
/// # Safety
/// `buffer` must be readable for `length` bytes when `length > 0`.
#[no_mangle]
#[link_section = ".text.rust_crc_32"]
pub unsafe extern "stdcall" fn rust_crc_32(
    partial: u32,
    poly: u32,
    buffer: *const u8,
    length: u32,
) -> u32 {
    if length == 0 || buffer.is_null() {
        return partial;
    }
    // SAFETY: caller guarantees `buffer` is valid for `length` bytes (kernel).
    let data = unsafe { core::slice::from_raw_parts(buffer, length as usize) };
    crc32_update(partial, poly, data)
}

/// `stdcall` rust_unicode_utf8_decode(ptr_inout, len_inout) -> codepoint in EAX.
///
/// Updates `*ptr_inout` and `*len_inout` like FASM advances `ESI`/`ECX`.
/// If `*len_inout == 0`, returns `0` and leaves pointers unchanged
/// (FASM leaves `EAX` stale; callers must not rely on `EAX` when `ECX` was 0).
///
/// Dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
///
/// # Safety
/// Both pointers must be valid; `*ptr_inout` must be readable for `*len_inout` bytes.
#[no_mangle]
#[link_section = ".text.rust_unicode_utf8_decode"]
pub unsafe extern "stdcall" fn rust_unicode_utf8_decode(
    ptr_inout: *mut *const u8,
    len_inout: *mut u32,
) -> u32 {
    let ptr = unsafe { *ptr_inout };
    let len = unsafe { *len_inout };
    if len == 0 || ptr.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees `ptr` is valid for `len` bytes (kernel).
    let data = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
    match utf8_decode(data) {
        None => 0,
        Some(step) => {
            unsafe {
                *ptr_inout = ptr.add(step.consumed);
                *len_inout = len - step.consumed as u32;
            }
            step.codepoint
        }
    }
}

/// `stdcall` rust_unicode_utf16_encode(cp) -> packed UTF-16 in EAX.
///
/// Dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
#[no_mangle]
#[link_section = ".text.rust_unicode_utf16_encode"]
pub extern "stdcall" fn rust_unicode_utf16_encode(cp: u32) -> u32 {
    utf16_encode(cp)
}

/// `stdcall` rust_unicode_cp866_encode(cp) -> CP866 byte in AL (EAX).
///
/// Dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Input is truncated to 16 bits like FASM `uni2ansi_char` (`AX`).
#[no_mangle]
#[link_section = ".text.rust_unicode_cp866_encode"]
pub extern "stdcall" fn rust_unicode_cp866_encode(cp: u32) -> u32 {
    cp866_encode(cp)
}

/// `stdcall` rust_cp866_to_upper(ch) -> uppercased CP866 byte in AL (EAX).
///
/// Cut B: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Input is truncated to 8 bits like FASM `cp866toUpper` (`AL`).
#[no_mangle]
#[link_section = ".text.rust_cp866_to_upper"]
pub extern "stdcall" fn rust_cp866_to_upper(ch: u32) -> u32 {
    u32::from(cp866_to_upper(ch as u8))
}

/// `stdcall` rust_utf16_to_upper(ch) -> uppercased UTF-16 unit in AX (EAX).
///
/// Cut C: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Input is truncated to 16 bits like FASM `utf16toUpper` (`AX`).
#[no_mangle]
#[link_section = ".text.rust_utf16_to_upper"]
pub extern "stdcall" fn rust_utf16_to_upper(ch: u32) -> u32 {
    u32::from(utf16_to_upper(ch as u16))
}

/// `stdcall` rust_strncmp(s1, s2, n) -> EAX ∈ {−1, 0, +1}.
///
/// Cut D: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`), matching FASM `proc strncmp stdcall`.
///
/// # Safety
/// `s1` and `s2` must be readable for the bytes actually compared.
#[no_mangle]
#[link_section = ".text.rust_strncmp"]
pub unsafe extern "stdcall" fn rust_strncmp(s1: *const u8, s2: *const u8, n: u32) -> i32 {
    // SAFETY: kernel callers pass valid C-string regions for this compare.
    unsafe { strncmp(s1, s2, n) }
}

/// `stdcall` rust_checksum_1(seed, data, length) -> EAX = partial sum.
///
/// Cut E: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline moves EAX → EDX.
///
/// # Safety
/// `data` must be readable for `length` bytes when `length > 0`.
#[no_mangle]
#[link_section = ".text.rust_checksum_1"]
pub unsafe extern "stdcall" fn rust_checksum_1(
    seed: u32,
    data: *const u8,
    length: u32,
) -> u32 {
    if length == 0 {
        return seed;
    }
    // SAFETY: kernel callers pass a readable buffer of `length` bytes.
    unsafe { checksum_1(seed, data, length) }
}

/// `stdcall` rust_checksum_2(sum) -> EAX = final checksum (INET order in low 16).
///
/// Cut F: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline moves EAX → EDX.
#[no_mangle]
#[link_section = ".text.rust_checksum_2"]
pub extern "stdcall" fn rust_checksum_2(sum: u32) -> u32 {
    checksum_2(sum)
}

/// `stdcall` rust_fs_calculate_time(block) -> EAX = seconds since 2001-01-01.
///
/// Cut G: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline passes ESI.
///
/// # Safety
/// `block` must point to a readable 8-byte BDFE datetime (kernel `ESI`).
#[no_mangle]
#[link_section = ".text.rust_fs_calculate_time"]
pub unsafe extern "stdcall" fn rust_fs_calculate_time(block: *const u8) -> u32 {
    // SAFETY: kernel trampoline passes ESI → valid BDFE block.
    unsafe { fs_calculate_time_ptr(block) }
}

/// `stdcall` rust_fs_time2bdfe(secs, out) — writes 8-byte BDFE at `out`.
///
/// Cut T: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline passes EAX/EDI then
/// performs `add edi, 8` to match the public ABI.
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[no_mangle]
#[link_section = ".text.rust_fs_time2bdfe"]
pub unsafe extern "stdcall" fn rust_fs_time2bdfe(secs: u32, out: *mut u8) {
    // SAFETY: kernel trampoline passes EDI → valid BDFE out block.
    unsafe { fs_time2bdfe_ptr(secs, out) }
}

/// `stdcall` rust_ntfs_datetime_to_bdfe(ft_lo, ft_hi, out) — writes 8-byte BDFE at `out`.
///
/// Cut AE: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline passes EAX/EDX/EDI then
/// performs `add edi, 8` to match the public ABI (composes Cut T calendar).
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[no_mangle]
#[link_section = ".text.rust_ntfs_datetime_to_bdfe"]
pub unsafe extern "stdcall" fn rust_ntfs_datetime_to_bdfe(
    filetime_lo: u32,
    filetime_hi: u32,
    out: *mut u8,
) {
    // SAFETY: kernel trampoline passes EDI → valid BDFE out block.
    unsafe { ntfs_datetime_to_bdfe_ptr(filetime_lo, filetime_hi, out) }
}

/// `stdcall` rust_ntfs_calculate_time(block) -> EDX:EAX FILETIME.
///
/// Cut AF: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline passes ESI.
///
/// Returns `u64` in `EDX:EAX` = `(hi << 32) | lo` matching FASM
/// `ntfsCalculateTime` FILETIME output (composes Cut G + AE bias constants).
///
/// # Safety
/// `block` must point to a readable 8-byte BDFE datetime (kernel `ESI`).
#[no_mangle]
#[link_section = ".text.rust_ntfs_calculate_time"]
pub unsafe extern "stdcall" fn rust_ntfs_calculate_time(block: *const u8) -> u64 {
    // SAFETY: kernel trampoline passes ESI → valid BDFE block.
    let (lo, hi) = unsafe { ntfs_calculate_time_ptr(block) };
    ((hi as u64) << 32) | (lo as u64)
}

/// `stdcall` rust_ntfs_test_bootsec(boot, partition_sectors) -> EAX.
///
/// Cut AG: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). Returns `0` valid / `1` invalid;
/// FASM trampoline maps EAX→CF and preserves EBX/EDX.
///
/// # Safety
/// `boot` must point to a readable NTFS bootsector (≥ 0x45 bytes; typically 512).
#[no_mangle]
#[link_section = ".text.rust_ntfs_test_bootsec"]
pub unsafe extern "stdcall" fn rust_ntfs_test_bootsec(
    boot: *const u8,
    partition_sectors: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes EBX→boot + EDX partition size.
    unsafe { ntfs_test_bootsec_ptr(boot, partition_sectors) }
}

/// `stdcall` rust_calculate_set_checksum_field(buf, len) -> EAX (AX = checksum).
///
/// Cut AH: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). Writes LE checksum to `buf+2` and returns
/// it in EAX; FASM trampoline preserves EBX/ECX/EDX/ESI/EDI around the call.
///
/// # Safety
/// `buf` must be readable for `len` bytes and writable for ≥4 bytes.
#[no_mangle]
#[link_section = ".text.rust_calculate_set_checksum_field"]
pub unsafe extern "stdcall" fn rust_calculate_set_checksum_field(
    buf: *mut u8,
    len: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes &file_dir_entry + derived length.
    u32::from(unsafe { calculate_set_checksum_field_ptr(buf, len) })
}

/// `stdcall` rust_exfat_hash_calculate(buf, len) -> EAX (AX = NameHash).
///
/// Cut AI: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). No store side effect; FASM trampoline
/// preserves EBX/ECX/EDX/ESI/EDI around the call.
///
/// # Safety
/// `buf` must be readable for `len` bytes when `len > 0`.
#[no_mangle]
#[link_section = ".text.rust_exfat_hash_calculate"]
pub unsafe extern "stdcall" fn rust_exfat_hash_calculate(buf: *const u8, len: u32) -> u32 {
    // SAFETY: kernel trampoline passes NameUTF16 pointer + byte length.
    u32::from(unsafe { exfat_hash_calculate_ptr(buf, len) })
}

/// `stdcall` rust_iso9660_compare_name(esi_inout, dir_record, type_encoding)
/// -> EAX = 0 match / 1 miss.
///
/// Cut AJ: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). On match advances `*esi_inout` to `/`
/// or NUL; on miss leaves it unchanged. FASM trampoline maps EAX → `clc`/`stc`.
///
/// # Safety
/// `esi_inout` must be valid; `*esi_inout` readable for the path component;
/// `dir_record` readable for the ISO9660 directory-record name span.
#[no_mangle]
#[link_section = ".text.rust_iso9660_compare_name"]
pub unsafe extern "stdcall" fn rust_iso9660_compare_name(
    esi_inout: *mut *const u8,
    dir_record: *const u8,
    type_encoding: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes &ESI slot + EDI record + encoding.
    unsafe { iso9660_compare_name_ptr(esi_inout, dir_record, type_encoding) }
}

/// `stdcall` rust_block_clip(clip, rect) -> EAX = 0 draw / 1 reject.
///
/// Cut H: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline maps EAX → `clc`/`stc`.
/// Mutates the 16-byte RECT at `rect` in place (partial mutate on Y-fail).
///
/// # Safety
/// `clip` must be a readable 16-byte RECT; `rect` must be a writable 16-byte RECT.
#[no_mangle]
#[link_section = ".text.rust_block_clip"]
pub unsafe extern "stdcall" fn rust_block_clip(clip: *const u8, rect: *mut u8) -> u32 {
    // SAFETY: kernel trampoline passes ESI/EDI → valid RECT blocks.
    unsafe { block_clip_ptr(clip, rect) }
}

/// `stdcall` rust_ntfs_decode_mcb_entry(esi_inout, buffer) -> EAX = 0 end / 1 more.
///
/// Cut I: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline maps EAX → `clc`/`stc`
/// (inverted polarity vs Cut H: 1 = CF set = more).
/// Advances `*esi_inout`; writes up to 16 bytes at `buffer` (partial on reject).
///
/// # Safety
/// `esi_inout` must point to a readable/writable pointer into a packed MCB
/// stream; `buffer` must be a writable 16-byte stack slot.
#[no_mangle]
#[link_section = ".text.rust_ntfs_decode_mcb_entry"]
pub unsafe extern "stdcall" fn rust_ntfs_decode_mcb_entry(
    esi_inout: *mut *mut u8,
    buffer: *mut u8,
) -> u32 {
    // SAFETY: kernel trampoline passes &ESI slot and caller stack buffer.
    unsafe { ntfs_decode_mcb_entry_ptr(esi_inout, buffer) }
}

/// `stdcall` rust_ntfs_restore_usa(record, size) -> EAX = 0 OK / 1 fail.
///
/// Cut J: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline maps EAX → `clc`/`stc`
/// (Cut H polarity: 0 = CF clear = OK, 1 = CF set = fail).
/// Mutates sector end-words in the record (partial on mid-loop USN reject).
///
/// # Safety
/// `record` must be a readable/writable NTFS record of `size` bytes.
#[no_mangle]
#[link_section = ".text.rust_ntfs_restore_usa"]
pub unsafe extern "stdcall" fn rust_ntfs_restore_usa(record: *mut u8, size: u32) -> u32 {
    // SAFETY: kernel trampoline passes EBX/EAX → valid record + size.
    unsafe { ntfs_restore_usa_ptr(record, size) }
}

/// `stdcall` rust_fat_next_short_name(name) -> EAX = 0 OK / 1 fail.
///
/// Cut K: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline maps EAX → `clc`/`stc`
/// (Cut H polarity: 0 = CF clear = OK, 1 = CF set = exhausted) and `cld`.
/// Mutates basename bytes 0..7 of the 11-byte 8.3 name at `name`.
///
/// # Safety
/// `name` must be a readable/writable 11-byte FAT 8.3 name buffer.
#[no_mangle]
#[link_section = ".text.rust_fat_next_short_name"]
pub unsafe extern "stdcall" fn rust_fat_next_short_name(name: *mut u8) -> u32 {
    // SAFETY: kernel trampoline passes EDI → valid 11-byte name.
    unsafe { fat_next_short_name_ptr(name) }
}

/// `stdcall` rust_fat_gen_short_name(src, out) — UTF-8 LFN → 8.3 at `out`.
///
/// Cut U: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline wraps with `pushad`/`popad`.
/// Writes 12 initial spaces then the 11-byte 8.3 form; may mutate via
/// inlined `fat_next_short_name` when lossy.
///
/// # Safety
/// `src` must be a readable NUL-terminated UTF-8 name.
/// `out` must be writable for 12 bytes (caller `sub esp,12`).
#[no_mangle]
#[link_section = ".text.rust_fat_gen_short_name"]
pub unsafe extern "stdcall" fn rust_fat_gen_short_name(src: *const u8, out: *mut u8) {
    // SAFETY: kernel trampoline passes ESI/EDI → valid name + out buffer.
    unsafe { fat_gen_short_name_ptr(src, out) }
}

/// `stdcall` rust_mouse_acceleration(delta, delay, speed_factor) -> EAX.
///
/// Cut L: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline loads
/// `[mouse_delay]` / `[mouse_speed_factor]` and passes them with `EAX`.
#[no_mangle]
#[link_section = ".text.rust_mouse_acceleration"]
pub extern "stdcall" fn rust_mouse_acceleration(
    delta: u32,
    delay: u32,
    speed_factor: u32,
) -> u32 {
    mouse_acceleration(delta, delay as u8, speed_factor as u16)
}

/// `stdcall` rust_tcp_xmit_timer(rtt, socket) -> void.
///
/// Cut M: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline increments
/// `[TCPS_rttupdated]` and preserves `ECX`/`EDX` around the call.
///
/// # Safety
/// `socket` must point to a writable `TCP_SOCKET` through `t_rttvar`.
#[no_mangle]
#[link_section = ".text.rust_tcp_xmit_timer"]
pub unsafe extern "stdcall" fn rust_tcp_xmit_timer(rtt: u32, socket: *mut u8) {
    // SAFETY: kernel trampoline passes EAX/EBX → valid rtt + socket.
    unsafe { tcp_xmit_timer_ptr(rtt, socket) }
}

/// `stdcall` rust_tcp_set_persist(socket) -> void.
///
/// Cut V: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline preserves
/// `EAX`/`EBX`/`ECX`/`EDX` around the call (callers keep EAX as socket).
///
/// # Safety
/// `socket` must point to a writable `TCP_SOCKET` through `timer_persist`.
#[no_mangle]
#[link_section = ".text.rust_tcp_set_persist"]
pub unsafe extern "stdcall" fn rust_tcp_set_persist(socket: *mut u8) {
    // SAFETY: kernel trampoline passes EAX → valid socket.
    unsafe { tcp_set_persist_ptr(socket) }
}

/// `stdcall` rust_set_io_access_rights(port, clear_access, io_map) -> void.
///
/// Cut X: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline injects
/// `tss._io_map_0` and preserves EAX/ECX/EDX/EDI/EBP around the call.
///
/// `clear_access == 0` → BTR (enable I/O); nonzero → BTS (disable I/O).
///
/// # Safety
/// `io_map` must be writable for the byte containing bit `port`
/// (production: full 8 KiB TSS I/O map; callers keep `port < 65536`).
#[no_mangle]
#[link_section = ".text.rust_set_io_access_rights"]
pub unsafe extern "stdcall" fn rust_set_io_access_rights(
    port: u32,
    clear_access: u32,
    io_map: *mut u8,
) {
    // SAFETY: kernel trampoline passes EAX/EBP + tss._io_map_0.
    unsafe { set_io_access_rights_ptr(port, clear_access, io_map) }
}

/// `stdcall` rust_fix_coff_relocs(coff, sym, delta) -> void.
///
/// Cut Y: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Matches legacy
/// `proc fix_coff_relocs stdcall uses ebx esi`.
///
/// # Safety
/// `coff`/`sym` must describe a valid COFF image; every patch address
/// `reloc.VA + sec.VA + delta` must be a writable dword.
#[no_mangle]
#[link_section = ".text.rust_fix_coff_relocs"]
pub unsafe extern "stdcall" fn rust_fix_coff_relocs(
    coff: *mut u8,
    sym: *const u8,
    delta: u32,
) {
    // SAFETY: kernel trampoline / load_library passes live COFF pointers.
    unsafe { fix_coff_relocs_ptr(coff, sym, delta) }
}

/// `stdcall` rust_is_partition_table_entry(entry, ebp_base, cap_lo, cap_hi) -> EAX.
///
/// Cut Z: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 16 bytes (`ret 16`). Returns `0` valid / `1` invalid;
/// FASM trampoline maps EAX→CF and preserves ECX/ESI/EBP.
///
/// # Safety
/// `entry` must be readable for 16 bytes (`PARTITION_TABLE_ENTRY`).
#[no_mangle]
#[link_section = ".text.rust_is_partition_table_entry"]
pub unsafe extern "stdcall" fn rust_is_partition_table_entry(
    entry: *const u8,
    ebp_base: u32,
    capacity_lo: u32,
    capacity_hi: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes ECX→entry + capacity from DISK.
    unsafe { is_partition_table_entry_ptr(entry, ebp_base, capacity_lo, capacity_hi) }
}

/// `stdcall` rust_is_protective_mbr(pt_array, capacity_lo) -> EAX.
///
/// Cut AD: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). Returns `0` protective / `1` not;
/// FASM trampoline maps EAX→ZF via `test eax,eax` and preserves ECX/ESI/EDI.
///
/// # Safety
/// `pt_array` is MBR+0x1BE; `pt_array-2` .. `pt_array+64` must be readable.
#[no_mangle]
#[link_section = ".text.rust_is_protective_mbr"]
pub unsafe extern "stdcall" fn rust_is_protective_mbr(
    pt_array: *const u8,
    capacity_lo: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes ECX→partition table + Capacity low dword.
    unsafe { is_protective_mbr_ptr(pt_array, capacity_lo) }
}

/// `stdcall` rust_pid_to_slot(pid, slot_base, thread_count) -> EAX.
///
/// Cut AA: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Returns slot index or `0`;
/// FASM trampoline injects `SLOT_BASE` / `[thread_count]` and preserves
/// EBX/ECX/EDX/ESI/EDI/EBP.
///
/// # Safety
/// `slot_base` must be a readable APPDATA table for the scanned range.
#[no_mangle]
#[link_section = ".text.rust_pid_to_slot"]
pub unsafe extern "stdcall" fn rust_pid_to_slot(
    pid: u32,
    slot_base: *const u8,
    thread_count: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes live SLOT_BASE + thread_count.
    unsafe { pid_to_slot_ptr(pid, slot_base, thread_count) }
}

/// `stdcall` rust_utf8to16(esi_inout, initial_eax) -> EAX.
///
/// Cut AB: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). Advances `*esi_inout` by the FASM ESI
/// delta; returns the final EAX bit pattern (`AX` = UTF-16 code unit).
/// `initial_eax` mirrors live EAX on entry (high bits affect 3-byte path).
///
/// # Safety
/// `esi_inout` must be valid; `*esi_inout` readable for the consumed sequence.
#[no_mangle]
#[link_section = ".text.rust_utf8to16"]
pub unsafe extern "stdcall" fn rust_utf8to16(
    esi_inout: *mut *const u8,
    initial_eax: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes &ESI stack slot + live EAX.
    unsafe { utf8to16_ptr(esi_inout, initial_eax) }
}

/// `stdcall` rust_ipv4_route(dest, device, source, addr, subnet, gw, devlist,
///                           source_out, device_idx_out) -> EAX.
///
/// Cut AC: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 36 bytes (`ret 36`). Writes `EDX`/`EDI` equivalents through
/// out-pointers; trampoline injects `IPv4_*` / `net_device_list` bases.
///
/// # Safety
/// Table bases must be readable for 16 dwords; out-pointers must be writable;
/// non-null device entries must expose `link_state` at offset 24.
#[no_mangle]
#[link_section = ".text.rust_ipv4_route"]
pub unsafe extern "stdcall" fn rust_ipv4_route(
    dest_ip: u32,
    device_ptr: u32,
    source_ip: u32,
    ipv4_address: *const u32,
    ipv4_subnet: *const u32,
    ipv4_gateway: *const u32,
    net_device_list: *const u32,
    source_out: *mut u32,
    device_idx_out: *mut u32,
) -> u32 {
    // SAFETY: kernel trampoline passes live table bases + stack out slots.
    let r = unsafe {
        ipv4_route_ptr(
            dest_ip,
            device_ptr,
            source_ip,
            ipv4_address,
            ipv4_subnet,
            ipv4_gateway,
            net_device_list,
        )
    };
    unsafe {
        *source_out = r.source_ip;
        *device_idx_out = r.device_idx4;
    }
    r.dest_ip
}

/// `stdcall` rust_anti_aliasing(fg, bg) -> EAX blended RGB.
///
/// Cut N: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline restores `EBP = EBX`
/// (original FASM used `BP` as a 16-bit counter then `mov ebp, ebx`).
#[no_mangle]
#[link_section = ".text.rust_anti_aliasing"]
pub extern "stdcall" fn rust_anti_aliasing(fg: u32, bg: u32) -> u32 {
    anti_aliasing(fg, bg)
}

/// `stdcall` rust_test_app_header(header, app_hdr, pages_free) -> EAX.
///
/// Cut O: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline loads
/// `[pg_data.pages_free]` and passes it with `EAX`/`EBX`.
/// Returns the header pointer on success, `0` on fail (matches FASM).
///
/// # Safety
/// `header` must be readable through the MENUET header; `app_hdr` writable
/// through `APP_HDR._emem`.
#[no_mangle]
#[link_section = ".text.rust_test_app_header"]
pub unsafe extern "stdcall" fn rust_test_app_header(
    header: *const u8,
    app_hdr: *mut u8,
    pages_free: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes EAX/EBX → valid image + APP_HDR.
    unsafe { test_app_header_ptr(header, app_hdr, pages_free) }
}

/// `stdcall` rust_is_region_userspace(base, len) -> EAX ∈ {0,1}.
///
/// Cut P: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`).
///
/// Return `1` = legacy FASM `ZF=1` (accept / overflow-to-zero quirk);
/// `0` = legacy FASM `ZF=0` (reject). The FASM trampoline reconstructs ZF
/// via `cmp eax, 1` and restores caller EAX/ECX/EDX with flag-neutral pops.
#[no_mangle]
#[link_section = ".text.rust_is_region_userspace"]
pub extern "stdcall" fn rust_is_region_userspace(base: u32, len: u32) -> u32 {
    is_region_userspace(base, len)
}

/// `stdcall` rust_utf16_to_8(ch, dest_inout, ecx_inout) -> packed (SF<<31)|EAX.
///
/// Cut Q: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`).
///
/// Updates `*ecx_inout` to the final FASM `ECX` bit pattern and advances
/// `*dest_inout` by the FASM `EDI` delta (0 on fail). Return dword packs
/// legacy SF sense in bit 31 and `EAX` residue in bits 30..0 (residues fit
/// in low 16). A future FASM trampoline reconstructs architectural SF and
/// restores the register ABI; production still uses the FASM leaf.
///
/// # Safety
/// `dest_inout` / `ecx_inout` must be valid; `*dest_inout` writable for up
/// to 3 bytes on success paths.
#[no_mangle]
#[link_section = ".text.rust_utf16_to_8"]
pub unsafe extern "stdcall" fn rust_utf16_to_8(
    ch: u32,
    dest_inout: *mut *mut u8,
    ecx_inout: *mut u32,
) -> u32 {
    // SAFETY: future kernel trampoline passes valid EDI/ECX slots.
    unsafe { utf16_to_8_ptr(ch, dest_inout, ecx_inout) }
}

/// `stdcall` rust_xfs_extent_unpack(extent_data, extent_out).
///
/// Cut R: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`).
///
/// `extent_data` → 16-byte big-endian `xfs_bmbt_rec`.
/// `extent_out` → writable 24-byte `xfs_bmbt_irec` (typically `&XFS.extent`
/// derived from the caller's EBP by the omit-frame-pointer trampoline).
///
/// # Safety
/// `extent_data` readable for 16 bytes; `extent_out` writable for 24 bytes.
#[no_mangle]
#[link_section = ".text.rust_xfs_extent_unpack"]
pub unsafe extern "stdcall" fn rust_xfs_extent_unpack(
    extent_data: *const u8,
    extent_out: *mut u8,
) {
    // SAFETY: kernel trampoline passes extent record + &XFS.extent.
    unsafe { xfs_extent_unpack_ptr(extent_data, extent_out) }
}

/// `stdcall` rust_xfs_get_addr_by_hash(hash, base, len) -> EDX:EAX packed.
///
/// Cut W: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`).
///
/// Returns `u64` in `EDX:EAX` = `(zf << 32) | result`, where `zf` is the
/// legacy found/miss ZF sense (`1`/`0`) and `result` is the BE address or
/// `ERROR_FILE_NOT_FOUND`. The FASM trampoline reconstructs ZF via
/// `cmp edx, 1` then restores EAX with a flag-neutral `pop`.
///
/// # Safety
/// `base` must be readable for `len * 8` bytes when `len > 0`.
#[no_mangle]
#[link_section = ".text.rust_xfs_get_addr_by_hash"]
pub unsafe extern "stdcall" fn rust_xfs_get_addr_by_hash(
    hash: u32,
    base: *const u8,
    len: u32,
) -> u64 {
    // SAFETY: kernel trampoline passes leaf-entry table pointer + count.
    let r = unsafe { xfs_get_addr_by_hash_ptr(hash, base, len) };
    pack_eax_zf(r.eax, r.zf)
}

/// `stdcall` rust_check_window_position(box, display_width, display_height).
///
/// Cut S: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`).
///
/// `box` → writable 16-byte `BOX` / `WDATA.box`. Display dimensions are
/// explicit so the Rust blob never references `_display` iglobals.
///
/// # Safety
/// `box` must be readable/writable for 16 bytes.
#[no_mangle]
#[link_section = ".text.rust_check_window_position"]
pub unsafe extern "stdcall" fn rust_check_window_position(
    box_ptr: *mut u8,
    display_width: i32,
    display_height: i32,
) {
    // SAFETY: kernel trampoline passes EDI → WDATA.box + display dims.
    unsafe { check_window_position_ptr(box_ptr, display_width, display_height) }
}
