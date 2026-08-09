//! Cut W: `xfs._.get_addr_by_hash` — binary search XFS dir leaf entries by hash.
//!
//! Matches `kernel/fs/xfs.asm` FASM leaf semantics (`movbe` BE loads, mid split,
//! below/above pointer advancement, miss via `ERROR_FILE_NOT_FOUND` + ZF=0).
//!
//! No tables / `.rodata`. Freestanding path uses only raw pointer arithmetic
//! (no slice indexing — avoids panic/reloc edges).

/// `sizeof.xfs_dir2_leaf_entry` (`hashval` + `address`).
pub const XFS_DIR2_LEAF_ENTRY_SIZE: u32 = 8;

/// `ERROR_FILE_NOT_FOUND` from `kernel/fs/fs_lfn.inc`.
pub const ERROR_FILE_NOT_FOUND: u32 = 5;

/// Offset of big-endian `hashval` within a leaf entry.
pub const OFF_HASHVAL: usize = 0;

/// Offset of big-endian `address` within a leaf entry.
pub const OFF_ADDRESS: usize = 4;

/// Cut W differential PRNG seed (`'CUTW'`).
pub const XFS_GET_ADDR_BY_HASH_PRNG_SEED: u32 = 0x4355_5457;

/// Search result matching legacy EAX + ZF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XfsHashLookupResult {
    /// Legacy EAX: BE address on hit, `ERROR_FILE_NOT_FOUND` on miss.
    pub eax: u32,
    /// Legacy ZF sense as `1` (found) / `0` (miss).
    pub zf: u32,
}

/// Pack `(eax, zf)` into the i686 `EDX:EAX` u64 return used by the FFI.
#[inline(always)]
pub fn pack_eax_zf(eax: u32, zf: u32) -> u64 {
    ((zf as u64) << 32) | (eax as u64)
}

/// Unpack FFI `u64` return into `(eax, zf)`.
#[inline(always)]
pub fn unpack_eax_zf(packed: u64) -> (u32, u32) {
    (packed as u32, (packed >> 32) as u32)
}

/// Trampoline model: `cmp edx, 1` leaves ZF iff Rust `zf == 1`.
#[inline(always)]
pub fn trampoline_zf_from_flag(zf: u32) -> bool {
    zf == 1
}

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

/// FASM-faithful binary search over a leaf-entry table.
///
/// `base` points at `len` contiguous `xfs_dir2_leaf_entry` records (8 bytes each).
/// `hash` is the register EAX input.
///
/// # Safety
/// When `len > 0`, `base` must be readable for `len * 8` bytes.
#[inline(always)]
pub unsafe fn xfs_get_addr_by_hash(hash: u32, base: *const u8, len: u32) -> XfsHashLookupResult {
    let mut cur = base;
    let mut remaining = len;

    loop {
        // test ecx, ecx / jz .not_found
        if remaining == 0 {
            return XfsHashLookupResult {
                eax: ERROR_FILE_NOT_FOUND,
                zf: 0,
            };
        }

        // shr ecx, 1
        let mid = remaining >> 1;
        // ebx + ecx*sizeof.xfs_dir2_leaf_entry
        let entry = unsafe { cur.add((mid as usize) * (XFS_DIR2_LEAF_ENTRY_SIZE as usize)) };

        // movbe esi, [ebx+ecx*8+hashval]
        let entry_hash = unsafe { load_be_u32(entry.add(OFF_HASHVAL)) };

        // cmp eax, esi
        if hash < entry_hash {
            // .below: mov edx, ecx ; jmp .next
            remaining = mid;
            continue;
        }
        if hash > entry_hash {
            // .above: lea ebx, [ebx+(ecx+1)*8] ; sub edx, ecx ; dec edx
            cur = unsafe { entry.add(XFS_DIR2_LEAF_ENTRY_SIZE as usize) };
            remaining = remaining - mid - 1;
            continue;
        }

        // equal: movbe eax, [ebx+ecx*8+address] ; ZF=1 from cmp
        let address = unsafe { load_be_u32(entry.add(OFF_ADDRESS)) };
        return XfsHashLookupResult {
            eax: address,
            zf: 1,
        };
    }
}

