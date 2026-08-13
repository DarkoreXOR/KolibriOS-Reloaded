//! Cut CR: `drawChar` — GUI glyph rasterization (scale + smoothing).
//!
//! Matches `kernel/gui/font.inc` FASM leaf. Reloc-free: pitch, delta-to-screen,
//! row count, smoothing mode, and `syscall_getpixel` are injected via
//! [`DrawCharCtx`]. Cut N `antiAliasing` is inlined (no cross-blob reloc).
//!
//! dtext stack ABI (REG-010): at `drawChar` entry, `[esp+4]` = row count,
//! `[esp+24]` = `widthX`, `[esp+44]` = `deltaToScreen`. First-row `bsf eax,edx`
//! sees caller EDX high bytes after `mov dl,[ebx]`.

use crate::font::anti_aliasing;

/// Cut CR differential PRNG seed (`'DCHR'`).
pub const DRAW_CHAR_PRNG_SEED: u32 = 0x4443_4852;

/// Injected trampoline context (10 dwords = 40 bytes).
pub const DRAW_CHAR_CTX_SIZE: usize = 40;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DrawCharCtx {
    pub color: u32,
    pub multiplier: u32,
    pub buffer: *mut u8,
    pub glyph: *const u8,
    pub rows: u32,
    pub width_x: u32,
    pub delta_to_screen: u32,
    pub smoothing: u32,
    pub getpixel: u32,
    pub edx_in: u32,
}

/// Host-side getpixel hook (kernel uses ctx function pointer).
pub type GetPixelFn = fn(u32) -> u32;

#[inline(always)]
fn bsf32(x: u32) -> u32 {
    x.trailing_zeros()
}

#[inline(always)]
unsafe fn bt_mem(base: *const u8, disp: isize, bit: u32) -> bool {
    let off = disp.wrapping_add((bit >> 3) as isize);
    let b = unsafe { *base.wrapping_offset(off) };
    ((b >> (bit & 7)) & 1) != 0
}

#[inline(always)]
unsafe fn load_pix(p: *const u8) -> u32 {
    u32::from_le_bytes(unsafe { [*p, *p.add(1), *p.add(2), *p.add(3)] })
}

#[inline(always)]
unsafe fn store_pix(p: *mut u8, v: u32) {
    let b = v.to_le_bytes();
    unsafe {
        *p = b[0];
        *p.add(1) = b[1];
        *p.add(2) = b[2];
        *p.add(3) = b[3];
    }
}

/// FASM `.subpixelLeft` `lea` sequence (production).
#[inline(always)]
fn fasm_subpixel_left(neigh: u32, font: u32) -> u32 {
    let mut eax = neigh;
    let mut ebx = font;
    let mut ecx = u32::from(ebx as u8);
    let mut edx = ecx.wrapping_mul(8).wrapping_add(ecx);
    edx = ecx.wrapping_mul(2).wrapping_add(edx);
    ecx = u32::from(eax as u8);
    ecx = ecx.wrapping_mul(4).wrapping_add(ecx);
    edx = edx.wrapping_add(ecx);
    edx >>= 4;
    eax = (eax & 0xFFFF_FF00) | (edx & 0xFF);

    ecx = u32::from((eax >> 8) as u8);
    edx = ecx.wrapping_mul(8).wrapping_add(ecx);
    edx = ecx.wrapping_mul(2).wrapping_add(edx);
    ecx = u32::from((ebx >> 8) as u8);
    ecx = ecx.wrapping_mul(4).wrapping_add(ecx);
    edx = edx.wrapping_add(ecx);
    edx >>= 4;
    eax = (eax & 0xFFFF_00FF) | ((edx & 0xFF) << 8);

    eax = eax.rotate_left(16);
    ebx = ebx.rotate_left(16);
    ecx = u32::from(eax as u8);
    edx = ecx;
    ecx <<= 3;
    ecx = ecx.wrapping_sub(edx);
    edx = u32::from(ebx as u8);
    ecx = ecx.wrapping_add(edx);
    ecx >>= 3;
    eax = (eax & 0xFFFF_FF00) | (ecx & 0xFF);
    eax.rotate_left(16)
}

/// FASM `.subpixelRight` `lea`/`shl` sequence (production).
#[inline(always)]
fn fasm_subpixel_right(neigh: u32, font: u32) -> u32 {
    let mut eax = neigh;
    let mut ebx = font;
    let mut ecx = u32::from(eax as u8);
    let mut edx = ecx;
    ecx <<= 3;
    ecx = ecx.wrapping_sub(edx);
    edx = u32::from(ebx as u8);
    ecx = ecx.wrapping_add(edx);
    ecx >>= 3;
    eax = (eax & 0xFFFF_FF00) | (ecx & 0xFF);

    ecx = u32::from((eax >> 8) as u8);
    edx = ecx.wrapping_mul(8).wrapping_add(ecx);
    edx = ecx.wrapping_mul(2).wrapping_add(edx);
    ecx = u32::from((ebx >> 8) as u8);
    ecx = ecx.wrapping_mul(4).wrapping_add(ecx);
    edx = edx.wrapping_add(ecx);
    edx >>= 4;
    eax = (eax & 0xFFFF_00FF) | ((edx & 0xFF) << 8);

    ebx = ebx.rotate_left(16);
    eax = eax.rotate_left(16);
    ecx = u32::from(ebx as u8);
    edx = ecx.wrapping_mul(8).wrapping_add(ecx);
    edx = ecx.wrapping_mul(2).wrapping_add(edx);
    ecx = u32::from(eax as u8);
    ecx = ecx.wrapping_mul(4).wrapping_add(ecx);
    edx = edx.wrapping_add(ecx);
    edx >>= 4;
    eax = (eax & 0xFFFF_FF00) | (edx & 0xFF);
    eax.rotate_left(16)
}

