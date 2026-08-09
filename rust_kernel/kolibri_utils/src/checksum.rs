//! Partial Internet checksum matching FASM `checksum_1` in `kernel/network/stack.inc`.
//!
//! Accumulates network-order 16-bit words into a 32-bit partial sum with an
//! 8-byte ADC stride and 4/2/1 remnant paths (length bits via SHR/CF).
//!
//! Freestanding FFI uses raw pointer walks (no slice indexing) so the dedicated
//! `.text.rust_checksum_1` section stays free of panic/bounds-check relocations.

/// Update a partial checksum over `length` bytes at `data`, matching FASM `checksum_1`.
///
/// `seed` is the incoming `EDX` value. Returns the outgoing `EDX` sum.
///
/// Inlined into the FFI entry so `.text.rust_checksum_1` stays reloc-free.
///
/// # Safety
/// `data` must be readable for `length` bytes when `length > 0`.
#[inline(always)]
pub unsafe fn checksum_1(mut sum: u32, data: *const u8, length: u32) -> u32 {
    let mut ptr = data;
    let len = length;

    // shr ecx,1 / pushf  → CF = odd trailing byte
    let cf_odd = (len & 1) != 0;
    let words = len >> 1;
    if words == 0 {
        // SAFETY: caller length contract; odd path reads one byte when cf_odd.
        return unsafe { finish_odd(sum, ptr, cf_odd) };
    }

    // shr ecx,1 / pushf  → CF = leftover 2-byte after 4-byte units
    let cf_2 = (words & 1) != 0;
    let dwords = words >> 1;
    if dwords == 0 {
        // SAFETY: length ≥ 2 when cf_2; otherwise no read.
        sum = unsafe { rem_2(sum, &mut ptr, cf_2) };
        return unsafe { finish_odd(sum, ptr, cf_odd) };
    }

    // shr ecx,1 / pushf  → CF = leftover 4-byte after 8-byte units
    let cf_4 = (dwords & 1) != 0;
    let qwords = dwords >> 1;
    if qwords != 0 {
        let mut ecx = qwords;
        let mut cf;
        loop {
            // SAFETY: each iteration consumes 8 bytes within `length`.
            unsafe {
                cf = add_dl(&mut sum, *ptr.add(1), false);
                cf = adc_dh(&mut sum, *ptr, cf);
                cf = add_dl(&mut sum, *ptr.add(3), cf);
                cf = adc_dh(&mut sum, *ptr.add(2), cf);
                cf = add_dl(&mut sum, *ptr.add(5), cf);
                cf = adc_dh(&mut sum, *ptr.add(4), cf);
                cf = add_dl(&mut sum, *ptr.add(7), cf);
                cf = adc_dh(&mut sum, *ptr.add(6), cf);
            }
            cf = adc_edx(&mut sum, cf);
            ptr = unsafe { ptr.add(8) };
            ecx -= 1;
            if ecx == 0 {
                break;
            }
        }
        // Post-loop adc edx, 0
        let _ = adc_edx(&mut sum, cf);
    }

    // SAFETY: remnant reads stay within original length.
    sum = unsafe { rem_4(sum, &mut ptr, cf_4) };
    sum = unsafe { rem_2(sum, &mut ptr, cf_2) };
    unsafe { finish_odd(sum, ptr, cf_odd) }
}

/// Host-friendly wrapper used by unit tests (builds a pointer from a slice).
#[cfg(test)]
pub fn checksum_1_slice(seed: u32, data: &[u8]) -> u32 {
    unsafe { checksum_1(seed, data.as_ptr(), data.len() as u32) }
}

#[inline(always)]
fn add_dl(sum: &mut u32, byte: u8, carry_in: bool) -> bool {
    let t = (*sum & 0xff) as u16 + u16::from(byte) + u16::from(carry_in);
    *sum = (*sum & !0xff) | u32::from(t as u8);
    t > 0xff
}

#[inline(always)]
fn adc_dh(sum: &mut u32, byte: u8, carry_in: bool) -> bool {
    let t = ((*sum >> 8) & 0xff) as u16 + u16::from(byte) + u16::from(carry_in);
    *sum = (*sum & !0xff00) | (u32::from(t as u8) << 8);
    t > 0xff
}

#[inline(always)]
fn adc_edx(sum: &mut u32, carry_in: bool) -> bool {
    if !carry_in {
        return false;
    }
    let (s, overflow) = sum.overflowing_add(1);
    *sum = s;
    overflow
}

