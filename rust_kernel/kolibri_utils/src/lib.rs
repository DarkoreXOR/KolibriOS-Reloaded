//! Cut A/B/C/D/E/F/G utilities: CRC32, Unicode, casefold, string, checksum,
//! and filesystem calendar helpers for KolibriOS hybrid migration.
//!
//! Freestanding on `os = "none"` targets (`no_std`, no allocator).
//! Host `cargo test` uses the normal Windows/Linux target with `std`.
//! See `docs/migration/cut-a-final-architecture.md`, `cut-b-plan.md`, `cut-c-plan.md`,
//! `cut-d-plan.md`, `cut-e-plan.md`, `cut-f-plan.md`, `cut-g-plan.md`.

#![cfg_attr(target_os = "none", no_std)]

mod casefold;
mod checksum;
mod crc;
mod string;
mod time;
mod unicode;

#[cfg(target_arch = "x86")]
mod ffi;

pub use casefold::{cp866_to_upper, utf16_to_upper};
pub use checksum::{checksum_1, checksum_2};
pub use crc::crc32_update;
pub use string::strncmp;
pub use time::{fs_calculate_time, BdfeTime};
pub use unicode::{cp866_encode, utf16_encode, utf8_decode};

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
