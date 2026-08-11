//! Cut AR: `r_f_port_area` — reserve / free I/O port ranges.
//!
//! Matches `kernel/kernel.asm` FASM leaf semantics for syscall 46
//! (`ReservePortArea` / `FreePortArea`):
//!
//! * `op == 0` → reserve `[start..=end]` (inclusive)
//! * `op != 0` → free exact `(tid, start, end)` match
//!
//! Table layout (`RESERVED_PORTS`):
//! * dword count at offset 0
//! * entry `i` (1-based) at offset `i * 16`: tid, start, end, (pad)
//! * max count 255
//!
//! Overlap (reserve): reject unless `start > old.end` or `end < old.start`
//! (FASM: `cmp start, old.end / ja ok` then `cmp end, old.start / jae err`).
//!
//! IO map updates match Cut X polarity **inline** (no cross-section call —
//! reloc-free blob requirement):
//! enable = clear bit, disable = set bit.
//!
//! `cli`/`sti` stay in the FASM trampoline (reserve path only).

/// Bytes per reservation table entry (`shl …, 4`).
pub const ENTRY_SIZE: usize = 16;

/// Maximum reservation count before reserve fails (`cmp eax, 255 / jae .err`).
pub const MAX_RESERVED: u32 = 255;

/// Offsets within one 16-byte entry.
pub const OFF_TID: usize = 0;
pub const OFF_START: usize = 4;
pub const OFF_END: usize = 8;

/// Cut AR differential PRNG seed (`'CUTR'`).
pub const R_F_PORT_AREA_PRNG_SEED: u32 = 0x4355_5452;

/// FASM-faithful reserve/free of an I/O port range.
///
/// Returns `0` on success, `1` on error (legacy EAX).
///
/// # Safety
/// * `reserved_ports` must point at a writable table with room for count dword
///   plus up to 255 sixteen-byte entries (production: 64 KiB block).
/// * `io_map` must be a writable 8 KiB TSS I/O permission bitmap.
/// * Callers must hold interrupts disabled across reserve when matching FASM
///   (`cli` … enable loop … `sti`); free path historically has no `cli`.
#[inline(always)]
pub unsafe fn r_f_port_area(
    op: u32,
    start: u32,
    end: u32,
    reserved_ports: *mut u8,
    tid: u32,
    io_map: *mut u8,
) -> u32 {
    if op != 0 {
        return unsafe { free_port_area(start, end, reserved_ports, tid, io_map) };
    }
    unsafe { reserve_port_area(start, end, reserved_ports, tid, io_map) }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`r_f_port_area`].
#[inline(always)]
pub unsafe fn r_f_port_area_ptr(
    op: u32,
    start: u32,
    end: u32,
    reserved_ports: *mut u8,
    tid: u32,
    io_map: *mut u8,
) -> u32 {
    unsafe { r_f_port_area(op, start, end, reserved_ports, tid, io_map) }
}

#[inline(always)]
unsafe fn load_u32(base: *const u8, off: usize) -> u32 {
    unsafe { core::ptr::read_unaligned(base.add(off) as *const u32) }
}

#[inline(always)]
unsafe fn store_u32(base: *mut u8, off: usize, val: u32) {
    unsafe { core::ptr::write_unaligned(base.add(off) as *mut u32, val) };
}

#[inline(always)]
unsafe fn entry_ptr(reserved_ports: *mut u8, index_1based: u32) -> *mut u8 {
    unsafe { reserved_ports.add((index_1based as usize) * ENTRY_SIZE) }
}

/// Inline Cut X BTR/BTS (enable=`clear_access==0`). Kept local so the AR
/// freestanding section has zero relocations.
#[inline(always)]
unsafe fn io_map_set_bit(port: u32, clear_access: u32, io_map: *mut u8) {
    let byte_index = (port >> 3) as usize;
    let bit = (port & 7) as u8;
    let mask = 1u8 << bit;
    let p = unsafe { io_map.add(byte_index) };
    let cur = unsafe { *p };
    if clear_access == 0 {
        unsafe { *p = cur & !mask };
    } else {
        unsafe { *p = cur | mask };
    }
}

