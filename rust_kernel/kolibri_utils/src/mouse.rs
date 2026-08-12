//! Cut L: `mouse_acceleration` — HID mouse delta acceleration curve.
//! Cut CF: `set_mouse_data` — HID mouse aggregator (compose Cut L).
//!
//! Matches `kernel/hid/mousedrv.inc` FASM leaf semantics, including the
//! AX-only absolute-value loop and the EAX high-word sign-restore quirk
//! (`test eax,eax` / `neg ax` after `mul al` leaves bits 16..31 intact).

/// PRNG seed for Cut CF host differential corpus (`'SMDT'`).
pub const SET_MOUSE_DATA_PRNG_SEED: u32 = 0x534D_4454;

/// Absolute-X flag in `BtnState` (`mousedrv.inc`).
pub const MOUSE_BTN_ABS_X: u32 = 0x8000_0000;
/// Absolute-Y flag in `BtnState`.
pub const MOUSE_BTN_ABS_Y: u32 = 0x4000_0000;
/// Button bits retained in `BTN_DOWN` (top 2 abs flags cleared).
pub const MOUSE_BTN_DOWN_MASK: u32 = 0x3FFF_FFFF;

/// Observable HID state mutated by `set_mouse_data`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseDataState {
    pub mouse_x: u16,
    pub mouse_y: u16,
    pub scroll_h: u16,
    pub scroll_v: u16,
    pub btn_down: u32,
    pub mouse_active: u32,
    pub osloop_nonperiodic_work: u32,
}

/// Display + accel tunables injected by the FASM trampoline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseDataParams {
    pub display_width: u32,
    pub display_height: u32,
    pub mouse_delay: u8,
    pub mouse_speed_factor: u16,
}

/// Apply KolibriOS `set_mouse_data` to `state` (in-place).
///
/// Mirrors `kernel/hid/mousedrv.inc` exactly, including 16-bit position
/// arithmetic, `(moving * dim) >> 15` absolute scaling (low 32 bits of mul),
/// and scroll `bts` bits 15 / 23 on `BTN_DOWN`.
#[inline(always)]
pub fn set_mouse_data(
    state: &mut MouseDataState,
    params: MouseDataParams,
    btn_state: u32,
    x_moving: u32,
    y_moving: u32,
    v_scroll: u32,
    h_scroll: u32,
) {
    // BTN_DOWN = BtnState & 0x3FFFFFFF
    let mut btn_down = btn_state & MOUSE_BTN_DOWN_MASK;

    // ---- X ----
    let abs_x = (btn_state & MOUSE_BTN_ABS_X) != 0;
    if abs_x || x_moving != 0 {
        let mut ax = if abs_x {
            // mul width; shr eax, 15 — only low 32 bits of product matter
            x_moving.wrapping_mul(params.display_width) >> 15
        } else {
            let accel = mouse_acceleration(
                x_moving,
                params.mouse_delay,
                params.mouse_speed_factor,
            );
            // add ax, [MOUSE_X] — 16-bit; jns → else zero
            let sum = (accel as u16).wrapping_add(state.mouse_x);
            if (sum as i16) < 0 {
                0u32
            } else {
                u32::from(sum)
            }
        };
        // cmp ax, width / jl .set_x — signed
        let width = params.display_width as u16;
        if (ax as i16) >= (width as i16) {
            ax = u32::from(width.wrapping_sub(1));
        }
        state.mouse_x = ax as u16;
    }

    // ---- Y ----
    let abs_y = (btn_state & MOUSE_BTN_ABS_Y) != 0;
    if abs_y || y_moving != 0 {
        let mut ax = if abs_y {
            y_moving.wrapping_mul(params.display_height) >> 15
        } else {
            // neg eax then mouse_acceleration
            let negated = (y_moving as i32).wrapping_neg() as u32;
            let accel = mouse_acceleration(
                negated,
                params.mouse_delay,
                params.mouse_speed_factor,
            );
            let sum = (accel as u16).wrapping_add(state.mouse_y);
            if (sum as i16) < 0 {
                0u32
            } else {
                u32::from(sum)
            }
        };
        // cmp ax, height / jl .set_y — signed
        let height = params.display_height as u16;
        if (ax as i16) >= (height as i16) {
            ax = u32::from(height.wrapping_sub(1));
        }
        state.mouse_y = ax as u16;
    }

    // ---- scrolls ----
    if v_scroll != 0 {
        state.scroll_v = state.scroll_v.wrapping_add(v_scroll as u16);
        btn_down |= 1u32 << 15;
    }
    if h_scroll != 0 {
        state.scroll_h = state.scroll_h.wrapping_add(h_scroll as u16);
        btn_down |= 1u32 << 23;
    }

    state.btn_down = btn_down;
    state.mouse_active = 1;
    state.osloop_nonperiodic_work = 1;
}

