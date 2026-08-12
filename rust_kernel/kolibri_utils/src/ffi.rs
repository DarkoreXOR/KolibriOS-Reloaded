//! FFI boundary for FASM trampolines (32-bit x86 only).
//!
//! Only ABI-stable primitives cross this boundary. All `unsafe` for raw pointers
//! is confined here. See `docs/migration/cut-a-implementation.md`.
//!
//! Calling convention: `stdcall` (callee pops args). Partial CRC is an explicit
//! argument because a Rust prologue must not rely on live `EAX` from FASM.

use crate::ahci_find_cmdslot::ahci_find_cmdslot;
use crate::ahci_is_sig_known::ahci_is_sig_known;
use crate::app_header::test_app_header_ptr;
use crate::casefold::{cp866_to_upper, utf16_to_upper};
use crate::checksum::{checksum_1, checksum_2};
use crate::cp866_to_utf8_string::cp866_to_utf8_string_ptr;
use crate::coff_get_align::coff_get_align_ptr;
use crate::v86_get_lin_addr::v86_get_lin_addr_ptr;
use crate::coff_reloc::fix_coff_relocs_ptr;
use crate::fix_coff_symbols::{fix_coff_symbols_ptr, GetProcExFn};
use crate::fs_get_time::{fs_get_time_ptr, FsReadCmosFn};
use crate::fs_read_cmos::{fs_read_cmos_ptr, FsCmosRawReadFn};
use crate::crc::crc32_update;
use crate::exfat_checksum::{calculate_set_checksum_field_ptr, exfat_hash_calculate_ptr};
use crate::fat_name::{fat_gen_short_name_ptr, fat_name_is_legal_ptr, fat_next_short_name_ptr};
use crate::font::anti_aliasing;
use crate::fs_operation_safe::file_system_is_operation_safe_ptr;
use crate::geometry::block_clip_ptr;
use crate::get_coff_sym::get_coff_sym_ptr;
use crate::get_pg_addr::get_pg_addr_ptr;
use crate::io_access::set_io_access_rights_ptr;
use crate::ipv4_find_fragment_slot::ipv4_find_fragment_slot_ptr;
use crate::ipv4_route::ipv4_route_ptr;
use crate::port_area::r_f_port_area_ptr;
use crate::net_ptr_to_num4::net_ptr_to_num4_ptr;
use crate::socket_check::socket_check_ptr;
use crate::iso9660_compare::iso9660_compare_name_ptr;
use crate::iso9660_copy_name::iso9660_copy_name_ptr;
use crate::hotkey::hotkey_do_test;
use crate::mouse::mouse_acceleration;
use crate::ntfs_bootsec::ntfs_test_bootsec_ptr;
use crate::ntfs_create_mcb::ntfs_create_mcb_entry_ptr;
use crate::ntfs_mcb::ntfs_decode_mcb_entry_ptr;
use crate::ntfs_usa::ntfs_restore_usa_ptr;
use crate::partition::{is_partition_table_entry_ptr, is_protective_mbr_ptr};
use crate::pci_make_config_cmd::pci_make_config_cmd;
use crate::pid_to_slot::pid_to_slot_ptr;
use crate::string::{strlen, strncmp, strncpy, strrchr};
use crate::swap_bytes_in_words::swap_bytes_in_words;
use crate::tcp::{tcp_outflags_ptr, tcp_set_persist_ptr, tcp_xmit_timer_ptr};
use crate::ext_read_all_times::ext_read_all_times_ptr;
use crate::ext_write_time::ext_write_time_pack_ptr;
use crate::time::{
    bdfe_to_fat_date, bdfe_to_fat_time, ext_unix_to_secs, fat_date_to_bdfe, fat_time_to_bdfe,
    fs_calculate_time_ptr, fs_time2bdfe_ptr, ntfs_calculate_time_ptr, ntfs_datetime_to_bdfe_ptr,
    xfs_bigtime_to_secs, xfs_conv_time_to_kos_epoch_ptr,
};
use crate::unicode::{cp866_decode, cp866_encode, utf16_encode, utf8_decode};
use crate::userspace::{is_region_userspace, is_string_userspace};
use crate::utf16_to_8::{utf16_to_8_ptr, pack_sf_zf_eax, utf16_to_8};
use crate::utf8to16::utf8to16_ptr;
use crate::window::check_window_position_ptr;
use crate::xfs_extent::xfs_extent_unpack_ptr;
use crate::xfs_get_last_dirblock::xfs_get_last_dirblock_ptr;
use crate::xfs_hash_lookup::{pack_eax_zf, xfs_get_addr_by_hash_ptr};
use crate::xfs_blkrel2sectabs::xfs_blkrel2sectabs_ptr;
use crate::xfs_hashname::xfs_hashname_ptr;
use crate::xfs_node_hash::xfs_get_before_by_hashval_ptr;
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

