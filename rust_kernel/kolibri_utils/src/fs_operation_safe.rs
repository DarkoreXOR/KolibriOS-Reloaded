//! Cut AZ: `file_system_is_operation_safe` — sysfn70/80 buffer-size → userspace gate.
//!
//! Matches `kernel/fs/fs_lfn.inc` FASM leaf semantics:
//! * Read `subfn = [inf+0]`, `base = [inf+16]`
//! * Compute buffer `len` from subfn (0 / 1 / 2–3 / 5 / 6)
//! * Unknown subfn → **accept** (`ZF=1`) without calling the region check
//!   (`cmp ecx, ecx` with `ecx` still 0)
//! * Else apply Cut P region gate **inlined** (same arithmetic as
//!   `userspace::is_region_userspace`) — no cross-section / cross-module call
//!   so the freestanding blob stays reloc-free.
//!
//! Legacy return channel is **ZF**. Rust returns `1` for FASM `ZF=1`, `0` for
//! FASM `ZF=0`. Trampoline reconstructs ZF via `cmp eax, 1`.

/// Cut AZ differential PRNG seed (`'CUTZ'`).
pub const FILE_SYSTEM_IS_OPERATION_SAFE_PRNG_SEED: u32 = 0x4355_545A;

/// Kolibri `OS_BASE` (must match Cut P / `const.inc`).
const OS_BASE: u32 = 0x8000_0000;
const OS_BASE_M1: u32 = 0x7FFF_FFFF;

/// Offsets into the sysfn70/80 information structure.
pub const OFF_SUBFN: usize = 0;
pub const OFF_ENCODING: usize = 8;
pub const OFF_COUNT_OR_SIZE: usize = 12;
pub const OFF_BUFFER_BASE: usize = 16;

/// Minimum readable size of the information structure for this leaf.
pub const INF_STRUCT_MIN: usize = 20;

/// BDVK block length when encoding ≤ 1 (CP866).
pub const BDVK_CP866: u32 = 304;
/// BDVK block length when encoding > 1 (Unicode).
pub const BDVK_UNICODE: u32 = 560;
/// Result header length added after BDVK×count (subfn 1).
pub const BDVK_HEADER: u32 = 32;
/// Fixed buffer length for subfn 5.
pub const SUBFN5_LEN: u32 = 40;
/// Fixed buffer length for subfn 6.
pub const SUBFN6_LEN: u32 = 32;

/// Compute the target-buffer byte length FASM would place in ECX.
///
/// Returns `(known, len)` where `known == false` means the unknown-subfn
/// `.switch_none` path (`cmp ecx,ecx` accept). Uses an if/else chain matching
/// FASM control flow — **no `match` jump tables** (those emit `.rodata` + GOT
/// and break reloc-free extract).
#[inline(always)]
pub fn fs_op_safe_buffer_len_ex(subfn: u32, encoding: u32, count_or_size: u32) -> (bool, u32) {
    // cmp dword [ebx], 0 / jnz .case1
    if subfn == 0 {
        return (true, count_or_size);
    }
    // cmp dword [ebx], 1 / jnz .case2_3
    if subfn == 1 {
        let bdvk = if encoding <= 1 {
            BDVK_CP866
        } else {
            BDVK_UNICODE
        };
        // imul ecx, [ebx+12] ; add ecx, 32
        return (true, bdvk.wrapping_mul(count_or_size).wrapping_add(BDVK_HEADER));
    }
    // .case2_3: cmp dword [ebx], 3 / ja .case5
    if subfn <= 3 {
        return (true, count_or_size); // subfn 2 or 3
    }
    // .case5
    if subfn == 5 {
        return (true, SUBFN5_LEN);
    }
    // .case6
    if subfn == 6 {
        return (true, SUBFN6_LEN);
    }
    // .switch_none
    (false, 0)
}

/// Convenience: `Some(len)` when the region check runs, `None` for accept-skip.
#[inline(always)]
pub fn fs_op_safe_buffer_len(subfn: u32, encoding: u32, count_or_size: u32) -> Option<u32> {
    let (known, len) = fs_op_safe_buffer_len_ex(subfn, encoding, count_or_size);
    if known {
        Some(len)
    } else {
        None
    }
}

/// Inlined Cut P region check (bit-identical to `userspace::is_region_userspace`).
///
/// Kept local so the AZ link_section never emits a reloc to Cut P's section.
#[inline(always)]
fn region_gate_zf(base: u32, len: u32) -> u32 {
    if base > OS_BASE_M1 {
        return 0;
    }
    let (sum, carry) = base.overflowing_add(len);
    if carry {
        return u32::from(sum == 0);
    }
    if sum > OS_BASE {
        return 0;
    }
    1
}

