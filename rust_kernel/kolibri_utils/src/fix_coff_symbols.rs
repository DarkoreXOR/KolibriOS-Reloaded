//! Cut BU: `fix_coff_symbols` — COFF symbol table resolve + internal VA fixup.
//!
//! Matches `kernel/core/dll.inc` FASM `proc fix_coff_symbols stdcall`:
//! * walk `sym_count` × `COFF_SYM` (18 B) starting at `symbols`
//! * external (`SectionNumber == 0`): resolve via injected `get_proc_ex` callback
//! * absolute/debug (`0xFFFF` / `0xFFFE`): skip
//! * internal: add section `VirtualAddress` to existing `Value`
//! * return `1` unless any external resolve returned `0` → `0`
//!
//! `get_proc_ex` stays FASM (`get_proc_ex` in `dll.inc`); injected as last arg.

use crate::coff_reloc::{COFF_SECTION_SIZE, COFF_SYM_SIZE, OFF_SEC_VIRTUAL_ADDRESS, OFF_SYM_VALUE};
use crate::get_coff_sym::COFF_SYM_NAME_LEN;

/// Cut BU differential PRNG seed (`'CUBU'`).
pub const FIX_COFF_SYMBOLS_PRNG_SEED: u32 = 0x4355_4255;

/// `COFF_SYM.SectionNumber` offset.
pub const OFF_SYM_SECTION: usize = 12;

pub type GetProcExFn = unsafe extern "stdcall" fn(proc_name: *const u8, imports: u32) -> u32;

#[inline(always)]
unsafe fn read_u16(p: *const u8) -> u16 {
    unsafe { u16::from_le_bytes([*p, *p.add(1)]) }
}

#[inline(always)]
unsafe fn read_u32(p: *const u8) -> u32 {
    unsafe { u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) }
}

#[inline(always)]
unsafe fn write_u32(p: *mut u8, v: u32) {
    unsafe {
        core::ptr::write_unaligned(p as *mut u32, v);
    }
}

/// Section header offset for symbol `section_num` (1-based COFF section index).
#[inline(always)]
fn section_ptr(sec: *const u8, section_num: u16) -> *const u8 {
    let idx = (section_num as u32).wrapping_sub(1);
    unsafe { sec.add((idx as usize).wrapping_mul(COFF_SECTION_SIZE)) }
}

/// FASM-faithful COFF symbol fixup loop.
///
/// # Safety
/// `sec` must address `nSections` readable section headers; `symbols` must
/// address `sym_count` writable `COFF_SYM` records; `strings` valid when any
/// symbol uses a long external name; `get_proc_ex` must match legacy stdcall ABI.
#[inline(always)]
pub unsafe fn fix_coff_symbols(
    sec: *const u8,
    mut symbols: *mut u8,
    mut sym_count: u32,
    strings: *const u8,
    imports: u32,
    get_proc_ex: GetProcExFn,
) -> u32 {
    let mut retval = 1u32;
    while sym_count != 0 {
        let sym = symbols;
        let section = unsafe { read_u16(sym.add(OFF_SYM_SECTION)) };
        if section == 0 {
            let name_dword = unsafe { read_u32(sym) };
            let name_ptr = if name_dword == 0 {
                let off = unsafe { read_u32(sym.add(4)) };
                unsafe { strings.add(off as usize) }
            } else {
                sym as *const u8
            };
            let resolved = unsafe { get_proc_ex(name_ptr, imports) };
            if resolved == 0 {
                retval = 0;
            }
            unsafe { write_u32(sym.add(OFF_SYM_VALUE), resolved) };
        } else if section != 0xFFFF && section != 0xFFFE {
            let sec_ptr = section_ptr(sec, section);
            let va = unsafe { read_u32(sec_ptr.add(OFF_SEC_VIRTUAL_ADDRESS)) };
            let cur = unsafe { read_u32(sym.add(OFF_SYM_VALUE)) };
            unsafe { write_u32(sym.add(OFF_SYM_VALUE), cur.wrapping_add(va)) };
        }
        symbols = unsafe { symbols.add(COFF_SYM_SIZE) };
        sym_count = sym_count.wrapping_sub(1);
    }
    retval
}

#[inline(always)]
pub unsafe fn fix_coff_symbols_ptr(
    sec: *const u8,
    symbols: *mut u8,
    sym_count: u32,
    strings: *const u8,
    imports: u32,
    get_proc_ex: GetProcExFn,
) -> u32 {
    unsafe { fix_coff_symbols(sec, symbols, sym_count, strings, imports, get_proc_ex) }
}

