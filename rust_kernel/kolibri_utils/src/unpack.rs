//! Cut CO: `unpack` — KPCK/LZMA + optional E8/E9 CALL/JMP filter.
//!
//! Matches `kernel/unpacker.inc`. This is **not** stock 5-byte LZMA:
//! * range init is a little-endian `lodsd` of 4 bytes (not 5-byte BE shift-in);
//! * match-literal uses FASM `lea [base+ecx*4+0x100*4]` with `CH=match_bit`
//!   (`256+symbol` / `512+symbol` planes inside `LZMA_LIT_SIZE=768`);
//! * `posState` is `dest_ptr & 3` (absolute pointer low bits);
//! * 32-bit wrapping `imul` for the range bound;
//! * E8/E9 filter uses FASM `shr ax,8`/`ror eax,16`/`xchg al,ah` (not `bswap`);
//! * flags method/E8 bits are tested on **AL** only.
//!
//! Probability table (`unpack.p`) is the existing ~31.2 KiB heap buffer
//! (7990 `u32` slots). Range/rep/prev-byte state is local — not FASM globals.

/// Cut CO differential PRNG seed (`'UPCK'`).
pub const UNPACK_PRNG_SEED: u32 = 0x5550_434B;

/// FASM `unpack.LZMA_BASE_SIZE` == `Literal` index.
pub const LZMA_BASE_SIZE: usize = 1846;
/// FASM `unpack.LZMA_LIT_SIZE`.
pub const LZMA_LIT_SIZE: usize = 768;
/// `lc + lp` with Kolibri fixed `lc=3`, `lp=0`.
pub const LC_LP: usize = 3;
/// Probability slots: `Literal + (LZMA_LIT_SIZE << (lc+lp))`.
pub const PROB_SLOTS: usize = LZMA_BASE_SIZE + (LZMA_LIT_SIZE << LC_LP);

const PB: u32 = 2;
const POS_STATE_MASK: u32 = (1 << PB) - 1;
const LC: u32 = 3;
const K_NUM_POS_BITS_MAX: u32 = 4;
const K_NUM_STATES: u32 = 12;
const K_NUM_LIT_STATES: u32 = 7;
const K_START_POS_MODEL_INDEX: u32 = 4;
const K_END_POS_MODEL_INDEX: u32 = 14;
const K_NUM_FULL_DISTANCES: u32 = 1 << (K_END_POS_MODEL_INDEX / 2);
const K_NUM_POS_SLOT_BITS: u32 = 6;
const K_NUM_LEN_TO_POS_STATES: u32 = 4;
const K_NUM_ALIGN_BITS: u32 = 4;
const K_ALIGN_TABLE_SIZE: u32 = 1 << K_NUM_ALIGN_BITS;
const K_MATCH_MIN_LEN: u32 = 2;
const K_TOP_VALUE: u32 = 1 << 24;
const K_BIT_MODEL_TOTAL: u32 = 1 << 11;
const K_NUM_MOVE_BITS: u32 = 5;
const K_LEN_NUM_LOW_BITS: u32 = 3;
const K_LEN_NUM_LOW_SYMBOLS: u32 = 1 << K_LEN_NUM_LOW_BITS;
const K_LEN_NUM_MID_BITS: u32 = 3;
const K_LEN_NUM_MID_SYMBOLS: u32 = 1 << K_LEN_NUM_MID_BITS;
const K_LEN_NUM_HIGH_BITS: u32 = 8;

const IS_MATCH: u32 = 0;
const IS_REP: u32 = IS_MATCH + (K_NUM_STATES << K_NUM_POS_BITS_MAX);
const IS_REP_G0: u32 = IS_REP + K_NUM_STATES;
const IS_REP_G1: u32 = IS_REP_G0 + K_NUM_STATES;
const IS_REP_G2: u32 = IS_REP_G1 + K_NUM_STATES;
const IS_REP_0_LONG: u32 = IS_REP_G2 + K_NUM_STATES;
const POS_SLOT: u32 = IS_REP_0_LONG + (K_NUM_STATES << K_NUM_POS_BITS_MAX);
const SPEC_POS: u32 = POS_SLOT + (K_NUM_LEN_TO_POS_STATES << K_NUM_POS_SLOT_BITS);
const ALIGN: u32 = SPEC_POS + K_NUM_FULL_DISTANCES - K_END_POS_MODEL_INDEX;
const L_ENCODER: u32 = ALIGN + K_ALIGN_TABLE_SIZE;
const K_NUM_LEN_PROBS: u32 = 2 + (16 << 3) + (16 << 3) + 256; // 514
const REP_L_ENCODER: u32 = L_ENCODER + K_NUM_LEN_PROBS;
const LITERAL: u32 = REP_L_ENCODER + K_NUM_LEN_PROBS;
const LEN_CHOICE: u32 = 0;
const LEN_CHOICE2: u32 = 1;
const LEN_LOW: u32 = 2;
const LEN_MID: u32 = LEN_LOW + (16 << 3);
const LEN_HIGH: u32 = LEN_MID + (16 << 3);

struct Rd {
    src: *const u8,
    code: u32,
    range: u32,
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}