/// `stdcall` rust_uni2ansi_char(cp) -> CP866 byte in AL (EAX).
///
/// Cut BZ: dedicated section for reloc-free extract + FASM `file` embed.
/// Same algorithm as Cut A `rust_unicode_cp866_encode`; separate symbol for
/// the public `uni2ansi_char` leaf in `parse_fn.inc`.
/// Input is truncated to 16 bits like FASM `uni2ansi_char` (`AX`).
#[no_mangle]
#[link_section = ".text.rust_uni2ansi_char"]
pub extern "stdcall" fn rust_uni2ansi_char(cp: u32) -> u32 {
    cp866_encode(cp)
}

/// `stdcall` rust_ansi2uni_char(ch) -> Unicode unit in AX (EAX).
///
/// Cut AN: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Input is truncated to 8 bits like FASM `ansi2uni_char` (`movzx eax, al`).
/// Callee cleans 4 bytes (`ret 4`).
#[no_mangle]
#[link_section = ".text.rust_ansi2uni_char"]
pub extern "stdcall" fn rust_ansi2uni_char(ch: u32) -> u32 {
    cp866_decode(ch)
}

/// `stdcall` rust_fat_time_to_bdfe(fat_time) -> EAX = BDFE time dword.
///
/// Cut AO: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline preserves ECX+EDX
/// (REG-001 / legacy `push ecx edx` body).
#[no_mangle]
#[link_section = ".text.rust_fat_time_to_bdfe"]
pub extern "stdcall" fn rust_fat_time_to_bdfe(fat_time: u32) -> u32 {
    fat_time_to_bdfe(fat_time)
}

/// `stdcall` rust_fat_date_to_bdfe(fat_date) -> EAX = BDFE date dword.
///
/// Cut BW: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline preserves ECX+EDX
/// (REG-001 / legacy `push ecx edx` body).
#[no_mangle]
#[link_section = ".text.rust_fat_date_to_bdfe"]
pub extern "stdcall" fn rust_fat_date_to_bdfe(fat_date: u32) -> u32 {
    fat_date_to_bdfe(fat_date)
}

/// `stdcall` rust_bdfe_to_fat_date(bdfe_date) -> EAX = FAT packed date.
///
/// Cut BX: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline preserves ECX+EDX
/// (REG-001 / legacy `push edx` body).
#[no_mangle]
#[link_section = ".text.rust_bdfe_to_fat_date"]
pub extern "stdcall" fn rust_bdfe_to_fat_date(bdfe_date: u32) -> u32 {
    bdfe_to_fat_date(bdfe_date)
}

/// `stdcall` rust_bdfe_to_fat_time(bdfe_time) -> EAX = FAT packed time.
///
/// Cut BY: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline preserves ECX+EDX
/// (REG-001 / legacy `push edx` body).
#[no_mangle]
#[link_section = ".text.rust_bdfe_to_fat_time"]
pub extern "stdcall" fn rust_bdfe_to_fat_time(bdfe_time: u32) -> u32 {
    bdfe_to_fat_time(bdfe_time)
}

/// `stdcall` rust_xfs_hashname(name, len) -> EAX = XFS dir name hash.
///
/// Cut AP: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline preserves ECX+EDX
/// (REG-001; legacy body left EDX untouched and `uses ecx esi`).
///
/// # Safety
/// `name` must be readable for `len` bytes when `len > 0`.
#[no_mangle]
#[link_section = ".text.rust_xfs_hashname"]
pub unsafe extern "stdcall" fn rust_xfs_hashname(name: *const u8, len: u32) -> u32 {
    // SAFETY: kernel trampoline passes directory-name pointer + length.
    unsafe { xfs_hashname_ptr(name, len) }
}

/// `stdcall` rust_get_pg_addr(linear, page_tabs, os_base) -> EAX = phys page.
///
/// Cut AQ: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline injects `page_tabs` /
/// `OS_BASE` and preserves ECX+EDX (REG-001; HD DMA / USB `get_phys_addr`
/// keep those registers live across the call).
///
/// # Safety
/// `page_tabs` must be readable for the computed PTE index on the high path.
#[no_mangle]
#[link_section = ".text.rust_get_pg_addr"]
pub unsafe extern "stdcall" fn rust_get_pg_addr(
    linear: u32,
    page_tabs: *const u32,
    os_base: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes live page_tabs + OS_BASE.
    unsafe { get_pg_addr_ptr(linear, page_tabs, os_base) }
}

/// `stdcall` rust_r_f_port_area(op, start, end, reserved_ports, tid, io_map) -> EAX.
///
/// Cut AR: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 24 bytes (`ret 24`). FASM trampoline injects
/// `RESERVED_PORTS`, `current_slot.tid`, and `tss._io_map_0`; preserves
/// ECX+EDX (REG-001); wraps **reserve** (`op==0`) with `cli`/`sti` to match
/// legacy interrupt window (free path has no `cli`).
///
/// Returns `0` success / `1` error.
///
/// # Safety
/// `reserved_ports` and `io_map` must be live writable kernel buffers.
#[no_mangle]
#[link_section = ".text.rust_r_f_port_area"]
pub unsafe extern "stdcall" fn rust_r_f_port_area(
    op: u32,
    start: u32,
    end: u32,
    reserved_ports: *mut u8,
    tid: u32,
    io_map: *mut u8,
) -> u32 {
    // SAFETY: kernel trampoline passes live table / tid / IO map.
    unsafe { r_f_port_area_ptr(op, start, end, reserved_ports, tid, io_map) }
}

