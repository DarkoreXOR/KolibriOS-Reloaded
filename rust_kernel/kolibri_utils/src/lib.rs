//! Cut A utilities: CRC32 and Unicode helpers for KolibriOS hybrid migration.
//!
//! Freestanding on `os = "none"` targets (`no_std`, no allocator).
//! Host `cargo test` uses the normal Windows/Linux target with `std`.
//! See `docs/migration/cut-a-implementation.md` and `docs/_meta/project-structure.md`.

#![cfg_attr(target_os = "none", no_std)]

mod crc;
mod unicode;

#[cfg(target_arch = "x86")]
mod ffi;

pub use crc::crc32_update;
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