#[inline(always)]
unsafe fn rd_init(src: *const u8) -> Rd {
    Rd {
        src: src.add(4),
        code: read_u32_le(src),
        range: 0xFFFF_FFFF,
    }
}

#[inline(always)]
unsafe fn rd_norm(rd: &mut Rd) {
    if rd.range < K_TOP_VALUE {
        rd.range <<= 8;
        rd.code <<= 8;
        let b = *rd.src;
        rd.src = rd.src.add(1);
        rd.code |= b as u32;
    }
}

/// FASM `RangeDecoderBitDecode`: bound = `(range >> 11) * prob` wrapping.
/// Returns 0/1. Updates `*prob`.
#[inline(always)]
unsafe fn decode_bit(rd: &mut Rd, prob: *mut u32) -> u32 {
    let p = *prob;
    let bound = (rd.range >> 11).wrapping_mul(p);
    if rd.code < bound {
        rd.range = bound;
        *prob = p.wrapping_add((K_BIT_MODEL_TOTAL.wrapping_sub(p)) >> K_NUM_MOVE_BITS);
        rd_norm(rd);
        0
    } else {
        rd.range = rd.range.wrapping_sub(bound);
        rd.code = rd.code.wrapping_sub(bound);
        *prob = p.wrapping_sub(p >> K_NUM_MOVE_BITS);
        rd_norm(rd);
        1
    }
}

#[inline(always)]
unsafe fn decode_bit_tree(rd: &mut Rd, probs: *mut u32, levels: u32) -> u32 {
    let mut symbol = 1u32;
    let mut n = levels;
    while n != 0 {
        let bit = decode_bit(rd, probs.add(symbol as usize));
        symbol = (symbol << 1) | bit;
        n = n.wrapping_sub(1);
    }
    symbol.wrapping_sub(1 << levels)
}

#[inline(always)]
unsafe fn decode_rev_bit_tree(rd: &mut Rd, probs: *mut u32, levels: u32) -> u32 {
    let mut symbol = 1u32;
    let mut acc = 0u32;
    let mut n = levels;
    while n != 0 {
        let bit = decode_bit(rd, probs.add(symbol as usize));
        symbol = (symbol << 1) | bit;
        acc = (acc >> 1) | (bit << 31);
        n = n.wrapping_sub(1);
    }
    acc >> (32u32.wrapping_sub(levels))
}

#[inline(always)]
unsafe fn decode_direct_bits(rd: &mut Rd, mut n: u32) -> u32 {
    let mut result = 0u32;
    while n != 0 {
        rd.range >>= 1;
        result <<= 1;
        if rd.code >= rd.range {
            rd.code = rd.code.wrapping_sub(rd.range);
            result |= 1;
        }
        rd_norm(rd);
        n = n.wrapping_sub(1);
    }
    result
}

#[inline(always)]
unsafe fn decode_len(rd: &mut Rd, enc: *mut u32, pos_state: u32) -> u32 {
    if decode_bit(rd, enc.add(LEN_CHOICE as usize)) == 0 {
        return decode_bit_tree(
            rd,
            enc.add((LEN_LOW + (pos_state << K_LEN_NUM_LOW_BITS)) as usize),
            K_LEN_NUM_LOW_BITS,
        );
    }
    if decode_bit(rd, enc.add(LEN_CHOICE2 as usize)) == 0 {
        return K_LEN_NUM_LOW_SYMBOLS.wrapping_add(decode_bit_tree(
            rd,
            enc.add((LEN_MID + (pos_state << K_LEN_NUM_MID_BITS)) as usize),
            K_LEN_NUM_MID_BITS,
        ));
    }
    K_LEN_NUM_LOW_SYMBOLS
        .wrapping_add(K_LEN_NUM_MID_SYMBOLS)
        .wrapping_add(decode_bit_tree(
            rd,
            enc.add(LEN_HIGH as usize),
            K_LEN_NUM_HIGH_BITS,
        ))
}

#[inline(always)]
unsafe fn decode_literal(rd: &mut Rd, probs: *mut u32) -> u8 {
    let mut symbol = 1u32;
    while symbol < 0x100 {
        let bit = decode_bit(rd, probs.add(symbol as usize));
        symbol = (symbol << 1) | bit;
    }
    symbol as u8
}

/// FASM `LzmaLiteralDecodeMatch`: `lea eax,[base+ecx*4+0x100*4]` with
/// `ecx = (match_bit << 8) | symbol` → planes `256+symbol` and `512+symbol`
/// (`LZMA_LIT_SIZE=768`). After a mismatch, continue on the unmatched plane.
#[inline(always)]
unsafe fn decode_literal_matched(rd: &mut Rd, probs: *mut u32, mut match_byte: u8) -> u8 {
    let mut symbol = 1u32;
    loop {
        let match_bit = (match_byte >> 7) & 1;
        match_byte <<= 1;
        let slot = 0x100 + ((match_bit as usize) << 8) + symbol as usize;
        let bit = decode_bit(rd, probs.add(slot));
        symbol = (symbol << 1) | bit;
        if symbol >= 0x100 {
            return symbol as u8;
        }
        if match_bit != (symbol & 1) as u8 {
            while symbol < 0x100 {
                let b = decode_bit(rd, probs.add(symbol as usize));
                symbol = (symbol << 1) | b;
            }
            return symbol as u8;
        }
    }
}

