//! Cut A/B/C/D/E/F/G/H/I/J/K/L/M/N/O/P/Q/R/S/T/U/V/W/X/Y/Z/AA/AB/AC/AD/AE/AF/AG/AH/AI/AJ/AK/AL/AM/AN/AO/AP/AQ/AR/AS/AT/AU/AV/AW/AX/AY/AZ/BA/BB/BC/BD/BE/BF/BG
//! utilities: CRC32, Unicode (incl. CP866 encode+decode), casefold, string, checksum,
//! filesystem calendar (BDFE↔secs), NTFS FILETIME↔BDFE, NTFS bootsector CF validate, exFAT
//! SetChecksum + NameHash rolling hash, ISO9660 path-component name match, XFS v5 bigtime→BDFE,
//! EXT Unix→BDFE, FAT packed-time→BDFE, video geometry, NTFS MCB decode+encode, NTFS USA restore,
//! FAT 8.3 short-name collision + LFN→8.3 generator + LFN charset legality, HID mouse acceleration
//! + hotkey field match, TCP RTT estimator + persist-timer arming + state→header flags, GUI font
//! anti-aliasing, MENUET app-header validation, syscall userspace region gate + sysfn70/80
//! operation-safe size→gate, UTF-16→UTF-8 streaming encode, UTF-8→UTF-16 streaming decode, XFS
//! extent unpack + dir leaf hash binary search + DA node first-match-by-hash + dir name hash +
//! AG-relative block→absolute sector, window screen-fit helpers, TSS I/O permission bitmap
//! updates + port-area reserve/free, COFF reloc application + symbol name→Value lookup, MBR/EBR
//! partition-table entry validation, GPT protective-MBR recognition, process TID→slot lookup,
//! IPv4 on-link/gateway/broadcast routing + fragment-slot lookup, AHCI free command-slot scan,
//! kernel VA→PA page translation, socket-list membership, NIC device-list ptr→index×4, PCI
//! config-space address encode, reverse character search (`strrchr`), bounded padded
//! copy (`strncpy`), and endian word-byte swap (`swap_bytes_in_words`) for KolibriOS hybrid
//! migration.
//!
//! Freestanding on `os = "none"` targets (`no_std`, no allocator).
//! Host `cargo test` uses the normal Windows/Linux target with `std`.
//! See `docs/migration/cut-a-final-architecture.md`, `cut-b-plan.md`, `cut-c-plan.md`,
//! `cut-d-plan.md`, `cut-e-plan.md`, `cut-f-plan.md`, `cut-g-plan.md`, `cut-h-plan.md`,
//! `cut-i-plan.md`, `cut-j-plan.md`, `cut-k-plan.md`, `cut-l-plan.md`, `cut-m-plan.md`,
//! `cut-n-plan.md`, `cut-o-plan.md`.

#![cfg_attr(target_os = "none", no_std)]

mod ahci_find_cmdslot;
mod app_header;
mod casefold;
mod checksum;
mod coff_reloc;
mod crc;
mod exfat_checksum;
mod fat_name;
mod font;
mod fs_operation_safe;
mod geometry;
mod get_coff_sym;
mod get_pg_addr;
mod io_access;
mod ipv4_find_fragment_slot;
mod ipv4_route;
mod iso9660_compare;
mod hotkey;
mod mouse;
mod ntfs_bootsec;
mod ntfs_create_mcb;
mod ntfs_mcb;
mod ntfs_usa;
mod partition;
mod pci_make_config_cmd;
mod pid_to_slot;
mod port_area;
mod net_ptr_to_num4;
mod socket_check;
mod string;
mod swap_bytes_in_words;
mod tcp;
mod time;
mod unicode;
mod userspace;
mod utf16_to_8;
mod utf8to16;
mod window;
mod xfs_blkrel2sectabs;
mod xfs_extent;
mod xfs_hash_lookup;
mod xfs_hashname;
mod xfs_node_hash;

#[cfg(target_arch = "x86")]
mod ffi;