/// Independent subpixel-left weights from the same `lea` encoding
/// (`11*font+5*neigh` blue, `11*neigh+5*font` green, `7*neigh+font` red).
#[inline(always)]
fn oracle_subpixel_left(neigh: u32, font: u32) -> u32 {
    let nb = neigh as u8;
    let ng = (neigh >> 8) as u8;
    let nr = (neigh >> 16) as u8;
    let fb = font as u8;
    let fg = (font >> 8) as u8;
    let fr = (font >> 16) as u8;
    let b = ((u32::from(fb) * 11).wrapping_add(u32::from(nb) * 5)) >> 4;
    let g = ((u32::from(ng) * 11).wrapping_add(u32::from(fg) * 5)) >> 4;
    let r = ((u32::from(nr) * 7).wrapping_add(u32::from(fr))) >> 3;
    (neigh & 0xFF00_0000) | (r << 16) | (g << 8) | b
}

/// Independent subpixel-right weights (`7*neigh+font` blue, `11*neigh+5*font`
/// green, `11*font+5*neigh` red).
#[inline(always)]
fn oracle_subpixel_right(neigh: u32, font: u32) -> u32 {
    let nb = neigh as u8;
    let ng = (neigh >> 8) as u8;
    let nr = (neigh >> 16) as u8;
    let fb = font as u8;
    let fg = (font >> 8) as u8;
    let fr = (font >> 16) as u8;
    let b = ((u32::from(nb) * 7).wrapping_add(u32::from(fb))) >> 3;
    let g = ((u32::from(ng) * 11).wrapping_add(u32::from(fg) * 5)) >> 4;
    let r = ((u32::from(fr) * 11).wrapping_add(u32::from(nr) * 5)) >> 4;
    (neigh & 0xFF00_0000) | (r << 16) | (g << 8) | b
}

#[inline(always)]
unsafe fn call_getpixel(
    fn_ptr: u32,
    index: u32,
    host: Option<GetPixelFn>,
) -> u32 {
    if let Some(h) = host {
        return h(index);
    }
    if fn_ptr == 0 {
        return 0;
    }
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax: u32;
        unsafe {
            // Thunk is register ABI (EBX=index, EAX=color, plain ret), not
            // stdcall — a callee `ret N` is invisible to LLVM (ESP drift).
            // Pin fn/index to EDX/EBX. Do not `in("esi")` (REG-017). lateout
            // ECX/EDX so LLVM cannot reuse them across neighbor blends (REG-019).
            core::arch::asm!(
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",
                "mov ebx, ecx",
                "call edx",
                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                in("edx") fn_ptr,
                in("ecx") index,
                lateout("eax") eax,
                lateout("ecx") _,
                lateout("edx") _,
            );
        }
        eax
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = fn_ptr;
        0
    }
}

#[inline(always)]
unsafe fn neighbor_color(
    pix: *mut u8,
    delta: u32,
    getpixel: u32,
    host: Option<GetPixelFn>,
    toward: i32,
) -> u32 {
    let mut eax = unsafe { load_pix(pix.wrapping_offset(toward as isize)) };
    if delta != 0 {
        let idx = (pix as u32)
            .wrapping_add(delta)
            .wrapping_add(toward as u32)
            >> 2;
        eax = unsafe { call_getpixel(getpixel, idx, host) };
    }
    eax
}

#[inline(always)]
unsafe fn fill_eax_down(edi: *mut u8, mut eax: i32, color: u32) {
    if eax < 0 {
        return;
    }
    loop {
        unsafe {
            store_pix(
                edi.wrapping_add((eax as usize).wrapping_mul(4)),
                color,
            );
        }
        eax -= 1;
        if eax < 0 {
            break;
        }
    }
}

/// FASM-faithful `drawChar`.
///
/// # Safety
/// `ctx` pointers and the glyph/buffer extents (rows × pitch, neighbor `bt`)
/// must be valid. `host` is used on the host; kernel passes `None` and a
/// getpixel thunk pointer.
#[inline(always)]
pub unsafe fn draw_char(ctx: *mut DrawCharCtx, host: Option<GetPixelFn>) {
    unsafe { draw_char_fasm(&mut *ctx, host, false) }
}

/// Kernel blob entry: 1× path only. The trampoline sends `esi != 1` to FASM
/// so the scaled diamond stays out of `.text.rust_draw_char` (REG-012).
#[inline(always)]
pub unsafe fn draw_char_ptr(ctx: *mut DrawCharCtx) {
    unsafe { draw_char_1x(&mut *ctx, None) }
}

