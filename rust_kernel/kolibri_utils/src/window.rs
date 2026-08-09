//! Cut S: `window._.check_window_position` — clamp a window `BOX` into the screen.
//!
//! Matches `kernel/gui/window.inc` FASM leaf semantics:
//! * width/height compared **unsigned** (`jae`) against display dims;
//! * left/top checked with **signed** `jl` / wrapping-add + `jge`;
//! * oversize dims become `display - 1`; off-screen edges nudged on-screen.
//!
//! No tables / `.rodata` — reloc-free friendly. Display dimensions are explicit
//! arguments (trampoline reads `_display.width` / `_display.height`).

/// PRNG seed for host differential corpus (Cut S).
pub const CHECK_WINDOW_POSITION_PRNG_SEED: u32 = 0x4357_5031; // 'CWP1'

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
}