#[inline(always)]
unsafe fn rem_4(mut sum: u32, ptr: &mut *const u8, do_it: bool) -> u32 {
    if !do_it {
        return sum;
    }
    unsafe {
        let mut cf = add_dl(&mut sum, *(*ptr).add(1), false);
        cf = adc_dh(&mut sum, **ptr, cf);
        cf = add_dl(&mut sum, *(*ptr).add(3), cf);
        cf = adc_dh(&mut sum, *(*ptr).add(2), cf);
        let _ = adc_edx(&mut sum, cf);
        *ptr = (*ptr).add(4);
    }
    sum
}

#[inline(always)]
unsafe fn rem_2(mut sum: u32, ptr: &mut *const u8, do_it: bool) -> u32 {
    if !do_it {
        return sum;
    }
    unsafe {
        let mut cf = add_dl(&mut sum, *(*ptr).add(1), false);
        cf = adc_dh(&mut sum, **ptr, cf);
        let _ = adc_edx(&mut sum, cf);
        *ptr = (*ptr).add(2);
    }
    sum
}

#[inline(always)]
unsafe fn finish_odd(mut sum: u32, ptr: *const u8, do_it: bool) -> u32 {
    if !do_it {
        return sum;
    }
    // add dh, [esi+0] ; adc edx, 0
    let byte = unsafe { *ptr };
    let t = ((sum >> 8) & 0xff) as u16 + u16::from(byte);
    sum = (sum & !0xff00) | (u32::from(t as u8) << 8);
    let _ = adc_edx(&mut sum, t > 0xff);
    sum
}

/// Host-side FASM-faithful oracle (independent transcription of `stack.inc`).
///
/// Flag stack is at most 3 CF bits (one per `pushf` in the FASM body).
#[cfg(test)]
pub fn checksum_1_fasm_oracle(mut edx: u32, data: &[u8]) -> u32 {
    let mut esi = 0usize;
    let mut ecx = data.len() as u32;
    let mut flag_stack = [false; 3];
    let mut flag_sp = 0usize;

    // First shr / pushf
    flag_stack[flag_sp] = (ecx & 1) != 0;
    flag_sp += 1;
    ecx >>= 1;
    if ecx == 0 {
        return oracle_no_2(edx, data, &mut esi, &flag_stack, &mut flag_sp);
    }

    flag_stack[flag_sp] = (ecx & 1) != 0;
    flag_sp += 1;
    ecx >>= 1;
    if ecx == 0 {
        return oracle_no_4(edx, data, &mut esi, &flag_stack, &mut flag_sp);
    }

    flag_stack[flag_sp] = (ecx & 1) != 0;
    flag_sp += 1;
    ecx >>= 1;
    if ecx == 0 {
        return oracle_no_8(edx, data, &mut esi, &flag_stack, &mut flag_sp);
    }

    // .loop
    let mut cf_pending;
    loop {
        cf_pending = o_add_dl(&mut edx, data[esi + 1], false);
        cf_pending = o_adc_dh(&mut edx, data[esi], cf_pending);
        cf_pending = o_add_dl(&mut edx, data[esi + 3], cf_pending);
        cf_pending = o_adc_dh(&mut edx, data[esi + 2], cf_pending);
        cf_pending = o_add_dl(&mut edx, data[esi + 5], cf_pending);
        cf_pending = o_adc_dh(&mut edx, data[esi + 4], cf_pending);
        cf_pending = o_add_dl(&mut edx, data[esi + 7], cf_pending);
        cf_pending = o_adc_dh(&mut edx, data[esi + 6], cf_pending);
        cf_pending = o_adc_edx(&mut edx, cf_pending);
        esi += 8;
        ecx -= 1;
        if ecx == 0 {
            break;
        }
    }
    let _ = o_adc_edx(&mut edx, cf_pending);
    oracle_no_8(edx, data, &mut esi, &flag_stack, &mut flag_sp)
}

#[cfg(test)]
fn oracle_no_8(
    mut edx: u32,
    data: &[u8],
    esi: &mut usize,
    flags: &[bool; 3],
    flag_sp: &mut usize,
) -> u32 {
    *flag_sp -= 1;
    if flags[*flag_sp] {
        let mut c = o_add_dl(&mut edx, data[*esi + 1], false);
        c = o_adc_dh(&mut edx, data[*esi], c);
        c = o_add_dl(&mut edx, data[*esi + 3], c);
        c = o_adc_dh(&mut edx, data[*esi + 2], c);
        let _ = o_adc_edx(&mut edx, c);
        *esi += 4;
    }
    oracle_no_4(edx, data, esi, flags, flag_sp)
}

#[cfg(test)]
fn oracle_no_4(
    mut edx: u32,
    data: &[u8],
    esi: &mut usize,
    flags: &[bool; 3],
    flag_sp: &mut usize,
) -> u32 {
    *flag_sp -= 1;
    if flags[*flag_sp] {
        let mut c = o_add_dl(&mut edx, data[*esi + 1], false);
        c = o_adc_dh(&mut edx, data[*esi], c);
        let _ = o_adc_edx(&mut edx, c);
        *esi += 2;
    }
    oracle_no_2(edx, data, esi, flags, flag_sp)
}