/// FASM-faithful sysfn70/80 safety gate → legacy ZF sense as `0`/`1`.
///
/// # Safety
/// `inf` must point at a readable information structure of at least
/// [`INF_STRUCT_MIN`] bytes (dwords at +0/+8/+12/+16).
#[inline(always)]
pub unsafe fn file_system_is_operation_safe(inf: *const u8) -> u32 {
    // SAFETY: caller guarantees readable inf struct.
    let subfn = unsafe { read_u32_le(inf.add(OFF_SUBFN)) };
    let encoding = unsafe { read_u32_le(inf.add(OFF_ENCODING)) };
    let count_or_size = unsafe { read_u32_le(inf.add(OFF_COUNT_OR_SIZE)) };
    let base = unsafe { read_u32_le(inf.add(OFF_BUFFER_BASE)) };

    let (known, len) = fs_op_safe_buffer_len_ex(subfn, encoding, count_or_size);
    if !known {
        1 // .switch_none: cmp ecx, ecx → ZF=1
    } else {
        region_gate_zf(base, len)
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`file_system_is_operation_safe`].
#[inline(always)]
pub unsafe fn file_system_is_operation_safe_ptr(inf: *const u8) -> u32 {
    unsafe { file_system_is_operation_safe(inf) }
}

/// Host/test helper: build a minimal 20-byte info struct.
#[inline(always)]
pub fn make_inf(subfn: u32, encoding: u32, count_or_size: u32, base: u32) -> [u8; INF_STRUCT_MIN] {
    let mut buf = [0u8; INF_STRUCT_MIN];
    write_u32_le(&mut buf[OFF_SUBFN..], subfn);
    write_u32_le(&mut buf[OFF_ENCODING..], encoding);
    write_u32_le(&mut buf[OFF_COUNT_OR_SIZE..], count_or_size);
    write_u32_le(&mut buf[OFF_BUFFER_BASE..], base);
    buf
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

#[inline(always)]
fn write_u32_le(dst: &mut [u8], v: u32) {
    let b = v.to_le_bytes();
    dst[0] = b[0];
    dst[1] = b[1];
    dst[2] = b[2];
    dst[3] = b[3];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::userspace::{is_region_userspace, trampoline_zf_from_rust_return};

    /// Independent FASM-flow oracle (does **not** call production helpers for
    /// the switch; region check uses a separate Cut-P-shaped oracle).
    fn fasm_oracle_region(base: u32, len: u32) -> u32 {
        if base > OS_BASE_M1 {
            return 0;
        }
        let (sum, carry) = base.overflowing_add(len);
        if carry {
            return u32::from(sum == 0);
        }
        if sum > OS_BASE {
            return 0;
        }
        1
    }

    fn fasm_oracle_len(subfn: u32, encoding: u32, count_or_size: u32) -> Option<u32> {
        // Mirror FASM control flow with unsigned compares (not Rust match sugar
        // alone — keep case2_3 / case5 / case6 branch shape explicit).
        if subfn == 0 {
            return Some(count_or_size);
        }
        if subfn == 1 {
            let bdvk = if encoding <= 1 { 304u32 } else { 560u32 };
            return Some(bdvk.wrapping_mul(count_or_size).wrapping_add(32));
        }
        // .case2_3: cmp subfn, 3 / ja .case5
        if subfn <= 3 {
            return Some(count_or_size); // subfn 2 or 3
        }
        if subfn == 5 {
            return Some(40);
        }
        if subfn == 6 {
            return Some(32);
        }
        None // .switch_none
    }

    fn fasm_oracle(subfn: u32, encoding: u32, count_or_size: u32, base: u32) -> u32 {
        match fasm_oracle_len(subfn, encoding, count_or_size) {
            None => 1,
            Some(len) => fasm_oracle_region(base, len),
        }
    }

    fn check(subfn: u32, encoding: u32, count_or_size: u32, base: u32, expect: u32) {
        let inf = make_inf(subfn, encoding, count_or_size, base);
        let got = unsafe { file_system_is_operation_safe(inf.as_ptr()) };
        let oracle = fasm_oracle(subfn, encoding, count_or_size, base);
        assert_eq!(
            oracle, expect,
            "oracle mismatch subfn={subfn} enc={encoding} n={count_or_size:#x} base={base:#x}"
        );
        assert_eq!(
            got, expect,
            "impl mismatch subfn={subfn} enc={encoding} n={count_or_size:#x} base={base:#x}"
        );
        assert_eq!(
            fs_op_safe_buffer_len(subfn, encoding, count_or_size),
            fasm_oracle_len(subfn, encoding, count_or_size)
        );
        assert_eq!(trampoline_zf_from_rust_return(got), expect == 1);
    }

    #[test]
    fn subfn0_size_passthrough() {
        check(0, 0, 1, 0, 1);
        check(0, 99, 0, 0, 1);
        check(0, 0, 1, 0x8000_0000, 0);
        check(0, 0, 2, 0x7FFF_FFFF, 0);
        check(0, 0, 1, 0x7FFF_FFFF, 1);
    }

    #[test]
    fn subfn1_cp866_and_unicode() {
        assert_eq!(fs_op_safe_buffer_len(1, 0, 1), Some(336));
        check(1, 0, 1, 0, 1);
        assert_eq!(fs_op_safe_buffer_len(1, 1, 1), Some(336));
        assert_eq!(fs_op_safe_buffer_len(1, 2, 1), Some(592));
        check(1, 2, 1, 0, 1);
        assert_eq!(fs_op_safe_buffer_len(1, 0, 0), Some(32));
        check(1, 0, 0, 0, 1);
        let wrap = 304u32.wrapping_mul(0x0100_0000).wrapping_add(32);
        assert_eq!(fs_op_safe_buffer_len(1, 0, 0x0100_0000), Some(wrap));
    }

    #[test]
    fn subfn2_and_3() {
        check(2, 0, 64, 0x1000, 1);
        check(3, 0, 8, 0x7FFF_FFFC, 0);
        check(3, 0, 4, 0x7FFF_FFFC, 1);
    }

    #[test]
    fn subfn5_and_6_fixed() {
        assert_eq!(fs_op_safe_buffer_len(5, 0, 999), Some(40));
        assert_eq!(fs_op_safe_buffer_len(6, 0, 999), Some(32));
        check(5, 0, 0, 0, 1);
        check(6, 0, 0, 0, 1);
        check(5, 0, 0, 0x8000_0000, 0);
    }

    #[test]
    fn unknown_subfn_accepts_without_region_check() {
        check(4, 0, 0xFFFF_FFFF, 0x8000_0000, 1);
        check(7, 0, 1, 0xFFFF_FFFF, 1);
        check(0xFFFF_FFFF, 0, 1, 0x8000_0000, 1);
    }

    #[test]
    fn overflow_to_zero_quirk_composed() {
        check(0, 0, 0xFFFF_FFFF, 1, 1);
        check(0, 0, 0xC000_0000, 0x4000_0000, 1);
        check(0, 0, 0xFFFF_FFFF, 2, 0);
    }

    #[test]
    fn length_helper_matches_oracle() {
        for subfn in [0u32, 1, 2, 3, 4, 5, 6, 7, 100] {
            for enc in [0u32, 1, 2, 0xFF] {
                for n in [0u32, 1, 2, 16, 0x100, 0xFFFF_FFFF] {
                    assert_eq!(
                        fs_op_safe_buffer_len(subfn, enc, n),
                        fasm_oracle_len(subfn, enc, n),
                        "subfn={subfn} enc={enc} n={n:#x}"
                    );
                }
            }
        }
    }

    #[test]
    fn composed_region_matches_cut_p() {
        for &(subfn, enc, n, base) in &[
            (0u32, 0u32, 16u32, 0u32),
            (1, 0, 2, 0x1000),
            (1, 2, 1, 0x7FFF_0000),
            (5, 0, 0, 0x7FFF_FFD8),
            (6, 0, 0, 0x7FFF_FFE0),
        ] {
            let len = fs_op_safe_buffer_len(subfn, enc, n).unwrap();
            let inf = make_inf(subfn, enc, n, base);
            let got = unsafe { file_system_is_operation_safe(inf.as_ptr()) };
            assert_eq!(got, is_region_userspace(base, len));
            assert_eq!(got, fasm_oracle_region(base, len));
            assert_eq!(got, region_gate_zf(base, len));
        }
    }

    #[test]
    fn prng_corpus_matches_oracle() {
        let mut state = FILE_SYSTEM_IS_OPERATION_SAFE_PRNG_SEED;
        for _ in 0..50_000u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let subfn = state % 16;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let encoding = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let count_or_size = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let base = state;

            let expect = fasm_oracle(subfn, encoding, count_or_size, base);
            let inf = make_inf(subfn, encoding, count_or_size, base);
            let got = unsafe { file_system_is_operation_safe(inf.as_ptr()) };
            assert_eq!(
                got, expect,
                "prng subfn={subfn} enc={encoding:#x} n={count_or_size:#x} base={base:#x}"
            );
        }
    }
}
