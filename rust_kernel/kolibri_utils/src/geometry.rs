//! Cut H: `block_clip` — clip a mutable RECT against a clip RECT.
//! Cut CD: `blit_clip` — dual `block_clip` compose + src/dst remap on `BLITTER`.
//!
//! Matches `kernel/video/blitter.inc` FASM leaf semantics (signed compares,
//! in-place mutate, reject via CF). No tables / `.rodata` — reloc-free friendly.

/// Cut CD PRNG seed (`'BLIT'`).
pub const BLIT_CLIP_PRNG_SEED: u32 = 0x424C_4954;

/// `BLITTER` layout offsets (`kernel/video/blitter.inc`).
pub const BLITTER_OFF_DC: usize = 0;
pub const BLITTER_OFF_SC: usize = 16;
pub const BLITTER_OFF_DST_X: usize = 32;
pub const BLITTER_OFF_DST_Y: usize = 36;
pub const BLITTER_OFF_SRC_X: usize = 40;
pub const BLITTER_OFF_SRC_Y: usize = 44;
pub const BLITTER_OFF_W: usize = 48;
pub const BLITTER_OFF_H: usize = 52;
pub const BLITTER_SIZE: usize = 64;

/// Axis-aligned rectangle: `{left, top, right, bottom}` as signed dwords
/// (KolibriOS `RECT` / `const.inc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    #[inline(always)]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Pack into the 16-byte little-endian memory layout FASM expects.
    #[inline(always)]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&self.left.to_le_bytes());
        b[4..8].copy_from_slice(&self.top.to_le_bytes());
        b[8..12].copy_from_slice(&self.right.to_le_bytes());
        b[12..16].copy_from_slice(&self.bottom.to_le_bytes());
        b
    }

    /// Parse from a 16-byte RECT block.
    #[inline(always)]
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        Self {
            left: i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            top: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            right: i32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            bottom: i32::from_le_bytes([b[12], b[13], b[14], b[15]]),
        }
    }
}

/// Result of [`block_clip`]: whether to draw, plus the (possibly mutated) rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockClipResult {
    /// `true` = draw (FASM CF=0); `false` = reject (FASM CF=1).
    pub draw: bool,
    pub rect: Rect,
}

/// Clip `rect` against `clip` (FASM `block_clip`).
///
/// Returns draw/reject and the rect after FASM-faithful mutation. On Y-axis
/// reject after X-axis clamping, the returned rect keeps the X mutations.
#[inline(always)]
pub fn block_clip(clip: Rect, mut rect: Rect) -> BlockClipResult {
    // X axis — signed compares matching FASM jge / jl / jle
    let left = rect.left;
    let right = rect.right;
    let clip_left = clip.left;
    let clip_right = clip.right;

    if left >= clip_right || right < clip_left {
        return BlockClipResult {
            draw: false,
            rect,
        };
    }
    if left < clip_left {
        rect.left = clip_left;
    }
    if right > clip_right {
        rect.right = clip_right;
    }

    // Y axis
    let top = rect.top;
    let bottom = rect.bottom;
    let clip_top = clip.top;
    let clip_bottom = clip.bottom;

    if top >= clip_bottom || bottom < clip_top {
        return BlockClipResult {
            draw: false,
            rect,
        };
    }
    if top < clip_top {
        rect.top = clip_top;
    }
    if bottom > clip_bottom {
        rect.bottom = clip_bottom;
    }

    BlockClipResult { draw: true, rect }
}

/// In-place clip via raw pointers (kernel `ESI`/`EDI` layout).
///
/// Returns `0` = draw (CF clear), `1` = reject (CF set) — trampoline maps these.
///
/// # Safety
/// `clip` must be readable for 16 bytes; `rect` must be readable/writable for 16 bytes.
#[inline(always)]
pub unsafe fn block_clip_ptr(clip: *const u8, rect: *mut u8) -> u32 {
    let mut clip_b = [0u8; 16];
    let mut rect_b = [0u8; 16];
    // SAFETY: caller guarantees readable clip / readable-writable rect (16 B).
    unsafe {
        core::ptr::copy_nonoverlapping(clip, clip_b.as_mut_ptr(), 16);
        core::ptr::copy_nonoverlapping(rect, rect_b.as_mut_ptr(), 16);
    }
    let r = block_clip(Rect::from_bytes(&clip_b), Rect::from_bytes(&rect_b));
    let out = r.rect.to_bytes();
    unsafe {
        core::ptr::copy_nonoverlapping(out.as_ptr(), rect, 16);
    }
    if r.draw {
        0
    } else {
        1
    }
}