/// FASM `lodsd` + `shr ax,8` / `ror eax,16` / `xchg al,ah` (unpacker `.c1`/`.c2`).
/// This is **not** `bswap eax` — high byte of the dword is discarded by `shr ax,8`.
#[inline(always)]
fn fasm_load_rel32(raw_le: u32) -> u32 {
    let mut eax = (raw_le & 0xFFFF_0000) | ((raw_le & 0xFFFF) >> 8);
    eax = eax.rotate_right(16);
    (eax & 0xFFFF_0000) | ((eax & 0xFF) << 8) | ((eax >> 8) & 0xFF)
}

#[inline(always)]
unsafe fn apply_e8_c1(src: &mut *const u8, dest: *mut u8) {
    let count = read_u32_le(*src);
    *src = src.add(4);
    if count == 0 {
        return;
    }
    let dl = **src;
    let mut esi = dest;
    let mut left = count;
    loop {
        let al = *esi;
        esi = esi.add(1);
        let t = al.wrapping_sub(0xE8);
        if t > 1 {
            continue;
        }
        if *esi != dl {
            continue;
        }
        let raw = read_u32_le(esi);
        esi = esi.add(4);
        let mut eax = fasm_load_rel32(raw);
        eax = eax.wrapping_sub(esi as u32);
        eax = eax.wrapping_add(dest as u32);
        let p = esi.sub(4);
        *p = eax as u8;
        *p.add(1) = (eax >> 8) as u8;
        *p.add(2) = (eax >> 16) as u8;
        *p.add(3) = (eax >> 24) as u8;
        left = left.wrapping_sub(1);
        if left == 0 {
            break;
        }
    }
}

/// FASM `.c2` / `.ctr1` two-byte-Jcc + E8/E9 filter.
#[inline(always)]
unsafe fn apply_e8_ctr1(src: &mut *const u8, dest: *mut u8) {
    let count = read_u32_le(*src);
    *src = src.add(4);
    if count == 0 {
        return;
    }
    let dl = **src;
    let mut esi = dest;
    let mut left = count;
    'outer: loop {
        let mut al = *esi;
        esi = esi.add(1);
        loop {
            if al != 0x0F {
                break;
            }
            al = *esi;
            esi = esi.add(1);
            if al < 0x80 {
                continue;
            }
            if al < 0x90 {
                // @@: cmp [esi], dl
                if *esi != dl {
                    continue 'outer;
                }
                patch_rel32(&mut esi, dest);
                left = left.wrapping_sub(1);
                if left == 0 {
                    return;
                }
                continue 'outer;
            }
            break;
        }
        let t = al.wrapping_sub(0xE8);
        if t > 1 {
            continue 'outer;
        }
        if *esi != dl {
            continue 'outer;
        }
        patch_rel32(&mut esi, dest);
        left = left.wrapping_sub(1);
        if left == 0 {
            return;
        }
    }
}

#[inline(always)]
unsafe fn patch_rel32(esi: &mut *mut u8, dest: *mut u8) {
    let raw = read_u32_le(*esi as *const u8);
    *esi = esi.add(4);
    let mut eax = fasm_load_rel32(raw);
    eax = eax.wrapping_sub(*esi as u32);
    eax = eax.wrapping_add(dest as u32);
    let p = (*esi).sub(4);
    *p = eax as u8;
    *p.add(1) = (eax >> 8) as u8;
    *p.add(2) = (eax >> 16) as u8;
    *p.add(3) = (eax >> 24) as u8;
}

/// KPCK/LZMA unpack matching FASM `unpack`.
///
/// # Safety
/// `packed` must be readable for the consumed stream (header + LZMA + optional
/// filter metadata). `unpacked` must be writable for `dest_len` bytes from
/// `packed[4..8]` (and extra bytes if an E8/E9 filter scans past the payload).
/// `p` must be writable for [`PROB_SLOTS`] dwords.
#[inline(always)]
pub unsafe fn unpack(packed: *const u8, unpacked: *mut u8, p: *mut u32) {
    let flags = read_u32_le(packed.add(8));
    // FASM tests **AL** only (`and al,0xC0` / `and al,not 0xC0` / `test al,80h`).
    let fl = flags as u8;
    if fl & 0xC0 == 0xC0 {
        return;
    }
    if fl & !0xC0 != 1 {
        return;
    }
    let dest_len = read_u32_le(packed.add(4));
    let mut src = packed.add(12);
    lzma_unpack(src, &mut src, unpacked, dest_len, p);
    if (fl & 0x80) != 0 {
        apply_e8_ctr1(&mut src, unpacked);
    } else if (fl & 0x40) != 0 {
        apply_e8_c1(&mut src, unpacked);
    }
}

