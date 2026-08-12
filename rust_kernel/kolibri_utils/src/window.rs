//! Cut S: `window._.check_window_position` — clamp a window `BOX` into the screen.
//! Cut CE: `window._.set_window_clientbox` — derive `WDATA.clientbox` + skin tops.
//!
//! Matches `kernel/gui/window.inc` FASM leaf semantics:
//! * width/height compared **unsigned** (`jae`) against display dims;
//! * left/top checked with **signed** `jl` / wrapping-add + `jge`;
//! * oversize dims become `display - 1`; off-screen edges nudged on-screen.
//! * clientbox: whole-window copy, or style insets from `window_topleft` + Leency +1.
//!
//! No tables / `.rodata` — reloc-free friendly. Display dimensions / skin height /
//! `window_topleft` base are explicit arguments (trampoline injects globals).

/// PRNG seed for host differential corpus (Cut S).
pub const CHECK_WINDOW_POSITION_PRNG_SEED: u32 = 0x4357_5031; // 'CWP1'

/// PRNG seed for host differential corpus (Cut CE).
pub const SET_WINDOW_CLIENTBOX_PRNG_SEED: u32 = 0x5357_4342; // 'SWCB'

/// `WSTYLE_CLIENTRELATIVE` (`const.inc`).
pub const WSTYLE_CLIENTRELATIVE: u8 = 0x20;

/// `WDATA.fl_wstyle` byte offset (`cl_workarea + 3`).
pub const WDATA_FL_WSTYLE_OFF: usize = 19;

/// `WDATA.clientbox` offset.
pub const WDATA_CLIENTBOX_OFF: usize = 32;

/// Default 5-entry `window_topleft` table (left,top pairs) matching `window.inc`.
pub const WINDOW_TOPLEFT_DEFAULT: [i32; 10] = [
    1, 21, // type 0
    0, 0, // type 1
    5, 20, // type 2
    5, 0, // type 3 top set from skinh
    5, 0, // type 4 top set from skinh
];

/// KolibriOS `BOX`: `{left, top, width, height}` as dwords (`const.inc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowBox {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl WindowBox {
    #[inline(always)]
    pub const fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    /// Pack into the 16-byte little-endian memory layout FASM expects.
    #[inline(always)]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&self.left.to_le_bytes());
        b[4..8].copy_from_slice(&self.top.to_le_bytes());
        b[8..12].copy_from_slice(&self.width.to_le_bytes());
        b[12..16].copy_from_slice(&self.height.to_le_bytes());
        b
    }

    /// Parse from a 16-byte `BOX` block.
    #[inline(always)]
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        Self {
            left: i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            top: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            width: i32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            height: i32::from_le_bytes([b[12], b[13], b[14], b[15]]),
        }
    }
}

/// Clamp `box` into a `display_width` × `display_height` screen (FASM leaf).
///
/// Mirrors `window._.check_window_position` control flow exactly, including
/// unsigned size compares and signed position compares after wrapping ADD.
#[inline(always)]
pub fn check_window_position(
    mut box_: WindowBox,
    display_width: i32,
    display_height: i32,
) -> WindowBox {
    let mut left = box_.left;
    let mut top = box_.top;
    let mut width = box_.width;
    let mut height = box_.height;

    // cmp ecx, esi / jae .fix_width_high  (unsigned)
    if (width as u32) >= (display_width as u32) {
        width = display_width.wrapping_sub(1);
        box_.width = width;
    }

    // or eax, eax / jl .fix_left_low
    if left < 0 {
        left = 0;
        box_.left = left;
    } else {
        // add eax, ecx / cmp eax, esi / jge .fix_left_high  (signed)
        let right = left.wrapping_add(width);
        if right >= display_width {
            left = display_width.wrapping_sub(width).wrapping_sub(1);
            box_.left = left;
        }
    }

    // cmp edx, esi / jae .fix_height_high  (unsigned; esi = height)
    if (height as u32) >= (display_height as u32) {
        height = display_height.wrapping_sub(1);
        box_.height = height;
    }

    // or ebx, ebx / jl .fix_top_low
    if top < 0 {
        top = 0;
        box_.top = top;
    } else {
        let bottom = top.wrapping_add(height);
        if bottom >= display_height {
            top = display_height.wrapping_sub(height).wrapping_sub(1);
            box_.top = top;
        }
    }

    box_
}