/// Separately coded FASM-faithful host oracle (not a call through [`block_clip`]).
///
/// Mirrors `blitter.inc` control flow with signed compares and the same
/// partial-mutate-on-Y-fail behavior.
#[cfg(test)]
pub fn fasm_oracle_block_clip(clip: Rect, mut rect: Rect) -> BlockClipResult {
    // push ebx — irrelevant for pure oracle
    let eax = rect.left;
    let ebx = rect.right;
    let ecx = clip.left;
    let edx = clip.right;

    if eax >= edx {
        return BlockClipResult {
            draw: false,
            rect,
        };
    }
    if ebx < ecx {
        return BlockClipResult {
            draw: false,
            rect,
        };
    }
    if eax < ecx {
        rect.left = ecx;
    }
    if ebx > edx {
        rect.right = edx;
    }

    let eax = rect.top;
    let ebx = rect.bottom;
    let ecx = clip.top;
    let edx = clip.bottom;

    if eax >= edx {
        return BlockClipResult {
            draw: false,
            rect,
        };
    }
    if ebx < ecx {
        return BlockClipResult {
            draw: false,
            rect,
        };
    }
    if eax < ecx {
        rect.top = ecx;
    }
    if ebx > edx {
        rect.bottom = edx;
    }

    BlockClipResult { draw: true, rect }
}

/// Geometry fields of a KolibriOS `BLITTER` (dc/sc + src/dst/w/h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlitterGeom {
    pub dc: Rect,
    pub sc: Rect,
    pub dst_x: i32,
    pub dst_y: i32,
    pub src_x: i32,
    pub src_y: i32,
    pub w: i32,
    pub h: i32,
}

impl BlitterGeom {
    #[inline(always)]
    pub const fn new(
        dc: Rect,
        sc: Rect,
        dst_x: i32,
        dst_y: i32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
    ) -> Self {
        Self {
            dc,
            sc,
            dst_x,
            dst_y,
            src_x,
            src_y,
            w,
            h,
        }
    }

    /// Pack geometry into the first 56 bytes of a `BLITTER` (bitmap/stride left 0).
    #[inline(always)]
    pub fn to_bytes(self) -> [u8; BLITTER_SIZE] {
        let mut b = [0u8; BLITTER_SIZE];
        b[BLITTER_OFF_DC..BLITTER_OFF_DC + 16].copy_from_slice(&self.dc.to_bytes());
        b[BLITTER_OFF_SC..BLITTER_OFF_SC + 16].copy_from_slice(&self.sc.to_bytes());
        b[BLITTER_OFF_DST_X..BLITTER_OFF_DST_X + 4].copy_from_slice(&self.dst_x.to_le_bytes());
        b[BLITTER_OFF_DST_Y..BLITTER_OFF_DST_Y + 4].copy_from_slice(&self.dst_y.to_le_bytes());
        b[BLITTER_OFF_SRC_X..BLITTER_OFF_SRC_X + 4].copy_from_slice(&self.src_x.to_le_bytes());
        b[BLITTER_OFF_SRC_Y..BLITTER_OFF_SRC_Y + 4].copy_from_slice(&self.src_y.to_le_bytes());
        b[BLITTER_OFF_W..BLITTER_OFF_W + 4].copy_from_slice(&self.w.to_le_bytes());
        b[BLITTER_OFF_H..BLITTER_OFF_H + 4].copy_from_slice(&self.h.to_le_bytes());
        b
    }