/// Context layout for the freestanding stdcall blob (trampoline-filled).
#[repr(C)]
pub struct SetMouseDataCtx {
    pub mouse_x: *mut u16,
    pub mouse_y: *mut u16,
    pub scroll_h: *mut u16,
    pub scroll_v: *mut u16,
    pub btn_down: *mut u32,
    pub mouse_active: *mut u32,
    pub display_width: u32,
    pub display_height: u32,
    pub mouse_delay: u32,
    pub mouse_speed_factor: u32,
    pub osloop_nonperiodic_work: *mut u32,
}

/// Pointer form used by `rust_set_mouse_data`.
///
/// # Safety
/// All pointers in `ctx` must be valid and writable for the duration of the call.
#[inline(always)]
pub unsafe fn set_mouse_data_ptr(
    btn_state: u32,
    x_moving: u32,
    y_moving: u32,
    v_scroll: u32,
    h_scroll: u32,
    ctx: *mut SetMouseDataCtx,
) {
    // SAFETY: trampoline passes a live stack/global ctx with valid pointers.
    let c = unsafe { &mut *ctx };
    let mut state = MouseDataState {
        mouse_x: unsafe { *c.mouse_x },
        mouse_y: unsafe { *c.mouse_y },
        scroll_h: unsafe { *c.scroll_h },
        scroll_v: unsafe { *c.scroll_v },
        btn_down: unsafe { *c.btn_down },
        mouse_active: unsafe { *c.mouse_active },
        osloop_nonperiodic_work: unsafe { *c.osloop_nonperiodic_work },
    };
    let params = MouseDataParams {
        display_width: c.display_width,
        display_height: c.display_height,
        mouse_delay: c.mouse_delay as u8,
        mouse_speed_factor: c.mouse_speed_factor as u16,
    };
    set_mouse_data(
        &mut state,
        params,
        btn_state,
        x_moving,
        y_moving,
        v_scroll,
        h_scroll,
    );
    unsafe {
        *c.mouse_x = state.mouse_x;
        *c.mouse_y = state.mouse_y;
        *c.scroll_h = state.scroll_h;
        *c.scroll_v = state.scroll_v;
        *c.btn_down = state.btn_down;
        *c.mouse_active = state.mouse_active;
        *c.osloop_nonperiodic_work = state.osloop_nonperiodic_work;
    }
}

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

#[cfg(test)]
mod set_mouse_data_tests {
    use super::{
        mouse_acceleration, set_mouse_data, MouseDataParams, MouseDataState,
        SET_MOUSE_DATA_PRNG_SEED, MOUSE_BTN_ABS_X, MOUSE_BTN_ABS_Y, MOUSE_BTN_DOWN_MASK,
    };

