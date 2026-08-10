//! Cut AH: `calculate_SetChecksum_field` — exFAT directory SetChecksum.
//!
//! Matches `kernel/fs/exfat.inc` FASM leaf semantics:
//! * Rolling 16-bit checksum: `((c & 1) ? 0x8000 : 0) + (c >> 1) + byte`
//! * Skip absolute indices 2 and 3 (the SetChecksum field itself)
//! * Write result to `buf[2..4]` (LE u16); return AX
//!
//! Buffer/length are passed explicitly so the Rust blob stays reloc-free
//! (no `exFAT` EBP layout / GOT). Internal [`exfat_rolling_checksum`] is the
//! shared core for a future NameHash extraction (inlined today in FASM).

/// Cut AH differential PRNG seed (`'CUTH'`).
pub const CALCULATE_SET_CHECKSUM_FIELD_PRNG_SEED: u32 = 0x4355_5448;

/// Minimum writable span so `[buf+2]` store is always in-bounds for the leaf.
pub const SET_CHECKSUM_MIN_STORE: usize = 4;

/// ExFAT File Directory Entry size (type `0x85`).
pub const EXFAT_FILE_DIR_ENTRY_SIZE: usize = 32;

/// Rolling 16-bit exFAT checksum / NameHash core.
///
/// When `skip_indices_2_3` is true, bytes at absolute offsets 2 and 3 are
/// omitted from the sum (SetChecksum field). NameHash uses `false`.
///
/// # Safety
/// `data` must be readable for `len` bytes when `len > 0`.
#[inline(always)]
pub unsafe fn exfat_rolling_checksum(data: *const u8, len: u32, skip_indices_2_3: bool) -> u16 {
    let mut sum: u16 = 0;
    let mut i: u32 = 0;
    while i < len {
        if !(skip_indices_2_3 && (i == 2 || i == 3)) {
            // SAFETY: i < len within caller contract.
            let byte = unsafe { *data.add(i as usize) };
            // mov bx, ax ; and ax,1 ; → 0x8000 or 0 ; shr bx,1 ; add ax,bx ; add ax,byte
            let rotated = if (sum & 1) != 0 { 0x8000u16 } else { 0 };
            let shifted = sum >> 1;
            sum = rotated
                .wrapping_add(shifted)
                .wrapping_add(u16::from(byte));
        }
        i = i.wrapping_add(1);
    }
    sum
}

/// Pure compute matching FASM (no store). Host/diff helper.
#[inline(always)]
pub unsafe fn calculate_set_checksum_field_sum(buf: *const u8, len: u32) -> u16 {
    // SAFETY: caller length/readability contract.
    unsafe { exfat_rolling_checksum(buf, len, true) }
}

/// Full leaf: compute + store AX at `buf+2`, return checksum.
///
/// # Safety
/// `buf` must be readable for `len` bytes and writable for at least
/// [`SET_CHECKSUM_MIN_STORE`] bytes (FASM always stores to `file_dir_entry+2`).
#[inline(always)]
pub unsafe fn calculate_set_checksum_field(buf: *mut u8, len: u32) -> u16 {
    // SAFETY: caller contract.
    let sum = unsafe { calculate_set_checksum_field_sum(buf, len) };
    // SAFETY: store requires ≥4 writable bytes (exFAT file_dir_entry is 32).
    unsafe {
        core::ptr::write_unaligned(buf.add(2) as *mut u16, sum);
    }
    sum
}

/// Pointer-form wrapper for the FFI boundary (same as [`calculate_set_checksum_field`]).
///
/// # Safety
/// Same as [`calculate_set_checksum_field`].
#[inline(always)]
pub unsafe fn calculate_set_checksum_field_ptr(buf: *mut u8, len: u32) -> u16 {
    unsafe { calculate_set_checksum_field(buf, len) }
}

/// Independent FASM-flow oracle (duplicated control flow; not a call to Rust).
///
/// Mirrors `calculate_SetChecksum_field` inner loop + final store semantics
/// for the returned sum (caller may apply store separately).
#[cfg(test)]
pub fn fasm_oracle_set_checksum(buf: &[u8]) -> u16 {
    let len = buf.len() as u32;
    let mut eax: u16 = 0;
    let mut esi: u32 = 0;
    let edx: u32 = 2;
    let edi: u32 = 3;
    let mut ecx = len;

    // FASM is do-while (dec/jnz). Legitimate callers use len ≥ 32; len==0 hangs
    // in FASM — oracle treats len==0 as empty sum (documented quirk).
    if ecx == 0 {
        return 0;
    }

    loop {
        if esi != edx && esi != edi {
            let mut bx = eax;
            let mut ax = eax;
            ax &= 0x1;
            ax = if ax == 0 { 0 } else { 0x8000 };
            bx >>= 1;
            ax = ax.wrapping_add(bx);
            let byte = buf[esi as usize];
            ax = ax.wrapping_add(u16::from(byte));
            eax = ax;
        }
        esi = esi.wrapping_add(1);
        ecx = ecx.wrapping_sub(1);
        if ecx == 0 {
            break;
        }
    }
    eax
}

/// Host-friendly: compute without mutating (oracle compare helper).
#[cfg(test)]
pub fn calculate_set_checksum_field_slice(data: &[u8]) -> u16 {
    unsafe { calculate_set_checksum_field_sum(data.as_ptr(), data.len() as u32) }
}