    #[inline(always)]
    pub fn from_bytes(b: &[u8; BLITTER_SIZE]) -> Self {
        let mut dc_b = [0u8; 16];
        let mut sc_b = [0u8; 16];
        dc_b.copy_from_slice(&b[BLITTER_OFF_DC..BLITTER_OFF_DC + 16]);
        sc_b.copy_from_slice(&b[BLITTER_OFF_SC..BLITTER_OFF_SC + 16]);
        Self {
            dc: Rect::from_bytes(&dc_b),
            sc: Rect::from_bytes(&sc_b),
            dst_x: i32::from_le_bytes([
                b[BLITTER_OFF_DST_X],
                b[BLITTER_OFF_DST_X + 1],
                b[BLITTER_OFF_DST_X + 2],
                b[BLITTER_OFF_DST_X + 3],
            ]),
            dst_y: i32::from_le_bytes([
                b[BLITTER_OFF_DST_Y],
                b[BLITTER_OFF_DST_Y + 1],
                b[BLITTER_OFF_DST_Y + 2],
                b[BLITTER_OFF_DST_Y + 3],
            ]),
            src_x: i32::from_le_bytes([
                b[BLITTER_OFF_SRC_X],
                b[BLITTER_OFF_SRC_X + 1],
                b[BLITTER_OFF_SRC_X + 2],
                b[BLITTER_OFF_SRC_X + 3],
            ]),
            src_y: i32::from_le_bytes([
                b[BLITTER_OFF_SRC_Y],
                b[BLITTER_OFF_SRC_Y + 1],
                b[BLITTER_OFF_SRC_Y + 2],
                b[BLITTER_OFF_SRC_Y + 3],
            ]),
            w: i32::from_le_bytes([
                b[BLITTER_OFF_W],
                b[BLITTER_OFF_W + 1],
                b[BLITTER_OFF_W + 2],
                b[BLITTER_OFF_W + 3],
            ]),
            h: i32::from_le_bytes([
                b[BLITTER_OFF_H],
                b[BLITTER_OFF_H + 1],
                b[BLITTER_OFF_H + 2],
                b[BLITTER_OFF_H + 3],
            ]),
        }
    }
}

/// Result of [`blit_clip`]: draw/reject plus (possibly mutated) geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlitClipResult {
    /// `true` = draw (FASM CF=0); `false` = reject (FASM CF=1).
    pub draw: bool,
    pub geom: BlitterGeom,
}

/// Compose source/dest clips for a `BLITTER` (FASM `blit_clip`).
///
/// On reject, returns the input geometry unchanged (temps discarded). On draw,
/// updates `w`, `h`, `src_x`, `src_y`, `dst_x`, `dst_y` to the clipped region.
#[inline(always)]
pub fn blit_clip(mut b: BlitterGeom) -> BlitClipResult {
    let src0 = b.src_x;
    let sy0 = b.src_y;
    let src_rect = Rect::new(src0, sy0, src0.wrapping_add(b.w), sy0.wrapping_add(b.h));
    let src_clipped = block_clip(b.sc, src_rect);
    if !src_clipped.draw {
        return BlitClipResult { draw: false, geom: b };
    }
    let sx0 = src_clipped.rect.left;
    let sy0c = src_clipped.rect.top;
    let sx1 = src_clipped.rect.right;
    let sy1 = src_clipped.rect.bottom;

    // dx0 = dst_x + sx0 - src_x; dy0 = dst_y + sy0 - src_y
    let dx0 = b.dst_x.wrapping_add(sx0).wrapping_sub(src0);
    let dy0 = b.dst_y.wrapping_add(sy0c).wrapping_sub(sy0);
    // dx1 = (dst_x - src_x) + sx1 = dx0 - sx0 + sx1
    let dx1 = dx0.wrapping_sub(sx0).wrapping_add(sx1);
    let dy1 = dy0.wrapping_sub(sy0c).wrapping_add(sy1);

    let dst_rect = Rect::new(dx0, dy0, dx1, dy1);
    let dst_clipped = block_clip(b.dc, dst_rect);
    if !dst_clipped.draw {
        return BlitClipResult { draw: false, geom: b };
    }
    let dx0 = dst_clipped.rect.left;
    let dy0 = dst_clipped.rect.top;
    let dx1 = dst_clipped.rect.right;
    let dy1 = dst_clipped.rect.bottom;

    b.w = dx1.wrapping_sub(dx0);
    b.h = dy1.wrapping_sub(dy0);
    // src_x = src_x + dx0 - dst_x; src_y = src_y + dy0 - dst_y
    b.src_x = src0.wrapping_add(dx0).wrapping_sub(b.dst_x);
    b.src_y = sy0.wrapping_add(dy0).wrapping_sub(b.dst_y);
    b.dst_x = dx0;
    b.dst_y = dy0;

    BlitClipResult { draw: true, geom: b }
}