/// `stdcall` rust_net_ptr_to_num4(device, list_base, max) -> EAX.
///
/// Cut AY: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Returns index×4 or `0xFFFFFFFF`.
/// FASM trampoline injects `net_device_list` + `NET_DEVICES_MAX`, moves
/// EAX→EDI, and preserves EAX/EBX/ECX/EDX/ESI/EBP (REG-001; TCP send keeps
/// EAX→socket across the call; ipv4/arp keep EDX→header).
///
/// # Safety
/// `list_base` must be a readable array of at least `max` device pointers.
#[no_mangle]
#[link_section = ".text.rust_net_ptr_to_num4"]
pub unsafe extern "stdcall" fn rust_net_ptr_to_num4(
    device: u32,
    list_base: *const u32,
    max: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes live net_device_list + NET_DEVICES_MAX.
    unsafe { net_ptr_to_num4_ptr(device, list_base, max) }
}

/// `stdcall` rust_socket_check(candidate, net_sockets) -> EAX.
///
/// Cut AS: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline injects `net_sockets`,
/// preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001), and restores ZF via
/// `test eax, eax` after the call (`pop` leaves flags).
///
/// Returns candidate pointer on hit, `0` on miss/null.
///
/// # Safety
/// `net_sockets` must be the live socket-list sentinel.
#[no_mangle]
#[link_section = ".text.rust_socket_check"]
pub unsafe extern "stdcall" fn rust_socket_check(
    candidate: u32,
    net_sockets: *const u8,
) -> u32 {
    // SAFETY: kernel trampoline passes live net_sockets sentinel.
    unsafe { socket_check_ptr(candidate, net_sockets) }
}

/// `stdcall` rust_get_coff_sym(pSym, count, sz_sym) -> EAX.
///
/// Cut AT: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Returns symbol `Value` or `0`.
/// FASM trampoline preserves EBX/ESI/EDI/EBP (REG-001; `load_library`
/// keeps ESI→DLLDESCR / EBX→coff across the call).
///
/// # Safety
/// `pSym` must address a readable `COFF_SYM` array for the walked range;
/// `sz_sym` must be readable for up to 8 bytes / until NUL.
#[no_mangle]
#[link_section = ".text.rust_get_coff_sym"]
pub unsafe extern "stdcall" fn rust_get_coff_sym(
    p_sym: *const u8,
    count: u32,
    sz_sym: *const u8,
) -> u32 {
    // SAFETY: kernel trampoline / load_library passes live symbol table + name.
    unsafe { get_coff_sym_ptr(p_sym, count, sz_sym) }
}

/// `stdcall` rust_ipv4_find_fragment_slot(packet, fragments, count) -> EAX.
///
/// Cut AU: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Returns slot pointer or `0xFFFF_FFFF`.
/// FASM trampoline injects `IPv4_fragments` + `IPv4_MAX_FRAGMENTS`, moves EAX→ESI,
/// and preserves EAX/EBX/ECX/EDX (REG-001; `ipv4_input` keeps EDX→packet /
/// EBX→device across the call).
///
/// # Safety
/// `packet` must be a readable IPv4 header (≥20 bytes); `fragments` must address
/// `count` readable `IPv4_FRAGMENT_slot` records.
#[no_mangle]
#[link_section = ".text.rust_ipv4_find_fragment_slot"]
pub unsafe extern "stdcall" fn rust_ipv4_find_fragment_slot(
    packet: *const u8,
    fragments: *const u8,
    count: u32,
) -> u32 {
    // SAFETY: kernel trampoline passes live packet header + fragment table.
    unsafe { ipv4_find_fragment_slot_ptr(packet, fragments, count) }
}

/// `stdcall` rust_ahci_find_cmdslot(slots, ncs) -> EAX.
///
/// Cut AV: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). Returns free slot index or `0xFFFF_FFFF`.
/// FASM trampoline reads `SACT|CI` and `(CAP>>8)&0xf` from the `PORT_DATA`
/// chain and preserves EBX/ECX/EDX/ESI (legacy push/pop; REG-001).
#[no_mangle]
#[link_section = ".text.rust_ahci_find_cmdslot"]
pub extern "stdcall" fn rust_ahci_find_cmdslot(slots: u32, ncs: u32) -> u32 {
    ahci_find_cmdslot(slots, ncs)
}