#[inline(always)]
unsafe fn lzma_unpack(
    src0: *const u8,
    src_out: &mut *const u8,
    dest: *mut u8,
    dest_len: u32,
    p: *mut u32,
) {
    let mut rd = rd_init(src0);
    let mut i = 0usize;
    while i < PROB_SLOTS {
        *p.add(i) = K_BIT_MODEL_TOTAL / 2;
        i = i.wrapping_add(1);
    }
    let limit = dest.add(dest_len as usize);
    let mut edi = dest;
    let mut state = 0u32;
    let mut prev = 0u8;
    let mut rep0 = 1u32;
    let mut rep1 = 1u32;
    let mut rep2 = 1u32;
    let mut rep3 = 1u32;

    while edi < limit {
        let pos_state = (edi as u32) & POS_STATE_MASK;
        let is_match = p.add((IS_MATCH + (state << K_NUM_POS_BITS_MAX) + pos_state) as usize);
        if decode_bit(&mut rd, is_match) == 0 {
            let lit_off = ((prev as u32) >> (8 - LC)).wrapping_mul(LZMA_LIT_SIZE as u32);
            let lit = p.add((LITERAL + lit_off) as usize);
            let al = if state < K_NUM_LIT_STATES {
                decode_literal(&mut rd, lit)
            } else {
                let match_b = *edi.sub(rep0 as usize);
                decode_literal_matched(&mut rd, lit, match_b)
            };
            *edi = al;
            edi = edi.add(1);
            prev = al;
            state = if state < 4 {
                0
            } else if state < 10 {
                state.wrapping_sub(3)
            } else {
                state.wrapping_sub(6)
            };
            continue;
        }

        if decode_bit(&mut rd, p.add((IS_REP + state) as usize)) == 0 {
            // new match
            // FASM .10: xchg shift — rep1←rep0, rep2←rep1, rep3←rep2; old rep3 dropped.
            let old0 = rep0;
            let old1 = rep1;
            let old2 = rep2;
            rep1 = old0;
            rep2 = old1;
            rep3 = old2;
            state = if state < 7 { 7 } else { 10 };
            let mut len = decode_len(&mut rd, p.add(L_ENCODER as usize), pos_state);
            let pos_slot_index = if len < K_NUM_LEN_TO_POS_STATES - 1 {
                len
            } else {
                K_NUM_LEN_TO_POS_STATES - 1
            };
            let slot = decode_bit_tree(
                &mut rd,
                p.add((POS_SLOT + (pos_slot_index << K_NUM_POS_SLOT_BITS)) as usize),
                K_NUM_POS_SLOT_BITS,
            );
            rep0 = slot;
            if slot >= K_START_POS_MODEL_INDEX {
                let mut dist = (2 | (slot & 1)) << ((slot >> 1) - 1);
                if slot < K_END_POS_MODEL_INDEX {
                    let extra = decode_rev_bit_tree(
                        &mut rd,
                        p.add((SPEC_POS + dist - slot) as usize),
                        (slot >> 1) - 1,
                    );
                    dist = dist.wrapping_add(extra);
                } else {
                    let direct = decode_direct_bits(&mut rd, (slot >> 1) - 1 - K_NUM_ALIGN_BITS);
                    dist = dist.wrapping_add(direct << K_NUM_ALIGN_BITS);
                    let align = decode_rev_bit_tree(
                        &mut rd,
                        p.add(ALIGN as usize),
                        K_NUM_ALIGN_BITS,
                    );
                    dist = dist.wrapping_add(align);
                }
                rep0 = dist;
            }
            rep0 = rep0.wrapping_add(1);
            if rep0 == 0 {
                break;
            }
            len = len.wrapping_add(K_MATCH_MIN_LEN);
            copy_match(edi, rep0, len);
            edi = edi.add(len as usize);
            prev = *edi.sub(1);
            continue;
        }

        if decode_bit(&mut rd, p.add((IS_REP_G0 + state) as usize)) == 0 {
            if decode_bit(
                &mut rd,
                p.add((IS_REP_0_LONG + (state << K_NUM_POS_BITS_MAX) + pos_state) as usize),
            ) == 0
            {
                // short rep
                state = if state < 7 { 9 } else { 11 };
                let al = *edi.sub(rep0 as usize);
                *edi = al;
                edi = edi.add(1);
                prev = al;
                continue;
            }
        } else if decode_bit(&mut rd, p.add((IS_REP_G1 + state) as usize)) == 0 {
            let t = rep1;
            rep1 = rep0;
            rep0 = t;
        } else if decode_bit(&mut rd, p.add((IS_REP_G2 + state) as usize)) == 0 {
            let t = rep2;
            rep2 = rep1;
            rep1 = rep0;
            rep0 = t;
        } else {
            let t = rep3;
            rep3 = rep2;
            rep2 = rep1;
            rep1 = rep0;
            rep0 = t;
        }
        state = if state < 7 { 8 } else { 11 };
        let len = decode_len(&mut rd, p.add(REP_L_ENCODER as usize), pos_state)
            .wrapping_add(K_MATCH_MIN_LEN);
        copy_match(edi, rep0, len);
        edi = edi.add(len as usize);
        prev = *edi.sub(1);
    }
    *src_out = rd.src;
}

