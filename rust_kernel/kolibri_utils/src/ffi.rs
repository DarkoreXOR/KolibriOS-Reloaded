//! FFI boundary for FASM trampolines (32-bit x86 only).
//!
//! Only ABI-stable primitives cross this boundary. All `unsafe` for raw pointers
//! is confined here. See `docs/migration/cut-a-implementation.md`.
//!
//! Calling convention: `stdcall` (callee pops args). Partial CRC is an explicit
//! argument because a Rust prologue must not rely on live `EAX` from FASM.

use crate::casefold::{cp866_to_upper, utf16_to_upper};
use crate::checksum::{checksum_1, checksum_2};
use crate::crc::crc32_update;
use crate::fat_name::fat_next_short_name_ptr;
use crate::geometry::block_clip_ptr;
use crate::mouse::mouse_acceleration;
use crate::ntfs_mcb::ntfs_decode_mcb_entry_ptr;
use crate::ntfs_usa::ntfs_restore_usa_ptr;
use crate::string::strncmp;
use crate::time::fs_calculate_time_ptr;
use crate::unicode::{cp866_encode, utf16_encode, utf8_decode};
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
