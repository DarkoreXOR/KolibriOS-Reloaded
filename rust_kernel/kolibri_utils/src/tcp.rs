//! Cut M: `tcp_xmit_timer` — TCP RFC793-style SRTT/RTTVAR update.
//! Cut V: `tcp_set_persist` — TCP persist-timer arming from SRTT/RTTVAR.
//! Cut BD: `tcp_outflags` — TCP state → header flags table lookup.
//!
//! Matches `kernel/network/tcp_subr.inc` FASM leaf semantics, including:
//! * gate on `TCP_SOCKET.t_rtt == 0` for the init path
//! * fixed-point shifts (`TCP_RTT_SHIFT=3`, `TCP_RTTVAR_SHIFT=2`)
//! * signed abs via CDQ/XOR/SUB (`i32::MIN` stays `0x8000_0000`)
//! * unsigned `add` then `ja` clamp-to-1 (zero or CF → 1)
//! * persist: retransmit mutual exclusion; `(srtt>>2 + rttvar)>>1 << rxtshift`;
//!   unsigned `tcpt_rangeset` clamp to `[8,94]`; OR persist flag; bump `t_rxtshift`
//! * outflags: dword `t_state` indexes an 11-byte TH_* table (no bounds check in
//!   FASM for in-range 0..=10; Rust defines only that domain)
//!
//! Field offsets are locked from a FASM struct audit of `socket.inc`.

/// `TCP_SOCKET.t_state` offset (bytes) — dword before `t_rxtshift`.
pub const TCP_OFF_T_STATE: usize = 114;
/// `TCP_SOCKET.t_rxtshift` offset (bytes) — `db` + 3-byte pad.
pub const TCP_OFF_T_RXTSHIFT: usize = 118;
/// `TCP_SOCKET.t_rtt` offset (bytes).
pub const TCP_OFF_T_RTT: usize = 202;
/// `TCP_SOCKET.t_srtt` offset (bytes).
pub const TCP_OFF_T_SRTT: usize = 210;
/// `TCP_SOCKET.t_rttvar` offset (bytes).
pub const TCP_OFF_T_RTTVAR: usize = 214;
/// `TCP_SOCKET.timer_flags` offset (bytes).
pub const TCP_OFF_TIMER_FLAGS: usize = 254;
/// `TCP_SOCKET.timer_persist` offset (bytes).
pub const TCP_OFF_TIMER_PERSIST: usize = 262;

const TCP_RTT_SHIFT: u32 = 3;
const TCP_RTTVAR_SHIFT: u32 = 2;

/// `TCP_time_pers_min` (`tcp.inc`).
pub const TCP_TIME_PERS_MIN: u32 = 8;
/// `TCP_time_pers_max` (`tcp.inc`).
pub const TCP_TIME_PERS_MAX: u32 = 94;
/// `TCP_max_rxtshift` (`tcp.inc`).
pub const TCP_MAX_RXTSHIFT: u8 = 12;
/// `timer_flag_retransmission` (`tcp_timer.inc`).
pub const TIMER_FLAG_RETRANSMISSION: u32 = 1;
/// `timer_flag_persist` (`tcp_timer.inc`).
pub const TIMER_FLAG_PERSIST: u32 = 8;

/// Cut M differential PRNG seed (documented).
pub const TCP_XMIT_TIMER_PRNG_SEED: u32 = 0x7C90_0001;
/// Cut V differential PRNG seed (documented).
pub const TCP_SET_PERSIST_PRNG_SEED: u32 = 0x7C90_0002;
/// Cut BD differential PRNG seed (`CUBD`).
pub const TCP_OUTFLAGS_PRNG_SEED: u32 = 0x4355_4244;

/// TCP header flag bits (`tcp.inc`).
pub const TH_FIN: u8 = 1 << 0;
pub const TH_SYN: u8 = 1 << 1;
pub const TH_RST: u8 = 1 << 2;
pub const TH_ACK: u8 = 1 << 4;