#[inline(always)]
unsafe fn copy_match(edi: *mut u8, rep0: u32, len: u32) {
    let mut n = len;
    let mut d = edi;
    let mut s = edi.sub(rep0 as usize);
    while n != 0 {
        *d = *s;
        d = d.add(1);
        s = s.add(1);
        n = n.wrapping_sub(1);
    }
}

/// Pointer-friendly wrapper used by the stdcall FFI export.
///
/// # Safety
/// Same contract as [`unpack`].
#[inline(always)]
pub unsafe fn unpack_ptr(packed: u32, unpacked: u32, p: u32) {
    unpack(packed as *const u8, unpacked as *mut u8, p as *mut u32)
}

// -----------------------------------------------------------------------------
// Independent FASM-semantic oracle (test-only). Spec-structured RangeCoder
// type; not a helper-for-helper copy of the production call tree.
// -----------------------------------------------------------------------------

#[cfg(test)]
struct OracleCoder {
    src: *const u8,
    code: u32,
    range: u32,
}

#[cfg(test)]
impl OracleCoder {
    unsafe fn new(stream: *const u8) -> Self {
        Self {
            src: stream.add(4),
            code: u32::from_le_bytes([*stream, *stream.add(1), *stream.add(2), *stream.add(3)]),
            range: !0,
        }
    }

    unsafe fn renormalize(&mut self) {
        if self.range < (1 << 24) {
            self.range = self.range << 8;
            let shifted = self.code << 8;
            let nxt = *self.src;
            self.src = self.src.add(1);
            self.code = shifted | (nxt as u32);
        }
    }

    unsafe fn bit(&mut self, slot: *mut u32) -> bool {
        let prob = *slot;
        let bound = (self.range >> 11).wrapping_mul(prob);
        if self.code >= bound {
            self.range -= bound;
            self.code -= bound;
            *slot = prob - (prob >> 5);
            self.renormalize();
            true
        } else {
            self.range = bound;
            *slot = prob + ((2048 - prob) >> 5);
            self.renormalize();
            false
        }
    }

    unsafe fn tree(&mut self, base: *mut u32, bits: u32) -> u32 {
        let mut v = 1u32;
        for _ in 0..bits {
            let one = self.bit(base.add(v as usize));
            v = (v << 1) + u32::from(one);
        }
        v - (1 << bits)
    }

    unsafe fn rev_tree(&mut self, base: *mut u32, bits: u32) -> u32 {
        let mut v = 1u32;
        let mut out = 0u32;
        for i in 0..bits {
            let one = self.bit(base.add(v as usize));
            v = (v << 1) + u32::from(one);
            out |= u32::from(one) << i;
        }
        out
    }

    unsafe fn direct(&mut self, bits: u32) -> u32 {
        let mut acc = 0u32;
        for _ in 0..bits {
            self.range >>= 1;
            acc <<= 1;
            if self.code >= self.range {
                self.code -= self.range;
                acc |= 1;
            }
            self.renormalize();
        }
        acc
    }

    unsafe fn len(&mut self, enc: *mut u32, pos: u32) -> u32 {
        if !self.bit(enc.add(0)) {
            return self.tree(enc.add((2 + (pos << 3)) as usize), 3);
        }
        if !self.bit(enc.add(1)) {
            return 8 + self.tree(enc.add((130 + (pos << 3)) as usize), 3);
        }
        16 + self.tree(enc.add(258), 8)
    }

    unsafe fn lit(&mut self, probs: *mut u32) -> u8 {
        let mut s = 1u32;
        while s < 256 {
            let one = self.bit(probs.add(s as usize));
            s = (s << 1) + u32::from(one);
        }
        s as u8
    }

    unsafe fn lit_match(&mut self, probs: *mut u32, mut m: u8) -> u8 {
        let mut s = 1u32;
        loop {
            let mb = (m >> 7) & 1;
            m <<= 1;
            let one = self.bit(probs.add(256 + ((mb as usize) << 8) + s as usize));
            s = (s << 1) + u32::from(one);
            if s >= 256 {
                return s as u8;
            }
            if mb != (s as u8 & 1) {
                while s < 256 {
                    let b = self.bit(probs.add(s as usize));
                    s = (s << 1) + u32::from(b);
                }
                return s as u8;
            }
        }
    }
}

#[cfg(test)]
unsafe fn oracle_filter_c1(src: &mut *const u8, dest: *mut u8) {
    let n = u32::from_le_bytes([**src, *(*src).add(1), *(*src).add(2), *(*src).add(3)]);
    *src = src.add(4);
    if n == 0 {
        return;
    }
    let key = **src;
    let mut cur = dest;
    let mut remain = n;
    while remain != 0 {
        let b = *cur;
        cur = cur.add(1);
        if b != 0xE8 && b != 0xE9 {
            continue;
        }
        if *cur != key {
            continue;
        }
        let raw = u32::from_le_bytes([*cur, *cur.add(1), *cur.add(2), *cur.add(3)]);
        cur = cur.add(4);
        let patched = fasm_load_rel32(raw).wrapping_sub(cur as u32).wrapping_add(dest as u32);
        let bytes = patched.to_le_bytes();
        let q = cur.sub(4);
        q.copy_from_nonoverlapping(bytes.as_ptr(), 4);
        remain -= 1;
    }
}