    /// Independent AX-only accel FASM flow (does not call production helper).
    fn fasm_oracle_accel(mut eax: u32, delay: u8, speed_factor: u16) -> u32 {
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

    /// Independent step-by-step FASM oracle (`mousedrv.inc` set_mouse_data).
    fn fasm_oracle_set_mouse_data(
        mut st: MouseDataState,
        params: MouseDataParams,
        btn_state: u32,
        x_moving: u32,
        y_moving: u32,
        v_scroll: u32,
        h_scroll: u32,
    ) -> MouseDataState {
        let mut btn_down = btn_state & MOUSE_BTN_DOWN_MASK;

        // X
        if (btn_state & MOUSE_BTN_ABS_X) != 0 {
            let mut ax = x_moving.wrapping_mul(params.display_width) >> 15;
            let width = params.display_width as u16;
            if (ax as i16) >= (width as i16) {
                ax = u32::from(width.wrapping_sub(1));
            }
            st.mouse_x = ax as u16;
        } else if x_moving != 0 {
            let accel =
                fasm_oracle_accel(x_moving, params.mouse_delay, params.mouse_speed_factor);
            let sum = (accel as u16).wrapping_add(st.mouse_x);
            let mut ax = if (sum as i16) < 0 {
                0u32
            } else {
                u32::from(sum)
            };
            let width = params.display_width as u16;
            if (ax as i16) >= (width as i16) {
                ax = u32::from(width.wrapping_sub(1));
            }
            st.mouse_x = ax as u16;
        }

        // Y
        if (btn_state & MOUSE_BTN_ABS_Y) != 0 {
            let mut ax = y_moving.wrapping_mul(params.display_height) >> 15;
            let height = params.display_height as u16;
            if (ax as i16) >= (height as i16) {
                ax = u32::from(height.wrapping_sub(1));
            }
            st.mouse_y = ax as u16;
        } else if y_moving != 0 {
            let negated = (y_moving as i32).wrapping_neg() as u32;
            let accel =
                fasm_oracle_accel(negated, params.mouse_delay, params.mouse_speed_factor);
            let sum = (accel as u16).wrapping_add(st.mouse_y);
            let mut ax = if (sum as i16) < 0 {
                0u32
            } else {
                u32::from(sum)
            };
            let height = params.display_height as u16;
            if (ax as i16) >= (height as i16) {
                ax = u32::from(height.wrapping_sub(1));
            }
            st.mouse_y = ax as u16;
        }

        if v_scroll != 0 {
            st.scroll_v = st.scroll_v.wrapping_add(v_scroll as u16);
            btn_down |= 1 << 15;
        }
        if h_scroll != 0 {
            st.scroll_h = st.scroll_h.wrapping_add(h_scroll as u16);
            btn_down |= 1 << 23;
        }
        st.btn_down = btn_down;
        st.mouse_active = 1;
        st.osloop_nonperiodic_work = 1;
        st
    }

    fn check(
        st0: MouseDataState,
        params: MouseDataParams,
        btn: u32,
        x: u32,
        y: u32,
        vs: u32,
        hs: u32,
    ) {
        let mut got = st0;
        set_mouse_data(&mut got, params, btn, x, y, vs, hs);
        let exp = fasm_oracle_set_mouse_data(st0, params, btn, x, y, vs, hs);
        assert_eq!(got, exp, "btn={btn:#x} x={x:#x} y={y:#x} vs={vs:#x} hs={hs:#x}");
    }

    fn default_params() -> MouseDataParams {
        MouseDataParams {
            display_width: 800,
            display_height: 600,
            mouse_delay: 3,
            mouse_speed_factor: 4,
        }
    }

    fn blank_state() -> MouseDataState {
        MouseDataState {
            mouse_x: 400,
            mouse_y: 300,
            scroll_h: 0,
            scroll_v: 0,
            btn_down: 0,
            mouse_active: 0,
            osloop_nonperiodic_work: 0,
        }
    }

    #[test]
    fn smdt_relative_basic() {
        check(blank_state(), default_params(), 0, 5, 0, 0, 0);
        check(blank_state(), default_params(), 0, 0, 5, 0, 0);
        check(blank_state(), default_params(), 1, 5, 5, 0, 0);
    }

    #[test]
    fn smdt_relative_zero_skips_axis() {
        let st0 = blank_state();
        let mut st = st0;
        set_mouse_data(&mut st, default_params(), 0, 0, 0, 0, 0);
        assert_eq!(st.mouse_x, st0.mouse_x);
        assert_eq!(st.mouse_y, st0.mouse_y);
        assert_eq!(st.mouse_active, 1);
        assert_eq!(st.osloop_nonperiodic_work, 1);
        assert_eq!(st.btn_down, 0);
    }

    #[test]
    fn smdt_absolute_scaling() {
        // mid-range absolute: moving=0x4000, width=800 → (0x4000*800)>>15 = 400
        check(
            blank_state(),
            default_params(),
            MOUSE_BTN_ABS_X | MOUSE_BTN_ABS_Y,
            0x4000,
            0x4000,
            0,
            0,
        );
    }

    #[test]
    fn smdt_clamp_and_negative_floor() {
        let mut st = blank_state();
        st.mouse_x = 1;
        st.mouse_y = 1;
        // large negative relative should floor to 0
        check(st, default_params(), 0, (-200i32) as u32, (-200i32) as u32, 0, 0);
        // large positive relative clamps to width-1 / height-1
        check(st, default_params(), 0, 5000, 5000, 0, 0);
    }

    #[test]
    fn smdt_scroll_bits() {
        let mut st = blank_state();
        set_mouse_data(&mut st, default_params(), 0x7, 0, 0, 3, 5);
        assert_eq!(st.scroll_v, 3);
        assert_eq!(st.scroll_h, 5);
        assert_eq!(st.btn_down & (1 << 15), 1 << 15);
        assert_eq!(st.btn_down & (1 << 23), 1 << 23);
        assert_eq!(st.btn_down & MOUSE_BTN_DOWN_MASK & !((1 << 15) | (1 << 23)), 0x7);
    }

    #[test]
    fn smdt_btn_mask_clears_abs_flags() {
        let mut st = blank_state();
        set_mouse_data(
            &mut st,
            default_params(),
            MOUSE_BTN_ABS_X | MOUSE_BTN_ABS_Y | 0x5,
            0x1000,
            0x1000,
            0,
            0,
        );
        assert_eq!(st.btn_down & !((1 << 15) | (1 << 23)), 0x5);
    }

    #[test]
    fn smdt_y_negation_before_accel() {
        // Y relative: accel(-delta); known hand value with defaults
        let st0 = blank_state();
        let mut st = st0;
        set_mouse_data(&mut st, default_params(), 0, 0, 5, 0, 0);
        let accel = mouse_acceleration((-5i32) as u32, 3, 4) as u16;
        let expect_y = accel.wrapping_add(st0.mouse_y);
        assert_eq!(st.mouse_y, expect_y);
    }

    #[test]
    fn smdt_prng_50k() {
        let mut state = SET_MOUSE_DATA_PRNG_SEED;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..50_000u32 {
            let st0 = MouseDataState {
                mouse_x: next() as u16,
                mouse_y: next() as u16,
                scroll_h: next() as u16,
                scroll_v: next() as u16,
                btn_down: next(),
                mouse_active: next() & 1,
                osloop_nonperiodic_work: next() & 1,
            };
            let params = MouseDataParams {
                display_width: (next() % 1920) + 1,
                display_height: (next() % 1200) + 1,
                mouse_delay: next() as u8,
                mouse_speed_factor: (next() & 0x1F) as u16,
            };
            let btn = next();
            let x = next() as i16 as u32; // bias toward small deltas
            let y = next() as i16 as u32;
            let vs = next() & 0xFF;
            let hs = next() & 0xFF;
            check(st0, params, btn, x, y, vs, hs);
        }
    }

    #[test]
    fn smdt_width_zero_edge() {
        let params = MouseDataParams {
            display_width: 0,
            display_height: 0,
            mouse_delay: 3,
            mouse_speed_factor: 4,
        };
        check(blank_state(), params, MOUSE_BTN_ABS_X | MOUSE_BTN_ABS_Y, 100, 100, 0, 0);
    }
}