#[inline(always)]
unsafe fn enable_range(start: u32, end: u32, io_map: *mut u8) {
    let mut port = start;
    loop {
        unsafe { io_map_set_bit(port, 0, io_map) };
        if port == end {
            break;
        }
        port = port.wrapping_add(1);
    }
}

#[inline(always)]
unsafe fn disable_range(start: u32, end: u32, io_map: *mut u8) {
    let mut port = start;
    loop {
        unsafe { io_map_set_bit(port, 1, io_map) };
        if port == end {
            break;
        }
        port = port.wrapping_add(1);
    }
}

#[inline(always)]
unsafe fn reserve_port_area(
    start: u32,
    end: u32,
    reserved_ports: *mut u8,
    tid: u32,
    io_map: *mut u8,
) -> u32 {
    // cmp ecx, edx / ja .err
    if start > end {
        return 1;
    }
    // cmp edx, 65536 / jae .err
    if end >= 65536 {
        return 1;
    }

    let count = unsafe { load_u32(reserved_ports, 0) };
    if count != 0 {
        if count >= MAX_RESERVED {
            return 1;
        }
        // Walk eax = count … 1
        let mut eax = count;
        loop {
            let ent = unsafe { entry_ptr(reserved_ports, eax) };
            let old_end = unsafe { load_u32(ent, OFF_END) };
            // cmp ecx, [ebx+8] / ja .rpal4  (start > old_end → ok)
            if start <= old_end {
                let old_start = unsafe { load_u32(ent, OFF_START) };
                // cmp edx, [ebx+4] / jae .err  (end >= old_start → overlap)
                if end >= old_start {
                    return 1;
                }
            }
            eax = eax.wrapping_sub(1);
            if eax == 0 {
                break;
            }
        }
    }

    // Enable ports in IO map (FASM does this under cli; trampoline owns cli).
    unsafe { enable_range(start, end, io_map) };

    let new_count = count.wrapping_add(1);
    unsafe { store_u32(reserved_ports, 0, new_count) };
    let ent = unsafe { entry_ptr(reserved_ports, new_count) };
    unsafe {
        store_u32(ent, OFF_TID, tid);
        store_u32(ent, OFF_START, start);
        store_u32(ent, OFF_END, end);
    }
    0
}