/// Independent oracle (separate control-flow transcription + independent
/// blend weights). `use_oracle_blend` selects Cut N / weight formulas
/// instead of production `lea` stores.
pub unsafe fn fasm_oracle_draw_char(ctx: *mut DrawCharCtx, host: Option<GetPixelFn>) {
    unsafe { draw_char_fasm(&mut *ctx, host, true) }
}

#[inline(always)]
unsafe fn draw_char_1x(ctx: &mut DrawCharCtx, host: Option<GetPixelFn>) {
    let color = ctx.color;
    let mut esi = 1u32;
    let mut edi = ctx.buffer;
    let mut ebx = ctx.glyph;
    let mut edx = ctx.edx_in;
    let mut rows = ctx.rows;
    let width_x = ctx.width_x;
    let delta = ctx.delta_to_screen;
    let smooth = ctx.smoothing as u8;
    let gp = ctx.getpixel;

    loop {
        edx = (edx & 0xFFFF_FF00) | u32::from(unsafe { *ebx });
        loop {
            if edx == 0 {
                break;
            }
            let bit = bsf32(edx);
            let mut eax = bit.wrapping_mul(esi);
            eax = eax.wrapping_shl(2);
            let saved = edi;
            edi = edi.wrapping_add(eax as usize);
            unsafe { store_pix(edi, color) };
            if smooth != 0 {
                smooth_left_1x(
                    ebx, edx, edi, color, delta, gp, host, smooth, false,
                );
                smooth_right_1x(
                    ebx, edx, edi, color, delta, gp, host, smooth, false,
                );
            }
            edx &= !(1u32 << bit);
            edi = saved;
        }
        ebx = ebx.wrapping_add(1);
        edi = edi.wrapping_add(width_x as usize);
        rows = rows.wrapping_sub(1);
        if rows == 0 {
            break;
        }
        edx = 0;
    }
}

#[inline(always)]
unsafe fn draw_char_fasm(ctx: &mut DrawCharCtx, host: Option<GetPixelFn>, oracle: bool) {
    let color = ctx.color;
    let mut esi = ctx.multiplier;
    let mut edi = ctx.buffer;
    let mut ebx = ctx.glyph;
    let mut edx = ctx.edx_in;
    let mut rows = ctx.rows;
    let width_x = ctx.width_x;
    let delta = ctx.delta_to_screen;
    let smooth = ctx.smoothing as u8;
    let gp = ctx.getpixel;

    loop {
        edx = (edx & 0xFFFF_FF00) | u32::from(unsafe { *ebx });
        loop {
            if edx == 0 {
                break;
            }
            let bit = bsf32(edx);
            let mut eax = bit.wrapping_mul(esi);
            eax = eax.wrapping_shl(2);
            let saved = edi;
            edi = edi.wrapping_add(eax as usize);
            let mut ecx = esi;
            esi = esi.wrapping_sub(1);
            if esi != 0 {
                scaled_square_and_smooth(
                    &mut esi, &mut edi, &mut ecx, ebx, edx, color, width_x, saved, oracle,
                );
            } else {
                unsafe { store_pix(edi, color) };
                esi = esi.wrapping_add(1);
                if smooth != 0 {
                    smooth_left_1x(
                        ebx, edx, edi, color, delta, gp, host, smooth, oracle,
                    );
                    smooth_right_1x(
                        ebx, edx, edi, color, delta, gp, host, smooth, oracle,
                    );
                }
            }
            edx &= !(1u32 << bit);
            edi = saved;
        }
        ebx = ebx.wrapping_add(1);
        edi = edi.wrapping_add((width_x.wrapping_mul(esi)) as usize);
        rows = rows.wrapping_sub(1);
        if rows == 0 {
            break;
        }
        edx = 0;
    }
}

#[inline(always)]
unsafe fn smooth_left_1x(
    ebx: *const u8,
    edx: u32,
    edi: *mut u8,
    color: u32,
    delta: u32,
    gp: u32,
    host: Option<GetPixelFn>,
    smooth: u8,
    oracle: bool,
) {
    let mut eax = bsf32(edx);
    eax = eax.wrapping_sub(1);
    if (eax as i32) < 0 {
        return;
    }
    if unsafe { bt_mem(ebx, 0, eax) } {
        return;
    }
    eax = eax.wrapping_sub(1);
    if (eax as i32) >= 0 && unsafe { bt_mem(ebx, 0, eax) } {
        return;
    }
    eax = eax.wrapping_add(1);
    if unsafe { bt_mem(ebx, 1, eax) } {
        eax = eax.wrapping_add(1);
        if !unsafe { bt_mem(ebx, 1, eax) } {
            blend_left(edi, color, delta, gp, host, smooth, oracle);
            return;
        }
        if unsafe { bt_mem(ebx, -1, eax) } {
            return;
        }
        eax = eax.wrapping_sub(2);
        if (eax as i32) < 0 {
            blend_left(edi, color, delta, gp, host, smooth, oracle);
            return;
        }
        if !unsafe { bt_mem(ebx, 1, eax) } {
            blend_left(edi, color, delta, gp, host, smooth, oracle);
            return;
        }
        eax = eax.wrapping_add(1);
    }
    if !unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    eax = eax.wrapping_add(1);
    if !unsafe { bt_mem(ebx, -1, eax) } {
        blend_left(edi, color, delta, gp, host, smooth, oracle);
        return;
    }
    if unsafe { bt_mem(ebx, 1, eax) } {
        return;
    }
    eax = eax.wrapping_sub(2);
    if (eax as i32) < 0 {
        blend_left(edi, color, delta, gp, host, smooth, oracle);
        return;
    }
    if unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    blend_left(edi, color, delta, gp, host, smooth, oracle);
}

