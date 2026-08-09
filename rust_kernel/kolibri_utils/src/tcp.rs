//! Cut M: `tcp_xmit_timer` — TCP RFC793-style SRTT/RTTVAR update.
//!
//! Matches `kernel/network/tcp_subr.inc` FASM leaf semantics, including:
//! * gate on `TCP_SOCKET.t_rtt == 0` for the init path
//! * fixed-point shifts (`TCP_RTT_SHIFT=3`, `TCP_RTTVAR_SHIFT=2`)
//! * signed abs via CDQ/XOR/SUB (`i32::MIN` stays `0x8000_0000`)
//! * unsigned `add` then `ja` clamp-to-1 (zero or CF → 1)
//!
//! Field offsets are locked from a FASM struct audit of `socket.inc`.

/// `TCP_SOCKET.t_rtt` offset (bytes).
pub const TCP_OFF_T_RTT: usize = 202;
/// `TCP_SOCKET.t_srtt` offset (bytes).
pub const TCP_OFF_T_SRTT: usize = 210;
/// `TCP_SOCKET.t_rttvar` offset (bytes).
pub const TCP_OFF_T_RTTVAR: usize = 214;

const TCP_RTT_SHIFT: u32 = 3;
const TCP_RTTVAR_SHIFT: u32 = 2;

/// Cut M differential PRNG seed (documented).
pub const TCP_XMIT_TIMER_PRNG_SEED: u32 = 0x7C90_0001;

/// Unaligned dword load — `TCP_SOCKET` fields sit at offset 202+ (SOCKET size 74
/// leaves them 2-byte aligned). Matches x86 permissive `[mem]` loads.
#[inline(always)]
unsafe fn read_u32(base: *const u8, off: usize) -> u32 {
    unsafe { core::ptr::read_unaligned(base.add(off) as *const u32) }
}

#[inline(always)]
unsafe fn write_u32(base: *mut u8, off: usize, val: u32) {
    unsafe { core::ptr::write_unaligned(base.add(off) as *mut u32, val) }
}

/// Unsigned `add` + `ja` clamp used by FASM: if CF or ZF after ADD, store 1.
#[inline(always)]
fn add_ja_clamp(old: u32, delta: u32) -> u32 {
    let (sum, cf) = old.overflowing_add(delta);
    if cf || sum == 0 {
        1
    } else {
        sum
    }
}

/// CDQ / XOR / SUB absolute value on a 32-bit register bit pattern.
#[inline(always)]
fn abs_cdq(eax: u32) -> u32 {
    let edx = if (eax as i32) < 0 {
        0xFFFF_FFFFu32
    } else {
        0
    };
    (eax ^ edx).wrapping_sub(edx)
}

/// Update smoothed RTT estimators on a `TCP_SOCKET` at `socket`.
///
/// Does **not** touch `TCPS_rttupdated` — the FASM trampoline increments it.
///
/// # Safety
/// `socket` must point to a writable `TCP_SOCKET` (at least through `t_rttvar`).
#[inline(always)]
pub unsafe fn tcp_xmit_timer(rtt: u32, socket: *mut u8) {
    let t_rtt = unsafe { read_u32(socket, TCP_OFF_T_RTT) };
    if t_rtt == 0 {
        unsafe {
            write_u32(socket, TCP_OFF_T_SRTT, rtt << TCP_RTT_SHIFT);
            write_u32(
                socket,
                TCP_OFF_T_RTTVAR,
                rtt << (TCP_RTTVAR_SHIFT - 1),
            );
        }
        return;
    }

    let srtt = unsafe { read_u32(socket, TCP_OFF_T_SRTT) };
    let mut eax = rtt;
    eax = eax.wrapping_sub(srtt >> TCP_RTT_SHIFT);
    eax = eax.wrapping_sub(1);
    let new_srtt = add_ja_clamp(srtt, eax);
    unsafe { write_u32(socket, TCP_OFF_T_SRTT, new_srtt) };

    eax = abs_cdq(eax);
    let rttvar = unsafe { read_u32(socket, TCP_OFF_T_RTTVAR) };
    eax = eax.wrapping_sub(rttvar >> TCP_RTTVAR_SHIFT);
    let new_rttvar = add_ja_clamp(rttvar, eax);
    unsafe { write_u32(socket, TCP_OFF_T_RTTVAR, new_rttvar) };
}