/// In-place `blit_clip` via raw `BLITTER*` (kernel `ECX` layout).
///
/// Returns `0` = draw, `1` = reject. Mutates only `dst_x/y`, `src_x/y`, `w`, `h`
/// on draw (matches FASM writeback); leaves `dc`/`sc`/bitmap/stride untouched.
///
/// # Safety
/// `blitter` must be readable/writable for [`BLITTER_SIZE`] bytes.
#[inline(always)]
pub unsafe fn blit_clip_ptr(blitter: *mut u8) -> u32 {
    let mut bytes = [0u8; BLITTER_SIZE];
    unsafe {
        core::ptr::copy_nonoverlapping(blitter, bytes.as_mut_ptr(), BLITTER_SIZE);
    }
    let r = blit_clip(BlitterGeom::from_bytes(&bytes));
    if r.draw {
        let g = r.geom;
        unsafe {
            core::ptr::copy_nonoverlapping(
                g.dst_x.to_le_bytes().as_ptr(),
                blitter.add(BLITTER_OFF_DST_X),
                4,
            );
            core::ptr::copy_nonoverlapping(
                g.dst_y.to_le_bytes().as_ptr(),
                blitter.add(BLITTER_OFF_DST_Y),
                4,
            );
            core::ptr::copy_nonoverlapping(
                g.src_x.to_le_bytes().as_ptr(),
                blitter.add(BLITTER_OFF_SRC_X),
                4,
            );
            core::ptr::copy_nonoverlapping(
                g.src_y.to_le_bytes().as_ptr(),
                blitter.add(BLITTER_OFF_SRC_Y),
                4,
            );
            core::ptr::copy_nonoverlapping(
                g.w.to_le_bytes().as_ptr(),
                blitter.add(BLITTER_OFF_W),
                4,
            );
            core::ptr::copy_nonoverlapping(
                g.h.to_le_bytes().as_ptr(),
                blitter.add(BLITTER_OFF_H),
                4,
            );
        }
        0
    } else {
        1
    }
}

