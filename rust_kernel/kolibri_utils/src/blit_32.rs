//! Cut CP: `blit_32` — syscall-73 LFB blit (clip + win_map + bpp stores).
//!
//! Matches `kernel/video/blitter.inc` FASM leaf semantics. Reloc-free: all
//! display/window/LFB pointers come from an injected [`Blit32Ctx`]. Cut CD
//! `blit_clip` is inlined (no cross-blob call).

use crate::geometry::{blit_clip, BlitterGeom, Rect, BLITTER_SIZE};

/// Cut CP PRNG seed (`'B32 '`).
pub const BLIT32_PRNG_SEED: u32 = 0x4233_3220;

/// `BLIT_CLIENT_RELATIVE` (`kernel/const.inc`).
pub const BLIT_CLIENT_RELATIVE: u32 = 0x2000_0000;

/// Userspace syscall-73 parameter block (40 bytes).
pub const PARAM_OFF_DST_X: usize = 0;
pub const PARAM_OFF_DST_Y: usize = 4;
pub const PARAM_OFF_W: usize = 8;
pub const PARAM_OFF_H: usize = 12;
pub const PARAM_OFF_SRC_X: usize = 16;
pub const PARAM_OFF_SRC_Y: usize = 20;
pub const PARAM_OFF_SRC_W: usize = 24;
pub const PARAM_OFF_SRC_H: usize = 28;
pub const PARAM_OFF_BITMAP: usize = 32;
pub const PARAM_OFF_STRIDE: usize = 36;
pub const PARAM_SIZE: usize = 40;

/// Injected trampoline context (17 dwords = 68 bytes).
pub const BLIT32_CTX_SIZE: usize = 68;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Blit32Ctx {
    pub win_left: i32,
    pub win_top: i32,
    pub win_width: i32,
    pub win_height: i32,
    pub client_left: i32,
    pub client_top: i32,
    pub slot_idx: u32,
    pub bpp: u32,
    pub display_width: u32,
    pub lfb_pitch: u32,
    pub win_map: *mut u8,
    pub lfb_base: *mut u8,
    pub bps_lut: *const u32,
    pub dwidth_lut: *const u32,
    pub select_cursor: u32,
    pub software_cursor: u32,
    pub check_mouse: u32,
}

/// Host-side mouse-under rewrite (`EAX=color`, `ECX=pos` → new color).
pub type CheckMouseFn = fn(u32, u32) -> u32;