#[inline(always)]
unsafe fn free_port_area(
    start: u32,
    end: u32,
    reserved_ports: *mut u8,
    tid: u32,
    io_map: *mut u8,
) -> u32 {
    let mut eax = unsafe { load_u32(reserved_ports, 0) };
    if eax == 0 {
        // .frpal2: inc eax → return 1
        return 1;
    }

    loop {
        let edi = unsafe { entry_ptr(reserved_ports, eax) };
        let ent_tid = unsafe { load_u32(edi, OFF_TID) };
        if ent_tid == tid {
            let ent_start = unsafe { load_u32(edi, OFF_START) };
            if ent_start == start {
                let ent_end = unsafe { load_u32(edi, OFF_END) };
                if ent_end == end {
                    // Compact: FASM `rep movsb` from edi+16 for (256-eax)*16 bytes.
                    // Manual forward copy — no `core::ptr::copy` (that emits a
                    // memcpy reloc and breaks Strategy A reloc-free extract).
                    let bytes = ((256u32.wrapping_sub(eax)) as usize) * ENTRY_SIZE;
                    let mut i = 0usize;
                    while i < bytes {
                        unsafe {
                            *edi.add(i) = *edi.add(ENTRY_SIZE + i);
                        }
                        i += 1;
                    }
                    let count = unsafe { load_u32(reserved_ports, 0) };
                    unsafe { store_u32(reserved_ports, 0, count.wrapping_sub(1)) };
                    unsafe { disable_range(start, end, io_map) };
                    return 0;
                }
            }
        }
        eax = eax.wrapping_sub(1);
        if eax == 0 {
            // .frpal2 after miss: inc eax → 1
            return 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io_access::{io_map_bit, IO_MAP_BYTES};

    /// Independent FASM-flow oracle (not a call into the Rust production path).
    fn oracle(
        op: u32,
        start: u32,
        end: u32,
        table: &mut [u8],
        tid: u32,
        io_map: &mut [u8],
    ) -> u32 {
        if op != 0 {
            return oracle_free(start, end, table, tid, io_map);
        }
        oracle_reserve(start, end, table, tid, io_map)
    }

    fn read_u32(buf: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
    }

    fn write_u32(buf: &mut [u8], off: usize, val: u32) {
        buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn oracle_enable(start: u32, end: u32, io_map: &mut [u8]) {
        let mut p = start;
        loop {
            let bi = (p >> 3) as usize;
            let bit = (p & 7) as u8;
            io_map[bi] &= !(1u8 << bit);
            if p == end {
                break;
            }
            p = p.wrapping_add(1);
        }
    }

    fn oracle_disable(start: u32, end: u32, io_map: &mut [u8]) {
        let mut p = start;
        loop {
            let bi = (p >> 3) as usize;
            let bit = (p & 7) as u8;
            io_map[bi] |= 1u8 << bit;
            if p == end {
                break;
            }
            p = p.wrapping_add(1);
        }
    }

    fn oracle_reserve(
        start: u32,
        end: u32,
        table: &mut [u8],
        tid: u32,
        io_map: &mut [u8],
    ) -> u32 {
        if start > end || end >= 65536 {
            return 1;
        }
        let count = read_u32(table, 0);
        if count != 0 {
            if count >= MAX_RESERVED {
                return 1;
            }
            let mut eax = count;
            loop {
                let off = (eax as usize) * ENTRY_SIZE;
                let old_end = read_u32(table, off + OFF_END);
                if start <= old_end {
                    let old_start = read_u32(table, off + OFF_START);
                    if end >= old_start {
                        return 1;
                    }
                }
                eax -= 1;
                if eax == 0 {
                    break;
                }
            }
        }
        oracle_enable(start, end, io_map);
        let new_count = count + 1;
        write_u32(table, 0, new_count);
        let off = (new_count as usize) * ENTRY_SIZE;
        write_u32(table, off + OFF_TID, tid);
        write_u32(table, off + OFF_START, start);
        write_u32(table, off + OFF_END, end);
        0
    }

    fn oracle_free(
        start: u32,
        end: u32,
        table: &mut [u8],
        tid: u32,
        io_map: &mut [u8],
    ) -> u32 {
        let mut eax = read_u32(table, 0);
        if eax == 0 {
            return 1;
        }
        loop {
            let off = (eax as usize) * ENTRY_SIZE;
            if read_u32(table, off + OFF_TID) == tid
                && read_u32(table, off + OFF_START) == start
                && read_u32(table, off + OFF_END) == end
            {
                let bytes = ((256 - eax) as usize) * ENTRY_SIZE;
                if bytes != 0 {
                    let src_start = off + ENTRY_SIZE;
                    table.copy_within(src_start..src_start + bytes, off);
                }
                let count = read_u32(table, 0);
                write_u32(table, 0, count - 1);
                oracle_disable(start, end, io_map);
                return 0;
            }
            eax -= 1;
            if eax == 0 {
                return 1;
            }
        }
    }

    fn fresh_table() -> Vec<u8> {
        vec![0u8; (1 + MAX_RESERVED as usize) * ENTRY_SIZE + 64]
    }

    fn fresh_map_denied() -> Vec<u8> {
        vec![0xFFu8; IO_MAP_BYTES]
    }

    fn run_pair(
        op: u32,
        start: u32,
        end: u32,
        tid: u32,
        table_a: &mut [u8],
        table_b: &mut [u8],
        map_a: &mut [u8],
        map_b: &mut [u8],
    ) {
        let ra = unsafe {
            r_f_port_area(
                op,
                start,
                end,
                table_a.as_mut_ptr(),
                tid,
                map_a.as_mut_ptr(),
            )
        };
        let rb = oracle(op, start, end, table_b, tid, map_b);
        assert_eq!(ra, rb, "ret op={op} [{start:#x}..={end:#x}] tid={tid}");
        assert_eq!(table_a, table_b, "table op={op} [{start:#x}..={end:#x}]");
        assert_eq!(map_a, map_b, "io_map op={op} [{start:#x}..={end:#x}]");
    }

    #[test]
    fn reserve_empty_table_enables_range() {
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        run_pair(0, 0x300, 0x30F, 7, &mut ta, &mut tb, &mut ma, &mut mb);
        assert_eq!(read_u32(&ta, 0), 1);
        assert_eq!(read_u32(&ta, 16 + OFF_TID), 7);
        assert_eq!(read_u32(&ta, 16 + OFF_START), 0x300);
        assert_eq!(read_u32(&ta, 16 + OFF_END), 0x30F);
        for p in 0x300u32..=0x30F {
            unsafe {
                assert_eq!(io_map_bit(ma.as_ptr(), p), 0);
            }
        }
        unsafe {
            assert_eq!(io_map_bit(ma.as_ptr(), 0x2FF), 1);
            assert_eq!(io_map_bit(ma.as_ptr(), 0x310), 1);
        }
    }

    #[test]
    fn reserve_rejects_inverted_and_out_of_range() {
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        run_pair(0, 0x10, 0x0F, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        run_pair(0, 0, 65536, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        // end == 65536 is jae .err; end == 0xFFFF is accepted
        run_pair(0, 0xFFFF, 0xFFFF, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        assert_eq!(read_u32(&ta, 0), 1);
    }

    #[test]
    fn overlap_detection_inclusive() {
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        run_pair(0, 0x100, 0x110, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        // Touching at endpoint: new.start == old.end → overlap (not ja)
        run_pair(0, 0x110, 0x120, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        // Adjacent: new.start == old.end+1 → ok
        run_pair(0, 0x111, 0x120, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        // Contained
        run_pair(0, 0x105, 0x108, 2, &mut ta, &mut tb, &mut ma, &mut mb);
        // Disjoint below
        run_pair(0, 0x80, 0x90, 3, &mut ta, &mut tb, &mut ma, &mut mb);
    }

    #[test]
    fn free_match_compacts_and_disables() {
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        run_pair(0, 0x200, 0x20F, 9, &mut ta, &mut tb, &mut ma, &mut mb);
        run_pair(0, 0x300, 0x30F, 9, &mut ta, &mut tb, &mut ma, &mut mb);
        run_pair(0, 0x400, 0x40F, 8, &mut ta, &mut tb, &mut ma, &mut mb);
        // Free middle entry owned by tid 9
        run_pair(1, 0x300, 0x30F, 9, &mut ta, &mut tb, &mut ma, &mut mb);
        assert_eq!(read_u32(&ta, 0), 2);
        // Remaining: 0x200 and 0x400 ranges
        assert_eq!(read_u32(&ta, 16 + OFF_START), 0x200);
        assert_eq!(read_u32(&ta, 32 + OFF_START), 0x400);
        for p in 0x300u32..=0x30F {
            unsafe {
                assert_eq!(io_map_bit(ma.as_ptr(), p), 1);
            }
        }
    }

    #[test]
    fn free_wrong_tid_or_range_fails() {
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        run_pair(0, 0x500, 0x50F, 3, &mut ta, &mut tb, &mut ma, &mut mb);
        run_pair(1, 0x500, 0x50F, 4, &mut ta, &mut tb, &mut ma, &mut mb); // wrong tid
        run_pair(1, 0x500, 0x50E, 3, &mut ta, &mut tb, &mut ma, &mut mb); // wrong end
        run_pair(1, 0, 0, 3, &mut ta, &mut tb, &mut ma, &mut mb); // empty miss
    }

    #[test]
    fn max_255_rejects_further_reserve() {
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        // Plant count=255 with non-overlapping single-port entries far apart
        // Using synthetic pre-fill of table only (no IO) then one more reserve.
        write_u32(&mut ta, 0, MAX_RESERVED);
        write_u32(&mut tb, 0, MAX_RESERVED);
        for i in 1..=MAX_RESERVED {
            let off = (i as usize) * ENTRY_SIZE;
            // Place entries at ports that leave 0xF000 free
            write_u32(&mut ta, off + OFF_TID, 1);
            write_u32(&mut ta, off + OFF_START, i - 1);
            write_u32(&mut ta, off + OFF_END, i - 1);
            write_u32(&mut tb, off + OFF_TID, 1);
            write_u32(&mut tb, off + OFF_START, i - 1);
            write_u32(&mut tb, off + OFF_END, i - 1);
        }
        run_pair(0, 0xF000, 0xF000, 1, &mut ta, &mut tb, &mut ma, &mut mb);
        assert_eq!(read_u32(&ta, 0), MAX_RESERVED);
    }

    #[test]
    fn system_style_seeded_ranges_block_low_ports() {
        // Mimic reserve_irqs_ports seed: 4 system ranges under tid=1
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        write_u32(&mut ta, 0, 4);
        write_u32(&mut tb, 0, 4);
        let ranges = [(0u32, 0x2Du32), (0x30, 0x4D), (0x50, 0xDF), (0xE5, 0xFF)];
        for (i, &(s, e)) in ranges.iter().enumerate() {
            let off = (i + 1) * ENTRY_SIZE;
            write_u32(&mut ta, off + OFF_TID, 1);
            write_u32(&mut ta, off + OFF_START, s);
            write_u32(&mut ta, off + OFF_END, e);
            write_u32(&mut tb, off + OFF_TID, 1);
            write_u32(&mut tb, off + OFF_START, s);
            write_u32(&mut tb, off + OFF_END, e);
        }
        // Overlap system range → err
        run_pair(0, 0x20, 0x28, 2, &mut ta, &mut tb, &mut ma, &mut mb);
        // Gap 0x2E..0x2F is free
        run_pair(0, 0x2E, 0x2F, 2, &mut ta, &mut tb, &mut ma, &mut mb);
        // High smoke ports
        run_pair(0, 0xF010, 0xF017, 2, &mut ta, &mut tb, &mut ma, &mut mb);
        run_pair(1, 0xF010, 0xF017, 2, &mut ta, &mut tb, &mut ma, &mut mb);
    }

    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn prng_50k_vs_oracle() {
        let mut state = R_F_PORT_AREA_PRNG_SEED;
        let mut ta = fresh_table();
        let mut tb = fresh_table();
        let mut ma = fresh_map_denied();
        let mut mb = fresh_map_denied();
        // Seed a few baseline ranges so free/overlap paths fire
        for &(s, e, tid) in &[
            (0x1000u32, 0x100F, 1u32),
            (0x2000, 0x201F, 2),
            (0x3000, 0x3000, 3),
        ] {
            run_pair(0, s, e, tid, &mut ta, &mut tb, &mut ma, &mut mb);
        }
        for _ in 0..50_000 {
            let op = xorshift32(&mut state) & 1;
            let a = xorshift32(&mut state) & 0x7FFF;
            let b = xorshift32(&mut state) & 0x7FFF;
            let (start, end) = if a <= b { (a, b) } else { (b, a) };
            // Keep ranges short so IO-map loops stay cheap
            let end = start.saturating_add((end - start) & 0xF);
            let tid = (xorshift32(&mut state) & 7) + 1;
            run_pair(op, start, end, tid, &mut ta, &mut tb, &mut ma, &mut mb);
        }
    }

    #[test]
    fn nonzero_op_means_free() {
        // Any nonzero op is free (legacy `test ebx,ebx` / `jnz`).
        for op in [1u32, 2, 0x80, 0xFFFF_FFFF] {
            let mut ta = fresh_table();
            let mut tb = fresh_table();
            let mut ma = fresh_map_denied();
            let mut mb = fresh_map_denied();
            run_pair(0, 0xABC0, 0xABC3, 5, &mut ta, &mut tb, &mut ma, &mut mb);
            run_pair(op, 0xABC0, 0xABC3, 5, &mut ta, &mut tb, &mut ma, &mut mb);
            assert_eq!(read_u32(&ta, 0), 0);
        }
    }
}