/// `TCPS_TIME_WAIT` — last defined state index for `.flaglist`.
pub const TCPS_TIME_WAIT: u32 = 10;

/// Unaligned dword load — `TCP_SOCKET` fields sit at offset 202+ (SOCKET size 74
/// leaves them 2-byte aligned). Matches x86 permissive `[mem]` loads.
#[inline(always)]
unsafe fn read_u8(base: *const u8, off: usize) -> u8 {
    unsafe { *base.add(off) }
}

#[inline(always)]
unsafe fn write_u8(base: *mut u8, off: usize, val: u8) {
    unsafe { *base.add(off) = val }
}

#[inline(always)]
unsafe fn read_u32(base: *const u8, off: usize) -> u32 {
    unsafe { core::ptr::read_unaligned(base.add(off) as *const u32) }
}

#[inline(always)]
unsafe fn write_u32(base: *mut u8, off: usize, val: u32) {
    unsafe { core::ptr::write_unaligned(base.add(off) as *mut u32, val) }
}

/// FASM `tcpt_rangeset` — unsigned `jb`/`ja` clamp into `[min, max]`.
#[inline(always)]
fn tcpt_rangeset(value: u32, min: u32, max: u32) -> u32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
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

/// Arm/restart the TCP persist timer on a `TCP_SOCKET` at `socket`.
///
/// Matches `tcp_set_persist` in `tcp_subr.inc`: early exit if retransmission
/// is armed; else compute RTO from SRTT/RTTVAR/`t_rxtshift`, clamp into
/// `[TCP_TIME_PERS_MIN, TCP_TIME_PERS_MAX]`, OR persist flag, and bump
/// `t_rxtshift` while `< TCP_MAX_RXTSHIFT`.
///
/// # Safety
/// `socket` must point to a writable `TCP_SOCKET` through `timer_persist`.
#[inline(always)]
pub unsafe fn tcp_set_persist(socket: *mut u8) {
    let flags = unsafe { read_u32(socket, TCP_OFF_TIMER_FLAGS) };
    if (flags & TIMER_FLAG_RETRANSMISSION) != 0 {
        return;
    }

    let srtt = unsafe { read_u32(socket, TCP_OFF_T_SRTT) };
    let rttvar = unsafe { read_u32(socket, TCP_OFF_T_RTTVAR) };
    let mut ebx = (srtt >> 2).wrapping_add(rttvar) >> 1;
    let shift = unsafe { read_u8(socket, TCP_OFF_T_RXTSHIFT) };
    // x86 `shl ebx, cl` masks CL to 5 bits for 32-bit ops — same as wrapping_shl.
    ebx = ebx.wrapping_shl(u32::from(shift));

    let persist = tcpt_rangeset(ebx, TCP_TIME_PERS_MIN, TCP_TIME_PERS_MAX);
    unsafe {
        write_u32(socket, TCP_OFF_TIMER_PERSIST, persist);
        write_u32(socket, TCP_OFF_TIMER_FLAGS, flags | TIMER_FLAG_PERSIST);
    }

    let rxtshift = unsafe { read_u8(socket, TCP_OFF_T_RXTSHIFT) };
    if rxtshift < TCP_MAX_RXTSHIFT {
        unsafe { write_u8(socket, TCP_OFF_T_RXTSHIFT, rxtshift.wrapping_add(1)) };
    }
}

/// Pointer-friendly entry used by the stdcall FFI.
///
/// # Safety
/// Same as [`tcp_set_persist`].
#[inline(always)]
pub unsafe fn tcp_set_persist_ptr(socket: *mut u8) {
    unsafe { tcp_set_persist(socket) }
}