/// `stdcall` rust_ahci_is_sig_known(sig) -> EAX.
///
/// Cut BM: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). Returns `1` when `sig` is a known SATA
/// PxSIG, else `0`. FASM trampoline maps to legacy ZF via `cmp eax, 1`.
/// Preserves EBX/ECX/EDX/ESI/EDI/EBP (legacy destroys flags only; REG-001).
#[no_mangle]
#[link_section = ".text.rust_ahci_is_sig_known"]
pub extern "stdcall" fn rust_ahci_is_sig_known(sig: u32) -> u32 {
    ahci_is_sig_known(sig)
}

/// `stdcall` rust_swap_bytes_in_words(base, len).
///
/// Cut BG: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). In-place swaps `len` words at `base`.
/// FASM trampoline preserves EAX/EBX/ECX/EDX/ESI/EDI/EBP (REG-001; legacy
/// restores EAX/EBX/ECX and leaves EDX+ESI/EDI/EBP untouched).
///
/// # Safety
/// `base` must be writable for `len * 2` bytes when `len > 0`.
#[no_mangle]
#[link_section = ".text.rust_swap_bytes_in_words"]
pub unsafe extern "stdcall" fn rust_swap_bytes_in_words(base: *mut u16, len: u32) {
    swap_bytes_in_words(base, len);
}

/// `stdcall` rust_pci_make_config_cmd(bus, devfn, reg) -> EAX.
///
/// Cut BA: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Returns PCI mechanism-1 config address
/// dword with enable bit. FASM trampoline unpacks AH/BH/BL and preserves
/// EBX/ECX/EDX (REG-001; callers keep ESI size encode across the call).
#[no_mangle]
#[link_section = ".text.rust_pci_make_config_cmd"]
pub extern "stdcall" fn rust_pci_make_config_cmd(bus: u32, devfn: u32, reg: u32) -> u32 {
    pci_make_config_cmd(bus, devfn, reg)
}

/// `stdcall` rust_coff_get_align(section) -> EAX = alignment mask.
///
/// Cut BK: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). Returns `(1 << n) - 1` from
/// `COFF_SECTION.Characteristics` (4K default/clamp). FASM trampoline keeps
/// register ABI (`EDX→section`) and preserves ECX/EDX (REG-001).
///
/// # Safety
/// `section` must point to a readable `COFF_SECTION` through Characteristics.
#[no_mangle]
#[link_section = ".text.rust_coff_get_align"]
pub unsafe extern "stdcall" fn rust_coff_get_align(section: *const u8) -> u32 {
    // SAFETY: kernel trampoline passes EDX→COFF_SECTION.
    unsafe { coff_get_align_ptr(section) }
}

/// `stdcall` rust_v86_get_lin_addr(v86_addr, page_tabs) -> EAX = linear.
///
/// Cut BL: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline injects `page_tabs`,
/// keeps register ABI (`EAX=V86 addr`), and preserves all GPRs (legacy
/// destroys nothing; REG-001).
///
/// # Safety
/// `page_tabs` must be readable at index `v86_addr >> 12`.
#[no_mangle]
#[link_section = ".text.rust_v86_get_lin_addr"]
pub unsafe extern "stdcall" fn rust_v86_get_lin_addr(
    v86_addr: u32,
    page_tabs: *const u32,
) -> u32 {
    // SAFETY: kernel trampoline injects page_tabs base.
    unsafe { v86_get_lin_addr_ptr(v86_addr, page_tabs) }
}

/// `stdcall` rust_xfs_blkrel2sectabs(block_lo, block_hi, agblklog, agblocks,
/// mask_lo, mask_hi, sectpblog, out_hi) -> EAX = sector_lo.
///
/// Cut AW: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 32 bytes (`ret 32`). Writes sector_hi to `*out_hi`.
/// FASM trampoline extracts XFS fields from EBP→XFS and preserves
/// EBX/ESI (legacy `uses esi`; EBX live across `read_blocks`).
///
/// # Safety
/// `out_hi` must be a writable `u32` slot.
#[no_mangle]
#[link_section = ".text.rust_xfs_blkrel2sectabs"]
pub unsafe extern "stdcall" fn rust_xfs_blkrel2sectabs(
    block_lo: u32,
    block_hi: u32,
    agblklog: u32,
    agblocks: u32,
    mask_lo: u32,
    mask_hi: u32,
    sectpblog: u32,
    out_hi: *mut u32,
) -> u32 {
    // SAFETY: kernel trampoline passes a live stack slot for sector_hi.
    unsafe {
        xfs_blkrel2sectabs_ptr(
            block_lo, block_hi, agblklog, agblocks, mask_lo, mask_hi, sectpblog,
            out_hi,
        )
    }
}