/// In-place clamp via raw pointer (kernel `EDI` → `WDATA.box` / `BOX`).
///
/// # Safety
/// `box_ptr` must be readable/writable for 16 bytes.
#[inline(always)]
pub unsafe fn check_window_position_ptr(
    box_ptr: *mut u8,
    display_width: i32,
    display_height: i32,
) {
    let mut b = [0u8; 16];
    // SAFETY: caller guarantees readable/writable BOX (16 B).
    unsafe {
        core::ptr::copy_nonoverlapping(box_ptr, b.as_mut_ptr(), 16);
    }
    let out = check_window_position(WindowBox::from_bytes(&b), display_width, display_height);
    let bytes = out.to_bytes();
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), box_ptr, 16);
    }
}

/// Result of `set_window_clientbox`: client box + mutated 5-entry topleft table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetWindowClientboxResult {
    pub clientbox: WindowBox,
    pub topleft: [i32; 10],
}

/// Pure `window._.set_window_clientbox` (FASM leaf).
///
/// Always writes `topleft[3].top` and `topleft[4].top` from `skinh`, then either
/// copies the whole window box or applies style insets with Leency `+1`.
///
/// **Contract:** when `WSTYLE_CLIENTRELATIVE` is set, `(fl_wstyle & 0x0F)` must be
/// in `0..=4` (the live 5-entry `window_topleft` table). Production callers only
/// use window types 0–4; the kernel ptr path indexes memory like FASM without
/// this bound.
#[inline(always)]
pub fn set_window_clientbox(
    box_: WindowBox,
    fl_wstyle: u8,
    skinh: i32,
    mut topleft: [i32; 10],
) -> SetWindowClientboxResult {
    // mov [window_topleft + 8*3 + 4], skinh  /  + 8*4 + 4
    topleft[7] = skinh;
    topleft[9] = skinh;

    if (fl_wstyle & WSTYLE_CLIENTRELATIVE) == 0 {
        return SetWindowClientboxResult {
            clientbox: WindowBox::new(0, 0, box_.width, box_.height),
            topleft,
        };
    }

    let style = (fl_wstyle & 0x0F) as usize;
    let left = topleft[style * 2];
    let top = topleft[style * 2 + 1];

    let width = box_.width.wrapping_sub(left.wrapping_mul(2)).wrapping_add(1);
    let height = box_
        .height
        .wrapping_sub(top)
        .wrapping_sub(left)
        .wrapping_add(1);

    SetWindowClientboxResult {
        clientbox: WindowBox::new(left, top, width, height),
        topleft,
    }
}

