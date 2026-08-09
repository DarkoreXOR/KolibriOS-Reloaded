//! Cut L: `mouse_acceleration` — HID mouse delta acceleration curve.
//!
//! Matches `kernel/hid/mousedrv.inc` FASM leaf semantics, including the
//! AX-only absolute-value loop and the EAX high-word sign-restore quirk
//! (`test eax,eax` / `neg ax` after `mul al` leaves bits 16..31 intact).

/// Apply KolibriOS mouse acceleration to a motion delta.
///
/// # Arguments
/// * `delta` — full `EAX` as presented by callers (`mov eax, [XMoving]` /
///   negated Y path). Only `AX` is accelerated; bits 16..31 participate in
///   the final signedness test.
/// * `delay` — `[mouse_delay]` byte (added to `AL` before square).
/// * `speed_factor` — `[mouse_speed_factor]` word; only the low 8 bits are
///   used as `CL` for `shr ax, cl` (x86 masks the count to 5 bits).
///
/// # Returns
/// Full `EAX` after the FASM sequence (callers typically consume only `AX`).
#[inline(always)]
pub fn mouse_acceleration(delta: u32, delay: u8, speed_factor: u16) -> u32 {
    let mut eax = delta;

    // FASM: `neg ax` / `jl mouse_acceleration` — abs on AX only.
    loop {
        let ax = eax as u16;
        let neg_ax = ax.wrapping_neg();
        eax = (eax & 0xFFFF_0000) | u32::from(neg_ax);
        // After NEG: SF = result MSB; OF = 1 iff operand was 0x8000.
        let sf = (neg_ax as i16) < 0;
        let of = ax == 0x8000;
        if sf != of {
            continue;
        }
        break;
    }

    // `add al, [mouse_delay]` — 8-bit wrap; AH unchanged until mul.
    let al = (eax as u8).wrapping_add(delay);
    eax = (eax & 0xFFFF_FF00) | u32::from(al);

    // `mul al` — AX = AL * AL (unsigned); bits 16..31 of EAX unchanged.
    let product = u16::from(al).wrapping_mul(u16::from(al));
    eax = (eax & 0xFFFF_0000) | u32::from(product);

    // `mov cx, [mouse_speed_factor]` / `dec ax` / `shr ax, cl` / `inc ax`
    // x86 SHR r/m16 masks CL to 5 bits; counts 16..31 zero the destination.
    let cl = (speed_factor as u8) & 31;
    let mut ax = eax as u16;
    ax = ax.wrapping_sub(1);
    ax = if cl >= 16 { 0 } else { ax >> cl };
    ax = ax.wrapping_add(1);
    eax = (eax & 0xFFFF_0000) | u32::from(ax);

    // `test eax, eax` / `jns` / `neg ax` — sign restore via high word.
    if (eax as i32) < 0 {
        let neg_ax = (eax as u16).wrapping_neg();
        eax = (eax & 0xFFFF_0000) | u32::from(neg_ax);
    }
    eax
}

#[cfg(test)]
mod tests {
    use super::mouse_acceleration;

    /// Independent step-by-step FASM oracle (mirrors mousedrv.inc:271–284).
    fn fasm_oracle(mut eax: u32, delay: u8, speed_factor: u16) -> u32 {
        // neg ax / jl loop
        loop {
            let before = eax as u16;
            let after = before.wrapping_neg();
            eax = (eax & !0xFFFFu32) | u32::from(after);
            let sf = (after as i16) < 0;
            let of = before == 0x8000;
            if sf != of {
                continue;
            }
            break;
        }
        let al = (eax as u8).wrapping_add(delay);
        eax = (eax & !0xFFu32) | u32::from(al);
        let prod = u16::from(al).wrapping_mul(u16::from(al));
        eax = (eax & !0xFFFFu32) | u32::from(prod);
        let mut ax = eax as u16;
        ax = ax.wrapping_sub(1);
        let cl = speed_factor as u8 & 31;
        ax = if cl >= 16 { 0 } else { ax >> cl };
        ax = ax.wrapping_add(1);
        eax = (eax & !0xFFFFu32) | u32::from(ax);
        if (eax as i32) < 0 {
            let n = (eax as u16).wrapping_neg();
            eax = (eax & !0xFFFFu32) | u32::from(n);
        }
        eax
    }