/// Buffered independent FASM-flow oracle (mutates `table` in place).
pub fn fix_coff_symbols_oracle(
    table: &mut [u8],
    sec: &mut [u8],
    sym_count: u32,
    strings: &[u8],
    imports: u32,
    get_proc_ex: GetProcExFn,
) -> u32 {
    fn ru16(buf: &[u8], off: usize) -> u16 {
        u16::from_le_bytes([buf[off], buf[off + 1]])
    }
    fn ru32(buf: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
    }
    fn wu32(buf: &mut [u8], off: usize, v: u32) {
        buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    let mut retval = 1u32;
    let mut off = 0usize;
    let mut remaining = sym_count;
    while remaining != 0 {
        let section = ru16(table, off + OFF_SYM_SECTION);
        if section == 0 {
            let name_dword = ru32(table, off);
            let name_ptr = if name_dword == 0 {
                let str_off = ru32(table, off + 4) as usize;
                strings.as_ptr().wrapping_add(str_off)
            } else {
                table.as_ptr().wrapping_add(off)
            };
            let resolved = unsafe { get_proc_ex(name_ptr, imports) };
            if resolved == 0 {
                retval = 0;
            }
            wu32(table, off + OFF_SYM_VALUE, resolved);
        } else if section != 0xFFFF && section != 0xFFFE {
            let idx = (section as u32).wrapping_sub(1) as usize;
            let sec_off = idx * COFF_SECTION_SIZE;
            assert!(sec_off + OFF_SEC_VIRTUAL_ADDRESS + 4 <= sec.len());
            let va = ru32(sec, sec_off + OFF_SEC_VIRTUAL_ADDRESS);
            let cur = ru32(table, off + OFF_SYM_VALUE);
            wu32(table, off + OFF_SYM_VALUE, cur.wrapping_add(va));
        }
        off += COFF_SYM_SIZE;
        remaining = remaining.wrapping_sub(1);
    }
    retval
}

/// Build one synthetic `COFF_SYM` with inline name prefix + section + value.
pub fn make_fix_sym(name8: &[u8; 8], section: u16, value: u32) -> [u8; COFF_SYM_SIZE] {
    let mut s = [0u8; COFF_SYM_SIZE];
    s[..8].copy_from_slice(name8);
    s[OFF_SYM_VALUE..OFF_SYM_VALUE + 4].copy_from_slice(&value.to_le_bytes());
    s[OFF_SYM_SECTION..OFF_SYM_SECTION + 2].copy_from_slice(&section.to_le_bytes());
    s
}

/// Long-name external symbol: Name dword 0 + string-table offset.
pub fn make_fix_sym_long(str_off: u32, value: u32) -> [u8; COFF_SYM_SIZE] {
    let mut s = [0u8; COFF_SYM_SIZE];
    s[OFF_SYM_VALUE..OFF_SYM_VALUE + 4].copy_from_slice(&value.to_le_bytes());
    s[4..8].copy_from_slice(&str_off.to_le_bytes());
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "stdcall" fn mock_get_proc(name: *const u8, _imports: u32) -> u32 {
        let b0 = unsafe { *name };
        if b0 == b'T' {
            0x0000_6000
        } else if b0 == b'U' {
            0
        } else {
            0x0000_A000
        }
    }

    fn run_fix(
        sec: &mut [u8],
        table: &mut [u8],
        count: u32,
        strings: &[u8],
    ) -> u32 {
        unsafe {
            fix_coff_symbols(
                sec.as_ptr(),
                table.as_mut_ptr(),
                count,
                strings.as_ptr(),
                0,
                mock_get_proc,
            )
        }
    }

    fn section_va(sec: &mut [u8], idx0: usize, va: u32) {
        let off = idx0 * COFF_SECTION_SIZE + OFF_SEC_VIRTUAL_ADDRESS;
        sec[off..off + 4].copy_from_slice(&va.to_le_bytes());
    }

    #[test]
    fn internal_adds_section_va() {
        let mut sec = [0u8; COFF_SECTION_SIZE * 2];
        section_va(&mut sec, 0, 0x1000);
        section_va(&mut sec, 1, 0x2000);

        let mut sym0 = make_fix_sym(b"INT1\0\0\0\0", 1, 0x50);
        let mut sym1 = make_fix_sym(b"INT2\0\0\0\0", 2, 0x80);
        let mut table = [0u8; COFF_SYM_SIZE * 2];
        table[..COFF_SYM_SIZE].copy_from_slice(&sym0);
        table[COFF_SYM_SIZE..].copy_from_slice(&sym1);

        assert_eq!(run_fix(&mut sec, &mut table, 2, b""), 1);
        let v0 = u32::from_le_bytes(table[8..12].try_into().unwrap());
        let v1 = u32::from_le_bytes(table[COFF_SYM_SIZE + 8..COFF_SYM_SIZE + 12].try_into().unwrap());
        assert_eq!(v0, 0x1050);
        assert_eq!(v1, 0x2080);
    }

    #[test]
    fn external_inline_resolves() {
        let mut sec = [0u8; COFF_SECTION_SIZE];
        let mut sym = make_fix_sym(b"TEST\0\0\0\0", 0, 0xDEAD);
        let mut table = sym;
        assert_eq!(run_fix(&mut sec, &mut table, 1, b""), 1);
        let v = u32::from_le_bytes(table[8..12].try_into().unwrap());
        assert_eq!(v, 0x6000);
    }

    #[test]
    fn external_unresolved_clears_retval() {
        let mut sec = [0u8; COFF_SECTION_SIZE];
        let mut sym = make_fix_sym(b"UNRES\0\0\0", 0, 0);
        let mut table = sym;
        assert_eq!(run_fix(&mut sec, &mut table, 1, b""), 0);
        assert_eq!(u32::from_le_bytes(table[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn skip_absolute_and_debug() {
        let mut sec = [0u8; COFF_SECTION_SIZE];
        section_va(&mut sec, 0, 0x9999);
        let abs = make_fix_sym(b"ABS\0\0\0\0\0", 0xFFFF, 0x1111);
        let dbg = make_fix_sym(b"DBG\0\0\0\0\0", 0xFFFE, 0x2222);
        let mut table = [0u8; COFF_SYM_SIZE * 2];
        table[..COFF_SYM_SIZE].copy_from_slice(&abs);
        table[COFF_SYM_SIZE..].copy_from_slice(&dbg);
        assert_eq!(run_fix(&mut sec, &mut table, 2, b""), 1);
        assert_eq!(u32::from_le_bytes(table[8..12].try_into().unwrap()), 0x1111);
        assert_eq!(
            u32::from_le_bytes(table[COFF_SYM_SIZE + 8..COFF_SYM_SIZE + 12].try_into().unwrap()),
            0x2222
        );
    }

    #[test]
    fn long_external_name() {
        let strings = b"TargetFn\0padding";
        let mut sec = [0u8; COFF_SECTION_SIZE];
        let mut sym = make_fix_sym_long(0, 0);
        let mut table = sym;
        assert_eq!(run_fix(&mut sec, &mut table, 1, strings), 1);
        assert_eq!(u32::from_le_bytes(table[8..12].try_into().unwrap()), 0x6000);
    }

    #[test]
    fn oracle_matches_rust() {
        let mut sec = [0u8; COFF_SECTION_SIZE * 3];
        section_va(&mut sec, 0, 0x100);
        section_va(&mut sec, 1, 0x200);
        section_va(&mut sec, 2, 0x300);

        let s0 = make_fix_sym(b"EXT1\0\0\0\0", 0, 0);
        let s1 = make_fix_sym(b"INT1\0\0\0\0", 1, 0x10);
        let s2 = make_fix_sym(b"UNRES\0\0\0", 0, 0);
        let mut rust_table = [0u8; COFF_SYM_SIZE * 3];
        rust_table[..COFF_SYM_SIZE].copy_from_slice(&s0);
        rust_table[COFF_SYM_SIZE..2 * COFF_SYM_SIZE].copy_from_slice(&s1);
        rust_table[2 * COFF_SYM_SIZE..].copy_from_slice(&s2);

        let mut oracle_table = rust_table;
        let mut oracle_sec = sec;

        let r = run_fix(&mut sec, &mut rust_table, 3, b"");
        let o = fix_coff_symbols_oracle(&mut oracle_table, &mut oracle_sec, 3, b"", 0, mock_get_proc);
        assert_eq!(r, o);
        assert_eq!(rust_table, oracle_table);
    }

    #[test]
    fn prng_differential_50k() {
        let mut sec = [0u8; COFF_SECTION_SIZE * 4];
        for i in 0..4 {
            section_va(&mut sec, i, 0x1000u32.wrapping_mul(i as u32 + 1));
        }
        let strings = b"TargetFn\0UNRES\0\0\0\0\0\0\0\0";

        let mut state = FIX_COFF_SYMBOLS_PRNG_SEED;
        for _ in 0..50_000 {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let count = (state & 3) + 1;
            let mut rust_table = [0u8; COFF_SYM_SIZE * 4];
            let mut oracle_table = [0u8; COFF_SYM_SIZE * 4];
            for i in 0..count as usize {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let kind = state % 5;
                let sym = match kind {
                    0 => make_fix_sym(b"TEST\0\0\0\0", 0, state),
                    1 => make_fix_sym(b"UNRES\0\0\0", 0, state),
                    2 => {
                        let secn = ((state % 3) + 1) as u16;
                        make_fix_sym(b"INTX\0\0\0\0", secn, state & 0xFF)
                    }
                    3 => make_fix_sym(b"ABS\0\0\0\0\0", 0xFFFF, state),
                    _ => make_fix_sym_long(0, state),
                };
                rust_table[i * COFF_SYM_SIZE..(i + 1) * COFF_SYM_SIZE].copy_from_slice(&sym);
                oracle_table[i * COFF_SYM_SIZE..(i + 1) * COFF_SYM_SIZE].copy_from_slice(&sym);
            }
            let mut rust_sec = sec;
            let mut oracle_sec = sec;
            let r = run_fix(&mut rust_sec, &mut rust_table, count, strings);
            let o = fix_coff_symbols_oracle(
                &mut oracle_table,
                &mut oracle_sec,
                count,
                strings,
                0,
                mock_get_proc,
            );
            assert_eq!(r, o, "retval count={count}");
            assert_eq!(rust_table, oracle_table, "table count={count}");
        }
    }

    #[test]
    fn seed_constant() {
        assert_eq!(FIX_COFF_SYMBOLS_PRNG_SEED, 0x4355_4255);
    }

    #[test]
    fn coff_sym_name_len_unchanged() {
        assert_eq!(COFF_SYM_NAME_LEN, 8);
    }
}