#[cfg(test)]
unsafe fn oracle_filter_c2(src: &mut *const u8, dest: *mut u8) {
    let n = u32::from_le_bytes([**src, *(*src).add(1), *(*src).add(2), *(*src).add(3)]);
    *src = src.add(4);
    if n == 0 {
        return;
    }
    let key = **src;
    let mut cur = dest;
    let mut remain = n;
    while remain != 0 {
        let mut b = *cur;
        cur = cur.add(1);
        let mut two_byte_jcc = false;
        loop {
            if b != 0x0F {
                break;
            }
            b = *cur;
            cur = cur.add(1);
            if b < 0x80 {
                continue;
            }
            if b < 0x90 {
                two_byte_jcc = true;
                break;
            }
            break;
        }
        if two_byte_jcc {
            if *cur != key {
                continue;
            }
        } else if b != 0xE8 && b != 0xE9 {
            continue;
        } else if *cur != key {
            continue;
        }
        let raw = u32::from_le_bytes([*cur, *cur.add(1), *cur.add(2), *cur.add(3)]);
        cur = cur.add(4);
        let patched = fasm_load_rel32(raw).wrapping_sub(cur as u32).wrapping_add(dest as u32);
        let bytes = patched.to_le_bytes();
        let q = cur.sub(4);
        q.copy_from_nonoverlapping(bytes.as_ptr(), 4);
        remain -= 1;
    }
}