/// In-place clientbox update via raw `WDATA*` + `window_topleft*`.
///
/// Matches FASM indexing (`style * 8 + window_topleft`) without a Rust-side table
/// bound so styles 0–4 are bit-identical; styles ≥5 would OOB like legacy FASM.
///
/// # Safety
/// `wdata` must cover offsets through `clientbox` (48 bytes).
/// `window_topleft` must be writable for at least the slots touched (indices
/// 0..(style*2+1) and always 7 and 9). Live callers use styles 0–4.
#[inline(always)]
pub unsafe fn set_window_clientbox_ptr(wdata: *mut u8, skinh: i32, window_topleft: *mut i32) {
    // SAFETY: caller guarantees WDATA + topleft extents for the live style.
    unsafe {
        // Always refresh skin tops for types 3 and 4.
        *window_topleft.add(7) = skinh;
        *window_topleft.add(9) = skinh;

        let mut box_bytes = [0u8; 16];
        core::ptr::copy_nonoverlapping(wdata, box_bytes.as_mut_ptr(), 16);
        let box_ = WindowBox::from_bytes(&box_bytes);
        let fl_wstyle = *wdata.add(WDATA_FL_WSTYLE_OFF);

        let client = if (fl_wstyle & WSTYLE_CLIENTRELATIVE) == 0 {
            WindowBox::new(0, 0, box_.width, box_.height)
        } else {
            let style = (fl_wstyle & 0x0F) as usize;
            let left = *window_topleft.add(style * 2);
            let top = *window_topleft.add(style * 2 + 1);
            let width = box_.width.wrapping_sub(left.wrapping_mul(2)).wrapping_add(1);
            let height = box_
                .height
                .wrapping_sub(top)
                .wrapping_sub(left)
                .wrapping_add(1);
            WindowBox::new(left, top, width, height)
        };

        let cb = client.to_bytes();
        core::ptr::copy_nonoverlapping(cb.as_ptr(), wdata.add(WDATA_CLIENTBOX_OFF), 16);
    }
}

/// Independently coded FASM-faithful host oracle for Cut CE (not the helper).
#[cfg(test)]
pub fn fasm_oracle_set_window_clientbox(
    box_: WindowBox,
    fl_wstyle: u8,
    skinh: i32,
    mut topleft: [i32; 10],
) -> SetWindowClientboxResult {
    // Mirror window.inc register/memory flow.
    topleft[7] = skinh;
    topleft[9] = skinh;

    if (fl_wstyle & WSTYLE_CLIENTRELATIVE) == 0 {
        return SetWindowClientboxResult {
            clientbox: WindowBox::new(0, 0, box_.width, box_.height),
            topleft,
        };
    }

    let mut eax = (fl_wstyle as u32) & 0x0F;
    let left = topleft[(eax as usize) * 2];
    // clientbox.left = left; width = box.width - 2*left + 1
    let mut width_eax = left;
    width_eax = width_eax.wrapping_shl(1);
    width_eax = width_eax.wrapping_neg();
    width_eax = width_eax.wrapping_add(box_.width);
    width_eax = width_eax.wrapping_add(1);

    eax = (fl_wstyle as u32) & 0x0F;
    let pushed_left = topleft[(eax as usize) * 2];
    let top = topleft[(eax as usize) * 2 + 1];
    // height = box.height - top - left + 1
    let mut height_eax = top;
    height_eax = height_eax.wrapping_neg();
    height_eax = height_eax.wrapping_sub(pushed_left);
    height_eax = height_eax.wrapping_add(box_.height);
    height_eax = height_eax.wrapping_add(1);

    SetWindowClientboxResult {
        clientbox: WindowBox::new(left, top, width_eax, height_eax),
        topleft,
    }
}

