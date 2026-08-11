//! Cut AT: `get_coff_sym` — PE/COFF symbol name → Value lookup.
//!
//! Matches `kernel/core/dll.inc` FASM leaf semantics:
//! * Do-while walk over `count` symbols of size [`COFF_SYM_SIZE`] (18)
//! * Each step: `strncmp(pSym, sz_sym, 8)` (NUL-aware, unsigned byte order)
//! * On match (`EAX==0`): return `[pSym + COFF_SYM.Value]` (offset 8)
//! * On exhaustion: return `0`
//!
//! Legacy quirk: initial `count == 0` still executes one compare, then
//! `dec` wraps to `0xFFFFFFFF` and continues (FASM `dec`/`jnz`). Host
//! differentials bound this case; production callers pass `nSymbols > 0`.
//!
//! Inline 8-byte name compare (do **not** call `rust_strncmp` — reloc-free).
//! No tables / `.rodata` / globals.

/// Cut AT differential PRNG seed (`'CUTT'`).
pub const GET_COFF_SYM_PRNG_SEED: u32 = 0x4355_5454;

/// `sizeof.COFF_SYM` (Name 8 + Value 4 + SectionNumber 2 + Type 2 + StorageClass 1 + NumAux 1).
pub const COFF_SYM_SIZE: usize = 18;

/// `sizeof.COFF_SYM.Name` / strncmp length.
pub const COFF_SYM_NAME_LEN: u32 = 8;

/// `COFF_SYM.Value` offset.
pub const OFF_SYM_VALUE: usize = 8;

/// Independent FASM-flow name equality (`strncmp(..., 8) == 0`).
#[inline(always)]
pub fn name_eq_n(a: &[u8], b: &[u8], n: u32) -> bool {
    if n == 0 {
        return true;
    }
    let mut i = 0u32;
    loop {
        let ca = a[i as usize];
        let cb = b[i as usize];
        if ca != cb {
            return false;
        }
        if ca == 0 {
            return true;
        }
        i = i.wrapping_add(1);
        if i == n {
            return true;
        }
    }
}