/// Pointer form for the freestanding FFI boundary (same as [`xfs_get_addr_by_hash`]).
///
/// # Safety
/// `base` must be readable for `len * 8` bytes when `len > 0`.
#[inline(always)]
pub unsafe fn xfs_get_addr_by_hash_ptr(hash: u32, base: *const u8, len: u32) -> XfsHashLookupResult {
    if len == 0 {
        return XfsHashLookupResult {
            eax: ERROR_FILE_NOT_FOUND,
            zf: 0,
        };
    }
    if base.is_null() {
        // Production callers never pass null with nonzero len; avoid panic paths.
        return XfsHashLookupResult {
            eax: ERROR_FILE_NOT_FOUND,
            zf: 0,
        };
    }
    unsafe { xfs_get_addr_by_hash(hash, base, len) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent oracle mirroring the FASM control-flow schedule.
    unsafe fn fasm_oracle(hash: u32, base: *const u8, len: u32) -> XfsHashLookupResult {
        let mut cur = base;
        let mut remaining = len;
        loop {
            if remaining == 0 {
                return XfsHashLookupResult {
                    eax: ERROR_FILE_NOT_FOUND,
                    zf: 0,
                };
            }
            let mid = remaining >> 1;
            let entry = cur.add((mid as usize) * (XFS_DIR2_LEAF_ENTRY_SIZE as usize));
            let entry_hash = load_be_u32(entry.add(OFF_HASHVAL));
            if hash < entry_hash {
                remaining = mid;
                continue;
            }
            if hash > entry_hash {
                cur = entry.add(XFS_DIR2_LEAF_ENTRY_SIZE as usize);
                remaining = remaining - mid - 1;
                continue;
            }
            return XfsHashLookupResult {
                eax: load_be_u32(entry.add(OFF_ADDRESS)),
                zf: 1,
            };
        }
    }

    fn be_entry(hash: u32, addr: u32) -> [u8; 8] {
        let mut e = [0u8; 8];
        e[0..4].copy_from_slice(&hash.to_be_bytes());
        e[4..8].copy_from_slice(&addr.to_be_bytes());
        e
    }

    fn table(entries: &[[u8; 8]]) -> Vec<u8> {
        let mut v = Vec::with_capacity(entries.len() * 8);
        for e in entries {
            v.extend_from_slice(e);
        }
        v
    }

    fn check(hash: u32, buf: &[u8], len: u32) {
        let got = unsafe { xfs_get_addr_by_hash(hash, buf.as_ptr(), len) };
        let exp = unsafe { fasm_oracle(hash, buf.as_ptr(), len) };
        assert_eq!(got, exp, "hash={hash:#x} len={len}");
        assert_eq!(
            trampoline_zf_from_flag(got.zf),
            got.zf == 1,
            "trampoline ZF model"
        );
        let packed = pack_eax_zf(got.eax, got.zf);
        assert_eq!(unpack_eax_zf(packed), (got.eax, got.zf));
    }

    #[test]
    fn empty_table_miss() {
        check(0, &[], 0);
        check(0x1234_5678, &[], 0);
        check(0xFFFF_FFFF, &[], 0);
    }

    #[test]
    fn single_hit_and_miss() {
        let buf = table(&[be_entry(0x10, 0xAABB_CCDD)]);
        check(0x10, &buf, 1);
        check(0x0F, &buf, 1);
        check(0x11, &buf, 1);
        check(0, &buf, 1);
        check(0xFFFF_FFFF, &buf, 1);
    }

    #[test]
    fn three_sorted_hits_and_gaps() {
        let buf = table(&[
            be_entry(1, 0x100),
            be_entry(5, 0x500),
            be_entry(9, 0x900),
        ]);
        check(1, &buf, 3);
        check(5, &buf, 3);
        check(9, &buf, 3);
        check(0, &buf, 3);
        check(3, &buf, 3);
        check(7, &buf, 3);
        check(10, &buf, 3);
    }

    #[test]
    fn power_of_two_lengths() {
        let mut ents = Vec::new();
        for i in 0..16u32 {
            ents.push(be_entry(i * 10, 0xA000_0000 | i));
        }
        let buf = table(&ents);
        for i in 0..16u32 {
            check(i * 10, &buf, 16);
            check(i * 10 + 1, &buf, 16);
        }
        check(0xFFFF_FFFF, &buf, 16);
    }

    #[test]
    fn duplicate_mid_prefers_first_equal_path() {
        let buf = table(&[
            be_entry(7, 0x1111),
            be_entry(7, 0x2222),
            be_entry(7, 0x3333),
        ]);
        let got = unsafe { xfs_get_addr_by_hash(7, buf.as_ptr(), 3) };
        let exp = unsafe { fasm_oracle(7, buf.as_ptr(), 3) };
        assert_eq!(got, exp);
        assert_eq!(got.zf, 1);
        assert_eq!(got.eax, 0x2222);
    }

    #[test]
    fn zero_and_max_hash_address() {
        let buf = table(&[
            be_entry(0, 0),
            be_entry(0x7FFF_FFFF, 0xFFFF_FFFF),
            be_entry(0xFFFF_FFFF, 1),
        ]);
        check(0, &buf, 3);
        check(0x7FFF_FFFF, &buf, 3);
        check(0xFFFF_FFFF, &buf, 3);
        check(1, &buf, 3);
    }

    #[test]
    fn ptr_null_and_empty() {
        unsafe {
            assert_eq!(
                xfs_get_addr_by_hash_ptr(1, core::ptr::null(), 0),
                XfsHashLookupResult {
                    eax: ERROR_FILE_NOT_FOUND,
                    zf: 0
                }
            );
            assert_eq!(
                xfs_get_addr_by_hash_ptr(1, core::ptr::null(), 5),
                XfsHashLookupResult {
                    eax: ERROR_FILE_NOT_FOUND,
                    zf: 0
                }
            );
        }
    }

    #[test]
    fn exhaustive_small_sorted_domains() {
        for n in 0u32..8 {
            let mut ents = Vec::new();
            for i in 0..n {
                ents.push(be_entry(i, 0x1000 + i));
            }
            let buf = table(&ents);
            for hash in 0u32..n + 2 {
                check(hash, &buf, n);
            }
            check(0x8000_0000, &buf, n);
            check(0xFFFF_FFFF, &buf, n);
        }
    }

    #[test]
    fn prng_sorted_tables_match_oracle() {
        let mut state = XFS_GET_ADDR_BY_HASH_PRNG_SEED;
        let mut xorshift = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };

        for _ in 0..50_000u32 {
            let n = (xorshift() % 33) as usize;
            let mut hashes: Vec<u32> = (0..n).map(|_| xorshift()).collect();
            hashes.sort_unstable();
            hashes.dedup();
            let len = hashes.len() as u32;
            let mut ents = Vec::new();
            for (i, &h) in hashes.iter().enumerate() {
                ents.push(be_entry(h, 0xC000_0000 | i as u32));
            }
            let buf = table(&ents);

            for &h in &hashes {
                check(h, &buf, len);
            }
            for _ in 0..4 {
                let miss = xorshift();
                check(miss, &buf, len);
            }
        }
    }

    #[test]
    fn pack_roundtrip_edges() {
        assert_eq!(pack_eax_zf(0, 0), 0);
        assert_eq!(pack_eax_zf(5, 0), 5);
        assert_eq!(pack_eax_zf(0xAABB_CCDD, 1), 0x1_0000_0000 | 0xAABB_CCDD);
        assert_eq!(unpack_eax_zf(pack_eax_zf(5, 0)), (5, 0));
        assert_eq!(unpack_eax_zf(pack_eax_zf(0x10, 1)), (0x10, 1));
    }
}
