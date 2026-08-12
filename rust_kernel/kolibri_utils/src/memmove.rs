//! Cut CJ: `memmove` — forward memory move (Kolibri semantics, not C `memmove`).
//!
//! Matches `kernel/kernel.asm` FASM leaf:
//! ```text
//!   test ecx, ecx / jle .ret          ; signed ≤0 → no-op
//!   push esi edi ecx
//!   edi=ebx; esi=eax
//!   if (ecx & ~3) != 0:
//!     push ecx; shr ecx,2; rep movsd; pop ecx; and ecx,3; jz .finish
//!   rep movsb
//!   pop ecx edi esi
//! ```
//!
//! Always copies **forward** (dword then tail bytes). Overlapping
//! `dest > src` is historically unsafe and differs from C `memmove`.
//! Callers that overlap use left-shifts (`dest < src`).
//! DF is assumed clear on entry (leaf does not `cld`/`std`).

/// Cut CJ differential PRNG seed (`'MMOV'`).
pub const MEMMOVE_PRNG_SEED: u32 = 0x4D4D_4F56;

/// FASM-faithful forward memory move.
///
/// `nbytes` is interpreted as a signed count: if `(nbytes as i32) <= 0`,
/// this is a no-op (matches `test ecx,ecx` / `jle`).
///
/// # Safety
/// When `nbytes as i32 > 0`, `from` must be readable for `nbytes` bytes and
/// `to` must be writable for `nbytes` bytes under forward-copy aliasing
/// (same rules as the legacy `rep movs*` path).
#[inline(always)]
pub unsafe fn memmove(from: *const u8, to: *mut u8, nbytes: u32) {
    if (nbytes as i32) <= 0 {
        return;
    }

    let mut ecx = nbytes;
    let mut esi = from;
    let mut edi = to;

    // FASM: test ecx, not 11b / jz @f (byte-only path)
    if (ecx & !3) != 0 {
        let mut n_dwords = ecx >> 2;
        while n_dwords != 0 {
            // Match `movsd`: snapshot 4 bytes, then write 4 bytes.
            // Use unaligned u32 load/store — i686 allows it; keeps blob small.
            let v = core::ptr::read_unaligned(esi as *const u32);
            core::ptr::write_unaligned(edi as *mut u32, v);
            esi = esi.add(4);
            edi = edi.add(4);
            n_dwords = n_dwords.wrapping_sub(1);
        }
        ecx &= 3;
        if ecx == 0 {
            return;
        }
    }

    while ecx != 0 {
        let b = *esi;
        *edi = b;
        esi = esi.add(1);
        edi = edi.add(1);
        ecx = ecx.wrapping_sub(1);
    }
}