/// Pointer-friendly entry used by the stdcall FFI.
///
/// # Safety
/// Same as [`tcp_xmit_timer`].
#[inline(always)]
pub unsafe fn tcp_xmit_timer_ptr(rtt: u32, socket: *mut u8) {
    unsafe { tcp_xmit_timer(rtt, socket) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-faithful oracle (mirrors tcp_subr.inc:515–575 body,
    /// excluding `inc [TCPS_rttupdated]` which the trampoline owns).
    fn fasm_oracle(rtt: u32, t_rtt: u32, mut t_srtt: u32, mut t_rttvar: u32) -> (u32, u32) {
        if t_rtt == 0 {
            return (rtt << 3, rtt << 1);
        }
        let mut eax = rtt;
        eax = eax.wrapping_sub(t_srtt >> 3);
        eax = eax.wrapping_sub(1);
        let (sum, cf) = t_srtt.overflowing_add(eax);
        t_srtt = if cf || sum == 0 { 1 } else { sum };

        let edx = if (eax as i32) < 0 {
            0xFFFF_FFFFu32
        } else {
            0
        };
        eax = (eax ^ edx).wrapping_sub(edx);
        eax = eax.wrapping_sub(t_rttvar >> 2);
        let (sum2, cf2) = t_rttvar.overflowing_add(eax);
        t_rttvar = if cf2 || sum2 == 0 { 1 } else { sum2 };
        (t_srtt, t_rttvar)
    }

    fn run_rust(rtt: u32, t_rtt: u32, t_srtt: u32, t_rttvar: u32) -> (u32, u32) {
        // ≥222 bytes so dword at t_rttmin (218) is in-bounds.
        let mut buf = [0u8; 256];
        unsafe {
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_RTT, t_rtt);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_SRTT, t_srtt);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_RTTVAR, t_rttvar);
            tcp_xmit_timer(rtt, buf.as_mut_ptr());
            (
                read_u32(buf.as_ptr(), TCP_OFF_T_SRTT),
                read_u32(buf.as_ptr(), TCP_OFF_T_RTTVAR),
            )
        }
    }

    fn check(rtt: u32, t_rtt: u32, t_srtt: u32, t_rttvar: u32) {
        let got = run_rust(rtt, t_rtt, t_srtt, t_rttvar);
        let exp = fasm_oracle(rtt, t_rtt, t_srtt, t_rttvar);
        assert_eq!(
            got, exp,
            "mismatch rtt={rtt:#x} t_rtt={t_rtt:#x} srtt={t_srtt:#x} rttvar={t_rttvar:#x}"
        );
        // t_rtt itself must not be mutated; neighbors stay untouched.
        let mut buf = [0u8; 256];
        unsafe {
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_RTT, t_rtt);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_SRTT, t_srtt);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_RTTVAR, t_rttvar);
            tcp_xmit_timer(rtt, buf.as_mut_ptr());
            assert_eq!(read_u32(buf.as_ptr(), TCP_OFF_T_RTT), t_rtt);
            assert_eq!(read_u32(buf.as_ptr(), 198), 0); // t_idle
            assert_eq!(read_u32(buf.as_ptr(), 206), 0); // t_rtseq
            assert_eq!(read_u32(buf.as_ptr(), 218), 0); // t_rttmin
        }
    }

    #[test]
    fn abs_cdq_matches_i32_wrapping_abs() {
        for &v in &[0u32, 1, 2, 0x7FFF_FFFF, 0x8000_0000, 0x8000_0001, 0xFFFF_FFFF] {
            assert_eq!(abs_cdq(v), (v as i32).wrapping_abs() as u32);
        }
    }

    #[test]
    fn init_path_named() {
        check(5, 0, 0, 0);
        check(0, 0, 99, 99);
        check(1, 0, 0, 0);
        check(0x1000, 0, 1, 1);
        check(0xFFFF_FFFF, 0, 0, 0);
    }

    #[test]
    fn update_path_named() {
        check(5, 1, 0, 20);
        check(5, 1, 40, 10);
        check(8, 1, 40, 10);
        check(1, 1, 1, 1);
        check(100, 1, 800, 50);
    }

    #[test]
    fn clamp_to_one_paths() {
        check(0, 1, 1, 1);
        check(0, 1, 8, 4);
        check(0, 1, 0x20, 1);
        check(1, 1, 8, 4);
        check(0, 1, 1, 4);
    }

    #[test]
    fn int_min_abs_path() {
        // rtt - (srtt>>3) - 1 = 0x80000000 with srtt=0, rtt=0x80000001
        check(0x8000_0001, 1, 0, 0);
    }

    #[test]
    fn offsets_locked() {
        assert_eq!(TCP_OFF_T_RTT, 202);
        assert_eq!(TCP_OFF_T_SRTT, 210);
        assert_eq!(TCP_OFF_T_RTTVAR, 214);
    }

    #[test]
    fn grid_small() {
        for t_rtt in [0u32, 1, 2] {
            for rtt in 0u32..64 {
                for srtt in [0u32, 1, 8, 40, 100, 0x1000] {
                    for rttvar in [0u32, 1, 4, 10, 20, 0x100] {
                        check(rtt, t_rtt, srtt, rttvar);
                    }
                }
            }
        }
    }

    #[test]
    fn prng_200k() {
        let mut state = TCP_XMIT_TIMER_PRNG_SEED;
        for _ in 0..200_000 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let rtt = state;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let t_rtt = state & 3;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let t_srtt = state;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let t_rttvar = state;
            check(rtt, t_rtt, t_srtt, t_rttvar);
        }
    }
}