/// Pointer form of [`name_eq_n`] for freestanding walk.
///
/// # Safety
/// `a` and `b` must be readable for up to `n` bytes or until a shared NUL.
#[inline(always)]
unsafe fn name_eq_n_ptr(a: *const u8, b: *const u8, n: u32) -> bool {
    if n == 0 {
        return true;
    }
    let mut i = 0u32;
    loop {
        // SAFETY: within strncmp(n) contract.
        let ca = unsafe { *a.add(i as usize) };
        let cb = unsafe { *b.add(i as usize) };
        if ca != cb {
            return false;
        }
        if ca == 0 {
            return true;
        }
        i = i.wrapping_add(1);
        if i == n {
            return true;
        }
    }
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// FASM-faithful COFF symbol Value lookup.
///
/// Returns the `Value` dword of the first symbol whose 8-byte name matches
/// `sz_sym` under strncmp semantics, or `0` if none match within `count`
/// iterations (with count=0 wrap quirk as above).
///
/// # Safety
/// `p_sym` must address a readable `COFF_SYM` array for the walked range;
/// `sz_sym` must be readable for up to 8 bytes / until NUL.
#[inline(always)]
pub unsafe fn get_coff_sym(mut p_sym: *const u8, mut count: u32, sz_sym: *const u8) -> u32 {
    loop {
        // stdcall strncmp, [pSym], [sz_sym], 8 / test eax,eax / jz .ok
        if unsafe { name_eq_n_ptr(p_sym, sz_sym, COFF_SYM_NAME_LEN) } {
            // mov eax,[pSym] / mov eax,[eax+COFF_SYM.Value]
            return unsafe { read_u32_le(p_sym.add(OFF_SYM_VALUE)) };
        }
        // add [pSym], sizeof.COFF_SYM
        p_sym = unsafe { p_sym.add(COFF_SYM_SIZE) };
        // dec [count] / jnz @b
        count = count.wrapping_sub(1);
        if count == 0 {
            // xor eax, eax / ret
            return 0;
        }
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`get_coff_sym`].
#[inline(always)]
pub unsafe fn get_coff_sym_ptr(p_sym: *const u8, count: u32, sz_sym: *const u8) -> u32 {
    unsafe { get_coff_sym(p_sym, count, sz_sym) }
}

/// Independent FASM-flow oracle over a packed symbol table buffer.
///
/// `table` is `count_cap * 18` bytes of synthetic `COFF_SYM` records.
/// Walks exactly as FASM for `count` in `1..=count_cap`. Does **not**
/// exercise the `count == 0` wrap quirk (would overrun the buffer).
pub fn get_coff_sym_oracle(table: &[u8], count: u32, sz_sym: &[u8]) -> u32 {
    assert!(sz_sym.len() >= COFF_SYM_NAME_LEN as usize);
    assert!(count > 0, "oracle excludes count==0 FASM wrap quirk");
    let need = (count as usize).saturating_mul(COFF_SYM_SIZE);
    assert!(table.len() >= need);

    let mut off = 0usize;
    let mut remaining = count;
    loop {
        let name = &table[off..off + COFF_SYM_NAME_LEN as usize];
        if name_eq_n(name, &sz_sym[..COFF_SYM_NAME_LEN as usize], COFF_SYM_NAME_LEN) {
            let v = &table[off + OFF_SYM_VALUE..off + OFF_SYM_VALUE + 4];
            return u32::from_le_bytes([v[0], v[1], v[2], v[3]]);
        }
        off += COFF_SYM_SIZE;
        remaining = remaining.wrapping_sub(1);
        if remaining == 0 {
            return 0;
        }
    }
}

/// Build one synthetic `COFF_SYM` (18 bytes) with 8-byte name + Value.
pub fn make_sym(name: &[u8], value: u32) -> [u8; COFF_SYM_SIZE] {
    let mut s = [0u8; COFF_SYM_SIZE];
    let n = core::cmp::min(name.len(), COFF_SYM_NAME_LEN as usize);
    s[..n].copy_from_slice(&name[..n]);
    s[OFF_SYM_VALUE..OFF_SYM_VALUE + 4].copy_from_slice(&value.to_le_bytes());
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string::strncmp_fasm_oracle;

    fn rust_vs(table: &[u8], count: u32, sz: &[u8]) -> u32 {
        unsafe { get_coff_sym(table.as_ptr(), count, sz.as_ptr()) }
    }

    fn pack(syms: &[[u8; COFF_SYM_SIZE]]) -> Vec<u8> {
        let mut v = Vec::with_capacity(syms.len() * COFF_SYM_SIZE);
        for s in syms {
            v.extend_from_slice(s);
        }
        v
    }

    #[test]
    fn name_eq_matches_strncmp_oracle() {
        let cases: &[(&[u8], &[u8])] = &[
            (b"EXPORTS\0", b"EXPORTS\0"),
            (b"_EXPORTS", b"_EXPORTS"),
            (b"EXPORTS\0", b"EXPORTT\0"),
            (b"ABCDEFGH", b"ABCDEFGH"),
            (b"ABCDEFGH", b"ABCDEFGI"),
            (b"A\0XXXXXX", b"A\0YYYYYY"),
            (b"\0XXXXXXX", b"\0YYYYYYY"),
        ];
        for &(a, b) in cases {
            let mut aa = [0u8; 8];
            let mut bb = [0u8; 8];
            aa.copy_from_slice(&a[..8]);
            bb.copy_from_slice(&b[..8]);
            let eq = name_eq_n(&aa, &bb, 8);
            let st = strncmp_fasm_oracle(&aa, &bb, 8);
            assert_eq!(eq, st == 0, "a={aa:?} b={bb:?}");
        }
    }

    #[test]
    fn named_vectors_match_oracle() {
        let exports = make_sym(b"EXPORTS\0", 0x1000);
        let under = make_sym(b"_EXPORTS", 0x2000);
        let other = make_sym(b"OTHER\0\0\0", 0x3000);
        let eight = make_sym(b"ABCDEFGH", 0x4000);

        let table = pack(&[exports, under, other, eight]);

        let cases: &[(&[u8], u32, u32)] = &[
            (b"EXPORTS\0xx", 4, 0x1000),
            (b"_EXPORTSxx", 4, 0x2000),
            (b"OTHER\0\0\0xx", 4, 0x3000),
            (b"ABCDEFGHxx", 4, 0x4000),
            (b"MISSING\0xx", 4, 0),
            (b"EXPORTS\0xx", 1, 0x1000),
            (b"_EXPORTSxx", 1, 0), // only first symbol scanned
            (b"ABCDEFGHxx", 3, 0), // eight is index 3; count=3 misses
            (b"ABCDEFGHxx", 4, 0x4000),
        ];

        for &(sz, count, expect) in cases {
            let got = rust_vs(&table, count, sz);
            let oracle = get_coff_sym_oracle(&table, count, sz);
            assert_eq!(oracle, expect, "oracle sz={sz:?} count={count}");
            assert_eq!(got, expect, "rust sz={sz:?} count={count}");
        }
    }

    #[test]
    fn hit_first_middle_last_and_miss() {
        let a = make_sym(b"AAA\0\0\0\0\0", 0x1111);
        let b = make_sym(b"BBB\0\0\0\0\0", 0x2222);
        let c = make_sym(b"CCC\0\0\0\0\0", 0x3333);
        let table = pack(&[a, b, c]);

        assert_eq!(rust_vs(&table, 3, b"AAA\0\0\0\0\0"), 0x1111);
        assert_eq!(rust_vs(&table, 3, b"BBB\0\0\0\0\0"), 0x2222);
        assert_eq!(rust_vs(&table, 3, b"CCC\0\0\0\0\0"), 0x3333);
        assert_eq!(rust_vs(&table, 3, b"DDD\0\0\0\0\0"), 0);
        assert_eq!(
            rust_vs(&table, 3, b"AAA\0\0\0\0\0"),
            get_coff_sym_oracle(&table, 3, b"AAA\0\0\0\0\0")
        );
    }

    #[test]
    fn first_match_wins_on_duplicate_names() {
        let a = make_sym(b"SAME\0\0\0\0", 0xAAAA);
        let b = make_sym(b"SAME\0\0\0\0", 0xBBBB);
        let table = pack(&[a, b]);
        assert_eq!(rust_vs(&table, 2, b"SAME\0\0\0\0"), 0xAAAA);
        assert_eq!(
            get_coff_sym_oracle(&table, 2, b"SAME\0\0\0\0"),
            0xAAAA
        );
    }

    #[test]
    fn single_symbol_hit_and_miss() {
        let a = make_sym(b"ONLY\0\0\0\0", 0x55AA);
        let table = pack(&[a]);
        assert_eq!(rust_vs(&table, 1, b"ONLY\0\0\0\0"), 0x55AA);
        assert_eq!(rust_vs(&table, 1, b"NONE\0\0\0\0"), 0);
    }

    #[test]
    fn prng_50k_matches_oracle() {
        let mut state = GET_COFF_SYM_PRNG_SEED;
        let mut next = || {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };

        for _ in 0..50_000 {
            let n_syms = (next() % 8) + 1; // 1..=8
            let mut syms = Vec::with_capacity(n_syms as usize);
            for i in 0..n_syms {
                let mut name = [0u8; 8];
                for b in &mut name {
                    *b = (next() & 0xFF) as u8;
                }
                // Ensure some printable / NUL variety
                if (next() & 7) == 0 {
                    name[(next() as usize) % 8] = 0;
                }
                let value = next().wrapping_mul(0x9E37_79B9).wrapping_add(i);
                syms.push(make_sym(&name, value));
            }
            let table = pack(&syms);

            // Query: either an existing name or a random miss
            let mut q = [0u8; 8];
            if (next() & 1) == 0 {
                let idx = (next() % n_syms) as usize;
                q.copy_from_slice(&syms[idx][..8]);
            } else {
                for b in &mut q {
                    *b = (next() & 0xFF) as u8;
                }
            }

            let count = (next() % n_syms) + 1; // 1..=n_syms
            let expect = get_coff_sym_oracle(&table, count, &q);
            let got = rust_vs(&table, count, &q);
            assert_eq!(got, expect, "count={count} n_syms={n_syms}");
        }
    }
}