/// Pointer-friendly wrapper used by the stdcall FFI export.
///
/// # Safety
/// Same contract as [`memmove`].
#[inline(always)]
pub unsafe fn memmove_ptr(from: u32, to: u32, nbytes: u32) {
    unsafe { memmove(from as *const u8, to as *mut u8, nbytes) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle on a single working buffer (from/to offsets).
    /// Re-implements the `rep movsd`/`movsb` control flow; does not call Rust `memmove`.
    fn oracle_in_place(buf: &mut [u8], from: usize, to: usize, nbytes: u32) {
        if (nbytes as i32) <= 0 {
            return;
        }
        let mut ecx = nbytes;
        let mut esi = from;
        let mut edi = to;
        if (ecx & !3) != 0 {
            let mut n_dwords = ecx >> 2;
            while n_dwords != 0 {
                let chunk = [
                    buf[esi],
                    buf[esi + 1],
                    buf[esi + 2],
                    buf[esi + 3],
                ];
                buf[edi] = chunk[0];
                buf[edi + 1] = chunk[1];
                buf[edi + 2] = chunk[2];
                buf[edi + 3] = chunk[3];
                esi += 4;
                edi += 4;
                n_dwords -= 1;
            }
            ecx &= 3;
            if ecx == 0 {
                return;
            }
        }
        while ecx != 0 {
            buf[edi] = buf[esi];
            esi += 1;
            edi += 1;
            ecx -= 1;
        }
    }

    fn check_in_place(initial: &[u8], from: usize, to: usize, nbytes: u32) {
        let mut expected = initial.to_vec();
        oracle_in_place(&mut expected, from, to, nbytes);
        let mut got = initial.to_vec();
        unsafe {
            memmove(
                got.as_ptr().add(from),
                got.as_mut_ptr().add(to),
                nbytes,
            );
        }
        assert_eq!(
            got, expected,
            "mismatch from={from} to={to} nbytes={nbytes:#x}"
        );
    }

    #[test]
    fn mmov_zero_and_negative() {
        let initial = b"Hello, world!\0\0\0\0".to_vec();
        check_in_place(&initial, 0, 8, 0);
        check_in_place(&initial, 0, 8, 0xFFFF_FFFF); // -1 as u32
        check_in_place(&initial, 0, 8, 0x8000_0000); // i32::MIN
    }

    #[test]
    fn mmov_non_overlap() {
        let mut initial = vec![0u8; 64];
        for i in 0..32 {
            initial[i] = (0x40 + i) as u8;
        }
        check_in_place(&initial, 0, 32, 16);
        check_in_place(&initial, 0, 32, 1);
        check_in_place(&initial, 0, 32, 3);
        check_in_place(&initial, 0, 32, 7);
        check_in_place(&initial, 1, 33, 15); // unaligned
    }

    #[test]
    fn mmov_identical() {
        let initial: Vec<u8> = (0..32).map(|i| i as u8).collect();
        check_in_place(&initial, 0, 0, 16);
        check_in_place(&initial, 4, 4, 1);
    }

    #[test]
    fn mmov_forward_overlap_left_shift() {
        // KEY_BUFF / msg_board style: from = to+1
        let initial: Vec<u8> = (0..64).map(|i| (i * 3) as u8).collect();
        check_in_place(&initial, 1, 0, 1);
        check_in_place(&initial, 1, 0, 3);
        check_in_place(&initial, 1, 0, 4);
        check_in_place(&initial, 1, 0, 5);
        check_in_place(&initial, 1, 0, 17);
        check_in_place(&initial, 1, 0, 63);
        check_in_place(&initial, 16, 0, 16); // struct compact style (gap)
        check_in_place(&initial, 2, 0, 8);
    }

    #[test]
    fn mmov_backward_overlap_legacy_quirk() {
        // Document dest>src forward-only quirk (not C memmove).
        let initial: Vec<u8> = (0..32).map(|i| (0xA0 + i) as u8).collect();
        check_in_place(&initial, 0, 1, 4);
        check_in_place(&initial, 0, 1, 8);
        check_in_place(&initial, 0, 3, 7);
    }

    #[test]
    fn mmov_one_byte() {
        let initial = b"ABCDEFGH".to_vec();
        check_in_place(&initial, 0, 4, 1);
        check_in_place(&initial, 7, 0, 1);
    }

    #[test]
    fn mmov_large_aligned() {
        let initial: Vec<u8> = (0..512).map(|i| (i & 0xff) as u8).collect();
        check_in_place(&initial, 0, 256, 256);
        check_in_place(&initial, 0, 256, 255);
        check_in_place(&initial, 1, 257, 250);
    }

    #[test]
    fn mmov_boundary_adjacent() {
        let initial: Vec<u8> = (0..64).map(|i| i as u8).collect();
        check_in_place(&initial, 0, 32, 32);
        check_in_place(&initial, 31, 0, 1);
        check_in_place(&initial, 30, 0, 2);
    }

    #[test]
    fn mmov_prng_50k() {
        let mut state = MEMMOVE_PRNG_SEED;
        fn next(state: &mut u32) -> u32 {
            let mut x = *state;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *state = x;
            x
        }

        for case_i in 0..50_000u32 {
            let kind = next(&mut state) % 6;
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                *b = (next(&mut state) & 0xff) as u8;
            }
            let (from, to, nbytes) = match kind {
                0 => {
                    let n = 1 + (next(&mut state) % 100);
                    (0usize, 128usize, n)
                }
                1 => {
                    let n = 1 + (next(&mut state) % 120);
                    (1usize, 0usize, n)
                }
                2 => {
                    let off = (next(&mut state) % 64) as usize;
                    let n = next(&mut state) % 64;
                    (off, off, n)
                }
                3 => {
                    let n = if next(&mut state) & 1 == 0 {
                        0
                    } else {
                        0xFFFF_0000 | (next(&mut state) & 0xFFFF)
                    };
                    (0usize, 64usize, n)
                }
                4 => {
                    let dist = 1 + (next(&mut state) % 4) as usize;
                    let n = 1 + (next(&mut state) % 32);
                    (0usize, dist, n)
                }
                _ => {
                    let from = 1 + (next(&mut state) % 40) as usize;
                    let to = 80 + (next(&mut state) % 40) as usize;
                    let n = 1 + (next(&mut state) % 50);
                    (from, to, n)
                }
            };
            if (nbytes as i32) > 0 {
                if from + (nbytes as usize) > buf.len() || to + (nbytes as usize) > buf.len() {
                    continue;
                }
            }
            let _ = case_i;
            check_in_place(&buf, from, to, nbytes);
        }
    }

    #[test]
    fn mmov_keymap_sized() {
        let initial: Vec<u8> = (0..256).map(|i| (i as u8).wrapping_mul(7)).collect();
        check_in_place(&initial, 0, 128, 128);
    }
}
