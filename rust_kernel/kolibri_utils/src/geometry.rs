//! Cut H: `block_clip` — clip a mutable RECT against a clip RECT.
//!
//! Matches `kernel/video/blitter.inc` FASM leaf semantics (signed compares,
//! in-place mutate, reject via CF). No tables / `.rodata` — reloc-free friendly.

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

#[cfg(test)]
mod tests {
    use super::*;

    fn r(l: i32, t: i32, ri: i32, b: i32) -> Rect {
        Rect::new(l, t, ri, b)
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
}