#[inline(always)]
unsafe fn smooth_right_1x(
    ebx: *const u8,
    edx: u32,
    edi: *mut u8,
    color: u32,
    delta: u32,
    gp: u32,
    host: Option<GetPixelFn>,
    smooth: u8,
    oracle: bool,
) {
    let mut eax = bsf32(edx);
    eax = eax.wrapping_add(1);
    if unsafe { bt_mem(ebx, 0, eax) } {
        return;
    }
    eax = eax.wrapping_add(1);
    if unsafe { bt_mem(ebx, 0, eax) } {
        return;
    }
    eax = eax.wrapping_sub(1);
    if unsafe { bt_mem(ebx, 1, eax) } {
        eax = eax.wrapping_sub(1);
        if !unsafe { bt_mem(ebx, 1, eax) } {
            blend_right(edi, color, delta, gp, host, smooth, oracle);
            return;
        }
        if unsafe { bt_mem(ebx, -1, eax) } {
            return;
        }
        eax = eax.wrapping_add(2);
        if !unsafe { bt_mem(ebx, 1, eax) } {
            blend_right(edi, color, delta, gp, host, smooth, oracle);
            return;
        }
        eax = eax.wrapping_sub(1);
    }
    if !unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    eax = eax.wrapping_sub(1);
    if !unsafe { bt_mem(ebx, -1, eax) } {
        blend_right(edi, color, delta, gp, host, smooth, oracle);
        return;
    }
    if unsafe { bt_mem(ebx, 1, eax) } {
        return;
    }
    eax = eax.wrapping_add(2);
    if unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    blend_right(edi, color, delta, gp, host, smooth, oracle);
}

#[inline(always)]
unsafe fn blend_left(
    edi: *mut u8,
    color: u32,
    delta: u32,
    gp: u32,
    host: Option<GetPixelFn>,
    smooth: u8,
    oracle: bool,
) {
    let mut eax = unsafe { neighbor_color(edi, delta, gp, host, -4) };
    if smooth == 1 {
        eax = anti_aliasing(eax, color);
    } else if oracle {
        eax = oracle_subpixel_left(eax, color);
    } else {
        eax = fasm_subpixel_left(eax, color);
    }
    unsafe { store_pix(edi.wrapping_offset(-4), eax) };
}

#[inline(always)]
unsafe fn blend_right(
    edi: *mut u8,
    color: u32,
    delta: u32,
    gp: u32,
    host: Option<GetPixelFn>,
    smooth: u8,
    oracle: bool,
) {
    let mut eax = unsafe { neighbor_color(edi, delta, gp, host, 4) };
    if smooth == 1 {
        eax = anti_aliasing(eax, color);
    } else if oracle {
        eax = oracle_subpixel_right(eax, color);
    } else {
        eax = fasm_subpixel_right(eax, color);
    }
    unsafe { store_pix(edi.wrapping_offset(4), eax) };
}

