//! XFS directory name hash (`xfs_hashname`) — Cut AP.
//!
//! FASM (`kernel/fs/xfs.asm`):
//! ```text
//! xor eax, eax
//! for each byte: rol eax, 7 ; xor al, [esi++] ; dec ecx ; jnz
//! ```
//!
//! `len == 0` hangs in legacy FASM (do-while `dec`/`jnz`). Rust returns 0
//! without reading (same documented quirk class as Cut AI NameHash).

/// Cut AP differential PRNG seed (`'CUTP'`).
pub const XFS_HASHNAME_PRNG_SEED: u32 = 0x4355_5450;

/// XFS directory name hash (ROL 7 ⊕ byte stream).
///
/// # Safety
/// `name` must be readable for `len` bytes when `len > 0`.
#[inline(always)]
pub unsafe fn xfs_hashname(name: *const u8, len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    let mut eax: u32 = 0;
    let mut i: u32 = 0;
    while i < len {
        // SAFETY: i < len within caller contract.
        let byte = unsafe { *name.add(i as usize) };
        eax = eax.rotate_left(7) ^ u32::from(byte);
        // FASM only XORs into AL; high bits of EAX after ROL are untouched by XOR.
        // `rotate_left(7) ^ byte` with byte zero-extended matches: XOR only hits AL.
        i = i.wrapping_add(1);
    }
    eax
}

/// Pointer-form wrapper for the freestanding FFI boundary.
///
/// # Safety
/// Same as [`xfs_hashname`].
#[inline(always)]
pub unsafe fn xfs_hashname_ptr(name: *const u8, len: u32) -> u32 {
    unsafe { xfs_hashname(name, len) }
}

/// Host-friendly slice helper.
#[cfg(test)]
pub fn xfs_hashname_slice(name: &[u8]) -> u32 {
    unsafe { xfs_hashname(name.as_ptr(), name.len() as u32) }
}

/// Independent FASM-flow oracle (do-while control flow; not a call to Rust).
///
/// Mirrors `proc xfs_hashname` register steps. `len == 0` returns 0 (FASM hangs).
#[cfg(test)]
pub fn fasm_oracle_xfs_hashname(name: &[u8]) -> u32 {
    let mut eax: u32 = 0;
    let mut ecx = name.len() as u32;
    let mut esi: usize = 0;
    if ecx == 0 {
        return 0;
    }
    loop {
        // rol eax, 7
        eax = eax.rotate_left(7);
        // xor al, [esi]
        let byte = u32::from(name[esi]);
        eax = (eax & 0xFFFF_FF00) | ((eax ^ byte) & 0xFF);
        esi = esi.wrapping_add(1);
        ecx = ecx.wrapping_sub(1);
        if ecx == 0 {
            break;
        }
    }
    eax
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn empty_returns_zero() {
        assert_eq!(xfs_hashname_slice(&[]), 0);
        assert_eq!(fasm_oracle_xfs_hashname(&[]), 0);
    }

    #[test]
    fn named_vectors() {
        // Single ASCII byte
        assert_eq!(xfs_hashname_slice(b"a"), fasm_oracle_xfs_hashname(b"a"));
        assert_eq!(xfs_hashname_slice(b"a"), u32::from(b'a'));

        // "README.TXT" — typical XFS shortform / leaf lookup name
        let readme = b"README.TXT";
        assert_eq!(
            xfs_hashname_slice(readme),
            fasm_oracle_xfs_hashname(readme)
        );

        // Two bytes: rol7 of first, xor second
        let ab = b"ab";
        let expect = {
            let mut eax = 0u32;
            eax = eax.rotate_left(7);
            eax = (eax & 0xFFFF_FF00) | ((eax ^ u32::from(b'a')) & 0xFF);
            eax = eax.rotate_left(7);
            eax = (eax & 0xFFFF_FF00) | ((eax ^ u32::from(b'b')) & 0xFF);
            eax
        };
        assert_eq!(xfs_hashname_slice(ab), expect);
        assert_eq!(fasm_oracle_xfs_hashname(ab), expect);

        // All-zero bytes still rotate
        let z3 = [0u8, 0, 0];
        assert_eq!(xfs_hashname_slice(&z3), fasm_oracle_xfs_hashname(&z3));

        // High bytes (non-ASCII / UTF-8 fragments)
        let hi = [0x00u8, 0x7F, 0x80, 0xFF];
        assert_eq!(xfs_hashname_slice(&hi), fasm_oracle_xfs_hashname(&hi));
    }

    #[test]
    fn single_byte_exhaustive() {
        for b in 0u16..=0xFF {
            let buf = [b as u8];
            assert_eq!(
                xfs_hashname_slice(&buf),
                fasm_oracle_xfs_hashname(&buf),
                "byte={b:#x}"
            );
        }
    }

    #[test]
    fn two_byte_exhaustive_sample() {
        // Full 64k is fine and cheap.
        for a in 0u16..=0xFF {
            for b in 0u16..=0xFF {
                let buf = [a as u8, b as u8];
                assert_eq!(
                    xfs_hashname_slice(&buf),
                    fasm_oracle_xfs_hashname(&buf),
                    "a={a:#x} b={b:#x}"
                );
            }
        }
    }

    #[test]
    fn prng_oracle_50k() {
        let mut state = XFS_HASHNAME_PRNG_SEED;
        for i in 0..50_000 {
            let len = (xorshift32(&mut state) % 256) as usize;
            let mut buf = vec![0u8; len];
            for b in &mut buf {
                *b = (xorshift32(&mut state) & 0xFF) as u8;
            }
            let rust = xfs_hashname_slice(&buf);
            let ora = fasm_oracle_xfs_hashname(&buf);
            assert_eq!(rust, ora, "prng#{i} len={len}");
            let via_ptr = unsafe { xfs_hashname_ptr(buf.as_ptr(), len as u32) };
            assert_eq!(via_ptr, rust);
        }
    }

    #[test]
    fn rust_matches_al_only_xor_semantics() {
        // Guard against using full-width XOR of zero-extended byte into EAX
        // incorrectly if rotate left produced non-zero high bytes that a
        // naive `eax ^= byte` would clear in bits 8–31 (it wouldn't — XOR
        // with zero-extended byte only flips AL). Both forms must agree.
        let mut buf = [0u8; 16];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (0x5A ^ (i as u8)).wrapping_mul(3);
        }
        assert_eq!(xfs_hashname_slice(&buf), fasm_oracle_xfs_hashname(&buf));
    }
}