/// Independently coded FASM-faithful host oracle (not a call through the helper).
///
/// Mirrors `window.inc` register flow with the same branch targets.
#[cfg(test)]
pub fn fasm_oracle_check_window_position(
    mut box_: WindowBox,
    display_width: i32,
    display_height: i32,
) -> WindowBox {
    let mut eax = box_.left;
    let mut ebx = box_.top;
    let mut ecx = box_.width;
    let mut edx = box_.height;

    let mut esi = display_width;
    // cmp ecx, esi / jae
    if (ecx as u32) >= (esi as u32) {
        ecx = esi.wrapping_sub(1);
        box_.width = ecx;
    }
    // check_left
    if eax < 0 {
        eax = 0;
        box_.left = eax;
    } else {
        eax = eax.wrapping_add(ecx);
        if eax >= esi {
            eax = esi.wrapping_sub(ecx).wrapping_sub(1);
            box_.left = eax;
        }
    }
    // check_height
    esi = display_height;
    if (edx as u32) >= (esi as u32) {
        edx = esi.wrapping_sub(1);
        box_.height = edx;
    }
    // check_top
    if ebx < 0 {
        ebx = 0;
        box_.top = ebx;
    } else {
        ebx = ebx.wrapping_add(edx);
        if ebx >= esi {
            ebx = esi.wrapping_sub(edx).wrapping_sub(1);
            box_.top = ebx;
        }
    }
    box_
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(l: i32, t: i32, w: i32, h: i32) -> WindowBox {
        WindowBox::new(l, t, w, h)
    }

    fn assert_match(box_: WindowBox, dw: i32, dh: i32) {
        let rust = check_window_position(box_, dw, dh);
        let oracle = fasm_oracle_check_window_position(box_, dw, dh);
        assert_eq!(rust, oracle, "box={box_:?} display={dw}x{dh}");
    }

    #[test]
    fn fully_inside_unchanged() {
        let box_ = b(10, 20, 100, 50);
        let out = check_window_position(box_, 800, 600);
        assert_eq!(out, box_);
        assert_match(box_, 800, 600);
    }

    #[test]
    fn clamp_oversize_width_and_height() {
        let out = check_window_position(b(0, 0, 900, 700), 800, 600);
        assert_eq!(out, b(0, 0, 799, 599));
        assert_match(b(0, 0, 900, 700), 800, 600);
    }

    #[test]
    fn nudge_left_low() {
        let out = check_window_position(b(-40, 10, 50, 40), 800, 600);
        assert_eq!(out, b(0, 10, 50, 40));
        assert_match(b(-40, 10, 50, 40), 800, 600);
    }

    #[test]
    fn nudge_left_high() {
        // left+width == display → jge fix → left = display - width - 1
        let out = check_window_position(b(750, 10, 50, 40), 800, 600);
        assert_eq!(out, b(749, 10, 50, 40));
        assert_match(b(750, 10, 50, 40), 800, 600);
    }

    #[test]
    fn nudge_top_low_and_high() {
        assert_eq!(
            check_window_position(b(10, -5, 50, 40), 800, 600),
            b(10, 0, 50, 40)
        );
        assert_eq!(
            check_window_position(b(10, 580, 50, 40), 800, 600),
            b(10, 559, 50, 40)
        );
        assert_match(b(10, -5, 50, 40), 800, 600);
        assert_match(b(10, 580, 50, 40), 800, 600);
    }

    #[test]
    fn width_eq_display_is_oversize() {
        // jae: width == display → clamp to display-1
        let out = check_window_position(b(0, 0, 800, 10), 800, 600);
        assert_eq!(out, b(0, 0, 799, 10));
        assert_match(b(0, 0, 800, 10), 800, 600);
    }

    #[test]
    fn touching_right_edge_ok() {
        // left+width == display-1 → no jge
        let box_ = b(749, 0, 50, 10);
        let out = check_window_position(box_, 800, 600);
        assert_eq!(out, box_);
        assert_match(box_, 800, 600);
    }

    #[test]
    fn oversize_then_left_high_uses_new_width() {
        // width clamped first, then left checked with clamped width
        let out = check_window_position(b(100, 0, 900, 10), 800, 600);
        // width→799; left+799=899 >= 800 → left = 800-799-1 = 0
        assert_eq!(out, b(0, 0, 799, 10));
        assert_match(b(100, 0, 900, 10), 800, 600);
    }

    #[test]
    fn display_zero_width_wrap() {
        // width >= 0 (u) always when display=0; width = 0-1 = -1
        let out = check_window_position(b(0, 0, 1, 1), 0, 600);
        assert_eq!(out.width, -1);
        assert_match(b(0, 0, 1, 1), 0, 600);
    }

    #[test]
    fn wrapping_add_before_signed_compare() {
        // left near I32::MAX, width positive → wrapping add
        let box_ = b(0x7fff_fff0, 0, 0x20, 10);
        assert_match(box_, 800, 600);
    }

    #[test]
    fn ptr_roundtrip() {
        let mut bytes = b(-10, 700, 50, 40).to_bytes();
        unsafe {
            check_window_position_ptr(bytes.as_mut_ptr(), 800, 600);
        }
        assert_eq!(WindowBox::from_bytes(&bytes), b(0, 559, 50, 40));
    }

    #[test]
    fn boundary_grid_differential() {
        let displays = [(1i32, 1), (2, 2), (80, 60), (800, 600), (1024, 768)];
        let coords = [
            i32::MIN,
            i32::MIN / 2,
            -1000,
            -1,
            0,
            1,
            2,
            79,
            80,
            81,
            599,
            600,
            601,
            799,
            800,
            801,
            0x7fff_fffe,
            i32::MAX,
        ];
        let sizes = [
            0i32,
            1,
            2,
            50,
            80,
            600,
            800,
            0xffff_ffff_u32 as i32,
            i32::MIN,
            i32::MAX,
        ];
        for &(dw, dh) in &displays {
            for &l in &coords {
                for &t in &coords {
                    for &w in &sizes {
                        for &h in &sizes {
                            assert_match(b(l, t, w, h), dw, dh);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn prng_corpus_200k() {
        // xorshift32
        let mut s = CHECK_WINDOW_POSITION_PRNG_SEED;
        for _ in 0..200_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let left = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let top = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let width = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let height = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let dw = (s % 2048) as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let dh = (s % 2048) as i32;
            assert_match(b(left, top, width, height), dw, dh);
        }
    }

    fn assert_swcb_match(box_: WindowBox, fl_wstyle: u8, skinh: i32, topleft: [i32; 10]) {
        let rust = set_window_clientbox(box_, fl_wstyle, skinh, topleft);
        let oracle = fasm_oracle_set_window_clientbox(box_, fl_wstyle, skinh, topleft);
        assert_eq!(
            rust, oracle,
            "box={box_:?} style={fl_wstyle:#x} skinh={skinh} topleft={topleft:?}"
        );
    }

    #[test]
    fn swcb_whole_window_copies_box() {
        let box_ = b(10, 20, 300, 200);
        let mut tl = WINDOW_TOPLEFT_DEFAULT;
        tl[7] = 99;
        tl[9] = 99;
        let out = set_window_clientbox(box_, 0x00, 22, tl);
        assert_eq!(out.clientbox, b(0, 0, 300, 200));
        assert_eq!(out.topleft[7], 22);
        assert_eq!(out.topleft[9], 22);
        assert_swcb_match(box_, 0x00, 22, tl);
    }

    #[test]
    fn swcb_client_relative_style0() {
        // type 0: left=1 top=21; skinh still written to 3/4
        let box_ = b(0, 0, 100, 80);
        let style = WSTYLE_CLIENTRELATIVE | 0;
        let out = set_window_clientbox(box_, style, 22, WINDOW_TOPLEFT_DEFAULT);
        // width = 100 - 2*1 + 1 = 99; height = 80 - 21 - 1 + 1 = 59
        assert_eq!(out.clientbox, b(1, 21, 99, 59));
        assert_eq!(out.topleft[7], 22);
        assert_eq!(out.topleft[9], 22);
        assert_swcb_match(box_, style, 22, WINDOW_TOPLEFT_DEFAULT);
    }

    #[test]
    fn swcb_client_relative_style1_no_frame() {
        let box_ = b(5, 5, 50, 40);
        let style = WSTYLE_CLIENTRELATIVE | 1;
        let out = set_window_clientbox(box_, style, 18, WINDOW_TOPLEFT_DEFAULT);
        // left=0 top=0; width=50-0+1=51; height=40-0-0+1=41
        assert_eq!(out.clientbox, b(0, 0, 51, 41));
        assert_swcb_match(box_, style, 18, WINDOW_TOPLEFT_DEFAULT);
    }

    #[test]
    fn swcb_client_relative_style3_uses_skinh_top() {
        let box_ = b(0, 0, 200, 150);
        let style = WSTYLE_CLIENTRELATIVE | 3;
        let out = set_window_clientbox(box_, style, 25, WINDOW_TOPLEFT_DEFAULT);
        // left=5; top=skinh=25; width=200-10+1=191; height=150-25-5+1=121
        assert_eq!(out.clientbox, b(5, 25, 191, 121));
        assert_eq!(out.topleft[7], 25);
        assert_eq!(out.topleft[9], 25);
        assert_swcb_match(box_, style, 25, WINDOW_TOPLEFT_DEFAULT);
    }

    #[test]
    fn swcb_client_relative_style4_uses_skinh_top() {
        let box_ = b(0, 0, 200, 150);
        let style = WSTYLE_CLIENTRELATIVE | 4;
        let out = set_window_clientbox(box_, style, 30, WINDOW_TOPLEFT_DEFAULT);
        assert_eq!(out.clientbox, b(5, 30, 191, 116));
        assert_swcb_match(box_, style, 30, WINDOW_TOPLEFT_DEFAULT);
    }

    #[test]
    fn swcb_style2_fixed_insets() {
        let box_ = b(0, 0, 100, 100);
        let style = WSTYLE_CLIENTRELATIVE | 2;
        let out = set_window_clientbox(box_, style, 22, WINDOW_TOPLEFT_DEFAULT);
        // left=5 top=20; width=100-10+1=91; height=100-20-5+1=76
        assert_eq!(out.clientbox, b(5, 20, 91, 76));
        assert_swcb_match(box_, style, 22, WINDOW_TOPLEFT_DEFAULT);
    }

    #[test]
    fn swcb_wrapping_dims() {
        let box_ = b(0, 0, 3, 3);
        let style = WSTYLE_CLIENTRELATIVE | 0;
        // left=1 top=21 → width=3-2+1=2; height=3-21-1+1 = -18
        let out = set_window_clientbox(box_, style, 22, WINDOW_TOPLEFT_DEFAULT);
        assert_eq!(out.clientbox.width, 2);
        assert_eq!(out.clientbox.height, 3i32.wrapping_sub(21).wrapping_sub(1).wrapping_add(1));
        assert_swcb_match(box_, style, 22, WINDOW_TOPLEFT_DEFAULT);
    }

    #[test]
    fn swcb_ptr_roundtrip() {
        let mut wdata = [0u8; 48];
        let box_ = b(10, 20, 300, 200);
        wdata[0..16].copy_from_slice(&box_.to_bytes());
        wdata[WDATA_FL_WSTYLE_OFF] = WSTYLE_CLIENTRELATIVE | 3;
        // poison clientbox
        wdata[32..48].copy_from_slice(&b(1, 2, 3, 4).to_bytes());
        let mut tl = WINDOW_TOPLEFT_DEFAULT;
        unsafe {
            set_window_clientbox_ptr(wdata.as_mut_ptr(), 25, tl.as_mut_ptr());
        }
        assert_eq!(tl[7], 25);
        assert_eq!(tl[9], 25);
        let cb = WindowBox::from_bytes(wdata[32..48].try_into().unwrap());
        assert_eq!(cb, b(5, 25, 291, 171));
    }

    #[test]
    fn swcb_prng_corpus_50k() {
        let mut s = SET_WINDOW_CLIENTBOX_PRNG_SEED;
        for _ in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let left = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let top = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let width = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let height = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let skinh = s as i32;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let style_nibble = (s % 5) as u8;
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let client_rel = (s & 1) != 0;
            let mut fl = style_nibble;
            if client_rel {
                fl |= WSTYLE_CLIENTRELATIVE;
            }
            // Randomize non-skin topleft slots; skin tops overwritten by leaf.
            let mut tl = WINDOW_TOPLEFT_DEFAULT;
            for i in 0..10 {
                if i == 7 || i == 9 {
                    continue;
                }
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                tl[i] = s as i32;
            }
            assert_swcb_match(b(left, top, width, height), fl, skinh, tl);
        }
    }
}