#[inline(always)]
unsafe fn scaled_square_and_smooth(
    esi: &mut u32,
    edi: &mut *mut u8,
    ecx: &mut u32,
    ebx: *const u8,
    edx: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
    _oracle: bool,
) {
    loop {
        unsafe { fill_eax_down(*edi, *esi as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        *ecx = ecx.wrapping_sub(1);
        if *ecx == 0 {
            break;
        }
    }
    *esi = esi.wrapping_add(1);
    *edi = saved;
    scaled_check_left(esi, edi, ebx, edx, color, width_x, saved);
    scaled_check_right(esi, edi, ebx, edx, color, width_x, saved);
}

#[inline(always)]
unsafe fn scaled_check_left(
    esi: &mut u32,
    edi: &mut *mut u8,
    ebx: *const u8,
    edx: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut eax = bsf32(edx);
    eax = eax.wrapping_sub(1);
    if (eax as i32) < 0 {
        return;
    }
    if unsafe { bt_mem(ebx, 0, eax) } {
        return;
    }
    if !unsafe { bt_mem(ebx, 1, eax) } {
        scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    let ecx_bit = eax;
    eax = eax.wrapping_add(1);
    if !unsafe { bt_mem(ebx, 1, eax) } {
        if !unsafe { bt_mem(ebx, -1, eax) } {
            scaled_down_right_low(esi, edi, ecx_bit, color, width_x, saved);
            scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
            return;
        }
        if unsafe { bt_mem(ebx, -2, eax) } {
            scaled_down_right_low(esi, edi, ecx_bit, color, width_x, saved);
            scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_sub(1);
        if unsafe { bt_mem(ebx, -1, eax) } {
            scaled_down_right_low(esi, edi, ecx_bit, color, width_x, saved);
            scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_sub(1);
        if (eax as i32) < 0 {
            scaled_down_right_high(esi, edi, ecx_bit, color, width_x, saved);
            scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
            return;
        }
        if unsafe { bt_mem(ebx, -2, eax) } {
            scaled_down_right_low(esi, edi, ecx_bit, color, width_x, saved);
            scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
            return;
        }
        scaled_down_right_high(esi, edi, ecx_bit, color, width_x, saved);
        scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    if unsafe { bt_mem(ebx, -1, eax) } {
        scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    eax = eax.wrapping_sub(2);
    if (eax as i32) < 0 {
        scaled_down_right_low(esi, edi, ecx_bit, color, width_x, saved);
        scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    if unsafe { bt_mem(ebx, 1, eax) } {
        scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    scaled_down_right_low(esi, edi, ecx_bit, color, width_x, saved);
    scaled_check_left_up(esi, edi, ebx, edx, color, width_x, saved);
}

#[inline(always)]
unsafe fn scaled_check_left_up(
    esi: &mut u32,
    edi: &mut *mut u8,
    ebx: *const u8,
    edx: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut eax = bsf32(edx);
    eax = eax.wrapping_sub(1);
    if !unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    let ecx_bit = eax;
    eax = eax.wrapping_add(1);
    if !unsafe { bt_mem(ebx, -1, eax) } {
        if unsafe { bt_mem(ebx, 1, eax) }
            && unsafe { bt_mem(ebx, 2, eax) }
        {
            scaled_up_right_low(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_sub(1);
        if unsafe { bt_mem(ebx, 1, eax) } {
            scaled_up_right_low(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_sub(1);
        if (eax as i32) < 0 {
            scaled_up_right_high(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        if unsafe { bt_mem(ebx, 2, eax) } {
            scaled_up_right_low(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        scaled_up_right_high(edi, ecx_bit, *esi, color, width_x, saved);
        return;
    }
    if unsafe { bt_mem(ebx, 1, eax) } {
        return;
    }
    eax = eax.wrapping_sub(2);
    if (eax as i32) < 0 {
        scaled_up_right_low(edi, ecx_bit, *esi, color, width_x, saved);
        return;
    }
    if unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    scaled_up_right_low(edi, ecx_bit, *esi, color, width_x, saved);
}

#[inline(always)]
unsafe fn scaled_down_right_low(
    esi: &mut u32,
    edi: &mut *mut u8,
    ecx_bit: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(*esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    *esi = esi.wrapping_sub(1);
    let eax = width_x.wrapping_mul(*esi);
    *edi = edi.wrapping_add(eax as usize);
    *edi = edi.wrapping_add(4);
    ecx = esi.wrapping_sub(1);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_sub(width_x as usize);
        *edi = edi.wrapping_add(4);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
    *esi = esi.wrapping_add(1);
    *edi = saved;
}

#[inline(always)]
unsafe fn scaled_down_right_high(
    esi: &mut u32,
    edi: &mut *mut u8,
    ecx_bit: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(*esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    *esi = esi.wrapping_sub(1);
    let eax = width_x.wrapping_mul(*esi);
    *edi = edi.wrapping_add(eax as usize);
    *edi = edi.wrapping_add(4);
    ecx = esi.wrapping_sub(1);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_sub(width_x as usize);
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_sub(width_x as usize);
        *edi = edi.wrapping_add(4);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
    *esi = esi.wrapping_add(1);
    *edi = saved;
}

#[inline(always)]
unsafe fn scaled_up_right_low(
    edi: &mut *mut u8,
    ecx_bit: u32,
    esi: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    *edi = edi.wrapping_add(4);
    ecx = esi.wrapping_sub(2);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        *edi = edi.wrapping_add(4);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
    *edi = saved;
}

#[inline(always)]
unsafe fn scaled_up_right_high(
    edi: &mut *mut u8,
    ecx_bit: u32,
    esi: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    *edi = edi.wrapping_add(4);
    ecx = esi.wrapping_sub(2);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        *edi = edi.wrapping_add(4);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
    *edi = saved;
}

#[inline(always)]
unsafe fn scaled_check_right(
    esi: &mut u32,
    edi: &mut *mut u8,
    ebx: *const u8,
    edx: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut eax = bsf32(edx);
    eax = eax.wrapping_add(1);
    if unsafe { bt_mem(ebx, 0, eax) } {
        return;
    }
    if !unsafe { bt_mem(ebx, 1, eax) } {
        scaled_check_right_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    let ecx_bit = eax;
    eax = eax.wrapping_sub(1);
    if !unsafe { bt_mem(ebx, 1, eax) } {
        if unsafe { bt_mem(ebx, -1, eax) }
            && unsafe { bt_mem(ebx, -2, eax) }
        {
            scaled_down_left_low(esi, edi, ecx_bit, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_add(1);
        if unsafe { bt_mem(ebx, -1, eax) } {
            scaled_down_left_low(esi, edi, ecx_bit, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_add(1);
        if unsafe { bt_mem(ebx, -2, eax) } {
            scaled_down_left_low(esi, edi, ecx_bit, color, width_x, saved);
            return;
        }
        scaled_down_left_high(esi, edi, ecx_bit, color, width_x, saved);
        return;
    }
    if unsafe { bt_mem(ebx, -1, eax) } {
        scaled_check_right_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    eax = eax.wrapping_add(2);
    if unsafe { bt_mem(ebx, 1, eax) } {
        scaled_check_right_up(esi, edi, ebx, edx, color, width_x, saved);
        return;
    }
    scaled_down_left_low(esi, edi, ecx_bit, color, width_x, saved);
}

#[inline(always)]
unsafe fn scaled_check_right_up(
    esi: &mut u32,
    edi: &mut *mut u8,
    ebx: *const u8,
    edx: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut eax = bsf32(edx);
    eax = eax.wrapping_add(1);
    if !unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    let ecx_bit = eax;
    eax = eax.wrapping_sub(1);
    if !unsafe { bt_mem(ebx, -1, eax) } {
        if unsafe { bt_mem(ebx, 1, eax) }
            && unsafe { bt_mem(ebx, 2, eax) }
        {
            scaled_up_left_low(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_add(1);
        if unsafe { bt_mem(ebx, 1, eax) } {
            scaled_up_left_low(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        eax = eax.wrapping_add(1);
        if unsafe { bt_mem(ebx, 2, eax) } {
            scaled_up_left_low(edi, ecx_bit, *esi, color, width_x, saved);
            return;
        }
        scaled_up_left_high(edi, ecx_bit, *esi, color, width_x, saved);
        return;
    }
    if unsafe { bt_mem(ebx, 1, eax) } {
        return;
    }
    eax = eax.wrapping_add(2);
    if unsafe { bt_mem(ebx, -1, eax) } {
        return;
    }
    scaled_up_left_low(edi, ecx_bit, *esi, color, width_x, saved);
}

#[inline(always)]
unsafe fn scaled_down_left_low(
    esi: &mut u32,
    edi: &mut *mut u8,
    ecx_bit: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(*esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    *esi = esi.wrapping_sub(1);
    let eax = width_x.wrapping_mul(*esi);
    *edi = edi.wrapping_add(eax as usize);
    ecx = esi.wrapping_sub(1);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_sub(width_x as usize);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
    *esi = esi.wrapping_add(1);
    *edi = saved;
}

#[inline(always)]
unsafe fn scaled_down_left_high(
    esi: &mut u32,
    edi: &mut *mut u8,
    ecx_bit: u32,
    color: u32,
    width_x: u32,
    saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(*esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    *esi = esi.wrapping_sub(1);
    let eax = width_x.wrapping_mul(*esi);
    *edi = edi.wrapping_add(eax as usize);
    ecx = esi.wrapping_sub(1);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_sub(width_x as usize);
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_sub(width_x as usize);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
    *esi = esi.wrapping_add(1);
    *edi = saved;
}

#[inline(always)]
unsafe fn scaled_up_left_low(
    edi: &mut *mut u8,
    ecx_bit: u32,
    esi: u32,
    color: u32,
    width_x: u32,
    _saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    ecx = esi.wrapping_sub(2);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
}

#[inline(always)]
unsafe fn scaled_up_left_high(
    edi: &mut *mut u8,
    ecx_bit: u32,
    esi: u32,
    color: u32,
    width_x: u32,
    _saved: *mut u8,
) {
    let mut ecx = ecx_bit.wrapping_mul(esi);
    ecx = ecx.wrapping_shl(2);
    *edi = edi.wrapping_add(ecx as usize);
    ecx = esi.wrapping_sub(2);
    loop {
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        unsafe { fill_eax_down(*edi, ecx as i32, color) };
        *edi = edi.wrapping_add(width_x as usize);
        if (ecx as i32) < 0 {
            break;
        }
        ecx = ecx.wrapping_sub(1);
        if (ecx as i32) < 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    fn mock_gp(_idx: u32) -> u32 {
        // Constant: host buffer addresses differ, so an index derived from
        // `edi+delta` must not be the oracle. Kernel getpixel uses LFB index.
        0x00CA_FEBA
    }

    fn run_both(
        glyph: &[u8],
        rows: u32,
        color: u32,
        mul: u32,
        pitch: u32,
        delta: u32,
        smooth: u32,
        edx_in: u32,
        gp: Option<GetPixelFn>,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut a = vec![0x11u8; 256 * 1024];
        let mut b = a.clone();
        let pad = 64usize;
        unsafe {
            let mut ca = DrawCharCtx {
                color,
                multiplier: mul,
                buffer: a.as_mut_ptr().add(pad),
                glyph: glyph.as_ptr().add(2),
                rows,
                width_x: pitch,
                delta_to_screen: delta,
                smoothing: smooth,
                getpixel: if gp.is_some() { 1 } else { 0 },
                edx_in,
            };
            let mut cb = ca;
            cb.buffer = b.as_mut_ptr().add(pad);
            draw_char(&mut ca, gp);
            fasm_oracle_draw_char(&mut cb, gp);
        }
        (a, b)
    }

    fn assert_eq_buf(a: &[u8], b: &[u8], msg: &str) {
        assert_eq!(a.len(), b.len(), "{msg} len");
        if a != b {
            for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                if x != y {
                    panic!("{msg} first diff at {i}: {x:#04x} vs {y:#04x}");
                }
            }
        }
    }

    #[test]
    fn dch_ctx_size() {
        assert_eq!(DRAW_CHAR_CTX_SIZE, 40);
        #[cfg(target_arch = "x86")]
        assert_eq!(core::mem::size_of::<DrawCharCtx>(), 40);
    }

    #[test]
    fn dch_empty_glyph_untouched() {
        let g = [0xAAu8, 0, 0, 0, 0, 0xBB];
        let (a, b) = run_both(&g, 3, 0x00FF_00FF, 1, 32, 0, 0, 0, None);
        assert_eq_buf(&a, &b, "empty");
        assert!(a.iter().all(|&x| x == 0x11), "empty must not plot");
    }

    #[test]
    fn dch_single_bit_no_smooth() {
        let mut g = [0u8; 8];
        g[2] = 0b0000_1000;
        let (a, b) = run_both(&g, 1, 0x00AB_CDEF, 1, 64, 0, 0, 0, None);
        assert_eq_buf(&a, &b, "bit3");
        let off = 64 + 3 * 4;
        let pix = u32::from_le_bytes(a[off..off + 4].try_into().unwrap());
        assert_eq!(pix, 0x00AB_CDEF);
    }

    #[test]
    fn dch_independent_unsmoothed_bits() {
        let mut g = [0u8; 20];
        g[2] = 0b1010_0101;
        g[3] = 0b0000_0001;
        let rows = 2u32;
        let color = 0x0011_2233;
        let pitch = 64u32;
        let (a, _) = run_both(&g, rows, color, 1, pitch, 0, 0, 0, None);
        for row in 0..rows as usize {
            let bits = g[2 + row];
            for bit in 0..8u32 {
                let off = 64 + row * pitch as usize + (bit as usize) * 4;
                let pix = u32::from_le_bytes(a[off..off + 4].try_into().unwrap());
                let expect = if bits & (1 << bit) != 0 { color } else { 0x1111_1111 };
                assert_eq!(pix, expect, "row {row} bit {bit}");
            }
        }
    }

    #[test]
    fn dch_dirty_edx_first_row() {
        let mut g = [0u8; 8];
        g[2] = 0x01;
        let (a, b) = run_both(&g, 1, 0x00FF_0000, 1, 64, 0, 0, 0x0000_0200, None);
        assert_eq_buf(&a, &b, "edx");
        let off = 64 + 9 * 4;
        let pix = u32::from_le_bytes(a[off..off + 4].try_into().unwrap());
        assert_eq!(pix, 0x00FF_0000, "bit 9 from DH");
    }

    #[test]
    fn dch_smoothing_off_skips_blend() {
        let mut g = [0u8; 8];
        g[2] = 0b0000_0010;
        let (a, b) = run_both(&g, 1, 0x00FF_FF00, 1, 64, 0, 0, 0, None);
        assert_eq_buf(&a, &b, "nosm");
        let left = u32::from_le_bytes(a[64..68].try_into().unwrap());
        assert_eq!(left, 0x1111_1111);
    }

    #[test]
    fn dch_aa_matches_cut_n_formula() {
        // Prev-row bit0 set so 1x .checkLeftUpSM / .checkRightUpSM take the blend.
        let mut g = [0u8; 8];
        g[1] = 0b0000_0001;
        g[2] = 0b0000_0010;
        g[3] = 0b0000_0100;
        let bg = 0x1111_1111u32;
        let fg = 0x00FF_0000u32;
        let (a, b) = run_both(&g, 1, fg, 1, 64, 0, 1, 0, None);
        assert_eq_buf(&a, &b, "aa");
        let left = u32::from_le_bytes(a[64..68].try_into().unwrap());
        assert_eq!(left, anti_aliasing(bg, fg));
    }

    #[test]
    fn dch_subpixel_independent_weights() {
        let mut g = [0u8; 8];
        g[1] = 0b0000_0001;
        g[2] = 0b0000_0010;
        g[3] = 0b0000_0100;
        let fg = 0x00AA_BBCC;
        let (a, b) = run_both(&g, 1, fg, 1, 64, 0, 2, 0, None);
        assert_eq_buf(&a, &b, "sp");
        let left = u32::from_le_bytes(a[64..68].try_into().unwrap());
        assert_eq!(left, oracle_subpixel_left(0x1111_1111, fg));
        let right = u32::from_le_bytes(a[72..76].try_into().unwrap());
        assert_eq!(right, oracle_subpixel_right(0x1111_1111, fg));
    }

    #[test]
    fn dch_subpixel_lea_matches_weights() {
        for n in [0u32, 1, 0x11, 0x00FF_FFFE, 0x00AA_BBCC, 0x1234_5678] {
            for f in [0u32, 0xFF, 0x00FF_0000, 0x00AA_BBCC, 0xFEDC_BA98] {
                assert_eq!(
                    fasm_subpixel_left(n, f),
                    oracle_subpixel_left(n, f),
                    "left n={n:#x} f={f:#x}"
                );
                assert_eq!(
                    fasm_subpixel_right(n, f),
                    oracle_subpixel_right(n, f),
                    "right n={n:#x} f={f:#x}"
                );
            }
        }
    }

    #[test]
    fn dch_multiplier_square() {
        let mut g = [0u8; 8];
        g[2] = 0b0000_0001;
        let (a, b) = run_both(&g, 1, 0x0000_00FF, 2, 64, 0, 0, 0, None);
        assert_eq_buf(&a, &b, "m2");
        for y in 0..2 {
            for x in 0..2 {
                let off = 64 + y * 64 + x * 4;
                let pix = u32::from_le_bytes(a[off..off + 4].try_into().unwrap());
                assert_eq!(pix, 0x0000_00FF, "m2 {x},{y}");
            }
        }
        // Isolated bit0: FASM js .checkRight / empty right neighbors → square only.
        for (i, ch) in a.chunks_exact(4).enumerate() {
            let pix = u32::from_le_bytes(ch.try_into().unwrap());
            let byte = i * 4;
            if byte < 64 {
                continue;
            }
            let rel = byte - 64;
            let row = rel / 64;
            let col = (rel % 64) / 4;
            if pix == 0x0000_00FF {
                assert!(row < 2 && col < 2, "unexpected plotted pixel dword {i} row={row} col={col}");
            }
        }
    }

    #[test]
    fn dch_nine_rows_cp866() {
        let mut g = [0u8; 16];
        for i in 0..9 {
            g[2 + i] = if i % 2 == 0 { 0x81 } else { 0 };
        }
        let (a, b) = run_both(&g, 9, 0x00C0_FFEE, 1, 48, 0, 0, 0, None);
        assert_eq_buf(&a, &b, "9row");
    }

    #[test]
    fn dch_getpixel_delta() {
        let mut g = [0u8; 8];
        g[1] = 0b0000_0001;
        g[2] = 0b0000_0010;
        g[3] = 0b0000_0100;
        let (a, b) = run_both(&g, 1, 0x00FF_FF00, 1, 64, 0x1000, 2, 0, Some(mock_gp));
        assert_eq_buf(&a, &b, "gp");
    }

    #[test]
    fn dch_full_row_no_edge_gap() {
        let mut g = [0u8; 8];
        g[2] = 0xFF;
        let (a, b) = run_both(&g, 1, 0x0000_FF00, 1, 64, 0, 2, 0, None);
        assert_eq_buf(&a, &b, "full");
        for bit in 0..8 {
            let off = 64 + bit * 4;
            let pix = u32::from_le_bytes(a[off..off + 4].try_into().unwrap());
            assert_eq!(pix, 0x0000_FF00);
        }
    }

    #[test]
    fn dch_bit0_bit7_edges() {
        let mut g = [0u8; 8];
        g[2] = 0x81;
        let (a, b) = run_both(&g, 1, 0x00DE_AD00, 1, 64, 0, 2, 0, None);
        assert_eq_buf(&a, &b, "edges");
    }

    #[test]
    fn dch_canary_around_buffer() {
        let mut g = [0u8; 8];
        g[2] = 0x10;
        let (a, b) = run_both(&g, 1, 0x00BE_EF00, 1, 32, 0, 0, 0, None);
        assert_eq_buf(&a, &b, "canary");
        assert_eq!(&a[0..8], &[0x11; 8]);
    }

    #[test]
    fn dch_prng_50k() {
        let mut rng = DRAW_CHAR_PRNG_SEED;
        for i in 0..50_000 {
            let rows = 1 + (xorshift32(&mut rng) % 16);
            let mul = 1;
            let smooth = xorshift32(&mut rng) % 3;
            let color = xorshift32(&mut rng) & 0x00FF_FFFF;
            let edx_in = if xorshift32(&mut rng) & 7 == 0 {
                xorshift32(&mut rng) & 0x0000_FF00
            } else {
                0
            };
            let delta = if xorshift32(&mut rng) & 3 == 0 {
                0x2000
            } else {
                0
            };
            let gp = if delta != 0 { Some(mock_gp as GetPixelFn) } else { None };
            let mut glyph = vec![0u8; rows as usize + 4];
            for b in glyph.iter_mut().skip(2).take(rows as usize) {
                *b = (xorshift32(&mut rng) & 0xFF) as u8;
            }
            let pitch = 128 + ((8 * mul as usize + 16) * 4) as u32;
            let (a, b) = run_both(
                &glyph, rows, color, mul, pitch, delta, smooth, edx_in, gp,
            );
            if a != b {
                let pos = a.iter().zip(b.iter()).position(|(x, y)| x != y);
                panic!(
                    "prng#{i} rows={rows} mul={mul} sm={smooth} d={delta:#x} edx={edx_in:#x} glyph={glyph:?} first_diff={pos:?}"
                );
            }
            if mul == 1 && smooth == 0 && edx_in == 0 && delta == 0 {
                let mut expect = vec![0x11u8; a.len()];
                let pitch_us = pitch as usize;
                for row in 0..rows as usize {
                    let bits = glyph[2 + row];
                    for bit in 0..8u32 {
                        if bits & (1 << bit) != 0 {
                            let off = 64 + row * pitch_us + (bit as usize) * 4;
                            expect[off..off + 4].copy_from_slice(&color.to_le_bytes());
                        }
                    }
                }
                if a != expect {
                    panic!("prng#{i} independent unsmoothed 1x mismatch");
                }
            }
        }
    }
}
