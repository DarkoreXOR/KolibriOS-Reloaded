//! Cut Z: `is_partition_table_entry` — MBR/EBR partition-table entry validation.
//! Cut AD: `is_protective_mbr` — GPT protective-MBR recognition (ZF).
//!
//! Matches `kernel/blkdev/disk.inc` FASM leaf semantics:
//! * Bootable must be `0` or `0x80` (`and al, 7Fh` / `jnz` → invalid)
//! * `(ebp + FirstAbsSector + Length) / 2 < Capacity` (unsigned 64-bit)
//!   via `add`/`adc` + `shr`/`rcr` + `sub`/`sbb` / `jnc` → invalid
//! * Protective MBR: `[pt-2]==0`, entry0 `{boot=0, type=0xEE, first=1,
//!   length=0xFFFFFFFF | Capacity_lo-1}`, entries 1–3 all zero
//!
//! Capacity is passed explicitly so the Rust blob stays reloc-free (no
//! `DISK` global / GOT). No tables / `.rodata`.

/// `PARTITION_TABLE_ENTRY` size in bytes (MBR slot).
pub const PARTITION_TABLE_ENTRY_SIZE: usize = 16;

/// Offset of `.Bootable` within the entry.
pub const OFF_BOOTABLE: usize = 0;

/// Offset of `.Type` / filesystem ID within the entry.
pub const OFF_TYPE: usize = 4;

/// Offset of `.FirstAbsSector` (LBA dword).
pub const OFF_FIRST_ABS_SECTOR: usize = 8;

/// Offset of `.Length` (sectors dword).
pub const OFF_LENGTH: usize = 12;

/// GPT protective partition type (`0xEE`).
pub const PROTECTIVE_MBR_TYPE: u8 = 0xEE;

/// Bytes occupied by partition entries 1–3 (must be zero for protective MBR).
pub const PROTECTIVE_TRAILING_BYTES: usize = 16 * 3;

/// Cut Z differential PRNG seed (`'CUTZ'`).
pub const IS_PARTITION_TABLE_ENTRY_PRNG_SEED: u32 = 0x4355_545A;

/// Cut AD differential PRNG seed (`'CUTD'`).
pub const IS_PROTECTIVE_MBR_PRNG_SEED: u32 = 0x4355_5444;

/// Cut CC differential PRNG seed (`'CUTC'`).
pub const PROCESS_PARTITION_TABLE_ENTRY_PRNG_SEED: u32 = 0x4355_5443;

/// `DISK.MediaInfo.Capacity` low dword offset within a `DISK` object.
pub const DISK_CAPACITY_LO_OFFSET: usize = 56;

/// Extended-partition type IDs recognized by `process_partition_table_entry`.
pub const EXTENDED_PARTITION_TYPES: [u8; 4] = [0x05, 0x0f, 0xc5, 0xd5];

/// Stdcall hook mirroring FASM `disk_add_partition`.
pub type DiskAddPartitionFn = unsafe extern "stdcall" fn(
    start_lo: u32,
    start_hi: u32,
    length_lo: u32,
    length_hi: u32,
    disk: u32,
);

/// FASM-faithful validity check (CF=0 valid / CF=1 invalid).
///
/// Returns `true` when the entry is **valid** (legacy clears CF).
#[inline(always)]
pub fn is_partition_table_entry(
    bootable: u8,
    first_abs_sector: u32,
    length: u32,
    ebp_base: u32,
    capacity: u64,
) -> bool {
    // 1. Bootable field: bits 0..6 must be clear (0 or 0x80 only).
    if (bootable & 0x7F) != 0 {
        return false;
    }

    // 2–4. edx:eax = ebp + FirstAbsSector + Length, then /2.
    // Matches zero-extend ebp into edx:eax, then two add/adc pairs,
    // then shr edx,1 / rcr eax,1 (unsigned 64-bit right shift by 1).
    let sum = (ebp_base as u64)
        .wrapping_add(first_abs_sector as u64)
        .wrapping_add(length as u64);
    let half = sum >> 1;

    // 5. sub/sbb vs Capacity; jnc → invalid (half >= capacity).
    // Valid iff unsigned half < capacity (borrow / CF=1 after sbb).
    half < capacity
}