/// FASM `.flaglist` for `tcp_outflags` — built on the stack so the freestanding
/// blob stays reloc-free (no `.rodata` / GOTOFF). Indices are `TCPS_*` 0..=10.
#[inline(always)]
fn tcp_outflags_table(buf: &mut [u8; 11]) {
    // Matches tcp_subr.inc `.flaglist` byte-for-byte.
    buf[0] = TH_RST | TH_ACK; // TCPS_CLOSED
    buf[1] = 0; // TCPS_LISTEN
    buf[2] = TH_SYN; // TCPS_SYN_SENT
    buf[3] = TH_SYN | TH_ACK; // TCPS_SYN_RECEIVED
    buf[4] = TH_ACK; // TCPS_ESTABLISHED
    buf[5] = TH_ACK; // TCPS_CLOSE_WAIT
    buf[6] = TH_FIN | TH_ACK; // TCPS_FIN_WAIT_1
    buf[7] = TH_FIN | TH_ACK; // TCPS_CLOSING
    buf[8] = TH_FIN | TH_ACK; // TCPS_LAST_ACK
    buf[9] = TH_ACK; // TCPS_FIN_WAIT_2
    buf[10] = TH_ACK; // TCPS_TIME_WAIT
}

/// Look up TCP header flags for `TCP_SOCKET.t_state` at `socket`.
///
/// Matches `tcp_outflags` in `tcp_subr.inc`: load dword state, then
/// `movzx` the byte at `flaglist[state]`. Defined for `state <= 10`;
/// out-of-range returns 0 (FASM would read past the table into following
/// code — not reproducible in a reloc-free blob).
///
/// # Safety
/// `socket` must point to a readable `TCP_SOCKET` through `t_state`.
#[inline(always)]
pub unsafe fn tcp_outflags(socket: *const u8) -> u32 {
    let state = unsafe { read_u32(socket, TCP_OFF_T_STATE) };
    if state > TCPS_TIME_WAIT {
        return 0;
    }
    let mut table = [0u8; 11];
    tcp_outflags_table(&mut table);
    u32::from(table[state as usize])
}