    fn check(delta: u32, delay: u8, factor: u16) {
        let got = mouse_acceleration(delta, delay, factor);
        let exp = fasm_oracle(delta, delay, factor);
        assert_eq!(
            got, exp,
            "delta={delta:#x} delay={delay} factor={factor} got={got:#x} exp={exp:#x}"
        );
    }

    #[test]
    fn default_tunables_basic_deltas() {
        // Defaults from mousedrv.inc iglobal: delay=3, factor=4
        for d in [0u32, 1, 2, 3, 4, 5, 8, 10, 16, 32, 64, 100, 127, 128, 255] {
            check(d, 3, 4);
            check((-(d as i32)) as u32, 3, 4);
        }
    }

    #[test]
    fn signed_i32_caller_shape() {
        // set_mouse_data loads full dword into EAX
        for d in -200i32..=200 {
            check(d as u32, 3, 4);
            check(d as u32, 0, 4);
            check(d as u32, 10, 4);
            check(d as u32, 3, 0);
            check(d as u32, 3, 1);
            check(d as u32, 3, 8);
            check(d as u32, 3, 15);
        }
    }

    #[test]
    fn ax_only_high_zero_negatives() {
        // AX negative but EAX high clear (unusual but defined)
        for ax in [0x8000u16, 0x8001, 0xFFFF, 0xFFFE, 0xF000] {
            check(u32::from(ax), 3, 4);
        }
    }

    #[test]
    fn al_wrap_and_large_products() {
        // |AX| large enough that AL + delay wraps
        for delay in [0u8, 1, 3, 10, 0xFF] {
            for factor in [0u16, 1, 4, 8, 16, 31] {
                check(200, delay, factor);
                check((-200i32) as u32, delay, factor);
                check(0x7FFF, delay, factor);
                check(0x8000, delay, factor);
                check(0xFFFF_8000, delay, factor);
            }
        }
    }

    #[test]
    fn zero_and_minmax() {
        check(0, 0, 0);
        check(0, 3, 4);
        check(0x7FFF_FFFF, 3, 4);
        check(0x8000_0000, 3, 4);
        check(0xFFFF_FFFF, 3, 4);
    }

    #[test]
    fn known_hand_values_default() {
        // delay=3, factor=4: |d|=1 → AL=4 → mul=16 → dec=15 → shr4=0 → inc=1
        assert_eq!(mouse_acceleration(1, 3, 4) as u16, 1);
        assert_eq!(mouse_acceleration((-1i32) as u32, 3, 4) as i16, -1);
        // |d|=5 → AL=8 → 64 → 63 → shr4=3 → inc=4
        assert_eq!(mouse_acceleration(5, 3, 4) as u16, 4);
        assert_eq!(mouse_acceleration((-5i32) as u32, 3, 4) as i16, -4);
    }

    #[test]
    fn exhaust_ax_default_tunables() {
        let delay = 3u8;
        let factor = 4u16;
        for ax in 0..=0xFFFFu32 {
            // high-zero form
            check(ax, delay, factor);
            // sign-extended negative form when AX looks signed
            if ax >= 0x8000 {
                check(ax | 0xFFFF_0000, delay, factor);
            }
        }
    }

    /// PRNG seed documented for Cut L differential testing.
    const PRNG_SEED: u32 = 0xA11C_E70Du32;

    #[test]
    fn prng_differential_200k() {
        let mut state = PRNG_SEED;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..200_000u32 {
            let delta = next();
            let delay = (next() & 0xFF) as u8;
            let factor = (next() & 0x1F) as u16; // meaningful SHR range + a bit
            check(delta, delay, factor);
        }
    }

    #[test]
    fn grid_delay_factor_small_deltas() {
        for delay in 0..=20u8 {
            for factor in 0..=16u16 {
                for d in -40i32..=40 {
                    check(d as u32, delay, factor);
                }
            }
        }
    }
}
