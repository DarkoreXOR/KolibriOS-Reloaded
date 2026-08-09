//! Cut A/B/C/D/E/F/G/H/I/J/K/L/M/N/O/P/Q/R/S/T utilities: CRC32, Unicode, casefold, string, checksum,
//! filesystem calendar (BDFE↔secs), video geometry, NTFS MCB decode, NTFS USA restore, FAT
//! 8.3 short-name collision, HID mouse acceleration, TCP RTT estimator, GUI
//! font anti-aliasing, MENUET app-header validation, syscall userspace
//! region gate, UTF-16→UTF-8 streaming encode, XFS extent unpack, and window
//! screen-fit helpers for KolibriOS hybrid migration.
//!
//! Freestanding on `os = "none"` targets (`no_std`, no allocator).
//! Host `cargo test` uses the normal Windows/Linux target with `std`.
//! See `docs/migration/cut-a-final-architecture.md`, `cut-b-plan.md`, `cut-c-plan.md`,
//! `cut-d-plan.md`, `cut-e-plan.md`, `cut-f-plan.md`, `cut-g-plan.md`, `cut-h-plan.md`,
//! `cut-i-plan.md`, `cut-j-plan.md`, `cut-k-plan.md`, `cut-l-plan.md`, `cut-m-plan.md`,
//! `cut-n-plan.md`, `cut-o-plan.md`.

#![cfg_attr(target_os = "none", no_std)]

mod app_header;
mod casefold;
mod checksum;
mod crc;
mod fat_name;
mod font;
mod geometry;
mod mouse;
mod ntfs_mcb;
mod ntfs_usa;
mod string;
mod tcp;
mod time;
mod unicode;
mod userspace;
mod utf16_to_8;
mod window;
mod xfs_extent;

#[cfg(target_arch = "x86")]
mod ffi;

pub use app_header::{
    test_app_header, OS_BASE, TEST_APP_HEADER_PRNG_SEED, APP_OFF_CMDLINE, APP_OFF_EDATA,
    APP_OFF_EIP, APP_OFF_EMEM, APP_OFF_ESP, APP_OFF_PATH,
};
pub use casefold::{cp866_to_upper, utf16_to_upper};
pub use checksum::{checksum_1, checksum_2};
pub use crc::crc32_update;
pub use fat_name::{fat_next_short_name, FatNextShortNameResult};
pub use font::{anti_aliasing, ANTI_ALIASING_PRNG_SEED};
pub use geometry::{block_clip, BlockClipResult, Rect};
pub use mouse::mouse_acceleration;
pub use ntfs_mcb::{ntfs_decode_mcb_entry, McbDecodeResult};
pub use ntfs_usa::{ntfs_restore_usa, UsaRestoreResult};
pub use string::strncmp;
pub use tcp::{
    tcp_xmit_timer, TCP_OFF_T_RTT, TCP_OFF_T_RTTVAR, TCP_OFF_T_SRTT, TCP_XMIT_TIMER_PRNG_SEED,
};
pub use time::{fs_calculate_time, fs_time2bdfe, fs_time2bdfe_ptr, BdfeTime};
pub use unicode::{cp866_encode, utf16_encode, utf8_decode};
pub use userspace::{
    is_region_userspace, trampoline_zf_from_rust_return, IS_REGION_USERSPACE_PRNG_SEED,
};
pub use utf16_to_8::{
    pack_sf_eax, trampoline_eax_from_packed, trampoline_sf_from_packed, unpack_sf_eax, utf16_to_8,
    utf16_to_8_ptr, Utf16To8Result, UTF16_TO_8_PRNG_SEED,
};
pub use window::{
    check_window_position, check_window_position_ptr, WindowBox, CHECK_WINDOW_POSITION_PRNG_SEED,
};
pub use xfs_extent::{
    read_xfs_bmbt_irec, write_xfs_bmbt_irec, xfs_extent_unpack, xfs_extent_unpack_into,
    xfs_extent_unpack_ptr, XfsBmbtIrec, OFF_BLOCKCOUNT, OFF_STARTBLOCK_HI, OFF_STARTBLOCK_LO,
    OFF_STARTOFF_HI, OFF_STARTOFF_LO, OFF_STATE, XFS_BMBT_IREC_SIZE, XFS_EXTENT_UNPACK_PRNG_SEED,
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
