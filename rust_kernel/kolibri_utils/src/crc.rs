//! Bit-oriented CRC matching `kernel/crc.inc` (`crc_32`).
//!
//! Partial hash in/out; no final XOR. Callers typically start with `!0` and XOR `!0` at the end.

/// Update a running CRC over `data` using reflected polynomial `poly`.
///
/// Matches FASM `crc_32`: for length 0, returns `partial` unchanged.
///
/// Inlined into the FFI entry so `.text.rust_crc_32` stays self-contained
/// (no cross-section call / relocation) for FASM section embedding.
#[inline(always)]
pub fn crc32_update(mut partial: u32, poly: u32, data: &[u8]) -> u32 {
    for &byte in data {
        let mut b = byte as u32;
        for _ in 0..8 {
            let mix = partial ^ b;
            partial >>= 1;
            if mix & 1 != 0 {
                partial ^= poly;
            }
            b >>= 1;
        }
    }
    partial
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Second transcription of `kernel/crc.inc` for differential comparison.
    fn fasm_oracle_crc32(mut eax: u32, poly: u32, buffer: &[u8]) -> u32 {
        let mut length = buffer.len() as i32;
        let mut esi = 0usize;
        loop {
            length -= 1;
            if length < 0 {
                break;
            }
            let mut ebx = buffer[esi] as u32;
            esi += 1;
            let mut ecx = 8i32;
            while ecx != 0 {
                let edx = eax ^ ebx;
                eax >>= 1;
                if edx & 1 != 0 {
                    eax ^= poly;
                }
                ebx >>= 1;
                ecx -= 1;
            }
        }
        eax
    }

    #[test]
    fn zero_length_preserves_partial() {
        assert_eq!(crc32_update(0xA5A5_A5A5, 0xEDB8_8320, &[]), 0xA5A5_A5A5);
        assert_eq!(fasm_oracle_crc32(0xA5A5_A5A5, 0xEDB8_8320, &[]), 0xA5A5_A5A5);
    }

    #[test]
    fn matches_oracle_gpt_poly() {
        let poly = 0xEDB8_8320u32;
        let samples: &[&[u8]] = &[
            b"",
            b"a",
            b"123456789",
            b"\0\0\0\0",
            &[0xff, 0x00, 0x7f, 0x80],
            &[0u8; 64],
            &[0xffu8; 17],
        ];
        for data in samples {
            for partial in [0u32, 0xffff_ffff, 0x1234_5678, 1] {
                let rust = crc32_update(partial, poly, data);
                let oracle = fasm_oracle_crc32(partial, poly, data);
                assert_eq!(rust, oracle, "partial={partial:#x} data={data:?}");
            }
        }
    }

    #[test]
    fn matches_oracle_ext_poly() {
        let poly = 0x82F6_3B78u32; // EXT_CRC_POLY
        let data = b"KolibriOS-ext-csum-seed";
        let rust = crc32_update(0xffff_ffff, poly, data);
        assert_eq!(rust, fasm_oracle_crc32(0xffff_ffff, poly, data));
    }

    #[test]
    fn ieee_style_finalize_known_vector() {
        // Standard CRC-32/ISO-HDLC of "123456789" is 0xCBF43926.
        let poly = 0xEDB8_8320u32;
        let mid = crc32_update(0xffff_ffff, poly, b"123456789");
        assert_eq!(mid ^ 0xffff_ffff, 0xCBF4_3926);
    }

    #[test]
    fn wrapping_and_chunking() {
        let poly = 0xEDB8_8320u32;
        let data: std::vec::Vec<u8> = (0u8..=255).collect();
        let one = crc32_update(0xffff_ffff, poly, &data);
        let mut acc = 0xffff_ffffu32;
        for chunk in data.chunks(7) {
            acc = crc32_update(acc, poly, chunk);
        }
        assert_eq!(one, acc);
        assert_eq!(one, fasm_oracle_crc32(0xffff_ffff, poly, &data));
    }
}