#[inline(always)]
fn i32_le(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

#[inline(always)]
fn u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// FASM 16bpp store sequence (`shr ah,2` / `shr ax,3` / `ror 8` / `add al,ah` / `rol 8`).
#[inline(always)]
pub fn fasm_pack_rgb565(color: u32) -> u16 {
    let mut eax = color & 0x00F8_FCF8;
    let ah = ((eax >> 8) & 0xFF) >> 2;
    let al = eax & 0xFF;
    let mut ax = (ah << 8) | al;
    ax >>= 3;
    eax = (eax & 0xFFFF_0000) | ax;
    eax = eax.rotate_right(8);
    let sum = (eax as u8).wrapping_add((eax >> 8) as u8);
    eax = (eax & 0xFFFF_FF00) | u32::from(sum);
    eax = eax.rotate_left(8);
    eax as u16
}

/// Independent RGB565 pack (not the FASM shift sequence).
#[inline(always)]
pub fn independent_pack_rgb565(color: u32) -> u16 {
    let r5 = ((color >> 16) as u16 & 0xF8) >> 3;
    let g6 = ((color >> 8) as u16 & 0xFC) >> 2;
    let b5 = (color as u16 & 0xF8) >> 3;
    (r5 << 11) | (g6 << 5) | b5
}

#[inline(always)]
unsafe fn lut_u32(lut: *const u32, y: i32) -> u32 {
    *lut.add(y as u32 as usize)
}

#[inline(always)]
unsafe fn invoke_kernel_check_mouse(fn_ptr: u32, color: u32, pos: u32) -> u32 {
    if fn_ptr == 0 {
        return color;
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax = color;
        core::arch::asm!(
            "push ebx",
            "push edx",
            "call {f}",
            "pop edx",
            "pop ebx",
            f = in(reg) fn_ptr,
            inout("eax") eax,
            in("ecx") pos,
            lateout("ecx") _,
        );
        eax
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = pos;
        color
    }
}

#[inline(always)]
fn apply_mouse(
    software: bool,
    color: u32,
    pos: u32,
    host_mouse: Option<CheckMouseFn>,
    kernel_fn: u32,
) -> u32 {
    if !software {
        return color;
    }
    if let Some(h) = host_mouse {
        return h(color, pos);
    }
    unsafe { invoke_kernel_check_mouse(kernel_fn, color, pos) }
}

/// Production `blit_32` (FASM-semantic stores; Cut CD clip compose).
///
/// `bitmap` is the source 32bpp buffer (kernel: `[params+32]`). Host tests pass
/// a native pointer because the packed param dword is only 32-bit.
///
/// # Safety
/// `params` must be a readable 40-byte blit block; `ctx` pointers must cover
/// the clipped rectangle in LFB, win_map, LUTs, and `bitmap`.
#[inline(always)]
pub unsafe fn blit_32(
    params: *const u8,
    flags: u32,
    ctx: &Blit32Ctx,
    bitmap: *const u8,
    host_mouse: Option<CheckMouseFn>,
) {
    let p = core::slice::from_raw_parts(params, PARAM_SIZE);
    let dst_x = i32_le(p, PARAM_OFF_DST_X);
    let dst_y = i32_le(p, PARAM_OFF_DST_Y);
    let w = i32_le(p, PARAM_OFF_W);
    let h = i32_le(p, PARAM_OFF_H);
    let src_x = i32_le(p, PARAM_OFF_SRC_X);
    let src_y = i32_le(p, PARAM_OFF_SRC_Y);
    let src_w = i32_le(p, PARAM_OFF_SRC_W);
    let src_h = i32_le(p, PARAM_OFF_SRC_H);
    let stride = i32_le(p, PARAM_OFF_STRIDE);

    let geom = BlitterGeom::new(
        Rect::new(
            0,
            0,
            ctx.win_width.wrapping_add(1),
            ctx.win_height.wrapping_add(1),
        ),
        Rect::new(0, 0, src_w, src_h),
        dst_x,
        dst_y,
        src_x,
        src_y,
        w,
        h,
    );
    let clipped = blit_clip(geom);
    if !clipped.draw {
        return;
    }
    let g = clipped.geom;
    if g.w == 0 || g.h == 0 {
        return;
    }

    let mut out_x = g.dst_x.wrapping_add(ctx.win_left);
    let mut out_y = g.dst_y.wrapping_add(ctx.win_top);
    if (flags & BLIT_CLIENT_RELATIVE) != 0 {
        out_x = out_x.wrapping_add(ctx.client_left);
        out_y = out_y.wrapping_add(ctx.client_top);
    }

    let software = ctx.select_cursor == ctx.software_cursor;
    let slot = ctx.slot_idx as u8;
    let bpp = ctx.bpp;

    let src_base = bitmap
        .wrapping_offset((g.src_y.wrapping_mul(stride)) as isize)
        .wrapping_offset((g.src_x.wrapping_mul(4)) as isize);

    if bpp == 32 {
        blit_core_32(
            ctx, software, host_mouse, slot, src_base, stride, g.w, g.h, out_x, out_y,
        );
    } else if bpp == 24 {
        blit_core_24(
            ctx, software, host_mouse, slot, src_base, stride, g.w, g.h, out_x, out_y,
        );
    } else {
        blit_core_16(
            ctx, software, host_mouse, slot, src_base, stride, g.w, g.h, out_x, out_y,
        );
    }
}

#[inline(always)]
unsafe fn blit_core_32(
    ctx: &Blit32Ctx,
    software: bool,
    host_mouse: Option<CheckMouseFn>,
    slot: u8,
    src_base: *const u8,
    stride: i32,
    w: i32,
    h: i32,
    out_x: i32,
    out_y: i32,
) {
    let mut row = 0i32;
    while row < h {
        let y = out_y.wrapping_add(row);
        let mut lfb_off = lut_u32(ctx.bps_lut, y).wrapping_add((out_x as u32).wrapping_mul(4));
        let mut map_off = lut_u32(ctx.dwidth_lut, y).wrapping_add(out_x as u32);
        let mut src = src_base.wrapping_offset((row.wrapping_mul(stride)) as isize);
        let mut col = 0i32;
        while col < w {
            if *ctx.win_map.add(map_off as usize) == slot {
                let mut color = u32::from_le_bytes([*src, *src.add(1), *src.add(2), *src.add(3)]);
                let pos = ((out_x.wrapping_add(col) as u32) << 16)
                    | (y as u32 & 0xFFFF);
                color = apply_mouse(software, color, pos, host_mouse, ctx.check_mouse);
                let p = ctx.lfb_base.add(lfb_off as usize);
                p.copy_from_nonoverlapping(color.to_le_bytes().as_ptr(), 4);
            }
            src = src.add(4);
            lfb_off = lfb_off.wrapping_add(4);
            map_off = map_off.wrapping_add(1);
            col = col.wrapping_add(1);
        }
        row = row.wrapping_add(1);
    }
}

#[inline(always)]
unsafe fn blit_core_24(
    ctx: &Blit32Ctx,
    software: bool,
    host_mouse: Option<CheckMouseFn>,
    slot: u8,
    src_base: *const u8,
    stride: i32,
    w: i32,
    h: i32,
    out_x: i32,
    out_y: i32,
) {
    let mut row = 0i32;
    while row < h {
        let y = out_y.wrapping_add(row);
        let mut lfb_off = lut_u32(ctx.bps_lut, y)
            .wrapping_add((out_x as u32).wrapping_mul(3));
        let mut map_off = lut_u32(ctx.dwidth_lut, y).wrapping_add(out_x as u32);
        let mut src = src_base.wrapping_offset((row.wrapping_mul(stride)) as isize);
        let mut col = 0i32;
        while col < w {
            if *ctx.win_map.add(map_off as usize) == slot {
                let mut color = u32::from_le_bytes([*src, *src.add(1), *src.add(2), *src.add(3)]);
                let pos = ((out_x.wrapping_add(col) as u32) << 16)
                    | (y as u32 & 0xFFFF);
                color = apply_mouse(software, color, pos, host_mouse, ctx.check_mouse);
                let p = ctx.lfb_base.add(lfb_off as usize);
                *p = color as u8;
                *p.add(1) = (color >> 8) as u8;
                *p.add(2) = (color >> 16) as u8;
            }
            src = src.add(4);
            lfb_off = lfb_off.wrapping_add(3);
            map_off = map_off.wrapping_add(1);
            col = col.wrapping_add(1);
        }
        row = row.wrapping_add(1);
    }
}

#[inline(always)]
unsafe fn blit_core_16(
    ctx: &Blit32Ctx,
    software: bool,
    host_mouse: Option<CheckMouseFn>,
    slot: u8,
    src_base: *const u8,
    stride: i32,
    w: i32,
    h: i32,
    out_x: i32,
    out_y: i32,
) {
    let mut row = 0i32;
    while row < h {
        let y = out_y.wrapping_add(row);
        let mut lfb_off = lut_u32(ctx.bps_lut, y)
            .wrapping_add((out_x as u32).wrapping_mul(2));
        let mut map_off = lut_u32(ctx.dwidth_lut, y).wrapping_add(out_x as u32);
        let mut src = src_base.wrapping_offset((row.wrapping_mul(stride)) as isize);
        let mut col = 0i32;
        while col < w {
            if *ctx.win_map.add(map_off as usize) == slot {
                let mut color = u32::from_le_bytes([*src, *src.add(1), *src.add(2), *src.add(3)]);
                let pos = ((out_x.wrapping_add(col) as u32) << 16)
                    | (y as u32 & 0xFFFF);
                color = apply_mouse(software, color, pos, host_mouse, ctx.check_mouse);
                let packed = fasm_pack_rgb565(color);
                let p = ctx.lfb_base.add(lfb_off as usize);
                p.copy_from_nonoverlapping(packed.to_le_bytes().as_ptr(), 2);
            }
            src = src.add(4);
            lfb_off = lfb_off.wrapping_add(2);
            map_off = map_off.wrapping_add(1);
            col = col.wrapping_add(1);
        }
        row = row.wrapping_add(1);
    }
}

/// `stdcall` pointer wrapper used by [`crate::ffi::rust_blit_32`].
///
/// # Safety
/// Same as [`blit_32`]; `ctx` must be a readable [`Blit32Ctx`].
/// Kernel params+32 is a 32-bit source pointer.
#[inline(always)]
pub unsafe fn blit_32_ptr(params: *const u8, flags: u32, ctx: *const Blit32Ctx) {
    let p = core::slice::from_raw_parts(params, PARAM_SIZE);
    let bitmap = u32_le(p, PARAM_OFF_BITMAP) as *const u8;
    blit_32(params, flags, &*ctx, bitmap, None);
}

/// Independent nested-loop oracle (clip via Cut CD FASM-flow oracle).
#[cfg(test)]
pub unsafe fn fasm_oracle_blit_32(
    params: *const u8,
    flags: u32,
    ctx: &Blit32Ctx,
    bitmap: *const u8,
    host_mouse: Option<CheckMouseFn>,
) {
    use crate::geometry::fasm_oracle_blit_clip;

    let p = core::slice::from_raw_parts(params, PARAM_SIZE);
    let dst_x = i32_le(p, PARAM_OFF_DST_X);
    let dst_y = i32_le(p, PARAM_OFF_DST_Y);
    let w = i32_le(p, PARAM_OFF_W);
    let h = i32_le(p, PARAM_OFF_H);
    let src_x = i32_le(p, PARAM_OFF_SRC_X);
    let src_y = i32_le(p, PARAM_OFF_SRC_Y);
    let src_w = i32_le(p, PARAM_OFF_SRC_W);
    let src_h = i32_le(p, PARAM_OFF_SRC_H);
    let stride = i32_le(p, PARAM_OFF_STRIDE);

    let geom = BlitterGeom::new(
        Rect::new(
            0,
            0,
            ctx.win_width.wrapping_add(1),
            ctx.win_height.wrapping_add(1),
        ),
        Rect::new(0, 0, src_w, src_h),
        dst_x,
        dst_y,
        src_x,
        src_y,
        w,
        h,
    );
    let clipped = fasm_oracle_blit_clip(geom);
    if !clipped.draw || clipped.geom.w == 0 || clipped.geom.h == 0 {
        return;
    }
    let g = clipped.geom;
    let mut ox = g.dst_x.wrapping_add(ctx.win_left);
    let mut oy = g.dst_y.wrapping_add(ctx.win_top);
    if (flags & BLIT_CLIENT_RELATIVE) != 0 {
        ox = ox.wrapping_add(ctx.client_left);
        oy = oy.wrapping_add(ctx.client_top);
    }
    let software = ctx.select_cursor == ctx.software_cursor;
    let slot = ctx.slot_idx as u8;
    let bytes_pp: u32 = if ctx.bpp == 32 {
        4
    } else if ctx.bpp == 24 {
        3
    } else {
        2
    };

    for row in 0..g.h {
        let y = oy.wrapping_add(row);
        let map_row = lut_u32(ctx.dwidth_lut, y).wrapping_add(ox as u32);
        let lfb_row = lut_u32(ctx.bps_lut, y).wrapping_add((ox as u32).wrapping_mul(bytes_pp));
        for col in 0..g.w {
            if *ctx.win_map.add((map_row.wrapping_add(col as u32)) as usize) != slot {
                continue;
            }
            let src = bitmap
                .wrapping_offset(
                    (g.src_y.wrapping_add(row).wrapping_mul(stride)
                        .wrapping_add(g.src_x.wrapping_add(col).wrapping_mul(4)))
                        as isize,
                );
            let mut color = u32::from_le_bytes([*src, *src.add(1), *src.add(2), *src.add(3)]);
            let pos = ((ox.wrapping_add(col) as u32) << 16) | (y as u32 & 0xFFFF);
            color = apply_mouse(software, color, pos, host_mouse, 0);
            let dst = ctx
                .lfb_base
                .add((lfb_row.wrapping_add((col as u32).wrapping_mul(bytes_pp))) as usize);
            if ctx.bpp == 32 {
                dst.copy_from_nonoverlapping(color.to_le_bytes().as_ptr(), 4);
            } else if ctx.bpp == 24 {
                *dst = color as u8;
                *dst.add(1) = (color >> 8) as u8;
                *dst.add(2) = (color >> 16) as u8;
            } else {
                let packed = independent_pack_rgb565(color);
                dst.copy_from_nonoverlapping(packed.to_le_bytes().as_ptr(), 2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Xor32(u32);
    impl Xor32 {
        fn next(&mut self) -> u32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.0 = x;
            x
        }
        fn bounded(&mut self, n: u32) -> u32 {
            if n == 0 {
                0
            } else {
                self.next() % n
            }
        }
    }

    const MAX_W: usize = 16;
    const MAX_H: usize = 16;
    const LFB_BYTES: usize = MAX_H * MAX_W * 4;

    fn fill_luts(bps: &mut [u32], dw: &mut [u32], pitch: u32, width: u32) {
        for y in 0..MAX_H {
            bps[y] = (y as u32).wrapping_mul(pitch);
            dw[y] = (y as u32).wrapping_mul(width);
        }
    }

    fn mouse_xor(color: u32, pos: u32) -> u32 {
        color ^ pos
    }

    unsafe fn run_pair(
        params: &mut [u8; PARAM_SIZE],
        flags: u32,
        ctx: &Blit32Ctx,
        bitmap: *const u8,
        lfb_a: &mut [u8],
        lfb_b: &mut [u8],
        mouse: Option<CheckMouseFn>,
    ) {
        core::ptr::write_bytes(ctx.lfb_base, 0xA5, lfb_a.len());
        blit_32(params.as_ptr(), flags, ctx, bitmap, mouse);
        lfb_a.copy_from_slice(core::slice::from_raw_parts(ctx.lfb_base, lfb_a.len()));
        core::ptr::write_bytes(ctx.lfb_base, 0xA5, lfb_a.len());
        fasm_oracle_blit_32(params.as_ptr(), flags, ctx, bitmap, mouse);
        lfb_b.copy_from_slice(core::slice::from_raw_parts(ctx.lfb_base, lfb_b.len()));
    }

    fn pack_params(
        dst_x: i32,
        dst_y: i32,
        w: i32,
        h: i32,
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        bitmap: u32,
        stride: i32,
    ) -> [u8; PARAM_SIZE] {
        let mut p = [0u8; PARAM_SIZE];
        p[PARAM_OFF_DST_X..PARAM_OFF_DST_X + 4].copy_from_slice(&dst_x.to_le_bytes());
        p[PARAM_OFF_DST_Y..PARAM_OFF_DST_Y + 4].copy_from_slice(&dst_y.to_le_bytes());
        p[PARAM_OFF_W..PARAM_OFF_W + 4].copy_from_slice(&w.to_le_bytes());
        p[PARAM_OFF_H..PARAM_OFF_H + 4].copy_from_slice(&h.to_le_bytes());
        p[PARAM_OFF_SRC_X..PARAM_OFF_SRC_X + 4].copy_from_slice(&src_x.to_le_bytes());
        p[PARAM_OFF_SRC_Y..PARAM_OFF_SRC_Y + 4].copy_from_slice(&src_y.to_le_bytes());
        p[PARAM_OFF_SRC_W..PARAM_OFF_SRC_W + 4].copy_from_slice(&src_w.to_le_bytes());
        p[PARAM_OFF_SRC_H..PARAM_OFF_SRC_H + 4].copy_from_slice(&src_h.to_le_bytes());
        p[PARAM_OFF_BITMAP..PARAM_OFF_BITMAP + 4].copy_from_slice(&bitmap.to_le_bytes());
        p[PARAM_OFF_STRIDE..PARAM_OFF_STRIDE + 4].copy_from_slice(&stride.to_le_bytes());
        p
    }

    #[test]
    fn bl32_rgb565_matches_independent() {
        let mut rng = Xor32(0x5650_5650);
        for _ in 0..10_000 {
            let c = rng.next() & 0x00FF_FFFF;
            assert_eq!(
                fasm_pack_rgb565(c),
                independent_pack_rgb565(c),
                "color {c:#010x}"
            );
        }
        assert_eq!(fasm_pack_rgb565(0x00FF00), independent_pack_rgb565(0x00FF00));
        assert_eq!(fasm_pack_rgb565(0xFF0000), independent_pack_rgb565(0xFF0000));
        assert_eq!(fasm_pack_rgb565(0x0000FF), independent_pack_rgb565(0x0000FF));
    }

    #[test]
    fn bl32_hw32_full_window_exact() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [1u8; MAX_W * MAX_H];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 4) as u32, MAX_W as u32);
        let mut src = [0u8; MAX_W * MAX_H * 4];
        for (i, ch) in src.chunks_exact_mut(4).enumerate() {
            ch.copy_from_slice(&(0xAA000000u32.wrapping_add(i as u32)).to_le_bytes());
        }
        let mut params = pack_params(
            0,
            0,
            4,
            3,
            0,
            0,
            8,
            8,
            src.as_ptr() as u32,
            (MAX_W * 4) as i32,
        );
        let ctx = Blit32Ctx {
            win_left: 0,
            win_top: 0,
            win_width: 7,
            win_height: 7,
            client_left: 0,
            client_top: 0,
            slot_idx: 1,
            bpp: 32,
            display_width: MAX_W as u32,
            lfb_pitch: (MAX_W * 4) as u32,
            win_map: map.as_mut_ptr(),
            lfb_base: lfb.as_mut_ptr(),
            bps_lut: bps.as_ptr(),
            dwidth_lut: dw.as_ptr(),
            select_cursor: 0x1111,
            software_cursor: 0x2222,
            check_mouse: 0,
        };
        let mut a = [0u8; LFB_BYTES];
        let mut b = [0u8; LFB_BYTES];
        unsafe {
            run_pair(&mut params, 0, &ctx, src.as_ptr(), &mut a, &mut b, None);
        }
        assert_eq!(a, b);
        // Pixel (0,0) owned → first source dword
        assert_eq!(&a[0..4], &src[0..4]);
    }

    #[test]
    fn bl32_win_map_skips_foreign() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [1u8; MAX_W * MAX_H];
        map[1] = 9; // skip x=1,y=0
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 4) as u32, MAX_W as u32);
        let mut src = [0u8; 64];
        src[0..4].copy_from_slice(&0x1111_1111u32.to_le_bytes());
        src[4..8].copy_from_slice(&0x2222_2222u32.to_le_bytes());
        src[8..12].copy_from_slice(&0x3333_3333u32.to_le_bytes());
        let mut params = pack_params(0, 0, 3, 1, 0, 0, 8, 8, src.as_ptr() as u32, 16);
        let ctx = Blit32Ctx {
            win_left: 0,
            win_top: 0,
            win_width: 7,
            win_height: 7,
            client_left: 0,
            client_top: 0,
            slot_idx: 1,
            bpp: 32,
            display_width: MAX_W as u32,
            lfb_pitch: (MAX_W * 4) as u32,
            win_map: map.as_mut_ptr(),
            lfb_base: lfb.as_mut_ptr(),
            bps_lut: bps.as_ptr(),
            dwidth_lut: dw.as_ptr(),
            select_cursor: 1,
            software_cursor: 2,
            check_mouse: 0,
        };
        let mut a = [0u8; LFB_BYTES];
        let mut b = [0u8; LFB_BYTES];
        unsafe {
            run_pair(&mut params, 0, &ctx, src.as_ptr(), &mut a, &mut b, None);
        }
        assert_eq!(a, b);
        assert_eq!(&a[0..4], &0x1111_1111u32.to_le_bytes());
        assert_eq!(&a[4..8], &[0xA5, 0xA5, 0xA5, 0xA5]); // skipped
        assert_eq!(&a[8..12], &0x3333_3333u32.to_le_bytes());
    }

    #[test]
    fn bl32_clip_reject_untouched() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [1u8; MAX_W * MAX_H];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 4) as u32, MAX_W as u32);
        let src = [0x44u8; 64];
        let mut params = pack_params(100, 100, 4, 4, 0, 0, 8, 8, src.as_ptr() as u32, 16);
        let ctx = Blit32Ctx {
            win_left: 0,
            win_top: 0,
            win_width: 7,
            win_height: 7,
            client_left: 0,
            client_top: 0,
            slot_idx: 1,
            bpp: 32,
            display_width: MAX_W as u32,
            lfb_pitch: (MAX_W * 4) as u32,
            win_map: map.as_mut_ptr(),
            lfb_base: lfb.as_mut_ptr(),
            bps_lut: bps.as_ptr(),
            dwidth_lut: dw.as_ptr(),
            select_cursor: 1,
            software_cursor: 2,
            check_mouse: 0,
        };
        let mut a = [0u8; LFB_BYTES];
        let mut b = [0u8; LFB_BYTES];
        unsafe {
            run_pair(&mut params, 0, &ctx, src.as_ptr(), &mut a, &mut b, None);
        }
        assert_eq!(a, b);
        assert!(a.iter().all(|&x| x == 0xA5));
    }

    #[test]
    fn bl32_zero_wh_untouched() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [1u8; MAX_W * MAX_H];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 4) as u32, MAX_W as u32);
        let src = [0x55u8; 16];
        for (w, h) in [(0, 4), (4, 0)] {
            let mut params = pack_params(0, 0, w, h, 0, 0, 8, 8, src.as_ptr() as u32, 16);
            let ctx = Blit32Ctx {
                win_left: 0,
                win_top: 0,
                win_width: 7,
                win_height: 7,
                client_left: 0,
                client_top: 0,
                slot_idx: 1,
                bpp: 32,
                display_width: MAX_W as u32,
                lfb_pitch: (MAX_W * 4) as u32,
                win_map: map.as_mut_ptr(),
                lfb_base: lfb.as_mut_ptr(),
                bps_lut: bps.as_ptr(),
                dwidth_lut: dw.as_ptr(),
                select_cursor: 1,
                software_cursor: 2,
                check_mouse: 0,
            };
            let mut a = [0u8; LFB_BYTES];
            let mut b = [0u8; LFB_BYTES];
            unsafe {
                run_pair(&mut params, 0, &ctx, src.as_ptr(), &mut a, &mut b, None);
            }
            assert_eq!(a, b);
            assert!(a.iter().all(|&x| x == 0xA5), "w={w} h={h}");
        }
    }

    #[test]
    fn bl32_software_cursor_mouse_rewrite() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [1u8; MAX_W * MAX_H];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 4) as u32, MAX_W as u32);
        let mut src = [0u8; 16];
        src[0..4].copy_from_slice(&0x00AA_BBCCu32.to_le_bytes());
        let mut params = pack_params(2, 1, 1, 1, 0, 0, 8, 8, src.as_ptr() as u32, 16);
        let ctx = Blit32Ctx {
            win_left: 1,
            win_top: 1,
            win_width: 7,
            win_height: 7,
            client_left: 0,
            client_top: 0,
            slot_idx: 1,
            bpp: 32,
            display_width: MAX_W as u32,
            lfb_pitch: (MAX_W * 4) as u32,
            win_map: map.as_mut_ptr(),
            lfb_base: lfb.as_mut_ptr(),
            bps_lut: bps.as_ptr(),
            dwidth_lut: dw.as_ptr(),
            select_cursor: 0xC0DE,
            software_cursor: 0xC0DE,
            check_mouse: 0,
        };
        let mut a = [0u8; LFB_BYTES];
        let mut b = [0u8; LFB_BYTES];
        unsafe {
            run_pair(&mut params, 0, &ctx, src.as_ptr(), &mut a, &mut b, Some(mouse_xor));
        }
        assert_eq!(a, b);
        let x = 2 + 1;
        let y = 1 + 1;
        let pos = ((x as u32) << 16) | (y as u32);
        let expect = 0x00AA_BBCCu32 ^ pos;
        let off = (y as usize) * MAX_W * 4 + (x as usize) * 4;
        assert_eq!(&a[off..off + 4], &expect.to_le_bytes());
    }

    #[test]
    fn bl32_client_relative_and_24bpp() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [3u8; MAX_W * MAX_H];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 3) as u32, MAX_W as u32);
        let mut src = [0u8; 16];
        src[0..4].copy_from_slice(&0x0011_2233u32.to_le_bytes());
        let mut params = pack_params(0, 0, 1, 1, 0, 0, 4, 4, src.as_ptr() as u32, 16);
        let ctx = Blit32Ctx {
            win_left: 1,
            win_top: 2,
            win_width: 7,
            win_height: 7,
            client_left: 2,
            client_top: 1,
            slot_idx: 3,
            bpp: 24,
            display_width: MAX_W as u32,
            lfb_pitch: (MAX_W * 3) as u32,
            win_map: map.as_mut_ptr(),
            lfb_base: lfb.as_mut_ptr(),
            bps_lut: bps.as_ptr(),
            dwidth_lut: dw.as_ptr(),
            select_cursor: 1,
            software_cursor: 2,
            check_mouse: 0,
        };
        let mut a = [0u8; LFB_BYTES];
        let mut b = [0u8; LFB_BYTES];
        unsafe {
            run_pair(
                &mut params,
                BLIT_CLIENT_RELATIVE,
                &ctx,
                src.as_ptr(),
                &mut a,
                &mut b,
                None,
            );
        }
        assert_eq!(a, b);
        let x = 1 + 2;
        let y = 2 + 1;
        let off = (y as usize) * MAX_W * 3 + (x as usize) * 3;
        assert_eq!(a[off], 0x33);
        assert_eq!(a[off + 1], 0x22);
        assert_eq!(a[off + 2], 0x11);
    }

    #[test]
    fn bl32_16bpp_and_odd_bpp_uses_16_path() {
        let mut lfb = [0u8; LFB_BYTES];
        let mut map = [1u8; MAX_W * MAX_H];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        fill_luts(&mut bps, &mut dw, (MAX_W * 2) as u32, MAX_W as u32);
        let mut src = [0u8; 16];
        src[0..4].copy_from_slice(&0x00FF_8000u32.to_le_bytes());
        for bpp in [16u32, 8, 0, 15] {
            let mut params = pack_params(0, 0, 1, 1, 0, 0, 4, 4, src.as_ptr() as u32, 16);
            let ctx = Blit32Ctx {
                win_left: 0,
                win_top: 0,
                win_width: 7,
                win_height: 7,
                client_left: 0,
                client_top: 0,
                slot_idx: 1,
                bpp,
                display_width: MAX_W as u32,
                lfb_pitch: (MAX_W * 2) as u32,
                win_map: map.as_mut_ptr(),
                lfb_base: lfb.as_mut_ptr(),
                bps_lut: bps.as_ptr(),
                dwidth_lut: dw.as_ptr(),
                select_cursor: 1,
                software_cursor: 2,
                check_mouse: 0,
            };
            let mut a = [0u8; LFB_BYTES];
            let mut b = [0u8; LFB_BYTES];
            unsafe {
                run_pair(&mut params, 0, &ctx, src.as_ptr(), &mut a, &mut b, None);
            }
            assert_eq!(a, b, "bpp={bpp}");
            let packed = independent_pack_rgb565(0x00FF_8000);
            assert_eq!(&a[0..2], &packed.to_le_bytes(), "bpp={bpp}");
        }
    }

    #[test]
    fn bl32_prng_50000() {
        let mut rng = Xor32(BLIT32_PRNG_SEED);
        let mut src = [0u8; MAX_W * MAX_H * 4];
        let mut map = [0u8; MAX_W * MAX_H];
        let mut lfb = [0u8; LFB_BYTES];
        let mut bps = [0u32; MAX_H];
        let mut dw = [0u32; MAX_H];
        let mut a = [0u8; LFB_BYTES];
        let mut b = [0u8; LFB_BYTES];
        for case in 0..50_000u32 {
            let bpp_sel = rng.bounded(4);
            let bpp = match bpp_sel {
                0 => 32,
                1 => 24,
                2 => 16,
                _ => rng.bounded(15), // odd bpp → 16-path
            };
            let bytes_pp = if bpp == 32 {
                4
            } else if bpp == 24 {
                3
            } else {
                2
            };
            let pitch = (MAX_W as u32) * bytes_pp;
            fill_luts(&mut bps, &mut dw, pitch, MAX_W as u32);
            let slot = (rng.bounded(4) + 1) as u8;
            for m in map.iter_mut() {
                *m = if rng.bounded(4) == 0 {
                    slot.wrapping_add(1)
                } else {
                    slot
                };
            }
            for ch in src.chunks_exact_mut(4) {
                ch.copy_from_slice(&rng.next().to_le_bytes());
            }
            let w = (rng.bounded(6) + 1) as i32;
            let h = (rng.bounded(6) + 1) as i32;
            let dst_x = rng.bounded(4) as i32;
            let dst_y = rng.bounded(4) as i32;
            let src_w = 8i32;
            let src_h = 8i32;
            let flags = if rng.bounded(2) == 0 {
                0
            } else {
                BLIT_CLIENT_RELATIVE
            };
            let software = rng.bounded(2) == 0;
            let mut params = pack_params(
                dst_x,
                dst_y,
                w,
                h,
                0,
                0,
                src_w,
                src_h,
                src.as_ptr() as u32,
                (MAX_W * 4) as i32,
            );
            let cur = 0xABCDu32;
            let ctx = Blit32Ctx {
                win_left: rng.bounded(3) as i32,
                win_top: rng.bounded(3) as i32,
                win_width: 10,
                win_height: 10,
                client_left: rng.bounded(2) as i32,
                client_top: rng.bounded(2) as i32,
                slot_idx: slot as u32,
                bpp,
                display_width: MAX_W as u32,
                lfb_pitch: pitch,
                win_map: map.as_mut_ptr(),
                lfb_base: lfb.as_mut_ptr(),
                bps_lut: bps.as_ptr(),
                dwidth_lut: dw.as_ptr(),
                select_cursor: cur,
                software_cursor: if software { cur } else { cur ^ 1 },
                check_mouse: 0,
            };
            let mouse = if software && rng.bounded(2) == 0 {
                Some(mouse_xor as CheckMouseFn)
            } else {
                None
            };
            unsafe {
                run_pair(&mut params, flags, &ctx, src.as_ptr(), &mut a, &mut b, mouse);
            }
            assert_eq!(a, b, "case {case} bpp={bpp}");
        }
    }

    #[test]
    fn bl32_ctx_size() {
        assert_eq!(PARAM_SIZE, 40);
        assert_eq!(BLITTER_SIZE, 64);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(core::mem::size_of::<Blit32Ctx>(), BLIT32_CTX_SIZE);
    }
}