/// Independent FASM-semantic oracle. Compare dest bytes against [`unpack`].
#[cfg(test)]
pub unsafe fn unpack_fasm_oracle(packed: *const u8, unpacked: *mut u8, p: *mut u32) {
    let flags = u32::from_le_bytes([
        *packed.add(8),
        *packed.add(9),
        *packed.add(10),
        *packed.add(11),
    ]);
    let fl = flags as u8;
    if fl & 0xC0 == 0xC0 {
        return;
    }
    if fl & !0xC0 != 1 {
        return;
    }
    let dest_len = u32::from_le_bytes([
        *packed.add(4),
        *packed.add(5),
        *packed.add(6),
        *packed.add(7),
    ]);
    for i in 0..PROB_SLOTS {
        *p.add(i) = 1024;
    }
    let mut rc = OracleCoder::new(packed.add(12));
    let end = unpacked.add(dest_len as usize);
    let mut d = unpacked;
    let mut st = 0u32;
    let mut prev = 0u8;
    let mut r0 = 1u32;
    let mut r1 = 1u32;
    let mut r2 = 1u32;
    let mut r3 = 1u32;
    while d < end {
        let ps = (d as u32) & 3;
        let match_slot = p.add((st * 16 + ps) as usize);
        if !rc.bit(match_slot) {
            let lit = p.add((LITERAL as usize) + (((prev as u32) >> 5) as usize) * LZMA_LIT_SIZE);
            let b = if st < 7 {
                rc.lit(lit)
            } else {
                rc.lit_match(lit, *d.sub(r0 as usize))
            };
            *d = b;
            d = d.add(1);
            prev = b;
            st = if st < 4 {
                0
            } else if st < 10 {
                st - 3
            } else {
                st - 6
            };
            continue;
        }
        if !rc.bit(p.add((IS_REP + st) as usize)) {
            let old0 = r0;
            let old1 = r1;
            let old2 = r2;
            r1 = old0;
            r2 = old1;
            r3 = old2;
            st = if st < 7 { 7 } else { 10 };
            let mut ln = rc.len(p.add(L_ENCODER as usize), ps);
            let psi = if ln < 3 { ln } else { 3 };
            let slot = rc.tree(p.add((POS_SLOT + (psi << 6)) as usize), 6);
            r0 = slot;
            if slot >= 4 {
                let mut dist = (2 | (slot & 1)) << ((slot >> 1) - 1);
                if slot < 14 {
                    dist += rc.rev_tree(p.add((SPEC_POS + dist - slot) as usize), (slot >> 1) - 1);
                } else {
                    dist += rc.direct((slot >> 1) - 1 - 4) << 4;
                    dist += rc.rev_tree(p.add(ALIGN as usize), 4);
                }
                r0 = dist;
            }
            r0 = r0.wrapping_add(1);
            if r0 == 0 {
                break;
            }
            ln += 2;
            let mut i = 0u32;
            while i < ln {
                *d.add(i as usize) = *d.sub(r0 as usize).add(i as usize);
                i += 1;
            }
            d = d.add(ln as usize);
            prev = *d.sub(1);
            continue;
        }
        if !rc.bit(p.add((IS_REP_G0 + st) as usize)) {
            if !rc.bit(p.add((IS_REP_0_LONG + st * 16 + ps) as usize)) {
                st = if st < 7 { 9 } else { 11 };
                let b = *d.sub(r0 as usize);
                *d = b;
                d = d.add(1);
                prev = b;
                continue;
            }
        } else if !rc.bit(p.add((IS_REP_G1 + st) as usize)) {
            let t = r1;
            r1 = r0;
            r0 = t;
        } else if !rc.bit(p.add((IS_REP_G2 + st) as usize)) {
            let t = r2;
            r2 = r1;
            r1 = r0;
            r0 = t;
        } else {
            let t = r3;
            r3 = r2;
            r2 = r1;
            r1 = r0;
            r0 = t;
        }
        st = if st < 7 { 8 } else { 11 };
        let ln = rc.len(p.add(REP_L_ENCODER as usize), ps) + 2;
        let mut i = 0u32;
        while i < ln {
            *d.add(i as usize) = *d.sub(r0 as usize).add(i as usize);
            i += 1;
        }
        d = d.add(ln as usize);
        prev = *d.sub(1);
    }
    let mut src = rc.src;
    if fl & 0x80 != 0 {
        oracle_filter_c2(&mut src, unpacked);
    } else if fl & 0x40 != 0 {
        oracle_filter_c1(&mut src, unpacked);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_both(packed: &mut [u8], dest_len: u32) -> (Vec<u8>, Vec<u8>) {
        // Prefix pad so match copies with small distances stay in-buffer.
        const PAD: usize = 8192;
        let cap = PAD + dest_len as usize + 512;
        let mut a = vec![0xA5u8; cap];
        let mut b = vec![0xA5u8; cap];
        let mut pa = vec![0u32; PROB_SLOTS];
        let mut pb = vec![0u32; PROB_SLOTS];
        unsafe {
            unpack(packed.as_ptr(), a.as_mut_ptr().add(PAD), pa.as_mut_ptr());
            unpack_fasm_oracle(packed.as_ptr(), b.as_mut_ptr().add(PAD), pb.as_mut_ptr());
        }
        (a, b)
    }

    fn hdr(dest_len: u32, flags: u32, rest: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(16 + rest.len() + 512);
        v.extend_from_slice(b"KPCK");
        v.extend_from_slice(&dest_len.to_le_bytes());
        v.extend_from_slice(&flags.to_le_bytes());
        v.extend_from_slice(rest);
        v.resize(v.len() + 512, 0);
        v
    }

    #[test]
    fn upck_flag_c0_leaves_dest() {
        let mut packed = hdr(8, 0xC0, &[0u8; 32]);
        let (a, b) = run_both(&mut packed, 8);
        assert_eq!(a[..8], [0xA5; 8]);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_method_not_lzma_leaves_dest() {
        let mut packed = hdr(8, 0x02, &[0u8; 32]);
        let (a, b) = run_both(&mut packed, 8);
        assert_eq!(a[..8], [0xA5; 8]);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_dest_len_zero_agrees() {
        let mut packed = hdr(0, 1, &[0u8; 64]);
        let (a, b) = run_both(&mut packed, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_dest_len_one_agrees() {
        let mut packed = hdr(1, 1, &[0u8; 64]);
        let (a, b) = run_both(&mut packed, 1);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_aligned_dest_random_stream() {
        let mut packed = hdr(16, 1, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        packed.extend_from_slice(&[0u8; 256]);
        let (a, b) = run_both(&mut packed, 16);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_e8_c1_count_zero() {
        // LZMA dest_len=0 → ESI at packed+16; count dword 0 → no-op filter.
        let mut packed = hdr(0, 1 | 0x40, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let (a, b) = run_both(&mut packed, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_e8_ctr1_count_zero() {
        let mut packed = hdr(0, 1 | 0x80, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let (a, b) = run_both(&mut packed, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn upck_e8_c1_patches_call() {
        // dest_len=0 so LZMA consumes only lodsd; filter metadata starts at +16.
        // Unpacked buffer is scanned from dest; plant E8 + key + disp.
        let mut packed = hdr(0, 1 | 0x40, &[]);
        // After header 12 bytes + lodsd 4 bytes, ESI at offset 16.
        // Ensure packed is long enough for lodsd during lzma init.
        packed.resize(64, 0);
        packed[8] = 1 | 0x40;
        packed[16] = 1; // count = 1
        packed[17] = 0;
        packed[18] = 0;
        packed[19] = 0;
        packed[20] = 0xAB; // key
        let dest_len = 0u32;
        let cap = 32usize;
        let mut dest_a = vec![0u8; cap];
        dest_a[0] = 0xE8;
        dest_a[1] = 0xAB;
        dest_a[2..6].copy_from_slice(&0x11223344u32.to_le_bytes());
        let mut dest_b = dest_a.clone();
        let mut pa = vec![0u32; PROB_SLOTS];
        let mut pb = vec![0u32; PROB_SLOTS];
        unsafe {
            unpack(packed.as_ptr(), dest_a.as_mut_ptr(), pa.as_mut_ptr());
            unpack_fasm_oracle(packed.as_ptr(), dest_b.as_mut_ptr(), pb.as_mut_ptr());
        }
        assert_eq!(dest_a, dest_b);
        assert_eq!(dest_a[0], 0xE8);
        let _ = dest_len;
    }

    #[test]
    fn upck_prng_50k_matches_oracle() {
        let mut state = UNPACK_PRNG_SEED;
        let mut packed = vec![0u8; 4096];
        let mut dest_a = vec![0xA5u8; 8192 + 48 + 512];
        let mut dest_b = vec![0xA5u8; 8192 + 48 + 512];
        let mut pa = vec![0u32; PROB_SLOTS];
        let mut pb = vec![0u32; PROB_SLOTS];
        const PAD: usize = 8192;
        for case in 0..50_000u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let dest_len = state % 49; // 0..48
            let method_ok = (state >> 8) & 7 != 0;
            let flags = if !method_ok {
                if (state >> 16) & 1 == 0 {
                    0xC0
                } else {
                    2 + ((state >> 20) & 0x3D)
                }
            } else {
                1 // LZMA, no E8/E9 (unbounded scan)
            };
            // Random LZMA bodies can emit huge match distances; keep the
            // success path at dest_len=0 (init only). dest_len>0 uses fail flags.
            let dest_len = if flags == 1 { 0 } else { dest_len };
            packed[..4].copy_from_slice(b"KPCK");
            packed[4..8].copy_from_slice(&dest_len.to_le_bytes());
            packed[8..12].copy_from_slice(&flags.to_le_bytes());
            for i in 12..packed.len() {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                packed[i] = state as u8;
            }
            dest_a.fill(0xA5);
            dest_b.fill(0xA5);
            unsafe {
                unpack(
                    packed.as_ptr(),
                    dest_a.as_mut_ptr().add(PAD),
                    pa.as_mut_ptr(),
                );
                unpack_fasm_oracle(
                    packed.as_ptr(),
                    dest_b.as_mut_ptr().add(PAD),
                    pb.as_mut_ptr(),
                );
            }
            assert_eq!(
                dest_a, dest_b,
                "case {case} dest_len={dest_len} flags={flags:#x}"
            );
        }
    }

    /// REG-015: production KPCK (LAUNCHER, flags 0x81) must decode to MENUET01.
    #[test]
    fn upck_real_launcher_kpck() {
        let packed = include_bytes!("../testdata/launcher.kpck");
        assert_eq!(&packed[..4], b"KPCK");
        let dest_len = u32::from_le_bytes(packed[4..8].try_into().unwrap());
        let flags = u32::from_le_bytes(packed[8..12].try_into().unwrap());
        assert_eq!(flags, 0x81);
        const PAD: usize = 8192;
        let cap = PAD + dest_len as usize + 512;
        let mut a = vec![0xA5u8; cap];
        let mut b = vec![0xA5u8; cap];
        let mut pa = vec![0u32; PROB_SLOTS];
        let mut pb = vec![0u32; PROB_SLOTS];
        unsafe {
            unpack(packed.as_ptr(), a.as_mut_ptr().add(PAD), pa.as_mut_ptr());
            unpack_fasm_oracle(packed.as_ptr(), b.as_mut_ptr().add(PAD), pb.as_mut_ptr());
        }
        assert_eq!(a, b);
        assert_eq!(&a[PAD..PAD + 8], b"MENUET01");
    }

    #[test]
    fn upck_fasm_load_rel32_is_not_bswap() {
        // FASM: shr ax,8 / ror eax,16 / xchg al,ah on 0x12345678 → 0x00563412.
        assert_eq!(fasm_load_rel32(0x1234_5678), 0x0056_3412);
        assert_ne!(fasm_load_rel32(0x1234_5678), 0x1234_5678u32.swap_bytes());
    }

    #[test]
    fn upck_flags_high_bytes_ignored() {
        // FASM checks AL only; 0x1000081 is method=1 + ctr1 (SDHCI.SYS class).
        let mut packed = hdr(0, 0x0100_0081, &[0u8; 64]);
        packed[16..20].copy_from_slice(&0u32.to_le_bytes()); // E8 count 0
        let (a, b) = run_both(&mut packed, 0);
        assert_eq!(a, b);
    }

    /// REG-016: @TASKBAR flags 0x81 with a non-zero E8/Jcc count must match FASM.
    #[test]
    fn upck_real_taskbar_kpck() {
        let packed = include_bytes!("../testdata/taskbar.kpck");
        assert_eq!(&packed[..4], b"KPCK");
        let dest_len = u32::from_le_bytes(packed[4..8].try_into().unwrap());
        let flags = u32::from_le_bytes(packed[8..12].try_into().unwrap());
        assert_eq!(flags, 0x81);
        const PAD: usize = 8192;
        let cap = PAD + dest_len as usize + 512;
        let mut a = vec![0xA5u8; cap];
        let mut b = vec![0xA5u8; cap];
        let mut pa = vec![0u32; PROB_SLOTS];
        let mut pb = vec![0u32; PROB_SLOTS];
        unsafe {
            unpack(packed.as_ptr(), a.as_mut_ptr().add(PAD), pa.as_mut_ptr());
            unpack_fasm_oracle(packed.as_ptr(), b.as_mut_ptr().add(PAD), pb.as_mut_ptr());
        }
        assert_eq!(a, b);
        assert_eq!(&a[PAD..PAD + 8], b"MENUET01");
    }
}