pub use ahci_find_cmdslot::{
    ahci_find_cmdslot, ahci_find_cmdslot_from_regs, AHCI_FIND_CMDSLOT_PRNG_SEED,
};
pub use app_header::{
    test_app_header, OS_BASE, TEST_APP_HEADER_PRNG_SEED, APP_OFF_CMDLINE, APP_OFF_EDATA,
    APP_OFF_EIP, APP_OFF_EMEM, APP_OFF_ESP, APP_OFF_PATH,
};
pub use casefold::{cp866_to_upper, utf16_to_upper};
pub use checksum::{checksum_1, checksum_2};
pub use coff_reloc::{
    fix_coff_relocs, fix_coff_relocs_buffered, fix_coff_relocs_ptr, COFF_HEADER_SIZE,
    COFF_RELOC_SIZE, COFF_SECTION_SIZE, COFF_SYM_SIZE, FIX_COFF_RELOCS_PRNG_SEED,
    RELOC_TYPE_DIR32, RELOC_TYPE_REL32,
};
pub use crc::crc32_update;
pub use exfat_checksum::{
    calculate_set_checksum_field, calculate_set_checksum_field_ptr, exfat_hash_calculate,
    exfat_hash_calculate_ptr, exfat_rolling_checksum, CALCULATE_SET_CHECKSUM_FIELD_PRNG_SEED,
    EXFAT_FILE_DIR_ENTRY_SIZE, EXFAT_HASH_CALCULATE_PRNG_SEED, SET_CHECKSUM_MIN_STORE,
};
pub use fat_name::{
    fat_gen_short_name, fat_next_short_name, FatNextShortNameResult, FAT_GEN_FILL_LEN,
    FAT_GEN_SHORT_NAME_PRNG_SEED, FAT_NAME_LEN,
};
pub use font::{anti_aliasing, ANTI_ALIASING_PRNG_SEED};
pub use fs_operation_safe::{
    file_system_is_operation_safe, file_system_is_operation_safe_ptr, fs_op_safe_buffer_len,
    fs_op_safe_buffer_len_ex, make_inf, BDVK_CP866, BDVK_HEADER, BDVK_UNICODE,
    FILE_SYSTEM_IS_OPERATION_SAFE_PRNG_SEED, INF_STRUCT_MIN, OFF_BUFFER_BASE, OFF_COUNT_OR_SIZE,
    OFF_ENCODING, OFF_SUBFN, SUBFN5_LEN, SUBFN6_LEN,
};
pub use geometry::{block_clip, BlockClipResult, Rect};
pub use get_coff_sym::{
    get_coff_sym, get_coff_sym_oracle, get_coff_sym_ptr, make_sym, name_eq_n, COFF_SYM_NAME_LEN,
    GET_COFF_SYM_PRNG_SEED, OFF_SYM_VALUE as GET_COFF_SYM_OFF_VALUE,
    COFF_SYM_SIZE as GET_COFF_SYM_SIZE,
};
pub use get_pg_addr::{
    get_pg_addr, get_pg_addr_ptr, GET_PG_ADDR_PRNG_SEED, IDENTITY_WINDOW, OS_BASE as GET_PG_ADDR_OS_BASE,
    PAGE_SIZE as GET_PG_ADDR_PAGE_SIZE, PAGE_TABS,
};
pub use io_access::{
    io_map_bit, set_io_access_rights, set_io_access_rights_ptr, IO_MAP_BITS, IO_MAP_BYTES,
    SET_IO_ACCESS_RIGHTS_PRNG_SEED,
};
pub use ipv4_find_fragment_slot::{
    ipv4_find_fragment_slot, ipv4_find_fragment_slot_from_keys, ipv4_find_fragment_slot_ptr,
    FRAGMENT_SLOT_SIZE, IPV4_FIND_FRAGMENT_SLOT_PRNG_SEED, IPV4_HEADER_MIN_LEN, IPV4_MAX_FRAGMENTS,
    OFF_HDR_DST_IP, OFF_HDR_IDENTIFICATION, OFF_HDR_SRC_IP, OFF_SLOT_DST_IP, OFF_SLOT_ID,
    OFF_SLOT_PTR, OFF_SLOT_SRC_IP, OFF_SLOT_TTL,
};
pub use ipv4_route::{
    ipv4_route, ipv4_route_ptr, Ipv4RouteResult, IPV4_BROADCAST,
    IPV4_ROUTE_PRNG_SEED, NET_DEVICES_MAX, OFF_LINK_STATE, TABLE_BYTES,
};
pub use iso9660_compare::{
    iso9660_compare_name, iso9660_compare_name_ptr, Iso9660CompareNameResult,
    ISO9660_COMPARE_NAME_PRNG_SEED, ISO9660_DIR_OFF_NAME, ISO9660_DIR_OFF_NAME_LEN,
};
pub use hotkey::{hotkey_do_test, HOTKEY_DO_TEST_PRNG_SEED, HOTKEY_TESTS_NUM};
pub use mouse::mouse_acceleration;
pub use net_ptr_to_num4::{
    net_ptr_to_num4, net_ptr_to_num4_from_slice, net_ptr_to_num4_ptr, NET_PTR_TO_NUM4_MISS,
    NET_PTR_TO_NUM4_PRNG_SEED,
};
pub use ntfs_bootsec::{
    fasm_oracle_ntfs_test_bootsec, make_valid_ntfs_bootsec, ntfs_test_bootsec,
    ntfs_test_bootsec_ptr, NTFS_BOOTSEC_MIN_LEN, NTFS_TEST_BOOTSEC_PRNG_SEED,
};
pub use ntfs_create_mcb::{
    mcb_size_width, mcb_start_width, ntfs_create_mcb_entry_fixture, ntfs_create_mcb_entry_ptr,
    CreateMcbFixtureResult, NTFS_CREATE_MCB_ENTRY_PRNG_SEED, OFF_RECORD_ALLOCATED_SIZE,
    OFF_RECORD_REAL_SIZE, OFF_SIZE_WITH_HEADER,
};
pub use ntfs_mcb::{ntfs_decode_mcb_entry, McbDecodeResult};
pub use ntfs_usa::{ntfs_restore_usa, UsaRestoreResult};
pub use partition::{
    is_partition_table_entry, is_partition_table_entry_ptr, is_protective_mbr,
    is_protective_mbr_ptr, make_entry, make_mbr_sector, make_protective_entry0,
    IS_PARTITION_TABLE_ENTRY_PRNG_SEED, IS_PROTECTIVE_MBR_PRNG_SEED, OFF_BOOTABLE,
    OFF_FIRST_ABS_SECTOR, OFF_LENGTH, OFF_TYPE, PARTITION_TABLE_ENTRY_SIZE, PROTECTIVE_MBR_TYPE,
    PROTECTIVE_TRAILING_BYTES,
};
pub use pci_make_config_cmd::{
    pci_make_config_cmd, pci_make_config_cmd_from_regs, PCI_MAKE_CONFIG_CMD_PRNG_SEED,
};
pub use pid_to_slot::{
    pid_to_slot, pid_to_slot_ptr, plant_slot, write_u32_le, APPDATA_SIZE, APPDATA_SIZE_SHIFT,
    OFF_TID as APPDATA_OFF_TID, OFF_STATE as APPDATA_OFF_STATE, PID_TO_SLOT_PRNG_SEED, TSTATE_FREE,
};
pub use port_area::{
    r_f_port_area, r_f_port_area_ptr, ENTRY_SIZE as PORT_AREA_ENTRY_SIZE, MAX_RESERVED,
    OFF_END as PORT_AREA_OFF_END, OFF_START as PORT_AREA_OFF_START, OFF_TID as PORT_AREA_OFF_TID,
    R_F_PORT_AREA_PRNG_SEED,
};
pub use socket_check::{
    socket_check, socket_check_from_first, socket_check_ptr, OFF_NEXT_PTR as SOCKET_OFF_NEXT_PTR,
    SOCKET_CHECK_PRNG_SEED,
};
pub use string::{strncmp, strncpy, strrchr, STRNCPY_PRNG_SEED, STRRCHR_PRNG_SEED};
pub use swap_bytes_in_words::{
    swap_bytes_in_words, swap_bytes_in_words_slice, SWAP_BYTES_IN_WORDS_PRNG_SEED,
};
pub use tcp::{
    tcp_outflags, tcp_outflags_ptr, tcp_set_persist, tcp_set_persist_ptr, tcp_xmit_timer,
    TCP_MAX_RXTSHIFT, TCP_OFF_TIMER_FLAGS, TCP_OFF_TIMER_PERSIST, TCP_OFF_T_RTT, TCP_OFF_T_RTTVAR,
    TCP_OFF_T_RXTSHIFT, TCP_OFF_T_SRTT, TCP_OFF_T_STATE, TCP_OUTFLAGS_PRNG_SEED,
    TCP_SET_PERSIST_PRNG_SEED, TCP_TIME_PERS_MAX, TCP_TIME_PERS_MIN, TCP_XMIT_TIMER_PRNG_SEED,
    TH_ACK, TH_FIN, TH_RST, TH_SYN, TIMER_FLAG_PERSIST, TIMER_FLAG_RETRANSMISSION, TCPS_TIME_WAIT,
};
pub use time::{
    bigtime_from_secs_2001, ext_read_time, ext_read_time_ptr, ext_unix_to_secs, fat_time_to_bdfe,
    filetime_from_secs_2001, fs_calculate_time, fs_time2bdfe, fs_time2bdfe_ptr, ntfs_calculate_time,
    ntfs_calculate_time_ptr, ntfs_datetime_to_bdfe, ntfs_datetime_to_bdfe_ptr, ntfs_filetime_to_secs,
    pack_bigtime_be, pack_filetime, xfs_bigtime_to_secs, xfs_conv_bigtime_to_kos_epoch,
    xfs_conv_bigtime_to_kos_epoch_ptr, BdfeTime, EXT_READ_TIME_PRNG_SEED, FAT_TIME_TO_BDFE_PRNG_SEED,
    NTFS_CALCULATE_TIME_PRNG_SEED, NTFS_DATETIME_TO_BDFE_PRNG_SEED, NTFS_FILETIME_BIAS_HI,
    NTFS_FILETIME_BIAS_LO, NTFS_FILETIME_PER_SEC, UNIXTIME_TO_KOS_OFFSET,
    XFS_BIGTIME_TO_KOS_OFFSET_NS, XFS_BIGTIME_TO_KOS_OFFSET_NS_HI, XFS_BIGTIME_TO_KOS_OFFSET_NS_LO,
    XFS_CONV_BIGTIME_TO_KOS_EPOCH_PRNG_SEED, XFS_NANOSEC_PER_SEC,
};
pub use unicode::{cp866_decode, cp866_encode, utf16_encode, utf8_decode, ANSI2UNI_CHAR_PRNG_SEED};
pub use userspace::{
    is_region_userspace, trampoline_zf_from_rust_return, IS_REGION_USERSPACE_PRNG_SEED,
};
pub use utf16_to_8::{
    pack_sf_eax, trampoline_eax_from_packed, trampoline_sf_from_packed, unpack_sf_eax, utf16_to_8,
    utf16_to_8_ptr, Utf16To8Result, UTF16_TO_8_PRNG_SEED,
};
pub use utf8to16::{utf8to16, utf8to16_ptr, Utf8To16Result, UTF8TO16_PRNG_SEED};
pub use window::{
    check_window_position, check_window_position_ptr, WindowBox, CHECK_WINDOW_POSITION_PRNG_SEED,
};
pub use xfs_blkrel2sectabs::{
    xfs_blkrel2sectabs, xfs_blkrel2sectabs_from_regs, xfs_blkrel2sectabs_ptr,
    XFS_BLKREL2SECTABS_PRNG_SEED,
};
pub use xfs_extent::{
    read_xfs_bmbt_irec, write_xfs_bmbt_irec, xfs_extent_unpack, xfs_extent_unpack_into,
    xfs_extent_unpack_ptr, XfsBmbtIrec, OFF_BLOCKCOUNT, OFF_STARTBLOCK_HI, OFF_STARTBLOCK_LO,
    OFF_STARTOFF_HI, OFF_STARTOFF_LO, OFF_STATE, XFS_BMBT_IREC_SIZE, XFS_EXTENT_UNPACK_PRNG_SEED,
};
pub use xfs_hash_lookup::{
    pack_eax_zf, trampoline_zf_from_flag, unpack_eax_zf, xfs_get_addr_by_hash,
    xfs_get_addr_by_hash_ptr, XfsHashLookupResult, ERROR_FILE_NOT_FOUND, OFF_ADDRESS, OFF_HASHVAL,
    XFS_DIR2_LEAF_ENTRY_SIZE, XFS_GET_ADDR_BY_HASH_PRNG_SEED,
};
pub use xfs_hashname::{xfs_hashname, xfs_hashname_ptr, XFS_HASHNAME_PRNG_SEED};
pub use xfs_node_hash::{
    entries_from_base, entries_from_node, xfs_get_before_by_hashval,
    xfs_get_before_by_hashval_ptr, BASE_TO_BTREE_DELTA, OFF_NODE_BEFORE, OFF_NODE_HASHVAL,
    OFF_V4_BTREE, OFF_V5_BTREE, XFS_DA_NODE_ENTRY_SIZE, XFS_GET_BEFORE_BY_HASHVAL_PRNG_SEED,
};

/// Phase C probe magic (must match FASM `PHASE_C_PROBE_MAGIC` and freestanding FFI).
pub const PHASE_C_PROBE_MAGIC: u32 = 0xC0DE_A11C;

#[cfg(test)]
mod phase_c_magic_tests {
    #[test]
    fn phase_c_probe_magic_is_stable() {
        assert_eq!(crate::PHASE_C_PROBE_MAGIC, 0xC0DE_A11C);
    }
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