#[cfg(test)]
fn oracle_no_2(
    mut edx: u32,
    data: &[u8],
    esi: &mut usize,
    flags: &[bool; 3],
    flag_sp: &mut usize,
) -> u32 {
    *flag_sp -= 1;
    if flags[*flag_sp] {
        let t = ((edx >> 8) & 0xff) as u16 + u16::from(data[*esi]);
        edx = (edx & !0xff00) | (u32::from(t as u8) << 8);
        let _ = o_adc_edx(&mut edx, t > 0xff);
    }
    edx
}

#[cfg(test)]
fn o_add_dl(sum: &mut u32, byte: u8, carry_in: bool) -> bool {
    let t = (*sum & 0xff) as u16 + u16::from(byte) + u16::from(carry_in);
    *sum = (*sum & !0xff) | u32::from(t as u8);
    t > 0xff
}

#[cfg(test)]
fn o_adc_dh(sum: &mut u32, byte: u8, carry_in: bool) -> bool {
    let t = ((*sum >> 8) & 0xff) as u16 + u16::from(byte) + u16::from(carry_in);
    *sum = (*sum & !0xff00) | (u32::from(t as u8) << 8);
    t > 0xff
}

#[cfg(test)]
fn o_adc_edx(sum: &mut u32, carry_in: bool) -> bool {
    if !carry_in {
        return false;
    }
    let (s, overflow) = sum.overflowing_add(1);
    *sum = s;
    overflow
}

#[cfg(test)]
mod tests {
    use super::{checksum_1_fasm_oracle, checksum_1_slice};

    #[test]
    fn empty_preserves_seed() {
        assert_eq!(checksum_1_slice(0, &[]), 0);
        assert_eq!(checksum_1_slice(0x1234_5678, &[]), 0x1234_5678);
        assert_eq!(checksum_1_fasm_oracle(0x1234_5678, &[]), 0x1234_5678);
    }

    #[test]
    fn named_vectors_match_oracle() {
        let seeds = [0u32, 1, 0xffff, 0x1234_5678, 0xffff_ffff, 0x00ff_00ff];
        let bufs: &[&[u8]] = &[
            &[],
            &[0x00],
            &[0xff],
            &[0x12, 0x34],
            &[0x12, 0x34, 0x56],
            &[0x12, 0x34, 0x56, 0x78],
            &[0x01, 0x02, 0x03, 0x04, 0x05],
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06],
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            &[0x00; 16],
            &[0xaa; 15],
            &[0x55; 9],
            &[0x08, 0x00, 0x00, 0x00, 0x12, 0x34, 0x00, 0x01],
        ];
        for &seed in &seeds {
            for &buf in bufs {
                let got = checksum_1_slice(seed, buf);
                let expect = checksum_1_fasm_oracle(seed, buf);
                assert_eq!(got, expect, "seed={seed:#x} buf={buf:?}");
            }
        }
    }

    #[test]
    fn exhaustive_lengths_0_to_64() {
        let seeds = [0u32, 0xabcd_ef01, 0xffff_ffff, 0x0001_0000];
        let mut buf = [0u8; 64];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        for seed in seeds {
            for len in 0..=64 {
                let data = &buf[..len];
                assert_eq!(
                    checksum_1_slice(seed, data),
                    checksum_1_fasm_oracle(seed, data),
                    "seed={seed:#x} len={len}"
                );
            }
        }
    }

    #[test]
    fn remnant_class_coverage() {
        let seed = 0x90ab_cdef;
        let pattern: [u8; 24] = [
            0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x01,
        ];
        for len in 0..=24 {
            let data = &pattern[..len];
            assert_eq!(
                checksum_1_slice(seed, data),
                checksum_1_fasm_oracle(seed, data),
                "len={len}"
            );
        }
    }

    #[test]
    fn carry_stress_all_ff() {
        let seed = 0xffff_ffff;
        for len in 0..=32 {
            let data = vec![0xffu8; len];
            assert_eq!(
                checksum_1_slice(seed, &data),
                checksum_1_fasm_oracle(seed, &data),
                "len={len}"
            );
        }
    }

    #[test]
    fn deterministic_prng_corpus_vs_oracle() {
        let mut state = 0xC0FF_EE02u32;
        let mut buf = [0u8; 128];
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let len = (state % 97) as usize;
            for i in 0..len {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                buf[i] = state as u8;
            }
            let seed = state;
            let data = &buf[..len];
            assert_eq!(
                checksum_1_slice(seed, data),
                checksum_1_fasm_oracle(seed, data),
                "seed={seed:#x} len={len}"
            );
        }
    }
}