/// Separately coded FASM-faithful host oracle for [`blit_clip`] (not via [`blit_clip`]).
#[cfg(test)]
pub fn fasm_oracle_blit_clip(mut b: BlitterGeom) -> BlitClipResult {
    let src_x = b.src_x;
    let src_y = b.src_y;
    let mut sx0 = src_x;
    let mut sy0 = src_y;
    let mut sx1 = src_x.wrapping_add(b.w);
    let mut sy1 = src_y.wrapping_add(b.h);

    // First block_clip against sc (mutate sx temps)
    let src_r = fasm_oracle_block_clip(b.sc, Rect::new(sx0, sy0, sx1, sy1));
    if !src_r.draw {
        return BlitClipResult { draw: false, geom: b };
    }
    sx0 = src_r.rect.left;
    sy0 = src_r.rect.top;
    sx1 = src_r.rect.right;
    sy1 = src_r.rect.bottom;

    let mut dx0 = b.dst_x.wrapping_add(sx0).wrapping_sub(src_x);
    let mut dy0 = b.dst_y.wrapping_add(sy0).wrapping_sub(src_y);
    let mut dx1 = dx0.wrapping_sub(sx0).wrapping_add(sx1);
    let mut dy1 = dy0.wrapping_sub(sy0).wrapping_add(sy1);

    let dst_r = fasm_oracle_block_clip(b.dc, Rect::new(dx0, dy0, dx1, dy1));
    if !dst_r.draw {
        return BlitClipResult { draw: false, geom: b };
    }
    dx0 = dst_r.rect.left;
    dy0 = dst_r.rect.top;
    dx1 = dst_r.rect.right;
    dy1 = dst_r.rect.bottom;

    b.w = dx1.wrapping_sub(dx0);
    b.h = dy1.wrapping_sub(dy0);
    b.src_x = src_x.wrapping_add(dx0).wrapping_sub(b.dst_x);
    b.src_y = src_y.wrapping_add(dy0).wrapping_sub(b.dst_y);
    b.dst_x = dx0;
    b.dst_y = dy0;

    BlitClipResult { draw: true, geom: b }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(l: i32, t: i32, ri: i32, b: i32) -> Rect {
        Rect::new(l, t, ri, b)
    }

    fn bg(
        dc: Rect,
        sc: Rect,
        dst_x: i32,
        dst_y: i32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
    ) -> BlitterGeom {
        BlitterGeom::new(dc, sc, dst_x, dst_y, src_x, src_y, w, h)
    }

    #[test]
    fn fully_inside_unchanged_draw() {
        let clip = r(0, 0, 100, 100);
        let rect = r(10, 20, 30, 40);
        let out = block_clip(clip, rect);
        assert!(out.draw);
        assert_eq!(out.rect, rect);
    }

    #[test]
    fn clamp_each_edge() {
        let clip = r(10, 10, 90, 90);
        let out = block_clip(clip, r(0, 0, 100, 100));
        assert!(out.draw);
        assert_eq!(out.rect, r(10, 10, 90, 90));
    }

    #[test]
    fn reject_left_of_clip() {
        let clip = r(50, 0, 100, 100);
        let rect = r(0, 10, 40, 20);
        let out = block_clip(clip, rect);
        assert!(!out.draw);
        assert_eq!(out.rect, rect); // X fail before mutate
    }

    #[test]
    fn reject_right_of_clip() {
        let clip = r(0, 0, 50, 100);
        let rect = r(60, 10, 80, 20);
        let out = block_clip(clip, rect);
        assert!(!out.draw);
        assert_eq!(out.rect, rect);
    }

    #[test]
    fn reject_above_clip() {
        let clip = r(0, 50, 100, 100);
        let rect = r(10, 0, 20, 40);
        let out = block_clip(clip, rect);
        assert!(!out.draw);
        assert_eq!(out.rect, rect); // Y fail; X needed no clamp
    }

    #[test]
    fn reject_below_clip() {
        let clip = r(0, 0, 100, 50);
        let rect = r(10, 60, 20, 80);
        let out = block_clip(clip, rect);
        assert!(!out.draw);
        assert_eq!(out.rect, rect);
    }

    #[test]
    fn y_fail_after_x_clamp_keeps_x_mutation() {
        let clip = r(10, 50, 90, 100);
        let rect = r(0, 0, 100, 40); // X overlaps and clamps; Y rejects
        let out = block_clip(clip, rect);
        assert!(!out.draw);
        assert_eq!(out.rect.left, 10);
        assert_eq!(out.rect.right, 90);
        assert_eq!(out.rect.top, 0);
        assert_eq!(out.rect.bottom, 40);
    }

    #[test]
    fn touching_right_edge_is_reject() {
        // left >= clip.right → fail (FASM jge)
        let clip = r(0, 0, 50, 50);
        let rect = r(50, 0, 60, 10);
        let out = block_clip(clip, rect);
        assert!(!out.draw);
    }

    #[test]
    fn touching_left_edge_with_overlap_draws() {
        // right == clip.left is NOT < clip.left → may still draw if extent > 0
        // FASM: `cmp right, clip.left` / `jl .fail` — equal does not fail
        let clip = r(50, 0, 100, 100);
        let rect = r(50, 10, 60, 20);
        let out = block_clip(clip, rect);
        assert!(out.draw);
        assert_eq!(out.rect, rect);
    }

    #[test]
    fn negative_coordinates() {
        let clip = r(-100, -100, 0, 0);
        let out = block_clip(clip, r(-150, -80, -20, -10));
        assert!(out.draw);
        assert_eq!(out.rect, r(-100, -80, -20, -10));
    }

    #[test]
    fn degenerate_zero_area_inside_draws() {
        // left==right inside clip: X checks pass (left < clip.right, right >= clip.left)
        let clip = r(0, 0, 100, 100);
        let rect = r(40, 40, 40, 50);
        let out = block_clip(clip, rect);
        assert!(out.draw);
        assert_eq!(out.rect, rect);
    }

    #[test]
    fn bytes_roundtrip_and_ptr() {
        let clip = r(5, 5, 50, 50);
        let rect = r(0, 0, 100, 100);
        let clip_b = clip.to_bytes();
        let mut rect_b = rect.to_bytes();
        let code = unsafe { block_clip_ptr(clip_b.as_ptr(), rect_b.as_mut_ptr()) };
        assert_eq!(code, 0);
        assert_eq!(Rect::from_bytes(&rect_b), r(5, 5, 50, 50));
    }

    #[test]
    fn ptr_reject_returns_one() {
        let clip = r(100, 100, 200, 200);
        let rect = r(0, 0, 10, 10);
        let clip_b = clip.to_bytes();
        let mut rect_b = rect.to_bytes();
        let code = unsafe { block_clip_ptr(clip_b.as_ptr(), rect_b.as_mut_ptr()) };
        assert_eq!(code, 1);
        assert_eq!(Rect::from_bytes(&rect_b), rect);
    }

    /// Differential: named + structured grids + PRNG vs FASM-faithful oracle.
    #[test]
    fn differential_oracle_corpus() {
        let named = [
            (r(0, 0, 100, 100), r(10, 20, 30, 40)),
            (r(10, 10, 90, 90), r(0, 0, 100, 100)),
            (r(50, 0, 100, 100), r(0, 10, 40, 20)),
            (r(0, 0, 50, 100), r(60, 10, 80, 20)),
            (r(0, 50, 100, 100), r(10, 0, 20, 40)),
            (r(0, 0, 100, 50), r(10, 60, 20, 80)),
            (r(10, 50, 90, 100), r(0, 0, 100, 40)),
            (r(0, 0, 50, 50), r(50, 0, 60, 10)),
            (r(50, 0, 100, 100), r(50, 10, 60, 20)),
            (r(-100, -100, 0, 0), r(-150, -80, -20, -10)),
            (r(0, 0, 100, 100), r(40, 40, 40, 50)),
            (r(i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2), r(-10, -10, 10, 10)),
        ];
        for (clip, rect) in named {
            assert_eq!(
                block_clip(clip, rect),
                fasm_oracle_block_clip(clip, rect),
                "named clip={clip:?} rect={rect:?}"
            );
        }

        // Structured grid over small signed coords (5^8 ≈ 390k cases)
        let coords = [-50i32, -1, 0, 25, 100];
        for &cl in &coords {
            for &ct in &coords {
                for &cr in &coords {
                    for &cb in &coords {
                        let clip = r(cl, ct, cr, cb);
                        for &rl in &coords {
                            for &rt in &coords {
                                for &rr in &coords {
                                    for &rb in &coords {
                                        let rect = r(rl, rt, rr, rb);
                                        assert_eq!(
                                            block_clip(clip, rect),
                                            fasm_oracle_block_clip(clip, rect),
                                            "grid clip={clip:?} rect={rect:?}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Deterministic PRNG corpus (seed documented for Cut H).
        const SEED: u32 = 0xC07_B10C; // "Cut H block"
        const CASES: u32 = 200_000;
        let mut state = SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..CASES {
            let a = next() as i32;
            let b = next() as i32;
            let c = next() as i32;
            let d = next() as i32;
            // Mix small and full-range coords
            let scale = (next() & 3) as i32;
            let map = |v: i32, s: i32| -> i32 {
                if s == 0 {
                    (v % 201) - 100
                } else if s == 1 {
                    (v % 2001) - 1000
                } else if s == 2 {
                    v >> 16
                } else {
                    v
                }
            };
            let clip = r(map(a, scale), map(b, scale), map(c, scale), map(d, scale));
            let e = next() as i32;
            let f = next() as i32;
            let g = next() as i32;
            let h = next() as i32;
            let rect = r(map(e, scale), map(f, scale), map(g, scale), map(h, scale));
            assert_eq!(
                block_clip(clip, rect),
                fasm_oracle_block_clip(clip, rect),
                "prng clip={clip:?} rect={rect:?}"
            );
        }
    }

    // ----- Cut CD: blit_clip -----

    #[test]
    fn blit_fully_inside_unchanged() {
        let b = bg(
            r(0, 0, 100, 100),
            r(0, 0, 100, 100),
            10,
            20,
            0,
            0,
            30,
            40,
        );
        let out = blit_clip(b);
        assert!(out.draw);
        assert_eq!(out.geom, b);
    }

    #[test]
    fn blit_src_clamp_remaps_dst() {
        // sc clips left 10px of src; dst should shift accordingly.
        let b = bg(r(0, 0, 200, 200), r(10, 0, 100, 100), 50, 60, 0, 0, 50, 40);
        let out = blit_clip(b);
        assert!(out.draw);
        assert_eq!(out.geom.src_x, 10);
        assert_eq!(out.geom.w, 40);
        assert_eq!(out.geom.dst_x, 60); // 50 + (10-0)
        assert_eq!(out.geom.dst_y, 60);
        assert_eq!(out.geom.h, 40);
    }

    #[test]
    fn blit_reject_src_outside_unchanged() {
        let b = bg(r(0, 0, 100, 100), r(50, 0, 100, 100), 0, 0, 0, 0, 40, 20);
        let out = blit_clip(b);
        assert!(!out.draw);
        assert_eq!(out.geom, b);
    }

    #[test]
    fn blit_reject_dst_outside_unchanged() {
        // src ok in sc; remapped dst fully outside dc.
        let b = bg(r(0, 0, 50, 50), r(0, 0, 100, 100), 100, 100, 0, 0, 20, 20);
        let out = blit_clip(b);
        assert!(!out.draw);
        assert_eq!(out.geom, b);
    }

    #[test]
    fn blit_clip_ptr_draw_and_reject() {
        let mut bytes = bg(
            r(0, 0, 100, 100),
            r(0, 0, 100, 100),
            10,
            10,
            0,
            0,
            20,
            20,
        )
        .to_bytes();
        // Plant non-zero bitmap/stride — must survive.
        bytes[56..60].copy_from_slice(&0xAABBCCDDu32.to_le_bytes());
        bytes[60..64].copy_from_slice(&0x11223344u32.to_le_bytes());
        let code = unsafe { blit_clip_ptr(bytes.as_mut_ptr()) };
        assert_eq!(code, 0);
        assert_eq!(&bytes[56..60], &0xAABBCCDDu32.to_le_bytes());
        assert_eq!(&bytes[60..64], &0x11223344u32.to_le_bytes());

        let mut rej = bg(r(0, 0, 10, 10), r(50, 50, 100, 100), 0, 0, 0, 0, 20, 20).to_bytes();
        let before = rej;
        let code = unsafe { blit_clip_ptr(rej.as_mut_ptr()) };
        assert_eq!(code, 1);
        assert_eq!(rej, before);
    }

    #[test]
    fn blit_clip_differential_oracle() {
        let named = [
            bg(r(0, 0, 100, 100), r(0, 0, 100, 100), 10, 20, 0, 0, 30, 40),
            bg(r(0, 0, 200, 200), r(10, 0, 100, 100), 50, 60, 0, 0, 50, 40),
            bg(r(0, 0, 100, 100), r(50, 0, 100, 100), 0, 0, 0, 0, 40, 20),
            bg(r(0, 0, 50, 50), r(0, 0, 100, 100), 100, 100, 0, 0, 20, 20),
            bg(r(10, 10, 90, 90), r(0, 0, 100, 100), 0, 0, 0, 0, 100, 100),
            bg(r(-50, -50, 50, 50), r(-100, -100, 100, 100), -20, -10, -30, -40, 80, 90),
        ];
        for b in named {
            assert_eq!(
                blit_clip(b),
                fasm_oracle_blit_clip(b),
                "named {b:?}"
            );
        }

        let mut state = BLIT_CLIP_PRNG_SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        const CASES: u32 = 50_000;
        for i in 0..CASES {
            let scale = (next() & 3) as i32;
            let map = |v: i32, s: i32| -> i32 {
                if s == 0 {
                    (v % 201) - 100
                } else if s == 1 {
                    (v % 2001) - 1000
                } else if s == 2 {
                    v >> 16
                } else {
                    v
                }
            };
            let vals: [i32; 12] = core::array::from_fn(|_| map(next() as i32, scale));
            // Ensure sc/dc have some extent variety; still allow degenerate.
            let b = bg(
                r(vals[0], vals[1], vals[2], vals[3]),
                r(vals[4], vals[5], vals[6], vals[7]),
                vals[8],
                vals[9],
                vals[10],
                vals[11],
                map(next() as i32, scale),
                map(next() as i32, scale),
            );
            assert_eq!(
                blit_clip(b),
                fasm_oracle_blit_clip(b),
                "prng case {i} {b:?}"
            );
        }
    }
}