/// Pointer-friendly entry used by the stdcall FFI.
///
/// # Safety
/// Same as [`tcp_outflags`].
#[inline(always)]
pub unsafe fn tcp_outflags_ptr(socket: *const u8) -> u32 {
    unsafe { tcp_outflags(socket) }
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
        assert_eq!(TCP_OFF_T_STATE, 114);
        assert_eq!(TCP_OFF_T_RTT, 202);
        assert_eq!(TCP_OFF_T_SRTT, 210);
        assert_eq!(TCP_OFF_T_RTTVAR, 214);
        assert_eq!(TCP_OFF_T_RXTSHIFT, 118);
        assert_eq!(TCP_OFF_TIMER_FLAGS, 254);
        assert_eq!(TCP_OFF_TIMER_PERSIST, 262);
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

    /// Independent FASM-flow oracle for `tcp_set_persist` (`tcp_subr.inc:469–500`).
    fn persist_oracle(
        flags: u32,
        srtt: u32,
        rttvar: u32,
        mut rxtshift: u8,
        persist_in: u32,
    ) -> (u32, u32, u8) {
        if (flags & TIMER_FLAG_RETRANSMISSION) != 0 {
            return (flags, persist_in, rxtshift);
        }
        let mut ebx = (srtt >> 2).wrapping_add(rttvar) >> 1;
        ebx = ebx.wrapping_shl(u32::from(rxtshift));
        let persist = if ebx < TCP_TIME_PERS_MIN {
            TCP_TIME_PERS_MIN
        } else if ebx > TCP_TIME_PERS_MAX {
            TCP_TIME_PERS_MAX
        } else {
            ebx
        };
        let flags_out = flags | TIMER_FLAG_PERSIST;
        if rxtshift < TCP_MAX_RXTSHIFT {
            rxtshift = rxtshift.wrapping_add(1);
        }
        (flags_out, persist, rxtshift)
    }

    fn persist_run(
        flags: u32,
        srtt: u32,
        rttvar: u32,
        rxtshift: u8,
        persist_in: u32,
    ) -> (u32, u32, u8) {
        // Need through timer_persist @ 262.
        let mut buf = [0u8; 288];
        unsafe {
            write_u8(buf.as_mut_ptr(), TCP_OFF_T_RXTSHIFT, rxtshift);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_SRTT, srtt);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_RTTVAR, rttvar);
            write_u32(buf.as_mut_ptr(), TCP_OFF_TIMER_FLAGS, flags);
            write_u32(buf.as_mut_ptr(), TCP_OFF_TIMER_PERSIST, persist_in);
            tcp_set_persist(buf.as_mut_ptr());
            (
                read_u32(buf.as_ptr(), TCP_OFF_TIMER_FLAGS),
                read_u32(buf.as_ptr(), TCP_OFF_TIMER_PERSIST),
                read_u8(buf.as_ptr(), TCP_OFF_T_RXTSHIFT),
            )
        }
    }

    fn persist_check(
        flags: u32,
        srtt: u32,
        rttvar: u32,
        rxtshift: u8,
        persist_in: u32,
    ) {
        let got = persist_run(flags, srtt, rttvar, rxtshift, persist_in);
        let exp = persist_oracle(flags, srtt, rttvar, rxtshift, persist_in);
        assert_eq!(
            got, exp,
            "persist mismatch flags={flags:#x} srtt={srtt:#x} rttvar={rttvar:#x} \
             rxtshift={rxtshift} persist_in={persist_in:#x}"
        );
        // Retransmit gate must leave neighbors alone when early-exiting.
        if (flags & TIMER_FLAG_RETRANSMISSION) != 0 {
            assert_eq!(got.0, flags);
            assert_eq!(got.1, persist_in);
            assert_eq!(got.2, rxtshift);
        }
    }

    #[test]
    fn persist_retransmit_gate() {
        persist_check(TIMER_FLAG_RETRANSMISSION, 40, 10, 0, 0xDEAD_BEEF);
        persist_check(
            TIMER_FLAG_RETRANSMISSION | TIMER_FLAG_PERSIST,
            100,
            20,
            3,
            55,
        );
        persist_check(0xFFFF_FFFF, 0, 0, 0, 99);
    }

    #[test]
    fn persist_named_clamp() {
        // Zero estimators → raw 0 → clamp to min 8; shift 0 → rxtshift becomes 1.
        persist_check(0, 0, 0, 0, 0);
        // tcp_output zeros rxtshift then calls: typical first arm.
        persist_check(0, 40, 10, 0, 0);
        // ((40>>2)+10)>>1 = 10; <<3 = 80 → in range.
        persist_check(0, 40, 10, 3, 1);
        // Force max clamp.
        persist_check(0, 0xFFFF_FFFF, 0xFFFF_FFFF, 5, 0);
        // Force near-min then shift.
        persist_check(0, 4, 0, 0, 0); // ((1)+0)>>1 = 0 → min 8
        persist_check(0, 32, 0, 0, 0); // (8+0)>>1 = 4 → min 8
        persist_check(0, 64, 0, 0, 0); // (16+0)>>1 = 8 → exact min
        persist_check(0, 64, 0, 4, 0); // 8<<4 = 128 → max 94
    }

    #[test]
    fn persist_rxtshift_saturate() {
        persist_check(0, 40, 10, 11, 0); // 11 → 12
        persist_check(0, 40, 10, 12, 0); // 12 stays 12
        persist_check(0, 40, 10, 255, 0); // 255 stays 255 (unsigned jae)
    }

    #[test]
    fn persist_sticky_flag_and_restart() {
        // Already-persist: still recompute timer and keep flag.
        persist_check(TIMER_FLAG_PERSIST, 40, 10, 1, 50);
        // Other timer bits preserved.
        persist_check(0xF0, 40, 10, 0, 0);
    }

    #[test]
    fn persist_shift_mask_edges() {
        // CL & 31 semantics via wrapping_shl: shift 32 ≡ 0.
        persist_check(0, 64, 0, 32, 0);
        persist_check(0, 64, 0, 31, 0);
        persist_check(0, 1, 0, 16, 0);
    }

    #[test]
    fn persist_grid() {
        for flags in [0u32, TIMER_FLAG_RETRANSMISSION, TIMER_FLAG_PERSIST, 0xF0] {
            for srtt in [0u32, 4, 32, 40, 64, 100, 0x1000, 0xFFFF_FFFF] {
                for rttvar in [0u32, 1, 10, 20, 0x100, 0xFFFF_FFFF] {
                    for rxtshift in [0u8, 1, 3, 7, 11, 12, 13, 31, 32, 255] {
                        persist_check(flags, srtt, rttvar, rxtshift, 0x1234_5678);
                    }
                }
            }
        }
    }

    #[test]
    fn persist_prng_200k() {
        let mut state = TCP_SET_PERSIST_PRNG_SEED;
        for _ in 0..200_000 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let flags = state;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let srtt = state;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let rttvar = state;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let rxtshift = (state & 0xFF) as u8;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let persist_in = state;
            persist_check(flags, srtt, rttvar, rxtshift, persist_in);
        }
    }

    /// Independent FASM-flow oracle for `tcp_outflags` (`tcp_subr.inc` `.flaglist`).
    fn outflags_oracle(state: u32) -> u32 {
        const FLAGLIST: [u8; 11] = [
            TH_RST | TH_ACK,
            0,
            TH_SYN,
            TH_SYN | TH_ACK,
            TH_ACK,
            TH_ACK,
            TH_FIN | TH_ACK,
            TH_FIN | TH_ACK,
            TH_FIN | TH_ACK,
            TH_ACK,
            TH_ACK,
        ];
        if state > TCPS_TIME_WAIT {
            0
        } else {
            u32::from(FLAGLIST[state as usize])
        }
    }

    fn outflags_run(state: u32) -> u32 {
        let mut buf = [0u8; 128];
        unsafe {
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_STATE, state);
            tcp_outflags(buf.as_ptr())
        }
    }

    fn outflags_check(state: u32) {
        let got = outflags_run(state);
        let exp = outflags_oracle(state);
        assert_eq!(got, exp, "outflags mismatch state={state}");
    }

    #[test]
    fn outflags_all_defined_states() {
        for state in 0u32..=TCPS_TIME_WAIT {
            outflags_check(state);
        }
    }

    #[test]
    fn outflags_named() {
        assert_eq!(outflags_run(0), u32::from(TH_RST | TH_ACK)); // CLOSED
        assert_eq!(outflags_run(1), 0); // LISTEN
        assert_eq!(outflags_run(2), u32::from(TH_SYN)); // SYN_SENT
        assert_eq!(outflags_run(3), u32::from(TH_SYN | TH_ACK)); // SYN_RECEIVED
        assert_eq!(outflags_run(4), u32::from(TH_ACK)); // ESTABLISHED
        assert_eq!(outflags_run(6), u32::from(TH_FIN | TH_ACK)); // FIN_WAIT_1
        assert_eq!(outflags_run(10), u32::from(TH_ACK)); // TIME_WAIT
    }

    #[test]
    fn outflags_out_of_range_returns_zero() {
        outflags_check(11);
        outflags_check(0xFFFF_FFFF);
        outflags_check(100);
    }

    #[test]
    fn outflags_does_not_mutate_socket() {
        let mut buf = [0u8; 128];
        unsafe {
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_STATE, 4);
            write_u32(buf.as_mut_ptr(), TCP_OFF_T_RXTSHIFT, 0xA5A5_A5A5);
            let _ = tcp_outflags(buf.as_ptr());
            assert_eq!(read_u32(buf.as_ptr(), TCP_OFF_T_STATE), 4);
            assert_eq!(read_u32(buf.as_ptr(), TCP_OFF_T_RXTSHIFT), 0xA5A5_A5A5);
        }
    }

    #[test]
    fn outflags_prng_50k() {
        let mut state = TCP_OUTFLAGS_PRNG_SEED;
        for _ in 0..50_000 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            // Mix in-range and a few out-of-range patterns.
            let t_state = if (state & 0x1F) == 0x1F {
                state
            } else {
                state % 11
            };
            outflags_check(t_state);
        }
    }
}
