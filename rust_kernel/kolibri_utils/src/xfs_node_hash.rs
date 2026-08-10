//! Cut AM: `xfs._.get_before_by_hashval` — linear first-match-by-hash over XFS DA node.
//!
//! Matches `kernel/fs/xfs.asm` FASM leaf semantics (`movbe` BE loads, unsigned
//! `jae` first `hashval >= target`).
//!
//! Entry table pointer: callers pass stdcall `_base = node + sizeof(intnode)`
//! where `sizeof` includes one embedded `xfs_da_node_entry`. The on-disk btree
//! starts `sizeof(entry)` (= 8) bytes before that address — i.e. at
//! `node + offsetof(btree)`. FASM indexes from EBX (node) + offsetof; the
//! trampoline passes `entries = _base - 8`, which is identical for both v4/v5.
//!
//! No tables / `.rodata`. Freestanding path uses only raw pointer arithmetic.

use crate::xfs_hash_lookup::{
    trampoline_zf_from_flag, XfsHashLookupResult, ERROR_FILE_NOT_FOUND,
};

/// `sizeof.xfs_da_node_entry` (`hashval` + `before`).
pub const XFS_DA_NODE_ENTRY_SIZE: u32 = 8;

/// Offset of `btree` within `xfs_da_intnode` (v4): `sizeof.xfs_da_node_hdr` = 16.
pub const OFF_V4_BTREE: usize = 16;

/// Offset of `btree` within `xfs_da3_intnode` (v5): `sizeof.xfs_da3_node_hdr` = 64.
pub const OFF_V5_BTREE: usize = 64;

/// Callers' `lea …+sizeof(intnode)` overshoots `offsetof(btree)` by one entry.
pub const BASE_TO_BTREE_DELTA: usize = XFS_DA_NODE_ENTRY_SIZE as usize;

/// Offset of big-endian `hashval` within a node entry.
pub const OFF_NODE_HASHVAL: usize = 0;

/// Offset of big-endian `before` within a node entry.
pub const OFF_NODE_BEFORE: usize = 4;

/// Cut AM differential PRNG seed (`'CUTM'`).
pub const XFS_GET_BEFORE_BY_HASHVAL_PRNG_SEED: u32 = 0x4355_544D;

/// Load a big-endian `u32` from `p` (unaligned-safe byte gather).
///
/// # Safety
/// `p` must be readable for 4 bytes.
#[inline(always)]
unsafe fn load_be_u32(p: *const u8) -> u32 {
    let b0 = unsafe { *p };
    let b1 = unsafe { *p.add(1) };
    let b2 = unsafe { *p.add(2) };
    let b3 = unsafe { *p.add(3) };
    u32::from_be_bytes([b0, b1, b2, b3])
}

#[inline(always)]
fn btree_off(version: u32) -> usize {
    if version == 5 {
        OFF_V5_BTREE
    } else {
        OFF_V4_BTREE
    }
}

/// FASM-faithful linear first-match over a DA node btree entry table.
///
/// `entries` points at `count` contiguous `xfs_da_node_entry` records (8 bytes
/// each) — i.e. `node + offsetof(btree)` / stdcall `_base - 8`.
///
/// # Safety
/// When `count > 0`, `entries` must be readable for `count * 8` bytes.
#[inline(always)]
pub unsafe fn xfs_get_before_by_hashval(
    entries: *const u8,
    count: u32,
    hash: u32,
) -> XfsHashLookupResult {
    let mut i = 0u32;
    while i < count {
        let entry = unsafe { entries.add((i as usize) * (XFS_DA_NODE_ENTRY_SIZE as usize)) };
        let entry_hash = unsafe { load_be_u32(entry.add(OFF_NODE_HASHVAL)) };
        if entry_hash >= hash {
            let before = unsafe { load_be_u32(entry.add(OFF_NODE_BEFORE)) };
            return XfsHashLookupResult {
                eax: before,
                zf: 1,
            };
        }
        i = i.wrapping_add(1);
    }
    XfsHashLookupResult {
        eax: ERROR_FILE_NOT_FOUND,
        zf: 0,
    }
}