/// `stdcall` rust_xfs_get_last_dirblock(inode, nextents_offset, inode_core_size,
/// dirblklog, out_hi) -> EAX = last_dirblock_lo.
///
/// Cut BO: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 20 bytes (`ret 20`). FASM trampoline passes `EBX` as the inode
/// base plus the live XFS offsets, then restores `EBX` and `ECX` to match the
/// legacy register-call ABI. Writes `last_dirblock_hi` to `*out_hi`.
///
/// # Safety
/// `inode` must point to a readable XFS inode buffer; `out_hi` must be writable.
#[no_mangle]
#[link_section = ".text.rust_xfs_get_last_dirblock"]
pub unsafe extern "stdcall" fn rust_xfs_get_last_dirblock(
    inode: *const u8,
    nextents_offset: u32,
    inode_core_size: u32,
    dirblklog: u32,
    out_hi: *mut u32,
) -> u32 {
    // SAFETY: kernel trampoline passes a live inode buffer and writable stack slot.
    unsafe { xfs_get_last_dirblock_ptr(inode, nextents_offset, inode_core_size, dirblklog, out_hi) }
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

/// `stdcall` rust_strrchr(s, c) -> EAX = ptr to last byte `c` or NULL.
///
/// Cut BB: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline preserves EDX (REG-001;
/// FASM leaf never touched EDX) and executes `cld` (legacy DF restore).
/// Returns the address as `u32` in EAX (`0` = NULL).
///
/// # Safety
/// `s` must be a readable NUL-terminated C string.
#[no_mangle]
#[link_section = ".text.rust_strrchr"]
pub unsafe extern "stdcall" fn rust_strrchr(s: *const u8, c: u32) -> u32 {
    // SAFETY: kernel callers pass valid C-string regions for this search.
    // Freestanding i686: usize == u32.
    unsafe { strrchr(s, c) as u32 }
}

/// `stdcall` rust_strncpy(s1, s2, n) -> EAX = s1.
///
/// Cut BF: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). Legacy FASM **clobbers EDX/ECX** —
/// trampoline must **not** invent EDX preserve (Cut BE smoke lesson).
/// Trampoline executes `cld` (legacy DF restore).
///
/// # Safety
/// `s2` readable for the scanned window; `s1` writable for `n` bytes when
/// `n > 0`.
#[no_mangle]
#[link_section = ".text.rust_strncpy"]
pub unsafe extern "stdcall" fn rust_strncpy(s1: *mut u8, s2: *const u8, n: u32) -> u32 {
    // SAFETY: kernel callers pass valid regions for this copy.
    // Freestanding i686: usize == u32.
    unsafe { strncpy(s1, s2, n) as u32 }
}

/// `stdcall` rust_strlen(s) -> EAX = length (byte count before NUL).
///
/// Cut BH: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline moves EAX → ECX and
/// restores EAX/EBX/EDX/ESI/EDI/EBP (legacy ECX-out register ABI).
///
/// # Safety
/// `s` must be a readable NUL-terminated C string.
#[no_mangle]
#[link_section = ".text.rust_strlen"]
pub unsafe extern "stdcall" fn rust_strlen(s: *const u8) -> u32 {
    // SAFETY: kernel callers pass valid C-string regions for this length.
    unsafe { strlen(s) }
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

/// `stdcall` rust_xfs_bigtime_to_secs(bt_lo, bt_hi) -> EAX seconds since 2001-01-01.
///
/// Cut AK: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline `movbe`-loads DQ, then
/// `call fsTime2bdfe` for calendar write + `EDI+=8` (matches original FASM
/// composition; keeps the large calendar out of the omit-FP XFS call path).
#[no_mangle]
#[link_section = ".text.rust_xfs_bigtime_to_secs"]
pub extern "stdcall" fn rust_xfs_bigtime_to_secs(bigtime_lo: u32, bigtime_hi: u32) -> u32 {
    xfs_bigtime_to_secs(bigtime_lo, bigtime_hi)
}

/// `stdcall` rust_xfs_conv_time_to_kos_epoch(secs, out) — writes 8-byte BDFE at `out`.
///
/// Cut BN: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline `movbe`-loads the high
/// dword from the XFS on-disk DQ, passes `EDI` as `out`, then performs
/// `add edi, 8` to match the public ABI. The low dword is intentionally ignored,
/// just like legacy FASM.
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[no_mangle]
#[link_section = ".text.rust_xfs_conv_time_to_kos_epoch"]
pub unsafe extern "stdcall" fn rust_xfs_conv_time_to_kos_epoch(secs: u32, out: *mut u8) {
    // SAFETY: kernel trampoline passes EDI → valid BDFE out block.
    unsafe { xfs_conv_time_to_kos_epoch_ptr(secs, out) }
}

/// `stdcall` rust_ext_unix_to_secs(i_time, extra) -> EAX seconds since 2001-01-01.
///
/// Cut AL: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 8 bytes (`ret 8`). FASM trampoline passes EAX/EDX then
/// `call fsTime2bdfe` for calendar write + `EDI+=8` (matches original FASM
/// composition; ECX preserved around the calendar call).
#[no_mangle]
#[link_section = ".text.rust_ext_unix_to_secs"]
pub extern "stdcall" fn rust_ext_unix_to_secs(i_time: u32, extra: u32) -> u32 {
    ext_unix_to_secs(i_time, extra)
}

/// `stdcall` rust_ext_read_all_times(inode, out) — writes 3× BDFE blocks at `out`.
///
/// Cut BR: dedicated section for reloc-free extract + FASM `file` embed.
/// Callee cleans 8 bytes (`ret 8`). Register-call trampoline passes ESI/EDI.
/// Inlines Cut AL + T (`ext_unix_to_secs` + `fs_time2bdfe_ptr`) — no cross-blob calls.
///
/// # Safety
/// `inode` must point to a readable EXT inode buffer; `out` writable for 24 bytes.
#[no_mangle]
#[link_section = ".text.rust_ext_read_all_times"]
pub unsafe extern "stdcall" fn rust_ext_read_all_times(inode: *const u8, out: *mut u8) {
    // SAFETY: kernel trampoline passes ESI/EDI from live inode + caller-owned out buffer.
    unsafe { ext_read_all_times_ptr(inode, out) }
}

/// `stdcall` rust_ext_write_time_pack(kos_secs, time_ptr, extra_time_ptr).
///
/// Cut BS: dedicated section for reloc-free extract + FASM `file` embed.
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline calls `fsGetTime` first.
/// `extra_time_ptr == 0xFFFFFFFF` skips extra write (FASM `-1` sentinel).
///
/// # Safety
/// `time_ptr` writable; when `extra_time_ptr != 0xFFFFFFFF`, it must address a writable `u32`.
#[no_mangle]
#[link_section = ".text.rust_ext_write_time_pack"]
pub unsafe extern "stdcall" fn rust_ext_write_time_pack(
    kos_secs: u32,
    time_ptr: *mut u32,
    extra_time_ptr: u32,
) {
    // SAFETY: kernel trampoline passes caller-owned inode field pointers.
    unsafe {
        ext_write_time_pack_ptr(kos_secs, time_ptr, extra_time_ptr as *mut u32)
    }
}

/// `stdcall` rust_ntfs_get_time_pack(kos_secs) -> EDX:EAX FILETIME.
///
/// Cut BT: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline passes EAX after `fsGetTime`.
///
/// Returns `u64` in `EDX:EAX` = `(hi << 32) | lo` matching FASM `ntfsGetTime`
/// FILETIME output (×10⁷ + bias add/adc after KOS seconds).
#[no_mangle]
#[link_section = ".text.rust_ntfs_get_time_pack"]
pub extern "stdcall" fn rust_ntfs_get_time_pack(kos_secs: u32) -> u64 {
    let (lo, hi) = crate::ntfs_get_time::ntfs_get_time_pack(kos_secs);
    ((hi as u64) << 32) | (lo as u64)
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

/// `stdcall` rust_iso9660_copy_name(esi_inout, edi_inout, max_len, nameenc, type_encoding)
///
/// Cut BI: volume-name encoding dispatch. Updates `*esi_inout` / `*edi_inout`
/// so EDI points at the written terminator. Callee cleans 20 bytes (`ret 20`).
///
/// # Safety
/// Inout pointers and buffers must obey `iso9660_copy_name` contracts.
#[no_mangle]
#[link_section = ".text.rust_iso9660_copy_name"]
pub unsafe extern "stdcall" fn rust_iso9660_copy_name(
    esi_inout: *mut *mut u8,
    edi_inout: *mut *mut u8,
    max_len: u32,
    nameenc: u32,
    type_encoding: u32,
) {
    // SAFETY: kernel trampoline passes &ESI / &EDI slots + encoding args.
    unsafe { iso9660_copy_name_ptr(esi_inout, edi_inout, max_len, nameenc, type_encoding) }
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

/// `stdcall` rust_ntfs_create_mcb_entry(start, size, dest, attr, frs, out_dest)
/// → EAX bit0 = need_cld (FRS slide ran); *out_dest = new EDI.
///
/// Cut AX: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 24 bytes (`ret 24`). FASM trampoline issues `cld` when
/// bit0 is set; preserves EBX; EBP→NTFS stays caller-owned (omit-FP).
///
/// # Safety
/// `dest`/`attr`/`frs` must be live writable FRS-relative regions;
/// `out_dest` must be a writable pointer slot.
#[no_mangle]
#[link_section = ".text.rust_ntfs_create_mcb_entry"]
pub unsafe extern "stdcall" fn rust_ntfs_create_mcb_entry(
    start: u32,
    size: u32,
    dest: *mut u8,
    attr: *mut u8,
    frs: *mut u8,
    out_dest: *mut *mut u8,
) -> u32 {
    // SAFETY: kernel trampoline passes live NTFS FRS/attr/dest pointers.
    unsafe { ntfs_create_mcb_entry_ptr(start, size, dest, attr, frs, out_dest) }
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

/// `stdcall` rust_fat_name_is_legal(name) -> EAX = 1 legal / 0 illegal.
///
/// Cut BC: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline maps EAX → `stc`/`clc`
/// (1 = CF set = legal, 0 = CF clear = illegal); preserves ECX/EDX (REG-001).
///
/// # Safety
/// `name` must be a readable NUL-terminated UTF-8 (or byte) name.
#[no_mangle]
#[link_section = ".text.rust_fat_name_is_legal"]
pub unsafe extern "stdcall" fn rust_fat_name_is_legal(name: *const u8) -> u32 {
    // SAFETY: kernel trampoline passes ESI → valid C-string name.
    unsafe { fat_name_is_legal_ptr(name) }
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

/// `stdcall` rust_tcp_outflags(socket) -> EAX = TCP header flags.
///
/// Cut BD: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). FASM trampoline preserves
/// `EAX`/`EBX`/`ECX` and places the result in `EDX` (legacy OUT).
///
/// # Safety
/// `socket` must point to a readable `TCP_SOCKET` through `t_state`.
#[no_mangle]
#[link_section = ".text.rust_tcp_outflags"]
pub unsafe extern "stdcall" fn rust_tcp_outflags(socket: *const u8) -> u32 {
    // SAFETY: kernel trampoline passes EAX → valid socket.
    unsafe { tcp_outflags_ptr(socket) }
}

/// `stdcall` rust_hotkey_do_test(funcs, kb_state, cl) -> EAX = 0 pass / ≠0 fail.
///
/// Cut BE: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`). FASM trampoline injects `[eax+4]` and
/// `[kb_state]`, preserves `EAX` (hotkey node), and maps the return to CF.
#[no_mangle]
#[link_section = ".text.rust_hotkey_do_test"]
pub extern "stdcall" fn rust_hotkey_do_test(funcs: u32, kb_state: u32, cl: u32) -> u32 {
    hotkey_do_test(funcs, kb_state, cl)
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

/// `stdcall` rust_fix_coff_symbols(sec, symbols, sym_count, strings, imports, get_proc_ex) -> EAX.
///
/// Cut BU: dedicated section for reloc-free extract + FASM `file` embed.
/// Callee cleans 24 bytes (`ret 24`). Injected `get_proc_ex` resolves externals.
///
/// # Safety
/// Live COFF symbol/section/string tables; `get_proc_ex` must match legacy ABI.
#[no_mangle]
#[link_section = ".text.rust_fix_coff_symbols"]
pub unsafe extern "stdcall" fn rust_fix_coff_symbols(
    sec: *const u8,
    symbols: *mut u8,
    sym_count: u32,
    strings: *const u8,
    imports: u32,
    get_proc_ex: GetProcExFn,
) -> u32 {
    unsafe { fix_coff_symbols_ptr(sec, symbols, sym_count, strings, imports, get_proc_ex) }
}

/// `stdcall` rust_fs_get_time(read_cmos) -> EAX.
///
/// Cut BV: dedicated section for reloc-free extract + FASM `file` embed.
/// Callee cleans 4 bytes (`ret 4`). Injected `read_cmos` wraps FASM `fsReadCMOS`.
///
/// # Safety
/// `read_cmos` must match legacy `fs_read_cmos_stdcall` ABI.
#[no_mangle]
#[link_section = ".text.rust_fs_get_time"]
pub unsafe extern "stdcall" fn rust_fs_get_time(read_cmos: FsReadCmosFn) -> u32 {
    unsafe { fs_get_time_ptr(read_cmos) }
}

/// `stdcall` rust_fs_read_cmos(raw_read, reg) -> EAX (AX = decoded 0–99).
///
/// Cut CA: dedicated section for reloc-free extract + FASM `file` embed.
/// Callee cleans 8 bytes (`ret 8`). Injected `raw_read` performs `out 0x70`/`in 0x71`.
///
/// # Safety
/// `raw_read` must match legacy `fs_cmos_raw_read_stdcall` ABI.
#[no_mangle]
#[link_section = ".text.rust_fs_read_cmos"]
pub unsafe extern "stdcall" fn rust_fs_read_cmos(raw_read: FsCmosRawReadFn, reg: u32) -> u32 {
    unsafe { fs_read_cmos_ptr(raw_read, reg) }
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

/// `stdcall` rust_is_string_userspace(base) -> EAX ∈ {0,1}.
///
/// Cut BJ: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`).
///
/// Return `1` = legacy FASM `ZF=1` (NUL found in userspace window);
/// `0` = legacy FASM `ZF=0` (reject). The FASM trampoline reconstructs ZF
/// via `cmp eax, 1` and restores caller EAX/ECX/EDX/EDI with flag-neutral pops.
///
/// # Safety
/// When `base as u32 <= OS_BASE-1`, `base` must be readable for
/// `min(OS_BASE - base, 0x10000)` bytes or until the first NUL inclusive.
#[no_mangle]
#[link_section = ".text.rust_is_string_userspace"]
pub unsafe extern "stdcall" fn rust_is_string_userspace(base: *const u8) -> u32 {
    // SAFETY: kernel trampoline passes the caller string pointer.
    unsafe { is_string_userspace(base) }
}

/// `stdcall` rust_file_system_is_operation_safe(inf) -> EAX ∈ {0,1}.
///
/// Cut AZ: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 4 bytes (`ret 4`). Inlines Cut P region check (no
/// cross-section call into `rust_is_region_userspace`).
///
/// Return `1` = legacy FASM `ZF=1` (safe / unknown-subfn accept);
/// `0` = legacy FASM `ZF=0` (unsafe). The FASM trampoline reconstructs ZF
/// via `cmp eax, 1` and restores caller EAX/EBX/ECX/EDX with flag-neutral pops.
///
/// # Safety
/// `inf` must point at a readable sysfn70/80 information structure (≥20 bytes).
#[no_mangle]
#[link_section = ".text.rust_file_system_is_operation_safe"]
pub unsafe extern "stdcall" fn rust_file_system_is_operation_safe(inf: *const u8) -> u32 {
    // SAFETY: kernel trampoline passes the caller info-struct pointer.
    unsafe { file_system_is_operation_safe_ptr(inf) }
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

/// `stdcall` rust_cp866_to_utf8_string(...) -> packed (SF<<31)|(ZF<<30)|EAX.
#[no_mangle]
#[link_section = ".text.rust_cp866_to_utf8_string"]
pub unsafe extern "stdcall" fn rust_cp866_to_utf8_string(
    src: *const u8,
    dest: *mut u8,
    ecx_in: u32,
    src_out: *mut u32,
    dest_out: *mut u32,
    ecx_out: *mut u32,
) -> u32 {
    // SAFETY: kernel trampoline passes valid out-slots and readable/writable buffers.
    unsafe { cp866_to_utf8_string_ptr(src, dest, ecx_in, src_out, dest_out, ecx_out) }
}

/// `stdcall` rust_utf16_to_8_string(...) -> packed (SF<<31)|(ZF<<30)|EAX.
#[no_mangle]
#[link_section = ".text.rust_utf16_to_8_string"]
pub unsafe extern "stdcall" fn rust_utf16_to_8_string(
    src: *const u8,
    dest: *mut u8,
    ecx_in: u32,
    src_out: *mut u32,
    dest_out: *mut u32,
    ecx_out: *mut u32,
) -> u32 {
    use crate::utf16_to_8::{pack_sf_zf_eax, utf16_to_8};

    let mut esi = src as usize;
    let mut edi = dest as usize;
    let mut ecx = ecx_in;
    let mut eax = 0u32;

    loop {
        let ax = unsafe { core::ptr::read_unaligned(esi as *const u16) };
        esi += 2;
        eax = (eax & 0xFFFF_0000) | u32::from(ax);

        let r = unsafe { utf16_to_8(ax, edi as *mut u8, ecx) };
        if r.sf != 0 {
            unsafe {
                *src_out = esi as u32;
                *dest_out = if r.edi_delta != 0 {
                    (edi + r.edi_delta as usize) as u32
                } else {
                    edi as u32
                };
                *ecx_out = r.ecx;
            }
            return pack_sf_zf_eax(1, 0, eax);
        }
        ecx = r.ecx;
        if r.edi_delta != 0 {
            edi += r.edi_delta as usize;
        }
        if eax == 0 {
            unsafe {
                *src_out = esi as u32;
                *dest_out = edi as u32;
                *ecx_out = ecx;
            }
            return pack_sf_zf_eax(0, 1, 0);
        }
    }
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

/// `stdcall` rust_xfs_get_before_by_hashval(entries, count, hash) -> EDX:EAX packed.
///
/// Cut AM: dedicated section for reloc-free extract + FASM `file` embed.
/// Must remain free of GOT/rodata/external calls (verified by extractor).
/// Callee cleans 12 bytes (`ret 12`).
///
/// Returns `u64` in `EDX:EAX` = `(zf << 32) | result`, where `zf` is the
/// legacy found/miss ZF sense (`1`/`0`) and `result` is the BE `before` or
/// `ERROR_FILE_NOT_FOUND`. The FASM trampoline reconstructs ZF via
/// `cmp edx, 1` then restores EAX with a flag-neutral `pop`.
///
/// `entries` is the btree table (`stdcall _base - 8` ≡ `node + offsetof(btree)`).
///
/// # Safety
/// `entries` must be readable for `count * 8` bytes when `count > 0`.
#[no_mangle]
#[link_section = ".text.rust_xfs_get_before_by_hashval"]
pub unsafe extern "stdcall" fn rust_xfs_get_before_by_hashval(
    entries: *const u8,
    count: u32,
    hash: u32,
) -> u64 {
    // SAFETY: kernel trampoline passes `_base - 8` + count/hash.
    let r = unsafe { xfs_get_before_by_hashval_ptr(entries, count, hash) };
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