/// Pointer-form wrapper for the FFI boundary.
///
/// Returns `0` = valid (CF clear), `1` = invalid (CF set), matching Cut H
/// `block_clip` EAX→CF trampoline polarity.
///
/// # Safety
/// `entry` must be readable for [`PARTITION_TABLE_ENTRY_SIZE`] bytes.
#[inline(always)]
pub unsafe fn is_partition_table_entry_ptr(
    entry: *const u8,
    ebp_base: u32,
    capacity_lo: u32,
    capacity_hi: u32,
) -> u32 {
    let bootable = unsafe { *entry.add(OFF_BOOTABLE) };
    let first = unsafe { read_u32_le(entry.add(OFF_FIRST_ABS_SECTOR)) };
    let length = unsafe { read_u32_le(entry.add(OFF_LENGTH)) };
    let capacity = ((capacity_hi as u64) << 32) | (capacity_lo as u64);
    if is_partition_table_entry(bootable, first, length, ebp_base, capacity) {
        0
    } else {
        1
    }
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Pack a synthetic 16-byte `PARTITION_TABLE_ENTRY` (other fields zero).
#[inline(always)]
pub fn make_entry(bootable: u8, first_abs: u32, length: u32) -> [u8; PARTITION_TABLE_ENTRY_SIZE] {
    let mut e = [0u8; PARTITION_TABLE_ENTRY_SIZE];
    e[OFF_BOOTABLE] = bootable;
    e[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4].copy_from_slice(&first_abs.to_le_bytes());
    e[OFF_LENGTH..OFF_LENGTH + 4].copy_from_slice(&length.to_le_bytes());
    e
}

/// Pack a synthetic entry including the filesystem type byte.
#[inline(always)]
pub fn make_entry_with_type(
    bootable: u8,
    ty: u8,
    first_abs: u32,
    length: u32,
) -> [u8; PARTITION_TABLE_ENTRY_SIZE] {
    let mut e = make_entry(bootable, first_abs, length);
    e[OFF_TYPE] = ty;
    e
}

#[inline(always)]
fn is_extended_partition_type(ty: u8) -> bool {
    ty == 0x05 || ty == 0x0f || ty == 0xc5 || ty == 0xd5
}

/// FASM-faithful 64-bit `ebp + FirstAbsSector` (zero-extended high via `adc`).
#[inline(always)]
pub fn partition_start_from_mbr(mbr_ebr_sector: u32, first_abs_sector: u32) -> (u32, u32) {
    let sum = (mbr_ebr_sector as u64).wrapping_add(first_abs_sector as u64);
    (sum as u32, (sum >> 32) as u32)
}

/// Independent FASM-flow oracle for Cut CC (does not call the Rust helper body).
#[inline(always)]
pub fn fasm_oracle_process_partition_table_entry(
    entry: &[u8; PARTITION_TABLE_ENTRY_SIZE],
    mbr_ebr_sector: u32,
    capacity: u64,
    extended_out: &mut u32,
    add_calls: &mut dyn FnMut(u32, u32, u32, u32),
) {
    if !is_partition_table_entry(
        entry[OFF_BOOTABLE],
        u32::from_le_bytes(entry[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4].try_into().unwrap()),
        u32::from_le_bytes(entry[OFF_LENGTH..OFF_LENGTH + 4].try_into().unwrap()),
        mbr_ebr_sector,
        capacity,
    ) {
        return;
    }
    let ty = entry[OFF_TYPE];
    if ty == 0 {
        return;
    }
    if is_extended_partition_type(ty) {
        *extended_out = u32::from_le_bytes(
            entry[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4]
                .try_into()
                .unwrap(),
        );
        return;
    }
    let first = u32::from_le_bytes(
        entry[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4]
            .try_into()
            .unwrap(),
    );
    let length = u32::from_le_bytes(entry[OFF_LENGTH..OFF_LENGTH + 4].try_into().unwrap());
    let (start_lo, start_hi) = partition_start_from_mbr(mbr_ebr_sector, first);
    add_calls(start_lo, start_hi, length, 0);
}

/// Parse one MBR/EBR partition-table slot (`kernel/blkdev/disk.inc`).
///
/// * Invalid entry (Cut Z rules) → no-op.
/// * Empty type → no-op.
/// * Extended type → writes `FirstAbsSector` to `*extended_out`.
/// * Normal partition → invokes `add_partition` with computed start + length.
///
/// # Safety
/// `entry` readable for 16 bytes; `extended_out` writable when extended path taken.
#[inline(always)]
pub unsafe fn process_partition_table_entry(
    entry: *const u8,
    mbr_ebr_sector: u32,
    capacity_lo: u32,
    capacity_hi: u32,
    extended_out: *mut u32,
    disk: u32,
    add_partition: DiskAddPartitionFn,
) {
    if unsafe {
        is_partition_table_entry_ptr(entry, mbr_ebr_sector, capacity_lo, capacity_hi)
    } != 0
    {
        return;
    }
    let ty = unsafe { *entry.add(OFF_TYPE) };
    if ty == 0 {
        return;
    }
    if is_extended_partition_type(ty) {
        let first = unsafe { read_u32_le(entry.add(OFF_FIRST_ABS_SECTOR)) };
        unsafe { *extended_out = first };
        return;
    }
    let first = unsafe { read_u32_le(entry.add(OFF_FIRST_ABS_SECTOR)) };
    let length = unsafe { read_u32_le(entry.add(OFF_LENGTH)) };
    let (start_lo, start_hi) = partition_start_from_mbr(mbr_ebr_sector, first);
    unsafe { add_partition(start_lo, start_hi, length, 0, disk) };
}

/// FASM-faithful GPT protective-MBR check (ZF set = protective).
///
/// Returns `true` when the MBR is a protective GPT MBR (legacy sets ZF).
///
/// `pre_table_word` is the word at `ecx-2` (MBR+0x1BC). Only the **low**
/// dword of disk capacity participates (legacy reads
/// `DISK.MediaInfo.Capacity+0` only).
#[inline(always)]
pub fn is_protective_mbr(
    pre_table_word: u16,
    entry0: &[u8; PARTITION_TABLE_ENTRY_SIZE],
    entries_1_3: &[u8; PROTECTIVE_TRAILING_BYTES],
    capacity_lo: u32,
) -> bool {
    // cmp [ecx-2], ax  (ax=0)
    if pre_table_word != 0 {
        return false;
    }
    // cmp [ecx+0], al  (al=0) — bootable must be exactly 0 (not 0x80)
    if entry0[OFF_BOOTABLE] != 0 {
        return false;
    }
    // cmp byte[ecx+4], 0xEE
    if entry0[OFF_TYPE] != PROTECTIVE_MBR_TYPE {
        return false;
    }
    // cmp dword[ecx+8], 1
    let first = u32::from_le_bytes([
        entry0[OFF_FIRST_ABS_SECTOR],
        entry0[OFF_FIRST_ABS_SECTOR + 1],
        entry0[OFF_FIRST_ABS_SECTOR + 2],
        entry0[OFF_FIRST_ABS_SECTOR + 3],
    ]);
    if first != 1 {
        return false;
    }
    // Length == -1 OR Length == (-1 + Capacity_lo)
    let length = u32::from_le_bytes([
        entry0[OFF_LENGTH],
        entry0[OFF_LENGTH + 1],
        entry0[OFF_LENGTH + 2],
        entry0[OFF_LENGTH + 3],
    ]);
    if length != 0xFFFF_FFFF {
        let expected = 0xFFFF_FFFFu32.wrapping_add(capacity_lo);
        if length != expected {
            return false;
        }
    }
    // repz scasw over entries 1–3 (48 bytes / 24 words)
    entries_1_3.iter().all(|&b| b == 0)
}

/// Pointer-form wrapper for the FFI boundary.
///
/// Returns `0` = protective (ZF set via `test eax,eax`), `1` = not protective.
///
/// `pt` points at the partition-table array (`ecx` / MBR+0x1BE). The word at
/// `pt-2` must be readable.
///
/// # Safety
/// `pt-2` .. `pt+64` must be readable (pre-word + 4×16-byte entries).
#[inline(always)]
pub unsafe fn is_protective_mbr_ptr(pt: *const u8, capacity_lo: u32) -> u32 {
    let pre = unsafe { read_u16_le(pt.sub(2)) };
    let mut entry0 = [0u8; PARTITION_TABLE_ENTRY_SIZE];
    unsafe {
        core::ptr::copy_nonoverlapping(pt, entry0.as_mut_ptr(), PARTITION_TABLE_ENTRY_SIZE);
    }
    let mut trail = [0u8; PROTECTIVE_TRAILING_BYTES];
    unsafe {
        core::ptr::copy_nonoverlapping(pt.add(16), trail.as_mut_ptr(), PROTECTIVE_TRAILING_BYTES);
    }
    if is_protective_mbr(pre, &entry0, &trail, capacity_lo) {
        0
    } else {
        1
    }
}

#[inline(always)]
unsafe fn read_u16_le(p: *const u8) -> u16 {
    let b = unsafe { core::slice::from_raw_parts(p, 2) };
    u16::from_le_bytes([b[0], b[1]])
}

/// Build a synthetic MBR sector with partition table at `0x1BE` for tests.
///
/// Returns `(sector, pt_offset)` where `pt_offset == 0x1BE`.
#[inline(always)]
pub fn make_mbr_sector(
    pre_table_word: u16,
    entry0: &[u8; PARTITION_TABLE_ENTRY_SIZE],
    entries_1_3: &[u8; PROTECTIVE_TRAILING_BYTES],
) -> ([u8; 512], usize) {
    let mut sector = [0u8; 512];
    let pt = 0x1BE;
    sector[pt - 2..pt].copy_from_slice(&pre_table_word.to_le_bytes());
    sector[pt..pt + 16].copy_from_slice(entry0);
    sector[pt + 16..pt + 64].copy_from_slice(entries_1_3);
    sector[0x1FE] = 0x55;
    sector[0x1FF] = 0xAA;
    (sector, pt)
}

/// Canonical protective entry0: boot=0, type=0xEE, first=1, length=0xFFFFFFFF.
#[inline(always)]
pub fn make_protective_entry0(length: u32) -> [u8; PARTITION_TABLE_ENTRY_SIZE] {
    let mut e = [0u8; PARTITION_TABLE_ENTRY_SIZE];
    e[OFF_BOOTABLE] = 0;
    e[OFF_TYPE] = PROTECTIVE_MBR_TYPE;
    e[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4].copy_from_slice(&1u32.to_le_bytes());
    e[OFF_LENGTH..OFF_LENGTH + 4].copy_from_slice(&length.to_le_bytes());
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle for Cut AD (branch order + wrapping length).
    fn protective_oracle(
        pre_table_word: u16,
        entry0: &[u8; PARTITION_TABLE_ENTRY_SIZE],
        entries_1_3: &[u8; PROTECTIVE_TRAILING_BYTES],
        capacity_lo: u32,
    ) -> bool {
        if pre_table_word != 0 {
            return false;
        }
        if entry0[OFF_BOOTABLE] != 0 {
            return false;
        }
        if entry0[OFF_TYPE] != PROTECTIVE_MBR_TYPE {
            return false;
        }
        let first = u32::from_le_bytes(entry0[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4].try_into().unwrap());
        if first != 1 {
            return false;
        }
        let length = u32::from_le_bytes(entry0[OFF_LENGTH..OFF_LENGTH + 4].try_into().unwrap());
        if length != 0xFFFF_FFFF {
            let mut edi = 0xFFFF_FFFFu32;
            edi = edi.wrapping_add(capacity_lo);
            if length != edi {
                return false;
            }
        }
        entries_1_3.iter().all(|&b| b == 0)
    }

    fn check_protective(
        pre: u16,
        entry0: &[u8; PARTITION_TABLE_ENTRY_SIZE],
        trail: &[u8; PROTECTIVE_TRAILING_BYTES],
        capacity_lo: u32,
    ) {
        let rust = is_protective_mbr(pre, entry0, trail, capacity_lo);
        let ora = protective_oracle(pre, entry0, trail, capacity_lo);
        assert_eq!(rust, ora, "pre={pre:#x} cap_lo={capacity_lo:#x} e0={entry0:?}");
        let (sector, pt) = make_mbr_sector(pre, entry0, trail);
        let ptr_r = unsafe { is_protective_mbr_ptr(sector.as_ptr().add(pt), capacity_lo) };
        assert_eq!(ptr_r == 0, rust);
    }

    /// Independent FASM-flow oracle: explicit edx:eax add/adc + shr/rcr + sub/sbb.
    fn oracle(
        bootable: u8,
        first_abs: u32,
        length: u32,
        ebp_base: u32,
        capacity: u64,
    ) -> bool {
        if (bootable & 0x7F) != 0 {
            return false;
        }
        // mov eax, ebp / xor edx, edx
        let mut eax = ebp_base;
        let mut edx = 0u32;
        // add eax, FirstAbsSector / adc edx, 0
        let (a, c1) = eax.overflowing_add(first_abs);
        eax = a;
        edx = edx.wrapping_add(u32::from(c1));
        // add eax, Length / adc edx, 0
        let (a2, c2) = eax.overflowing_add(length);
        eax = a2;
        edx = edx.wrapping_add(u32::from(c2));
        // shr edx, 1 / rcr eax, 1
        let lsb = eax & 1;
        eax = (eax >> 1) | ((edx & 1) << 31);
        edx >>= 1;
        let _ = lsb;
        let half = ((edx as u64) << 32) | (eax as u64);
        // sub/sbb vs capacity; jnc → invalid
        half < capacity
    }

    fn check(
        bootable: u8,
        first: u32,
        length: u32,
        ebp: u32,
        capacity: u64,
    ) {
        let rust = is_partition_table_entry(bootable, first, length, ebp, capacity);
        let ora = oracle(bootable, first, length, ebp, capacity);
        assert_eq!(
            rust, ora,
            "boot={bootable:#x} first={first:#x} len={length:#x} ebp={ebp:#x} cap={capacity:#x}"
        );
        let entry = make_entry(bootable, first, length);
        let ptr_r = unsafe {
            is_partition_table_entry_ptr(
                entry.as_ptr(),
                ebp,
                capacity as u32,
                (capacity >> 32) as u32,
            )
        };
        assert_eq!(ptr_r == 0, rust);
    }

    #[test]
    fn empty_entry_small_disk_valid() {
        // Bootable=0, first=0, length=0 → half=0; capacity>0 → valid
        check(0, 0, 0, 0, 1);
        check(0x80, 0, 0, 0, 1);
    }

    #[test]
    fn empty_entry_zero_capacity_invalid() {
        // half=0, capacity=0 → 0 < 0 is false → invalid
        check(0, 0, 0, 0, 0);
    }

    #[test]
    fn bootable_mask() {
        check(0x00, 0, 1, 0, 100);
        check(0x80, 0, 1, 0, 100);
        for bad in [0x01u8, 0x7F, 0x81, 0xFF, 0x40, 0xC0] {
            check(bad, 0, 1, 0, 100);
            assert!(!is_partition_table_entry(bad, 0, 1, 0, 100));
        }
    }

    #[test]
    fn half_capacity_boundary() {
        // sum=10 → half=5; capacity=5 → 5<5 false → invalid
        check(0, 3, 7, 0, 5);
        // capacity=6 → valid
        check(0, 3, 7, 0, 6);
        // odd sum: sum=11 → half=5
        check(0, 4, 7, 0, 5);
        check(0, 4, 7, 0, 6);
    }

    #[test]
    fn ebp_base_included() {
        // ebp=100, first=10, length=10 → sum=120 → half=60
        check(0, 10, 10, 100, 60);
        check(0, 10, 10, 100, 61);
    }

    #[test]
    fn overflow_64bit_wrap() {
        // ebp=0xFFFF_FFFF, first=1, length=1 → sum wraps to 1 → half=0
        check(0, 1, 1, 0xFFFF_FFFF, 1);
        check(0, 1, 1, 0xFFFF_FFFF, 0);
        // Large high half
        check(0, 0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, u64::MAX);
        check(0, 0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, 0);
    }

    #[test]
    fn twice_media_slack() {
        // Comment in disk.inc: length may slightly exceed media; /2 allows up to ~2×.
        // first=0, length=capacity*2-1 → half = capacity-1 (if even path) …
        // length = 2*cap - 1, ebp=0, first=0 → sum=2*cap-1 → half=cap-1 < cap → valid
        let cap = 1_000_000u64;
        check(0, 0, (2 * cap - 1) as u32, 0, cap);
        // length = 2*cap → half=cap → invalid
        check(0, 0, (2 * cap) as u32, 0, cap);
    }

    #[test]
    fn high_capacity_qword() {
        let cap = 0x1_0000_0000u64; // 2^32
        check(0, 0, 1, 0, cap);
        // sum = 2^33 → half = 2^32 == cap → invalid
        check(0, 0xFFFF_FFFF, 0xFFFF_FFFF, 2, cap);
    }

    #[test]
    fn ptr_form_reads_le_fields() {
        let mut e = [0u8; 16];
        e[0] = 0x80;
        e[8..12].copy_from_slice(&0x10u32.to_le_bytes());
        e[12..16].copy_from_slice(&0x20u32.to_le_bytes());
        // sum=0x30 → half=0x18; cap=0x19 → valid
        let r = unsafe { is_partition_table_entry_ptr(e.as_ptr(), 0, 0x19, 0) };
        assert_eq!(r, 0);
        let r2 = unsafe { is_partition_table_entry_ptr(e.as_ptr(), 0, 0x18, 0) };
        assert_eq!(r2, 1);
    }

    #[test]
    fn prng_corpus_50k() {
        let mut state = IS_PARTITION_TABLE_ENTRY_PRNG_SEED;
        let mut next = || {
            state = state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            state
        };
        for _ in 0..50_000 {
            let boot = (next() & 0xFF) as u8;
            // Bias toward legal bootable values sometimes
            let bootable = if next() & 7 == 0 {
                if next() & 1 == 0 { 0 } else { 0x80 }
            } else {
                boot
            };
            let first = next();
            let length = next();
            let ebp = next();
            let cap_lo = next();
            let cap_hi = next() & 0xFFFF; // keep range varied but not always huge
            let capacity = ((cap_hi as u64) << 32) | (cap_lo as u64);
            check(bootable, first, length, ebp, capacity);
        }
    }

    #[test]
    fn exhaustive_bootable_byte() {
        for b in 0u8..=255 {
            let valid_boot = (b & 0x7F) == 0;
            let r = is_partition_table_entry(b, 0, 1, 0, 100);
            if !valid_boot {
                assert!(!r, "boot={b:#x} should be invalid");
            } else {
                assert!(r, "boot={b:#x} with small part should be valid");
            }
        }
    }

    // --- Cut AD: is_protective_mbr ---

    #[test]
    fn protective_canonical_ffffffff() {
        let e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        check_protective(0, &e0, &trail, 0);
        check_protective(0, &e0, &trail, 100);
        check_protective(0, &e0, &trail, 0xFFFF_FFFF);
    }

    #[test]
    fn protective_length_capacity_minus_one() {
        let cap = 1_000_000u32;
        let e0 = make_protective_entry0(cap.wrapping_sub(1));
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        check_protective(0, &e0, &trail, cap);
        // Wrong length
        let bad = make_protective_entry0(cap);
        check_protective(0, &bad, &trail, cap);
        assert!(!is_protective_mbr(0, &bad, &trail, cap));
    }

    #[test]
    fn protective_capacity_zero_length_wrap() {
        // -1 + 0 = 0xFFFFFFFF — same as canonical all-ones length
        let e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        check_protective(0, &e0, &trail, 0);
        // length 0 with capacity 1 → expected = 0; protective
        let e0z = make_protective_entry0(0);
        check_protective(0, &e0z, &trail, 1);
        assert!(is_protective_mbr(0, &e0z, &trail, 1));
    }

    #[test]
    fn protective_rejects_pre_word() {
        let e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        check_protective(1, &e0, &trail, 100);
        assert!(!is_protective_mbr(1, &e0, &trail, 100));
        check_protective(0xAA55, &e0, &trail, 100);
    }

    #[test]
    fn protective_rejects_bootable_nonzero() {
        let mut e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        e0[OFF_BOOTABLE] = 0x80;
        check_protective(0, &e0, &trail, 100);
        assert!(!is_protective_mbr(0, &e0, &trail, 100));
    }

    #[test]
    fn protective_rejects_wrong_type() {
        let mut e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        e0[OFF_TYPE] = 0x0B;
        check_protective(0, &e0, &trail, 100);
        assert!(!is_protective_mbr(0, &e0, &trail, 100));
    }

    #[test]
    fn protective_rejects_wrong_first_lba() {
        let mut e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        e0[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4]
            .copy_from_slice(&0u32.to_le_bytes());
        check_protective(0, &e0, &trail, 100);
        assert!(!is_protective_mbr(0, &e0, &trail, 100));
        e0[OFF_FIRST_ABS_SECTOR..OFF_FIRST_ABS_SECTOR + 4]
            .copy_from_slice(&2u32.to_le_bytes());
        check_protective(0, &e0, &trail, 100);
    }

    #[test]
    fn protective_rejects_nonzero_trailing() {
        let e0 = make_protective_entry0(0xFFFF_FFFF);
        let mut trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        trail[0] = 1;
        check_protective(0, &e0, &trail, 100);
        assert!(!is_protective_mbr(0, &e0, &trail, 100));
        trail[0] = 0;
        trail[47] = 0xFF;
        check_protective(0, &e0, &trail, 100);
    }

    #[test]
    fn protective_ptr_reads_pre_word_and_fields() {
        let e0 = make_protective_entry0(0xFFFF_FFFF);
        let trail = [0u8; PROTECTIVE_TRAILING_BYTES];
        let (mut sector, pt) = make_mbr_sector(0, &e0, &trail);
        let r = unsafe { is_protective_mbr_ptr(sector.as_ptr().add(pt), 500) };
        assert_eq!(r, 0);
        sector[pt - 2] = 1;
        let r2 = unsafe { is_protective_mbr_ptr(sector.as_ptr().add(pt), 500) };
        assert_eq!(r2, 1);
    }

    #[test]
    fn protective_prng_corpus_50k() {
        let mut state = IS_PROTECTIVE_MBR_PRNG_SEED;
        let mut next = || {
            state = state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            state
        };
        for _ in 0..50_000 {
            let pre = (next() & 0xFFFF) as u16;
            // Bias toward canonical protective shapes sometimes
            let (pre, e0, trail, cap) = if next() & 0xF == 0 {
                let cap = next();
                let len = if next() & 1 == 0 {
                    0xFFFF_FFFF
                } else {
                    0xFFFF_FFFFu32.wrapping_add(cap)
                };
                (0u16, make_protective_entry0(len), [0u8; PROTECTIVE_TRAILING_BYTES], cap)
            } else {
                let mut e0 = [0u8; PARTITION_TABLE_ENTRY_SIZE];
                for b in &mut e0 {
                    *b = (next() & 0xFF) as u8;
                }
                let mut trail = [0u8; PROTECTIVE_TRAILING_BYTES];
                for b in &mut trail {
                    *b = (next() & 0xFF) as u8;
                }
                (pre, e0, trail, next())
            };
            check_protective(pre, &e0, &trail, cap);
        }
    }

    // --- Cut CC: process_partition_table_entry ---

    struct MockAdd {
        calls: Vec<(u32, u32, u32, u32)>,
    }

    fn run_cc(
        entry: &[u8; PARTITION_TABLE_ENTRY_SIZE],
        mbr: u32,
        cap_lo: u32,
        cap_hi: u32,
        extended_inout: &mut u32,
    ) -> Vec<(u32, u32, u32, u32)> {
        use std::cell::Cell;
        thread_local! {
            static MOCK: Cell<*mut MockAdd> = const { Cell::new(core::ptr::null_mut()) };
        }
        let mut mock = MockAdd { calls: Vec::new() };
        unsafe extern "stdcall" fn shim(
            start_lo: u32,
            start_hi: u32,
            length_lo: u32,
            length_hi: u32,
            _disk: u32,
        ) {
            MOCK.with(|cell| {
                let ptr = cell.get();
                assert!(!ptr.is_null());
                // SAFETY: test-only — set immediately before each call.
                unsafe { (*ptr).calls.push((start_lo, start_hi, length_lo, length_hi)) };
            });
        }
        MOCK.with(|cell| cell.set(&mut mock as *mut MockAdd));
        unsafe {
            process_partition_table_entry(
                entry.as_ptr(),
                mbr,
                cap_lo,
                cap_hi,
                extended_inout,
                0xD15C_0000,
                shim,
            );
        }
        MOCK.with(|cell| cell.set(core::ptr::null_mut()));
        mock.calls
    }

    fn check_cc(
        entry: &[u8; PARTITION_TABLE_ENTRY_SIZE],
        mbr: u32,
        cap_lo: u32,
        cap_hi: u32,
        extended_inout: &mut u32,
    ) {
        let cap = ((cap_hi as u64) << 32) | (cap_lo as u64);
        let mut oracle_ext = *extended_inout;
        let mut oracle_calls: Vec<(u32, u32, u32, u32)> = Vec::new();
        fasm_oracle_process_partition_table_entry(
            entry,
            mbr,
            cap,
            &mut oracle_ext,
            &mut |a, b, c, d| oracle_calls.push((a, b, c, d)),
        );
        let got = run_cc(entry, mbr, cap_lo, cap_hi, extended_inout);
        assert_eq!(*extended_inout, oracle_ext);
        assert_eq!(got, oracle_calls);
    }

    #[test]
    fn cc_invalid_entry_no_op() {
        let e = make_entry_with_type(0x01, 0x07, 0, 10); // illegal bootable
        let mut ext = 0xBEEFu32;
        check_cc(&e, 0, 100, 0, &mut ext);
        assert_eq!(ext, 0xBEEF);
    }

    #[test]
    fn cc_empty_type_no_op() {
        let e = make_entry_with_type(0, 0, 10, 20);
        let mut ext = 0u32;
        check_cc(&e, 0, 100, 0, &mut ext);
    }

    #[test]
    fn cc_extended_writes_stack_slot() {
        for &ty in &EXTENDED_PARTITION_TYPES {
            let e = make_entry_with_type(0, ty, 0x1234, 0);
            let mut ext = 0u32;
            check_cc(&e, 0, 0x10000, 0, &mut ext);
            assert_eq!(ext, 0x1234);
        }
    }

    #[test]
    fn cc_normal_add_partition() {
        let e = make_entry_with_type(0, 0x07, 100, 500);
        let mut ext = 0u32;
        let calls = run_cc(&e, 1000, 0x10000, 0, &mut ext);
        assert_eq!(calls, vec![(1100, 0, 500, 0)]);
    }

    #[test]
    fn cc_start_carry_to_high_dword() {
        let e = make_entry_with_type(0, 0x83, 0xFFFF_FFFF, 1);
        let mut ext = 0u32;
        let calls = run_cc(&e, 1, 0xFFFF_FFFF, 0, &mut ext);
        assert_eq!(calls, vec![(0, 1, 1, 0)]);
    }

    #[test]
    fn cc_prng_corpus_50k() {
        let mut state = PROCESS_PARTITION_TABLE_ENTRY_PRNG_SEED;
        let mut next = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            state
        };
        for _ in 0..50_000 {
            let bootable = if next() & 7 == 0 {
                if next() & 1 == 0 { 0 } else { 0x80 }
            } else {
                (next() & 0xFF) as u8
            };
            let ty = (next() & 0xFF) as u8;
            let first = next();
            let length = next();
            let mbr = next();
            let cap_lo = next();
            let cap_hi = next() & 0xFFFF;
            let e = make_entry_with_type(bootable, ty, first, length);
            let mut ext = next();
            check_cc(&e, mbr, cap_lo, cap_hi, &mut ext);
        }
    }
}