/// Pointer form for the freestanding FFI boundary.
///
/// # Safety
/// Same as [`xfs_get_before_by_hashval`].
#[inline(always)]
pub unsafe fn xfs_get_before_by_hashval_ptr(
    entries: *const u8,
    count: u32,
    hash: u32,
) -> XfsHashLookupResult {
    if count == 0 {
        // FASM with count=0 still enters the loop (hit on entry0 possible; miss
        // hangs). Production callers pass the on-disk BE count. Safe miss.
        return XfsHashLookupResult {
            eax: ERROR_FILE_NOT_FOUND,
            zf: 0,
        };
    }
    if entries.is_null() {
        return XfsHashLookupResult {
            eax: ERROR_FILE_NOT_FOUND,
            zf: 0,
        };
    }
    unsafe { xfs_get_before_by_hashval(entries, count, hash) }
}

/// Helper: node + version → entries pointer (same as `_base - 8` from callers).
#[inline(always)]
pub fn entries_from_node(node: *const u8, version: u32) -> *const u8 {
    unsafe { node.add(btree_off(version)) }
}

/// Helper: stdcall `_base` → entries pointer.
#[inline(always)]
pub fn entries_from_base(base: *const u8) -> *const u8 {
    unsafe { base.sub(BASE_TO_BTREE_DELTA) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfs_hash_lookup::{pack_eax_zf, unpack_eax_zf};

    /// Independent oracle mirroring the FASM control-flow schedule (count>0).
    unsafe fn fasm_oracle(entries: *const u8, count: u32, hash: u32) -> XfsHashLookupResult {
        let mut ecx = 0u32;
        loop {
            let entry = entries.add((ecx as usize) * (XFS_DA_NODE_ENTRY_SIZE as usize));
            let entry_hash = load_be_u32(entry.add(OFF_NODE_HASHVAL));
            if entry_hash >= hash {
                return XfsHashLookupResult {
                    eax: load_be_u32(entry.add(OFF_NODE_BEFORE)),
                    zf: 1,
                };
            }
            ecx = ecx.wrapping_add(1);
            if ecx == count {
                return XfsHashLookupResult {
                    eax: ERROR_FILE_NOT_FOUND,
                    zf: 0,
                };
            }
        }
    }

    fn be_u32(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    fn plant_entry(buf: &mut [u8], btree: usize, index: usize, hash: u32, before: u32) {
        let off = btree + index * 8;
        buf[off..off + 4].copy_from_slice(&be_u32(hash));
        buf[off + 4..off + 8].copy_from_slice(&be_u32(before));
    }

    fn check_entries(entries: &[u8], count: u32, hash: u32) {
        let got = unsafe { xfs_get_before_by_hashval(entries.as_ptr(), count, hash) };
        let exp = unsafe { fasm_oracle(entries.as_ptr(), count, hash) };
        assert_eq!(got, exp, "hash={hash:#x} count={count}");
        assert_eq!(trampoline_zf_from_flag(got.zf), got.zf == 1);
        let packed = pack_eax_zf(got.eax, got.zf);
        assert_eq!(unpack_eax_zf(packed), (got.eax, got.zf));
    }

    fn check_node(node: &[u8], count: u32, hash: u32, version: u32) {
        let off = btree_off(version);
        check_entries(&node[off..], count, hash);
        // `_base = node + sizeof = node + off + 8` → entries_from_base
        let base = unsafe { node.as_ptr().add(off + BASE_TO_BTREE_DELTA) };
        let via_base = unsafe { entries_from_base(base) };
        assert_eq!(via_base, unsafe { node.as_ptr().add(off) });
        let got = unsafe { xfs_get_before_by_hashval_ptr(via_base, count, hash) };
        let exp = unsafe { fasm_oracle(node.as_ptr().add(off), count, hash) };
        assert_eq!(got, exp);
    }

    #[test]
    fn empty_count_safe_miss() {
        let buf = [0u8; 8];
        let got = unsafe { xfs_get_before_by_hashval_ptr(buf.as_ptr(), 0, 1) };
        assert_eq!(
            got,
            XfsHashLookupResult {
                eax: ERROR_FILE_NOT_FOUND,
                zf: 0
            }
        );
    }

    #[test]
    fn miss_returns_error_file_not_found() {
        let mut buf = vec![0u8; 24];
        plant_entry(&mut buf, 0, 0, 0x10, 0x100);
        plant_entry(&mut buf, 0, 1, 0x20, 0x200);
        plant_entry(&mut buf, 0, 2, 0x30, 0x300);
        let got = unsafe { xfs_get_before_by_hashval(buf.as_ptr(), 3, 0x40) };
        assert_eq!(got.zf, 0);
        assert_eq!(got.eax, ERROR_FILE_NOT_FOUND);
    }

    #[test]
    fn v4_single_hit_and_miss() {
        let mut buf = vec![0u8; 16 + 8];
        plant_entry(&mut buf, OFF_V4_BTREE, 0, 0x10, 0xAABB_CCDD);
        check_node(&buf, 1, 0x10, 4);
        check_node(&buf, 1, 0x0F, 4);
        check_node(&buf, 1, 0x11, 4);
    }

    #[test]
    fn v5_layout_uses_offset_64() {
        let mut buf = vec![0u8; 64 + 24];
        plant_entry(&mut buf, OFF_V4_BTREE, 0, 0x1111, 0x2222);
        plant_entry(&mut buf, OFF_V5_BTREE, 0, 0x30, 0x300);
        plant_entry(&mut buf, OFF_V5_BTREE, 1, 0x50, 0x500);
        plant_entry(&mut buf, OFF_V5_BTREE, 2, 0x70, 0x700);
        check_node(&buf, 3, 0x30, 5);
        check_node(&buf, 3, 0x40, 5);
        check_node(&buf, 3, 0x71, 5);
        check_node(&buf, 1, 0x1111, 4);
    }

    #[test]
    fn first_match_not_exact_only() {
        let mut buf = vec![0u8; 24];
        plant_entry(&mut buf, 0, 0, 1, 0x100);
        plant_entry(&mut buf, 0, 1, 5, 0x500);
        plant_entry(&mut buf, 0, 2, 9, 0x900);
        check_entries(&buf, 3, 3);
        check_entries(&buf, 3, 5);
        check_entries(&buf, 3, 0);
        check_entries(&buf, 3, 10);
    }

    #[test]
    fn duplicate_hashes_take_first() {
        let mut buf = vec![0u8; 24];
        plant_entry(&mut buf, 0, 0, 7, 0x1111);
        plant_entry(&mut buf, 0, 1, 7, 0x2222);
        plant_entry(&mut buf, 0, 2, 7, 0x3333);
        let got = unsafe { xfs_get_before_by_hashval(buf.as_ptr(), 3, 7) };
        assert_eq!(got.zf, 1);
        assert_eq!(got.eax, 0x1111);
    }

    #[test]
    fn base_minus_eight_matches_node_offset() {
        for version in [4u32, 5, 6] {
            let off = btree_off(version);
            let mut node = vec![0u8; off + 16];
            plant_entry(&mut node, off, 0, 0xAA, 0xBB);
            plant_entry(&mut node, off, 1, 0xCC, 0xDD);
            let base = unsafe { node.as_ptr().add(off + 8) };
            assert_eq!(entries_from_base(base), entries_from_node(node.as_ptr(), version));
            check_node(&node, 2, 0xAA, version);
            check_node(&node, 2, 0xAB, version);
        }
    }

    #[test]
    fn prng_sorted_tables_match_oracle() {
        let mut state = XFS_GET_BEFORE_BY_HASHVAL_PRNG_SEED;
        let mut xorshift = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };

        for _ in 0..50_000u32 {
            let n_raw = (xorshift() % 33) as usize;
            let mut hashes: Vec<u32> = (0..n_raw).map(|_| xorshift()).collect();
            hashes.sort_unstable();
            hashes.dedup();
            let len = hashes.len() as u32;
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u8; (len as usize) * 8];
            for (i, &h) in hashes.iter().enumerate() {
                plant_entry(&mut buf, 0, i, h, 0xC000_0000 | i as u32);
            }
            for &h in &hashes {
                check_entries(&buf, len, h);
            }
            for _ in 0..4 {
                check_entries(&buf, len, xorshift());
            }
        }
    }
}