/// Host-friendly NameHash path (no skip) for future-cluster foreshadow tests.
#[cfg(test)]
pub fn exfat_namehash_slice(data: &[u8]) -> u16 {
    unsafe { exfat_rolling_checksum(data.as_ptr(), data.len() as u32, false) }
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
    fn empty_and_short_skip_field() {
        assert_eq!(calculate_set_checksum_field_slice(&[]), 0);
        assert_eq!(fasm_oracle_set_checksum(&[]), 0);

        // Indices 0,1 only — nothing skipped yet.
        let b = [0x11u8, 0x22];
        assert_eq!(
            calculate_set_checksum_field_slice(&b),
            fasm_oracle_set_checksum(&b)
        );

        // Indices 0..3 — bytes 2 and 3 skipped; sum from 0 and 1 only.
        let b4 = [0x11u8, 0x22, 0xAA, 0xBB];
        let rust = calculate_set_checksum_field_slice(&b4);
        let ora = fasm_oracle_set_checksum(&b4);
        assert_eq!(rust, ora);
        // Changing skipped bytes must not change sum.
        let b4b = [0x11u8, 0x22, 0x00, 0x00];
        assert_eq!(calculate_set_checksum_field_slice(&b4b), rust);
    }

    #[test]
    fn store_side_effect() {
        let mut buf = [0u8; 32];
        buf[0] = 0x85;
        buf[1] = 0x02;
        buf[4] = 0x10;
        let sum = unsafe { calculate_set_checksum_field(buf.as_mut_ptr(), 32) };
        assert_eq!(sum, fasm_oracle_set_checksum(&buf[..]));
        // After store, bytes 2–3 hold the checksum LE — but those indices are
        // skipped on recompute, so recomputing over the mutated buffer yields
        // the same sum.
        let stored = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(stored, sum);
        assert_eq!(
            unsafe { calculate_set_checksum_field_sum(buf.as_ptr(), 32) },
            sum
        );
    }

    #[test]
    fn named_file_plus_stream_entry() {
        // Minimal: 32-byte file entry + 32-byte stream extension.
        let mut buf = [0u8; 64];
        buf[0] = 0x85;
        buf[1] = 1; // SecondaryCount
        // SetChecksum field garbage — must be ignored.
        buf[2] = 0xFF;
        buf[3] = 0xFE;
        buf[32] = 0xC0; // Stream extension
        buf[33] = 0x03;
        for i in 4..32 {
            buf[i] = (i as u8).wrapping_mul(3);
        }
        for i in 34..64 {
            buf[i] = (i as u8).wrapping_mul(5);
        }
        let rust = calculate_set_checksum_field_slice(&buf);
        assert_eq!(rust, fasm_oracle_set_checksum(&buf));
        // NameHash (no skip) differs when field bytes are non-contributing under skip.
        let hash = exfat_namehash_slice(&buf);
        assert_ne!(hash, rust);
    }

    #[test]
    fn wrap_and_odd_bit_path() {
        // Force the (c&1)?0x8000 branch repeatedly.
        let mut buf = [0u8; 16];
        for i in 0..16 {
            buf[i] = 0xFF;
        }
        assert_eq!(
            calculate_set_checksum_field_slice(&buf),
            fasm_oracle_set_checksum(&buf)
        );
        let buf2 = [1u8; 16];
        assert_eq!(
            calculate_set_checksum_field_slice(&buf2),
            fasm_oracle_set_checksum(&buf2)
        );
    }

    #[test]
    fn full_max_dirent_span() {
        // file(32) + stream(32) + 17×name(32) = 608
        let mut buf = vec![0u8; 608];
        buf[0] = 0x85;
        buf[32] = 0xC0;
        for n in 0..17 {
            buf[64 + n * 32] = 0xC1;
            for j in 2..32 {
                buf[64 + n * 32 + j] = ((n * 32 + j) & 0xFF) as u8;
            }
        }
        assert_eq!(
            calculate_set_checksum_field_slice(&buf),
            fasm_oracle_set_checksum(&buf)
        );
    }

    #[test]
    fn differential_prng_50k() {
        let mut state = CALCULATE_SET_CHECKSUM_FIELD_PRNG_SEED;
        for _ in 0..50_000 {
            let len = (xorshift32(&mut state) % 608) as usize;
            // Always allocate ≥4 so store path can be exercised when desired.
            let mut buf = vec![0u8; len.max(4)];
            for b in buf.iter_mut().take(len) {
                *b = (xorshift32(&mut state) & 0xFF) as u8;
            }
            let slice = &buf[..len];
            let rust = calculate_set_checksum_field_slice(slice);
            let ora = fasm_oracle_set_checksum(slice);
            assert_eq!(rust, ora, "len={len}");

            // Store path when buffer is large enough.
            if len >= 4 {
                let mut owned = slice.to_vec();
                let sum = unsafe {
                    calculate_set_checksum_field(owned.as_mut_ptr(), len as u32)
                };
                assert_eq!(sum, rust);
                assert_eq!(u16::from_le_bytes([owned[2], owned[3]]), sum);
            }
        }
    }

    #[test]
    fn namehash_matches_no_skip_oracle() {
        // Foreshadow: NameHash is the same loop without skip.
        let data = b"Hello-exFAT-NameHash\0\0";
        let mut eax: u16 = 0;
        for &byte in data {
            let rotated = if (eax & 1) != 0 { 0x8000u16 } else { 0 };
            eax = rotated
                .wrapping_add(eax >> 1)
                .wrapping_add(u16::from(byte));
        }
        assert_eq!(exfat_namehash_slice(data), eax);
    }
}
