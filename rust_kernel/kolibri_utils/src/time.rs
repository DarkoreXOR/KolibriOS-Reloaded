//! Cut G: `fsCalculateTime` — BDFE datetime → seconds since 2001-01-01.
//! Cut T: `fsTime2bdfe` — seconds since 2001-01-01 → BDFE datetime (EDI+=8).
//! Cut AE: `ntfs_datetime_to_bdfe` — NTFS FILETIME (1601×10⁷) → BDFE (EDI+=8).
//! Cut AF: `ntfsCalculateTime` — BDFE → NTFS FILETIME (1601×10⁷); inverse of AE.
//! Cut AK: `xfs._.conv_bigtime_to_kos_epoch` — XFS v5 bigtime (ns) → BDFE (EDI+=8).
//! Cut AL: `ext_read_time` — EXT/ext4 Unix (+extra epoch bits) → BDFE (EDI+=8).
//!
//! Matches `kernel/fs/fs_common.inc` / `ntfs.inc` / `xfs.asm` / `ext.inc` FASM leaf
//! semantics for the production domain (year &lt; 3025 so `BH` pollution after
//! `shr ebx,2` does not apply). Month tables are stack-materialized so the
//! freestanding FFI section stays reloc-free (no `.rodata` absolute loads).

/// NTFS FILETIME bias: 2001-01-01 00:00:00 as 100ns ticks since 1601-01-01.
/// FASM `sub eax, 3365781504` / `sbb edx, 29389701`.
pub const NTFS_FILETIME_BIAS_LO: u32 = 3365781504;
pub const NTFS_FILETIME_BIAS_HI: u32 = 29389701;
/// 10_000_000 = 100ns units per second (FASM `mov ecx, 10000000`).
pub const NTFS_FILETIME_PER_SEC: u32 = 10_000_000;

/// PRNG seed for Cut AE differential corpus (`'CUTE'`).
pub const NTFS_DATETIME_TO_BDFE_PRNG_SEED: u32 = 0x4355_5445;

/// PRNG seed for Cut AF differential corpus (`'CUTF'`).
pub const NTFS_CALCULATE_TIME_PRNG_SEED: u32 = 0x4355_5446;

/// XFS v5 bigtime → KOS epoch constants (`xfs.asm` Cut AK).
/// `BIGTIME_TO_KOS_OFFSET_NS = (0x80000000 + (365*31+8)*86400) * 1_000_000_000`.
pub const XFS_NANOSEC_PER_SEC: u32 = 1_000_000_000;
pub const XFS_BIGTIME_TO_KOS_OFFSET_NS_LO: u32 = 0x1135_0000; // 288_686_080
pub const XFS_BIGTIME_TO_KOS_OFFSET_NS_HI: u32 = 0x2B61_0A37; // 727_779_895
/// Full 64-bit bias (host helpers / tests).
pub const XFS_BIGTIME_TO_KOS_OFFSET_NS: u64 =
    ((XFS_BIGTIME_TO_KOS_OFFSET_NS_HI as u64) << 32) | (XFS_BIGTIME_TO_KOS_OFFSET_NS_LO as u64);

/// PRNG seed for Cut AK differential corpus (`'CUTK'`).
pub const XFS_CONV_BIGTIME_TO_KOS_EPOCH_PRNG_SEED: u32 = 0x4355_544B;

/// Unix epoch → Kolibri 2001-01-01 seconds (`fs_lfn.inc` / `ext.inc`).
/// FASM `(365*31+8)*24*60*60` = 978_307_200.
pub const UNIXTIME_TO_KOS_OFFSET: u32 = 978_307_200;

/// PRNG seed for Cut AL differential corpus (`'CUTL'`).
pub const EXT_READ_TIME_PRNG_SEED: u32 = 0x4355_544C;

/// BDFE-style datetime block layout (same offsets as FASM / `fsTime2bdfe`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BdfeTime {
    pub sec: u8,
    pub min: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

impl BdfeTime {
    /// Pack into the 8-byte memory layout FASM expects at `ESI`.
    #[inline(always)]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0] = self.sec;
        b[1] = self.min;
        b[2] = self.hour;
        // b[3] pad
        b[4] = self.day;
        b[5] = self.month;
        b[6] = self.year as u8;
        b[7] = (self.year >> 8) as u8;
        b
    }

    /// Parse from an 8-byte BDFE block (FASM `ESI` layout).
    #[inline(always)]
    pub fn from_bytes(b: &[u8; 8]) -> Self {
        Self {
            sec: b[0],
            min: b[1],
            hour: b[2],
            day: b[4],
            month: b[5],
            year: u16::from_le_bytes([b[6], b[7]]),
        }
    }
}

/// FASM `months` then `months2` (24 bytes) — stack-materialized with volatile
/// immediates so LLVM cannot promote the table to `.rodata` (reloc-free blob).
#[inline(always)]
fn materialize_month_tables(out: &mut [u8; 24]) {
    // SAFETY: `out` is a local 24-byte array; volatile prevents .rodata promotion.
    unsafe {
        let p = out.as_mut_ptr();
        // months (non-leap)
        core::ptr::write_volatile(p.add(0), 31);
        core::ptr::write_volatile(p.add(1), 28);
        core::ptr::write_volatile(p.add(2), 31);
        core::ptr::write_volatile(p.add(3), 30);
        core::ptr::write_volatile(p.add(4), 31);
        core::ptr::write_volatile(p.add(5), 30);
        core::ptr::write_volatile(p.add(6), 31);
        core::ptr::write_volatile(p.add(7), 31);
        core::ptr::write_volatile(p.add(8), 30);
        core::ptr::write_volatile(p.add(9), 31);
        core::ptr::write_volatile(p.add(10), 30);
        core::ptr::write_volatile(p.add(11), 31);
        // months2 (leap)
        core::ptr::write_volatile(p.add(12), 31);
        core::ptr::write_volatile(p.add(13), 29);
        core::ptr::write_volatile(p.add(14), 31);
        core::ptr::write_volatile(p.add(15), 30);
        core::ptr::write_volatile(p.add(16), 31);
        core::ptr::write_volatile(p.add(17), 30);
        core::ptr::write_volatile(p.add(18), 31);
        core::ptr::write_volatile(p.add(19), 31);
        core::ptr::write_volatile(p.add(20), 30);
        core::ptr::write_volatile(p.add(21), 31);
        core::ptr::write_volatile(p.add(22), 30);
        core::ptr::write_volatile(p.add(23), 31);
    }
}

/// Convert BDFE datetime to seconds since 2001-01-01 (FASM `fsCalculateTime`).
///
/// # Safety / domain
/// Matches FASM for years where `years/4 < 256` (year &lt; 3025) and for month
/// indices that only read within the 24-byte concatenated tables when
/// interpreted as FASM does. Host tests cover the production domain densely.
#[inline(always)]
pub fn fs_calculate_time(t: BdfeTime) -> u32 {
    let year = t.year as u32;
    let mut years = year.wrapping_sub(2001);
    // FASM: `sub eax, 2001` / `jnc` else `xor eax,eax`
    if year < 2001 {
        years = 0;
    }

    let mut tables = [0u8; 24];
    materialize_month_tables(&mut tables);
    // Leap table when `(years + 1) & 3 == 0` — matches FASM `inc`/`test`/`jnz`.
    let table_base: usize = if ((years + 1) & 3) == 0 { 12 } else { 0 };

    // Sum days in months before `month` (FASM loop over table).
    // month byte is used as `movzx` then `dec` then loop `dec`/`js`.
    let mut month_idx = (t.month as i32).wrapping_sub(1);
    let mut month_sum: u32 = 0;
    loop {
        month_idx = month_idx.wrapping_sub(1);
        if month_idx < 0 {
            break;
        }
        // FASM indexes `[edx+eax]` with eax after dec — for valid months 1..12
        // this stays in 0..11 relative to the selected 12-byte table.
        // For out-of-range months, FASM can read past the selected table into
        // the twin / following memory; we mirror the concatenated 24-byte layout.
        let idx = table_base.wrapping_add(month_idx as usize);
        let day_len = if idx < 24 {
            // SAFETY: idx verified < 24.
            unsafe { core::ptr::read_volatile(tables.as_ptr().add(idx)) as u32 }
        } else {
            0
        };
        month_sum = month_sum.wrapping_add(day_len);
    }

    // Production domain: years < 1024 ⇒ after `shr ebx,2`, BH=0, so `mov bl`
    // zero-extends for subsequent adds — pure u32 math matches FASM.
    let mut days = years
        .wrapping_mul(365)
        .wrapping_add(years >> 2)
        .wrapping_add(month_sum);
    days = days.wrapping_sub(1).wrapping_add(t.day as u32);

    let mut total = days.wrapping_mul(24).wrapping_add(t.hour as u32);
    total = total.wrapping_mul(60).wrapping_add(t.min as u32);
    total = total.wrapping_mul(60).wrapping_add(t.sec as u32);
    total
}

/// Pointer form used by the FFI trampoline.
///
/// # Safety
/// `block` must point to a readable 8-byte BDFE datetime.
#[inline(always)]
pub unsafe fn fs_calculate_time_ptr(block: *const u8) -> u32 {
    let mut b = [0u8; 8];
    // SAFETY: caller guarantees 8 readable bytes (kernel trampoline / tests).
    unsafe {
        core::ptr::copy_nonoverlapping(block, b.as_mut_ptr(), 8);
    }
    fs_calculate_time(BdfeTime::from_bytes(&b))
}

/// Convert seconds since 2001-01-01 to BDFE datetime (FASM `fsTime2bdfe`).
///
/// Mirrors `fs_common.inc`: divide chain → leap-day adjust with signed `jns`
/// after `sub edx, years/4` → month peel over `months`/`months2` using the
/// 16-bit `DX` (`sub dl` / `dec dh` / `jns`) loop. Production domain: years
/// where the day-of-year remainder stays within FASM's DX handling.
#[inline(always)]
pub fn fs_time2bdfe(secs: u32) -> BdfeTime {
    // xor edx,edx; mov ecx,60; div; mov [edi],dl
    let mut eax = secs;
    let sec = (eax % 60) as u8;
    eax /= 60;
    // xor edx,edx; div ecx; mov [edi+1],dl
    let min = (eax % 60) as u8;
    eax /= 60;
    // xor edx,edx; mov cl,24; div; mov [edi+2],dx  (hour as word; pad cleared)
    let hour = (eax % 24) as u8;
    eax /= 24;
    // xor edx,edx; mov cx,365; div
    let mut edx = eax % 365;
    eax /= 365;
    let mut ebx = eax.wrapping_add(2001); // calendar year candidate
    let leaps = eax >> 2;
    // sub edx, eax(=leaps); jns → else dec year / add 365 / leap +1
    let (subbed, _) = edx.overflowing_sub(leaps);
    if (subbed as i32) < 0 {
        ebx = ebx.wrapping_sub(1);
        edx = subbed.wrapping_add(365);
        if (ebx & 3) == 0 {
            edx = edx.wrapping_add(1);
        }
    } else {
        edx = subbed;
    }

    let mut tables = [0u8; 24];
    materialize_month_tables(&mut tables);
    // FASM: ecx = months-1; if (year&3)==0 then ecx += 12 (months2)
    let table_base: usize = if (ebx & 3) == 0 { 12 } else { 0 };

    // Month peel: eax = month counter; DX = day-of-year (16-bit semantics).
    let mut month: u32 = 0;
    let mut dx = (edx & 0xffff) as u16;
    loop {
        month = month.wrapping_add(1);
        let idx = table_base.wrapping_add((month as usize).wrapping_sub(1));
        let mlen = if idx < 24 {
            // SAFETY: idx verified < 24.
            unsafe { core::ptr::read_volatile(tables.as_ptr().add(idx)) as u16 }
        } else {
            0
        };
        // sub dl, [ecx]; jnc @b
        let (new_dl, borrow) = (dx as u8).overflowing_sub(mlen as u8);
        if !borrow {
            dx = (dx & 0xff00) | (new_dl as u16);
            continue;
        }
        // CF set: write new_dl into DL, then dec dh / jns @b
        let dh = (dx >> 8) as u8;
        let (new_dh, _) = dh.overflowing_sub(1);
        dx = ((new_dh as u16) << 8) | (new_dl as u16);
        if (new_dh as i8) >= 0 {
            continue;
        }
        // Restore oversubtraction; day is 1-based.
        let day_u8 = new_dl.wrapping_add(mlen as u8).wrapping_add(1);
        return BdfeTime {
            sec,
            min,
            hour,
            day: day_u8,
            month: month as u8,
            year: ebx as u16,
        };
    }
}

/// Pointer form used by the FFI trampoline — writes 8 BDFE bytes at `out`.
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[inline(always)]
pub unsafe fn fs_time2bdfe_ptr(secs: u32, out: *mut u8) {
    let t = fs_time2bdfe(secs);
    let b = t.to_bytes();
    // SAFETY: caller guarantees 8 writable bytes (kernel trampoline / tests).
    unsafe {
        core::ptr::copy_nonoverlapping(b.as_ptr(), out, 8);
    }
}

/// Convert NTFS FILETIME dwords to Kolibri seconds since 2001-01-01.
///
/// Mirrors `ntfs_datetime_to_bdfe` bias/`sbb`/clamp/`div` before the
/// `jmp fsTime2bdfe`. Pre-2001 inputs wrap (unsigned underflow). When the
/// post-bias high dword is `>= 10_000_000`, FASM zeros EDX before dividing
/// (quotient then uses the low dword only).
#[inline(always)]
pub fn ntfs_filetime_to_secs(filetime_lo: u32, filetime_hi: u32) -> u32 {
    let (eax, borrow) = filetime_lo.overflowing_sub(NTFS_FILETIME_BIAS_LO);
    let mut edx = filetime_hi
        .wrapping_sub(NTFS_FILETIME_BIAS_HI)
        .wrapping_sub(if borrow { 1 } else { 0 });
    // cmp edx, ecx / jc @f / xor edx, edx
    if edx >= NTFS_FILETIME_PER_SEC {
        edx = 0;
    }
    // div ecx — EDX:EAX / 10_000_000 → EAX quotient (secs); EDX remainder discarded
    let dividend = ((edx as u64) << 32) | (eax as u64);
    (dividend / (NTFS_FILETIME_PER_SEC as u64)) as u32
}

/// Convert NTFS FILETIME to BDFE datetime (FASM `ntfs_datetime_to_bdfe`).
///
/// Composes [`ntfs_filetime_to_secs`] + [`fs_time2bdfe`].
#[inline(always)]
pub fn ntfs_datetime_to_bdfe(filetime_lo: u32, filetime_hi: u32) -> BdfeTime {
    fs_time2bdfe(ntfs_filetime_to_secs(filetime_lo, filetime_hi))
}

/// Pointer form used by the FFI trampoline — writes 8 BDFE bytes at `out`.
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[inline(always)]
pub unsafe fn ntfs_datetime_to_bdfe_ptr(filetime_lo: u32, filetime_hi: u32, out: *mut u8) {
    let t = ntfs_datetime_to_bdfe(filetime_lo, filetime_hi);
    let b = t.to_bytes();
    // SAFETY: caller guarantees 8 writable bytes (kernel trampoline / tests).
    unsafe {
        core::ptr::copy_nonoverlapping(b.as_ptr(), out, 8);
    }
}

/// Pack FILETIME lo/hi into a u64 (host tests / corpus helpers).
#[inline(always)]
pub fn pack_filetime(lo: u32, hi: u32) -> u64 {
    ((hi as u64) << 32) | (lo as u64)
}

/// FILETIME (100ns since 1601) for a given seconds-since-2001 value, using
/// the inverse of the FASM bias add (`ntfsCalculateTime` path).
///
/// Mirrors FASM `mov edx, 10000000` / `mul edx` / `add`/`adc` bias.
#[inline(always)]
pub fn filetime_from_secs_2001(secs: u32) -> (u32, u32) {
    let product = (secs as u64).wrapping_mul(NTFS_FILETIME_PER_SEC as u64);
    let bias = pack_filetime(NTFS_FILETIME_BIAS_LO, NTFS_FILETIME_BIAS_HI);
    let ft = product.wrapping_add(bias);
    (ft as u32, (ft >> 32) as u32)
}

/// Convert BDFE datetime to NTFS FILETIME (FASM `ntfsCalculateTime`).
///
/// Composes [`fs_calculate_time`] + [`filetime_from_secs_2001`]. Inverse of
/// [`ntfs_datetime_to_bdfe`] on the production calendar domain.
#[inline(always)]
pub fn ntfs_calculate_time(t: BdfeTime) -> (u32, u32) {
    filetime_from_secs_2001(fs_calculate_time(t))
}

/// Pointer form used by the FFI trampoline — reads 8 BDFE bytes at `block`.
///
/// Returns `(lo, hi)` matching FASM `EDX:EAX` FILETIME.
///
/// # Safety
/// `block` must point to a readable 8-byte BDFE datetime block.
#[inline(always)]
pub unsafe fn ntfs_calculate_time_ptr(block: *const u8) -> (u32, u32) {
    // SAFETY: caller guarantees 8 readable BDFE bytes (kernel trampoline / tests).
    let b = unsafe { core::ptr::read(block as *const [u8; 8]) };
    ntfs_calculate_time(BdfeTime::from_bytes(&b))
}

/// Convert XFS v5 bigtime dwords (native after `movbe`) to Kolibri seconds
/// since 2001-01-01.
///
/// Mirrors `xfs._.conv_bigtime_to_kos_epoch` bias/`sbb`/clamp/`div` before the
/// `call fsTime2bdfe`. Inputs below the KOS epoch clamp to 0; when the
/// post-bias high dword is `>= 1_000_000_000`, FASM forces
/// `{edx,eax} = {999_999_999, 0xFFFF_FFFF}` before dividing.
#[inline(always)]
pub fn xfs_bigtime_to_secs(bigtime_lo: u32, bigtime_hi: u32) -> u32 {
    // Exact CF of `sub`/`sbb`: underflow iff unsigned 64-bit input < bias.
    if ((bigtime_hi as u64) << 32 | bigtime_lo as u64) < XFS_BIGTIME_TO_KOS_OFFSET_NS {
        return 0;
    }
    let (eax, borrow) = bigtime_lo.overflowing_sub(XFS_BIGTIME_TO_KOS_OFFSET_NS_LO);
    let edx = bigtime_hi
        .wrapping_sub(XFS_BIGTIME_TO_KOS_OFFSET_NS_HI)
        .wrapping_sub(if borrow { 1 } else { 0 });
    let _ = borrow;
    // cmp edx, NANOSEC_PER_SEC / jb .time_to_bdfe / else max clamp
    let (eax, edx) = if edx >= XFS_NANOSEC_PER_SEC {
        (u32::MAX, XFS_NANOSEC_PER_SEC - 1)
    } else {
        (eax, edx)
    };
    let dividend = ((edx as u64) << 32) | (eax as u64);
    (dividend / (XFS_NANOSEC_PER_SEC as u64)) as u32
}

/// Convert XFS v5 bigtime to BDFE datetime (FASM `xfs._.conv_bigtime_to_kos_epoch`).
///
/// Composes [`xfs_bigtime_to_secs`] + [`fs_time2bdfe`].
#[inline(always)]
pub fn xfs_conv_bigtime_to_kos_epoch(bigtime_lo: u32, bigtime_hi: u32) -> BdfeTime {
    fs_time2bdfe(xfs_bigtime_to_secs(bigtime_lo, bigtime_hi))
}

/// Convert EXT/ext4 Unix seconds (+ optional `i_*TimeExtra`) to Kolibri seconds
/// since 2001-01-01.
///
/// Mirrors `ext_read_time` in `ext.inc` before `call fsTime2bdfe`:
/// * `edx &= 3` (ext4 extra epoch bits);
/// * if `i_time` is signed-negative, `dec edx` (sign-extension trick);
/// * 64-bit `sub`/`sbb` of [`UNIXTIME_TO_KOS_OFFSET`];
/// * `js` → clamp 0 (pre-2001); `jnz` → clamp `0xFFFFFFFF` (past KOS u32 range).
#[inline(always)]
pub fn ext_unix_to_secs(i_time: u32, extra: u32) -> u32 {
    let mut edx = extra & 3;
    if (i_time as i32) < 0 {
        edx = edx.wrapping_sub(1);
    }
    let (eax, borrow) = i_time.overflowing_sub(UNIXTIME_TO_KOS_OFFSET);
    let edx = edx.wrapping_sub(if borrow { 1 } else { 0 });
    // `js .clamp_0` — SF of `sbb edx, 0`
    if (edx as i32) < 0 {
        return 0;
    }
    // `jnz .clamp_max`
    if edx != 0 {
        return u32::MAX;
    }
    eax
}

/// Convert EXT/ext4 Unix time to BDFE datetime (FASM `ext_read_time`).
///
/// Composes [`ext_unix_to_secs`] + [`fs_time2bdfe`].
#[inline(always)]
pub fn ext_read_time(i_time: u32, extra: u32) -> BdfeTime {
    fs_time2bdfe(ext_unix_to_secs(i_time, extra))
}

/// Pointer form — writes 8 BDFE bytes at `out` (host/tests; kernel uses Cut T).
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[inline(always)]
pub unsafe fn ext_read_time_ptr(i_time: u32, extra: u32, out: *mut u8) {
    let t = ext_read_time(i_time, extra);
    let b = t.to_bytes();
    // SAFETY: caller guarantees 8 writable bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(b.as_ptr(), out, 8);
    }
}

/// Pointer form used by the FFI trampoline — writes 8 BDFE bytes at `out`.
///
/// # Safety
/// `out` must point to a writable 8-byte BDFE datetime block.
#[inline(always)]
pub unsafe fn xfs_conv_bigtime_to_kos_epoch_ptr(bigtime_lo: u32, bigtime_hi: u32, out: *mut u8) {
    let t = xfs_conv_bigtime_to_kos_epoch(bigtime_lo, bigtime_hi);
    let b = t.to_bytes();
    // SAFETY: caller guarantees 8 writable bytes (kernel trampoline / tests).
    unsafe {
        core::ptr::copy_nonoverlapping(b.as_ptr(), out, 8);
    }
}

/// Pack native bigtime lo/hi for a given seconds-since-2001 value
/// (`bias + secs * 1e9`). Sub-second remainder is zero.
#[inline(always)]
pub fn bigtime_from_secs_2001(secs: u32) -> (u32, u32) {
    let bt = XFS_BIGTIME_TO_KOS_OFFSET_NS
        .wrapping_add((secs as u64).wrapping_mul(XFS_NANOSEC_PER_SEC as u64));
    (bt as u32, (bt >> 32) as u32)
}

/// Pack a native 64-bit bigtime into the on-disk DQ big-endian layout
/// (`hi_be` at +0, `lo_be` at +4) matching FASM `movbe` loads.
#[inline(always)]
pub fn pack_bigtime_be(lo: u32, hi: u32) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..4].copy_from_slice(&hi.to_be_bytes());
    b[4..8].copy_from_slice(&lo.to_be_bytes());
    b
}

/// FASM-faithful host oracle — separately coded control-flow mirror of
/// `fs_common.inc` (not a call through [`fs_calculate_time`]).
///
/// Uses the same 24-byte `months`||`months2` layout and the FASM month loop /
/// `mul` chain. Validated against production on year &lt; 3025.
#[cfg(test)]
pub fn fasm_oracle_fs_calculate_time(t: BdfeTime) -> u32 {
    const MONTHS: [u8; 24] = [
        31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31, // months
        31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31, // months2
    ];

    // movzx eax, year; sub 2001; jnc else xor
    let mut eax: u32 = t.year as u32;
    let cf_borrow = eax < 2001;
    eax = eax.wrapping_sub(2001);
    if cf_borrow {
        eax = 0;
    }

    // edx = months; ebx = years; inc eax; test eax,3; jnz; else edx+=12
    let mut edx_base: usize = 0;
    let ebx = eax; // years
    eax = eax.wrapping_add(1);
    if (eax & 3) == 0 {
        edx_base = 12;
    }

    // movzx eax, month; dec eax; xor ecx,ecx; loop dec/js add
    eax = t.month as u32;
    eax = eax.wrapping_sub(1);
    let mut ecx: u32 = 0;
    loop {
        eax = eax.wrapping_sub(1);
        // js — signed
        if (eax as i32) < 0 {
            break;
        }
        let idx = edx_base.wrapping_add(eax as usize);
        let add = if idx < 24 { MONTHS[idx] as u32 } else { 0 };
        ecx = ecx.wrapping_add(add);
    }

    // eax = years * 365; shr ebx,2; add; add ecx
    eax = ebx.wrapping_mul(365);
    let ebx2 = ebx >> 2;
    eax = eax.wrapping_add(ebx2).wrapping_add(ecx);

    // mov bl, day; dec eax; add eax, ebx  (BH=0 when years/4 < 256)
    let bl = t.day as u32;
    eax = eax.wrapping_sub(1).wrapping_add(bl);

    // mov dl, 24; mul edx  (edx high cleared in this domain)
    eax = eax.wrapping_mul(24);

    // mov bl, hour; add
    eax = eax.wrapping_add(t.hour as u32);

    // mul 60; add min; mul 60; add sec
    eax = eax.wrapping_mul(60).wrapping_add(t.min as u32);
    eax = eax.wrapping_mul(60).wrapping_add(t.sec as u32);
    eax
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(year: u16, month: u8, day: u8, hour: u8, min: u8, sec: u8) -> BdfeTime {
        BdfeTime {
            sec,
            min,
            hour,
            day,
            month,
            year,
        }
    }

    #[test]
    fn epoch_2001_01_01_midnight() {
        // days = 0*365 + 0 + 0 + 1 - 1 = 0 → 0 seconds
        assert_eq!(fs_calculate_time(t(2001, 1, 1, 0, 0, 0)), 0);
    }

    #[test]
    fn epoch_plus_one_second() {
        assert_eq!(fs_calculate_time(t(2001, 1, 1, 0, 0, 1)), 1);
    }

    #[test]
    fn year_before_2001_clamps() {
        // years forced to 0; still uses month/day/time of the block
        assert_eq!(
            fs_calculate_time(t(1999, 1, 1, 0, 0, 0)),
            fs_calculate_time(t(2001, 1, 1, 0, 0, 0))
        );
        assert_eq!(
            fs_calculate_time(t(2000, 6, 15, 12, 30, 45)),
            fs_calculate_time(t(2001, 6, 15, 12, 30, 45))
        );
    }

    #[test]
    fn non_leap_2001_feb_28() {
        // Jan 31 days before Feb 28 → days = 31 + 28 - 1 = 58
        let sec = fs_calculate_time(t(2001, 2, 28, 0, 0, 0));
        assert_eq!(sec, 58 * 24 * 3600);
    }

    #[test]
    fn leap_2004_feb_29() {
        // years=3; (3+1)&3==0 → leap table; Jan 31 + Feb 29 - 1 + 3*365 + 0
        // years/4 = 0
        let days = 3 * 365 + 0 + 31 + 29 - 1;
        let expect = days * 24 * 3600;
        assert_eq!(fs_calculate_time(t(2004, 2, 29, 0, 0, 0)), expect as u32);
    }

    #[test]
    fn non_leap_2003_uses_feb_28_table() {
        // years=2; (2+1)&3 != 0 → non-leap; Mar 1 = Jan+Feb = 31+28
        let days = 2 * 365 + (2 / 4) + 31 + 28 + 1 - 1;
        assert_eq!(fs_calculate_time(t(2003, 3, 1, 0, 0, 0)), (days * 86400) as u32);
    }

    #[test]
    fn end_of_day_components() {
        let s = fs_calculate_time(t(2001, 1, 1, 23, 59, 59));
        assert_eq!(s, 23 * 3600 + 59 * 60 + 59);
    }

    #[test]
    fn known_vector_2010_07_04_12_00_00() {
        // years=9; leap days=2; Jul = Jan..Jun = 31+28+31+30+31+30=181 (2010 non-leap)
        // (9+1)&3 != 0 → non-leap
        let month_sum = 31 + 28 + 31 + 30 + 31 + 30;
        let days = 9 * 365 + 2 + month_sum + 4 - 1;
        let expect = days * 86400 + 12 * 3600;
        assert_eq!(fs_calculate_time(t(2010, 7, 4, 12, 0, 0)), expect as u32);
    }

    #[test]
    fn bytes_roundtrip_layout() {
        let bt = t(2024, 12, 31, 23, 58, 57);
        let b = bt.to_bytes();
        assert_eq!(BdfeTime::from_bytes(&b), bt);
        assert_eq!(unsafe { fs_calculate_time_ptr(b.as_ptr()) }, fs_calculate_time(bt));
    }

    #[test]
    fn month_one_and_twelve_edges() {
        assert_eq!(fs_calculate_time(t(2001, 1, 1, 0, 0, 0)), 0);
        // Dec 31 2001: sum Jan..Nov = 334 (non-leap), +31 -1 = 364 days from epoch start
        let month_sum = 31 + 28 + 31 + 30 + 31 + 30 + 31 + 31 + 30 + 31 + 30;
        let days = month_sum + 31 - 1;
        assert_eq!(fs_calculate_time(t(2001, 12, 31, 0, 0, 0)), (days * 86400) as u32);
    }

    /// Differential: named + structured grid + PRNG vs FASM-faithful oracle.
    #[test]
    fn differential_oracle_corpus() {
        let named = [
            t(2001, 1, 1, 0, 0, 0),
            t(2001, 1, 1, 0, 0, 1),
            t(2000, 1, 1, 0, 0, 0),
            t(1990, 6, 15, 12, 0, 0),
            t(2004, 2, 29, 0, 0, 0),
            t(2004, 3, 1, 0, 0, 0),
            t(2003, 2, 28, 23, 59, 59),
            t(2010, 7, 4, 12, 0, 0),
            t(2024, 2, 29, 11, 22, 33),
            t(2025, 12, 31, 23, 59, 59),
            t(3024, 1, 1, 0, 0, 0), // last year with years/4 < 256
        ];
        for bt in named {
            assert_eq!(
                fs_calculate_time(bt),
                fasm_oracle_fs_calculate_time(bt),
                "named {bt:?}"
            );
        }

        // Structured grid: years 2001..=2032 × months 1..=12 × sample days/times
        let days_s = [1u8, 15, 28, 29, 30, 31];
        let hours = [0u8, 12, 23];
        let mins = [0u8, 30, 59];
        let secs = [0u8, 1, 59];
        for year in 2001u16..=2032 {
            for month in 1u8..=12 {
                for &day in &days_s {
                    for &hour in &hours {
                        for &min in &mins {
                            for &sec in &secs {
                                let bt = t(year, month, day, hour, min, sec);
                                assert_eq!(
                                    fs_calculate_time(bt),
                                    fasm_oracle_fs_calculate_time(bt),
                                    "grid {bt:?}"
                                );
                            }
                        }
                    }
                }
            }
        }

        // Deterministic PRNG corpus (seed documented for Cut G).
        // Domain: year 2001..3024, month 1..12, day 1..31, h/m/s full byte range sample.
        const SEED: u32 = 0xC07_A71_E; // "Cut G time"
        const CASES: u32 = 200_000;
        let mut state = SEED;
        let mut next = || -> u32 {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..CASES {
            let r0 = next();
            let r1 = next();
            let year = 2001 + (r0 % 1024) as u16; // years < 1024
            let month = 1 + ((r0 >> 16) % 12) as u8;
            let day = 1 + ((r1 % 31) as u8);
            let hour = ((r1 >> 8) % 24) as u8;
            let min = ((r1 >> 16) % 60) as u8;
            let sec = ((r1 >> 24) % 60) as u8;
            let bt = t(year, month, day, hour, min, sec);
            assert_eq!(
                fs_calculate_time(bt),
                fasm_oracle_fs_calculate_time(bt),
                "prng {bt:?}"
            );
        }
    }

    /// Cross-check a few vectors against independent calendar arithmetic.
    #[test]
    fn independent_calendar_spot_checks() {
        // 2001-01-02 00:00:00 → 86400
        assert_eq!(fs_calculate_time(t(2001, 1, 2, 0, 0, 0)), 86400);
        // 2002-01-01: 365 days (2001 non-leap) → 365*86400
        assert_eq!(fs_calculate_time(t(2002, 1, 1, 0, 0, 0)), 365 * 86400);
        // 2005-01-01: years=4; 4*365 + 1 leap day (from shr years/4) = 1461
        assert_eq!(fs_calculate_time(t(2005, 1, 1, 0, 0, 0)), 1461 * 86400);
    }

    // ----- Cut T: fsTime2bdfe -----

    #[test]
    fn time2bdfe_epoch_zero() {
        assert_eq!(fs_time2bdfe(0), t(2001, 1, 1, 0, 0, 0));
    }

    #[test]
    fn time2bdfe_one_second() {
        assert_eq!(fs_time2bdfe(1), t(2001, 1, 1, 0, 0, 1));
    }

    #[test]
    fn time2bdfe_one_day() {
        assert_eq!(fs_time2bdfe(86400), t(2001, 1, 2, 0, 0, 0));
    }

    #[test]
    fn time2bdfe_leap_2004_feb_29() {
        // days = 3*365 + 0 + 31 + 29 - 1 = 1154; secs = 1154 * 86400
        assert_eq!(fs_time2bdfe(99705600), t(2004, 2, 29, 0, 0, 0));
    }

    #[test]
    fn time2bdfe_2010_07_04_noon() {
        assert_eq!(fs_time2bdfe(299937600), t(2010, 7, 4, 12, 0, 0));
    }

    #[test]
    fn time2bdfe_end_of_day() {
        assert_eq!(fs_time2bdfe(86399), t(2001, 1, 1, 23, 59, 59));
    }

    #[test]
    fn time2bdfe_bytes_layout_hour_word_pad() {
        let mut buf = [0xAAu8; 8];
        unsafe { fs_time2bdfe_ptr(86399, buf.as_mut_ptr()) };
        assert_eq!(buf[0], 59);
        assert_eq!(buf[1], 59);
        assert_eq!(buf[2], 23);
        assert_eq!(buf[3], 0); // FASM `mov [edi+2], dx` clears pad
        assert_eq!(buf[4], 1);
        assert_eq!(buf[5], 1);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 2001);
    }

    /// Differential: FASM-flow oracle vs Rust for Cut T.
    #[test]
    fn time2bdfe_differential_oracle_corpus() {
        let named = [
            0u32,
            1,
            59,
            60,
            3599,
            3600,
            86399,
            86400,
            99705600,  // 2004-02-29
            299937600, // 2010-07-04 12:00:00
            u32::MAX,
            365 * 86400,
            366 * 86400,
            1461 * 86400, // ~2005-01-01
        ];
        for &secs in &named {
            assert_eq!(
                fs_time2bdfe(secs),
                fasm_oracle_fs_time2bdfe(secs),
                "named secs={secs}"
            );
        }

        // Structured grid over day counts and time-of-day remainders.
        for days in 0u32..=4000 {
            for &tod in &[0u32, 1, 3600, 12 * 3600, 86399] {
                let secs = days.saturating_mul(86400).saturating_add(tod);
                assert_eq!(
                    fs_time2bdfe(secs),
                    fasm_oracle_fs_time2bdfe(secs),
                    "grid days={days} tod={tod}"
                );
            }
        }

        // Deterministic PRNG corpus (seed documented for Cut T).
        const SEED: u32 = 0xC07_72B_FE; // "Cut T 2bdfe"
        const CASES: u32 = 200_000;
        let mut state = SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..CASES {
            let secs = next();
            assert_eq!(
                fs_time2bdfe(secs),
                fasm_oracle_fs_time2bdfe(secs),
                "prng secs={secs}"
            );
        }
    }

    /// Roundtrip: valid BdfeTime → secs (G) → BdfeTime (T) recovers fields
    /// for the production calendar domain (month 1..12, plausible days).
    #[test]
    fn time2bdfe_roundtrip_with_calculate_time() {
        let named = [
            t(2001, 1, 1, 0, 0, 0),
            t(2001, 1, 1, 0, 0, 1),
            t(2001, 1, 1, 23, 59, 59),
            t(2001, 2, 28, 0, 0, 0),
            t(2004, 2, 29, 0, 0, 0),
            t(2004, 3, 1, 0, 0, 0),
            t(2010, 7, 4, 12, 0, 0),
            t(2024, 2, 29, 11, 22, 33),
            t(2025, 12, 31, 23, 59, 59),
        ];
        for bt in named {
            let secs = fs_calculate_time(bt);
            assert_eq!(fs_time2bdfe(secs), bt, "roundtrip {bt:?}");
        }

        const SEED: u32 = 0xC07_72B_FE;
        const CASES: u32 = 50_000;
        let mut state = SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..CASES {
            let r0 = next();
            let r1 = next();
            let year = 2001 + (r0 % 50) as u16;
            let month = 1 + ((r0 >> 16) % 12) as u8;
            // Keep days valid for the month (FASM tables).
            let max_day = match month {
                2 => {
                    if (year % 4) == 0 {
                        29
                    } else {
                        28
                    }
                }
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            let day = 1 + ((r1 % max_day as u32) as u8);
            let hour = ((r1 >> 8) % 24) as u8;
            let min = ((r1 >> 16) % 60) as u8;
            let sec = ((r1 >> 24) % 60) as u8;
            let bt = t(year, month, day, hour, min, sec);
            let secs = fs_calculate_time(bt);
            assert_eq!(fs_time2bdfe(secs), bt, "roundtrip prng {bt:?}");
        }
    }

    // ----- Cut AE: ntfs_datetime_to_bdfe -----

    #[test]
    fn ntfs_dt_epoch_2001() {
        let (lo, hi) = filetime_from_secs_2001(0);
        assert_eq!(lo, NTFS_FILETIME_BIAS_LO);
        assert_eq!(hi, NTFS_FILETIME_BIAS_HI);
        assert_eq!(ntfs_filetime_to_secs(lo, hi), 0);
        assert_eq!(ntfs_datetime_to_bdfe(lo, hi), t(2001, 1, 1, 0, 0, 0));
    }

    #[test]
    fn ntfs_dt_one_second() {
        let (lo, hi) = filetime_from_secs_2001(1);
        assert_eq!(ntfs_datetime_to_bdfe(lo, hi), t(2001, 1, 1, 0, 0, 1));
    }

    #[test]
    fn ntfs_dt_leap_2004_02_29() {
        let (lo, hi) = filetime_from_secs_2001(99705600);
        assert_eq!(ntfs_datetime_to_bdfe(lo, hi), t(2004, 2, 29, 0, 0, 0));
    }

    #[test]
    fn ntfs_dt_2010_07_04_noon() {
        let (lo, hi) = filetime_from_secs_2001(299937600);
        assert_eq!(ntfs_datetime_to_bdfe(lo, hi), t(2010, 7, 4, 12, 0, 0));
    }

    #[test]
    fn ntfs_dt_end_of_day() {
        let (lo, hi) = filetime_from_secs_2001(86399);
        assert_eq!(ntfs_datetime_to_bdfe(lo, hi), t(2001, 1, 1, 23, 59, 59));
    }

    #[test]
    fn ntfs_dt_pre_2001_wraps() {
        // FILETIME 0 (1601) underflows bias; must match FASM wrap semantics.
        let got = ntfs_datetime_to_bdfe(0, 0);
        let expect = fasm_oracle_ntfs_datetime_to_bdfe(0, 0);
        assert_eq!(got, expect, "FILETIME 0 wrap");
    }

    #[test]
    fn ntfs_dt_clamp_high_edx() {
        // After bias, force EDX >= 10_000_000 so FASM zeros EDX before div.
        // Construct: post-bias edx = NTFS_FILETIME_PER_SEC, eax = 50_000_000
        // → secs = 50_000_000 / 10_000_000 = 5.
        // filetime_lo = BIAS_LO + 50_000_000 (no borrow)
        // filetime_hi = BIAS_HI + 10_000_000
        let lo = NTFS_FILETIME_BIAS_LO.wrapping_add(50_000_000);
        let hi = NTFS_FILETIME_BIAS_HI.wrapping_add(NTFS_FILETIME_PER_SEC);
        assert_eq!(ntfs_filetime_to_secs(lo, hi), 5);
        assert_eq!(
            ntfs_datetime_to_bdfe(lo, hi),
            fasm_oracle_ntfs_datetime_to_bdfe(lo, hi)
        );
    }

    #[test]
    fn ntfs_dt_just_below_clamp() {
        // EDX = 9_999_999 after bias → no clamp; large dividend.
        let lo = NTFS_FILETIME_BIAS_LO; // eax = 0 after sub
        let hi = NTFS_FILETIME_BIAS_HI.wrapping_add(NTFS_FILETIME_PER_SEC - 1);
        let secs = ntfs_filetime_to_secs(lo, hi);
        let expect_secs = fasm_oracle_ntfs_filetime_to_secs(lo, hi);
        assert_eq!(secs, expect_secs);
        // Quotient must fit u32 for div not to #DE; this case:
        // dividend = 9999999 << 32 = 42949668254744576 / 1e7 = 4294966825 — fits.
        assert_eq!(
            ntfs_datetime_to_bdfe(lo, hi),
            fasm_oracle_ntfs_datetime_to_bdfe(lo, hi)
        );
    }

    #[test]
    fn ntfs_dt_ptr_writes_layout() {
        let (lo, hi) = filetime_from_secs_2001(86399);
        let mut buf = [0xAAu8; 8];
        unsafe { ntfs_datetime_to_bdfe_ptr(lo, hi, buf.as_mut_ptr()) };
        assert_eq!(buf[0], 59);
        assert_eq!(buf[1], 59);
        assert_eq!(buf[2], 23);
        assert_eq!(buf[3], 0); // hour word pad
        assert_eq!(buf[4], 1);
        assert_eq!(buf[5], 1);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 2001);
    }

    #[test]
    fn ntfs_dt_oracle_named_and_boundary() {
        let vectors: &[(u32, u32)] = &[
            (NTFS_FILETIME_BIAS_LO, NTFS_FILETIME_BIAS_HI),
            filetime_from_secs_2001(1),
            filetime_from_secs_2001(86400),
            filetime_from_secs_2001(99705600),
            filetime_from_secs_2001(299937600),
            filetime_from_secs_2001(86399),
            (0, 0),
            (1, 0),
            (u32::MAX, u32::MAX),
            (NTFS_FILETIME_BIAS_LO.wrapping_sub(1), NTFS_FILETIME_BIAS_HI), // just before 2001
            (
                NTFS_FILETIME_BIAS_LO.wrapping_add(50_000_000),
                NTFS_FILETIME_BIAS_HI.wrapping_add(NTFS_FILETIME_PER_SEC),
            ),
            (
                NTFS_FILETIME_BIAS_LO,
                NTFS_FILETIME_BIAS_HI.wrapping_add(NTFS_FILETIME_PER_SEC - 1),
            ),
            (
                NTFS_FILETIME_BIAS_LO,
                NTFS_FILETIME_BIAS_HI.wrapping_add(NTFS_FILETIME_PER_SEC),
            ),
        ];
        for &(lo, hi) in vectors {
            // Skip cases where unsigned div quotient would exceed u32 (#DE on HW).
            if fasm_oracle_ntfs_div_overflows(lo, hi) {
                continue;
            }
            assert_eq!(
                ntfs_datetime_to_bdfe(lo, hi),
                fasm_oracle_ntfs_datetime_to_bdfe(lo, hi),
                "named lo={lo:#x} hi={hi:#x}"
            );
        }
    }

    #[test]
    fn ntfs_dt_prng_oracle_50k() {
        const CASES: usize = 50_000;
        let mut state = NTFS_DATETIME_TO_BDFE_PRNG_SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let mut checked = 0usize;
        for _ in 0..CASES {
            let lo = next();
            let hi = next();
            if fasm_oracle_ntfs_div_overflows(lo, hi) {
                continue;
            }
            assert_eq!(
                ntfs_datetime_to_bdfe(lo, hi),
                fasm_oracle_ntfs_datetime_to_bdfe(lo, hi),
                "prng lo={lo:#x} hi={hi:#x}"
            );
            checked += 1;
        }
        assert!(checked > 40_000, "too many #DE-skipped vectors: {checked}");
    }

    #[test]
    fn ntfs_dt_roundtrip_via_secs() {
        // Production-domain BDFE → CalculateTime → FILETIME → datetime_to_bdfe
        let samples = [
            t(2001, 1, 1, 0, 0, 0),
            t(2001, 1, 1, 23, 59, 59),
            t(2004, 2, 29, 0, 0, 0),
            t(2010, 7, 4, 12, 0, 0),
            t(2020, 12, 31, 23, 59, 59),
        ];
        for bt in samples {
            let secs = fs_calculate_time(bt);
            let (lo, hi) = filetime_from_secs_2001(secs);
            assert_eq!(ntfs_datetime_to_bdfe(lo, hi), bt, "roundtrip {bt:?}");
        }
    }

    // ----- Cut AF: ntfsCalculateTime -----

    #[test]
    fn ntfs_ct_epoch_bias() {
        let (lo, hi) = ntfs_calculate_time(t(2001, 1, 1, 0, 0, 0));
        assert_eq!(lo, NTFS_FILETIME_BIAS_LO);
        assert_eq!(hi, NTFS_FILETIME_BIAS_HI);
    }

    #[test]
    fn ntfs_ct_plus_one_second() {
        let (lo, hi) = ntfs_calculate_time(t(2001, 1, 1, 0, 0, 1));
        let (elo, ehi) = filetime_from_secs_2001(1);
        assert_eq!((lo, hi), (elo, ehi));
    }

    #[test]
    fn ntfs_ct_leap_2004() {
        let (lo, hi) = ntfs_calculate_time(t(2004, 2, 29, 0, 0, 0));
        assert_eq!((lo, hi), fasm_oracle_ntfs_calculate_time(t(2004, 2, 29, 0, 0, 0)));
        // Round-trip through AE
        assert_eq!(ntfs_datetime_to_bdfe(lo, hi), t(2004, 2, 29, 0, 0, 0));
    }

    #[test]
    fn ntfs_ct_end_of_day() {
        let bt = t(2001, 1, 1, 23, 59, 59);
        assert_eq!(ntfs_calculate_time(bt), fasm_oracle_ntfs_calculate_time(bt));
    }

    #[test]
    fn ntfs_ct_named_oracle_vectors() {
        let samples = [
            t(2001, 1, 1, 0, 0, 0),
            t(2001, 1, 1, 0, 0, 1),
            t(2001, 1, 1, 23, 59, 59),
            t(2001, 12, 31, 0, 0, 0),
            t(2004, 2, 29, 12, 0, 0),
            t(2010, 7, 4, 12, 0, 0),
            t(2020, 12, 31, 23, 59, 59),
            t(1999, 6, 15, 12, 30, 45), // year clamp via G
            t(2000, 2, 29, 0, 0, 0),    // pre-2001 leap → clamp to 2001 path
        ];
        for bt in samples {
            assert_eq!(
                ntfs_calculate_time(bt),
                fasm_oracle_ntfs_calculate_time(bt),
                "named {bt:?}"
            );
        }
    }

    #[test]
    fn ntfs_ct_ptr_matches() {
        let bt = t(2010, 7, 4, 12, 0, 0);
        let b = bt.to_bytes();
        let got = unsafe { ntfs_calculate_time_ptr(b.as_ptr()) };
        assert_eq!(got, ntfs_calculate_time(bt));
    }

    #[test]
    fn ntfs_ct_ae_roundtrip() {
        let samples = [
            t(2001, 1, 1, 0, 0, 0),
            t(2001, 1, 2, 0, 0, 0),
            t(2004, 2, 29, 0, 0, 0),
            t(2010, 7, 4, 12, 0, 0),
            t(2020, 12, 31, 23, 59, 59),
        ];
        for bt in samples {
            let (lo, hi) = ntfs_calculate_time(bt);
            assert_eq!(ntfs_datetime_to_bdfe(lo, hi), bt, "AF→AE {bt:?}");
        }
    }

    #[test]
    fn ntfs_ct_prng_oracle_50k() {
        const CASES: usize = 50_000;
        let mut state = NTFS_CALCULATE_TIME_PRNG_SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for i in 0..CASES {
            let year = 2001 + (next() % 400); // stay in production domain
            let month = 1 + (next() % 12) as u8;
            let day = 1 + (next() % 28) as u8; // always valid day
            let hour = (next() % 24) as u8;
            let min = (next() % 60) as u8;
            let sec = (next() % 60) as u8;
            let bt = t(year as u16, month, day, hour, min, sec);
            assert_eq!(
                ntfs_calculate_time(bt),
                fasm_oracle_ntfs_calculate_time(bt),
                "prng#{i} {bt:?}"
            );
        }
    }

    // ----- Cut AK: xfs._.conv_bigtime_to_kos_epoch -----

    #[test]
    fn xfs_bt_epoch_2001() {
        let (lo, hi) = bigtime_from_secs_2001(0);
        assert_eq!(lo, XFS_BIGTIME_TO_KOS_OFFSET_NS_LO);
        assert_eq!(hi, XFS_BIGTIME_TO_KOS_OFFSET_NS_HI);
        assert_eq!(xfs_bigtime_to_secs(lo, hi), 0);
        assert_eq!(
            xfs_conv_bigtime_to_kos_epoch(lo, hi),
            t(2001, 1, 1, 0, 0, 0)
        );
    }

    #[test]
    fn xfs_bt_one_second() {
        let (lo, hi) = bigtime_from_secs_2001(1);
        assert_eq!(xfs_conv_bigtime_to_kos_epoch(lo, hi), t(2001, 1, 1, 0, 0, 1));
    }

    #[test]
    fn xfs_bt_leap_2004_02_29() {
        let (lo, hi) = bigtime_from_secs_2001(99_705_600);
        assert_eq!(xfs_conv_bigtime_to_kos_epoch(lo, hi), t(2004, 2, 29, 0, 0, 0));
    }

    #[test]
    fn xfs_bt_2010_07_04_noon() {
        let (lo, hi) = bigtime_from_secs_2001(299_937_600);
        assert_eq!(
            xfs_conv_bigtime_to_kos_epoch(lo, hi),
            t(2010, 7, 4, 12, 0, 0)
        );
    }

    #[test]
    fn xfs_bt_end_of_day() {
        let (lo, hi) = bigtime_from_secs_2001(86_399);
        assert_eq!(
            xfs_conv_bigtime_to_kos_epoch(lo, hi),
            t(2001, 1, 1, 23, 59, 59)
        );
    }

    #[test]
    fn xfs_bt_pre_epoch_clamps_zero() {
        assert_eq!(xfs_bigtime_to_secs(0, 0), 0);
        assert_eq!(
            xfs_conv_bigtime_to_kos_epoch(0, 0),
            fasm_oracle_xfs_conv_bigtime_to_kos_epoch(0, 0)
        );
        // Just below bias
        let lo = XFS_BIGTIME_TO_KOS_OFFSET_NS_LO.wrapping_sub(1);
        let hi = XFS_BIGTIME_TO_KOS_OFFSET_NS_HI;
        // borrow from lo → effective hi-1; still under if lo was 0 after wrap with hi unchanged...
        // bias-1 as u64:
        let bt = XFS_BIGTIME_TO_KOS_OFFSET_NS - 1;
        assert_eq!(xfs_bigtime_to_secs(bt as u32, (bt >> 32) as u32), 0);
        let _ = (lo, hi);
    }

    #[test]
    fn xfs_bt_subsec_remainder_discarded() {
        // bias + 1.5e9 ns → 1 second (div truncates)
        let bt = XFS_BIGTIME_TO_KOS_OFFSET_NS + 1_500_000_000;
        let lo = bt as u32;
        let hi = (bt >> 32) as u32;
        assert_eq!(xfs_bigtime_to_secs(lo, hi), 1);
        assert_eq!(
            xfs_conv_bigtime_to_kos_epoch(lo, hi),
            fasm_oracle_xfs_conv_bigtime_to_kos_epoch(lo, hi)
        );
    }

    #[test]
    fn xfs_bt_high_edx_clamp() {
        // Force post-bias edx >= 1e9: bigtime = bias + (1e9 << 32)
        let bt = XFS_BIGTIME_TO_KOS_OFFSET_NS + ((XFS_NANOSEC_PER_SEC as u64) << 32);
        let lo = bt as u32;
        let hi = (bt >> 32) as u32;
        let secs = xfs_bigtime_to_secs(lo, hi);
        assert_eq!(secs, fasm_oracle_xfs_bigtime_to_secs(lo, hi));
        assert_eq!(secs, u32::MAX); // max clamp → 0xFFFFFFFF secs
        assert_eq!(
            xfs_conv_bigtime_to_kos_epoch(lo, hi),
            fasm_oracle_xfs_conv_bigtime_to_kos_epoch(lo, hi)
        );
    }

    #[test]
    fn xfs_bt_just_below_high_clamp() {
        // post-bias edx = 1e9 - 1, eax = 0
        let bt = XFS_BIGTIME_TO_KOS_OFFSET_NS + (((XFS_NANOSEC_PER_SEC as u64) - 1) << 32);
        let lo = bt as u32;
        let hi = (bt >> 32) as u32;
        let secs = xfs_bigtime_to_secs(lo, hi);
        assert_eq!(secs, fasm_oracle_xfs_bigtime_to_secs(lo, hi));
        assert_ne!(secs, u32::MAX); // not the max clamp path
    }

    #[test]
    fn xfs_bt_ptr_writes_layout() {
        let (lo, hi) = bigtime_from_secs_2001(86_399);
        let mut buf = [0xAAu8; 8];
        unsafe { xfs_conv_bigtime_to_kos_epoch_ptr(lo, hi, buf.as_mut_ptr()) };
        assert_eq!(buf[0], 59);
        assert_eq!(buf[1], 59);
        assert_eq!(buf[2], 23);
        assert_eq!(buf[3], 0);
        assert_eq!(buf[4], 1);
        assert_eq!(buf[5], 1);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 2001);
    }

    #[test]
    fn xfs_bt_pack_be_roundtrip_layout() {
        let (lo, hi) = bigtime_from_secs_2001(1);
        let be = pack_bigtime_be(lo, hi);
        // movbe from hi_be (+0) / lo_be (+4)
        let hi2 = u32::from_be_bytes([be[0], be[1], be[2], be[3]]);
        let lo2 = u32::from_be_bytes([be[4], be[5], be[6], be[7]]);
        assert_eq!((lo2, hi2), (lo, hi));
    }

    #[test]
    fn xfs_bt_oracle_named_and_boundary() {
        let vectors: &[(u32, u32)] = &[
            (0, 0),
            (XFS_BIGTIME_TO_KOS_OFFSET_NS_LO, XFS_BIGTIME_TO_KOS_OFFSET_NS_HI),
            bigtime_from_secs_2001(1),
            bigtime_from_secs_2001(86_399),
            bigtime_from_secs_2001(99_705_600),
            bigtime_from_secs_2001(299_937_600),
            (
                (XFS_BIGTIME_TO_KOS_OFFSET_NS - 1) as u32,
                ((XFS_BIGTIME_TO_KOS_OFFSET_NS - 1) >> 32) as u32,
            ),
            (
                (XFS_BIGTIME_TO_KOS_OFFSET_NS + 1_500_000_000) as u32,
                ((XFS_BIGTIME_TO_KOS_OFFSET_NS + 1_500_000_000) >> 32) as u32,
            ),
            (
                (XFS_BIGTIME_TO_KOS_OFFSET_NS + ((XFS_NANOSEC_PER_SEC as u64) << 32)) as u32,
                ((XFS_BIGTIME_TO_KOS_OFFSET_NS + ((XFS_NANOSEC_PER_SEC as u64) << 32)) >> 32) as u32,
            ),
            (
                (XFS_BIGTIME_TO_KOS_OFFSET_NS + (((XFS_NANOSEC_PER_SEC as u64) - 1) << 32)) as u32,
                ((XFS_BIGTIME_TO_KOS_OFFSET_NS + (((XFS_NANOSEC_PER_SEC as u64) - 1) << 32)) >> 32)
                    as u32,
            ),
            (u32::MAX, u32::MAX),
        ];
        for &(lo, hi) in vectors {
            assert_eq!(
                xfs_bigtime_to_secs(lo, hi),
                fasm_oracle_xfs_bigtime_to_secs(lo, hi),
                "secs lo={lo:#x} hi={hi:#x}"
            );
            assert_eq!(
                xfs_conv_bigtime_to_kos_epoch(lo, hi),
                fasm_oracle_xfs_conv_bigtime_to_kos_epoch(lo, hi),
                "bdfe lo={lo:#x} hi={hi:#x}"
            );
        }
    }

    #[test]
    fn xfs_bt_prng_oracle_50k() {
        const CASES: usize = 50_000;
        let mut state = XFS_CONV_BIGTIME_TO_KOS_EPOCH_PRNG_SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for i in 0..CASES {
            let lo = next();
            let hi = next();
            assert_eq!(
                xfs_bigtime_to_secs(lo, hi),
                fasm_oracle_xfs_bigtime_to_secs(lo, hi),
                "prng#{i} secs lo={lo:#x} hi={hi:#x}"
            );
            assert_eq!(
                xfs_conv_bigtime_to_kos_epoch(lo, hi),
                fasm_oracle_xfs_conv_bigtime_to_kos_epoch(lo, hi),
                "prng#{i} bdfe lo={lo:#x} hi={hi:#x}"
            );
        }
    }

    // ----- Cut AL: ext_read_time -----

    #[test]
    fn ext_rt_epoch_2001() {
        // Unix 978_307_200 = 2001-01-01 00:00:00
        assert_eq!(ext_unix_to_secs(UNIXTIME_TO_KOS_OFFSET, 0), 0);
        assert_eq!(ext_read_time(UNIXTIME_TO_KOS_OFFSET, 0), t(2001, 1, 1, 0, 0, 0));
    }

    #[test]
    fn ext_rt_one_second() {
        assert_eq!(
            ext_read_time(UNIXTIME_TO_KOS_OFFSET + 1, 0),
            t(2001, 1, 1, 0, 0, 1)
        );
    }

    #[test]
    fn ext_rt_pre_epoch_clamp() {
        assert_eq!(ext_unix_to_secs(0, 0), 0);
        assert_eq!(ext_unix_to_secs(UNIXTIME_TO_KOS_OFFSET - 1, 0), 0);
        assert_eq!(ext_read_time(0, 0), t(2001, 1, 1, 0, 0, 0));
    }

    #[test]
    fn ext_rt_signed_negative_sign_extend() {
        // i_time MSB set + extra=0 → dec edx → SF after sbb → clamp 0
        assert_eq!(ext_unix_to_secs(0xFFFF_FFFF, 0), 0);
        assert_eq!(ext_unix_to_secs(0x8000_0000, 0), 0);
    }

    #[test]
    fn ext_rt_extra_epoch_bit_one() {
        // i_time=-1, extra=1 → after dec edx=0 → unsigned 0xFFFFFFFF
        // 0xFFFFFFFF - OFFSET = KOS secs
        let secs = ext_unix_to_secs(0xFFFF_FFFF, 1);
        assert_eq!(secs, 0xFFFF_FFFF - UNIXTIME_TO_KOS_OFFSET);
        assert_eq!(secs, fasm_oracle_ext_unix_to_secs(0xFFFF_FFFF, 1));
    }

    #[test]
    fn ext_rt_extra_masks_to_two_bits() {
        // Only low 2 bits of extra matter
        assert_eq!(
            ext_unix_to_secs(UNIXTIME_TO_KOS_OFFSET, 0xFFFF_FFFC),
            ext_unix_to_secs(UNIXTIME_TO_KOS_OFFSET, 0)
        );
        assert_eq!(
            ext_unix_to_secs(UNIXTIME_TO_KOS_OFFSET, 0xFFFF_FFFD),
            ext_unix_to_secs(UNIXTIME_TO_KOS_OFFSET, 1)
        );
    }

    #[test]
    fn ext_rt_high_epoch_clamps_max() {
        // extra&3 == 2 with non-negative i_time large enough that edx stays >0
        // after offset → clamp max
        let secs = ext_unix_to_secs(0, 2);
        assert_eq!(secs, u32::MAX);
        assert_eq!(secs, fasm_oracle_ext_unix_to_secs(0, 2));
        assert_eq!(ext_read_time(0, 2), fasm_oracle_ext_read_time(0, 2));
    }

    #[test]
    fn ext_rt_end_of_day() {
        assert_eq!(
            ext_read_time(UNIXTIME_TO_KOS_OFFSET + 86_399, 0),
            t(2001, 1, 1, 23, 59, 59)
        );
    }

    #[test]
    fn ext_rt_leap_2004_02_29() {
        assert_eq!(
            ext_read_time(UNIXTIME_TO_KOS_OFFSET + 99_705_600, 0),
            t(2004, 2, 29, 0, 0, 0)
        );
    }

    #[test]
    fn ext_rt_ptr_writes_layout() {
        let mut buf = [0xAAu8; 8];
        unsafe { ext_read_time_ptr(UNIXTIME_TO_KOS_OFFSET + 86_399, 0, buf.as_mut_ptr()) };
        assert_eq!(buf[0], 59);
        assert_eq!(buf[1], 59);
        assert_eq!(buf[2], 23);
        assert_eq!(buf[3], 0);
        assert_eq!(buf[4], 1);
        assert_eq!(buf[5], 1);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 2001);
    }

    #[test]
    fn ext_rt_oracle_named_and_boundary() {
        let vectors: &[(u32, u32)] = &[
            (0, 0),
            (UNIXTIME_TO_KOS_OFFSET, 0),
            (UNIXTIME_TO_KOS_OFFSET + 1, 0),
            (UNIXTIME_TO_KOS_OFFSET - 1, 0),
            (UNIXTIME_TO_KOS_OFFSET + 86_399, 0),
            (UNIXTIME_TO_KOS_OFFSET + 99_705_600, 0),
            (0x8000_0000, 0),
            (0x8000_0000, 1),
            (0xFFFF_FFFF, 0),
            (0xFFFF_FFFF, 1),
            (0xFFFF_FFFF, 2),
            (0xFFFF_FFFF, 3),
            (0, 1),
            (0, 2),
            (0, 3),
            (0x1234_5678, 0xDEAD_BEEF),
            (u32::MAX, u32::MAX),
        ];
        for &(time, extra) in vectors {
            assert_eq!(
                ext_unix_to_secs(time, extra),
                fasm_oracle_ext_unix_to_secs(time, extra),
                "secs time={time:#x} extra={extra:#x}"
            );
            assert_eq!(
                ext_read_time(time, extra),
                fasm_oracle_ext_read_time(time, extra),
                "bdfe time={time:#x} extra={extra:#x}"
            );
        }
    }

    #[test]
    fn ext_rt_prng_oracle_50k() {
        const CASES: usize = 50_000;
        let mut state = EXT_READ_TIME_PRNG_SEED;
        let mut next = || -> u32 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for i in 0..CASES {
            let time = next();
            let extra = next();
            assert_eq!(
                ext_unix_to_secs(time, extra),
                fasm_oracle_ext_unix_to_secs(time, extra),
                "prng#{i} secs time={time:#x} extra={extra:#x}"
            );
            assert_eq!(
                ext_read_time(time, extra),
                fasm_oracle_ext_read_time(time, extra),
                "prng#{i} bdfe time={time:#x} extra={extra:#x}"
            );
        }
    }
}

/// FASM-faithful host oracle for `fsTime2bdfe` — separate control-flow mirror
/// of `fs_common.inc` (not a call through [`fs_time2bdfe`]).
#[cfg(test)]
pub fn fasm_oracle_fs_time2bdfe(secs: u32) -> BdfeTime {
    const MONTHS: [u8; 24] = [
        31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31, // months
        31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31, // months2
    ];

    let mut eax = secs;
    let mut edx: u32;
    let ecx_60: u32 = 60;

    // div 60 → sec
    edx = eax % ecx_60;
    eax /= ecx_60;
    let sec = edx as u8;

    // div 60 → min
    edx = eax % ecx_60;
    eax /= ecx_60;
    let min = edx as u8;

    // div 24 → hour (stored as DX word)
    let ecx_24: u32 = 24;
    edx = eax % ecx_24;
    eax /= ecx_24;
    let hour = edx as u8;

    // div 365 → years / day-of-year
    let ecx_365: u32 = 365;
    edx = eax % ecx_365;
    eax /= ecx_365;
    let mut ebx = eax.wrapping_add(2001);
    let leaps = eax >> 2;
    let (subbed, _) = edx.overflowing_sub(leaps);
    if (subbed as i32) < 0 {
        ebx = ebx.wrapping_sub(1);
        edx = subbed.wrapping_add(365);
        if (ebx & 3) == 0 {
            edx = edx.wrapping_add(1);
        }
    } else {
        edx = subbed;
    }

    let table_base: usize = if (ebx & 3) == 0 { 12 } else { 0 };
    let mut month: u32 = 0;
    // DX as 16-bit register view of EDX low half
    let mut dx = (edx & 0xffff) as u16;
    loop {
        month = month.wrapping_add(1);
        let idx = table_base + (month as usize - 1);
        let mlen = if idx < 24 { MONTHS[idx] as u16 } else { 0 };
        let dl = dx as u8;
        let (new_dl, borrow) = dl.overflowing_sub(mlen as u8);
        if !borrow {
            dx = (dx & 0xff00) | (new_dl as u16);
            continue;
        }
        let dh = (dx >> 8) as u8;
        let (new_dh, _) = dh.overflowing_sub(1);
        dx = ((new_dh as u16) << 8) | (new_dl as u16);
        if (new_dh as i8) >= 0 {
            continue;
        }
        let day = new_dl.wrapping_add(mlen as u8).wrapping_add(1);
        return BdfeTime {
            sec,
            min,
            hour,
            day,
            month: month as u8,
            year: ebx as u16,
        };
    }
}

/// FASM-faithful FILETIME→secs oracle (`ntfs.inc` bias/clamp/div only).
#[cfg(test)]
pub fn fasm_oracle_ntfs_filetime_to_secs(filetime_lo: u32, filetime_hi: u32) -> u32 {
    let (eax, borrow) = filetime_lo.overflowing_sub(3365781504u32);
    let mut edx = filetime_hi
        .wrapping_sub(29389701u32)
        .wrapping_sub(if borrow { 1 } else { 0 });
    let ecx = 10_000_000u32;
    if edx >= ecx {
        edx = 0;
    }
    let dividend = ((edx as u64) << 32) | (eax as u64);
    (dividend / (ecx as u64)) as u32
}

/// True when FASM `div ecx` would raise #DE (quotient does not fit in EAX).
#[cfg(test)]
pub fn fasm_oracle_ntfs_div_overflows(filetime_lo: u32, filetime_hi: u32) -> bool {
    let (eax, borrow) = filetime_lo.overflowing_sub(3365781504u32);
    let mut edx = filetime_hi
        .wrapping_sub(29389701u32)
        .wrapping_sub(if borrow { 1 } else { 0 });
    let ecx = 10_000_000u32;
    if edx >= ecx {
        edx = 0;
    }
    let dividend = ((edx as u64) << 32) | (eax as u64);
    (dividend / (ecx as u64)) > u32::MAX as u64
}

/// FASM-faithful host oracle for `ntfs_datetime_to_bdfe` — bias/clamp/div
/// mirrored from `ntfs.inc`, then calendar via [`fasm_oracle_fs_time2bdfe`]
/// (not a call through [`ntfs_datetime_to_bdfe`]).
#[cfg(test)]
pub fn fasm_oracle_ntfs_datetime_to_bdfe(filetime_lo: u32, filetime_hi: u32) -> BdfeTime {
    fasm_oracle_fs_time2bdfe(fasm_oracle_ntfs_filetime_to_secs(filetime_lo, filetime_hi))
}

/// FASM-faithful host oracle for `ntfsCalculateTime` — G oracle then
/// `mul 10000000` + bias `add`/`adc` (not a call through [`ntfs_calculate_time`]).
#[cfg(test)]
pub fn fasm_oracle_ntfs_calculate_time(t: BdfeTime) -> (u32, u32) {
    let secs = fasm_oracle_fs_calculate_time(t);
    // mov edx, 10000000 / mul edx / add eax, bias_lo / adc edx, bias_hi
    let product = (secs as u64).wrapping_mul(NTFS_FILETIME_PER_SEC as u64);
    let (lo, c1) = (product as u32).overflowing_add(NTFS_FILETIME_BIAS_LO);
    let hi = ((product >> 32) as u32)
        .wrapping_add(NTFS_FILETIME_BIAS_HI)
        .wrapping_add(if c1 { 1 } else { 0 });
    (lo, hi)
}

/// FASM-faithful bigtime→secs oracle (`xfs.asm` bias/clamp/div only).
///
/// Independently mirrors the `sub`/`sbb`/`jnc`/`cmp`/`div` control flow —
/// not a call through [`xfs_bigtime_to_secs`].
#[cfg(test)]
pub fn fasm_oracle_xfs_bigtime_to_secs(bigtime_lo: u32, bigtime_hi: u32) -> u32 {
    let bias_lo = 0x1135_0000u32;
    let bias_hi = 0x2B61_0A37u32;
    let nano = 1_000_000_000u32;
    let (eax, borrow) = bigtime_lo.overflowing_sub(bias_lo);
    let edx = bigtime_hi
        .wrapping_sub(bias_hi)
        .wrapping_sub(if borrow { 1 } else { 0 });
    // jnc .after — CF from sbb
    let cf = ((bigtime_hi as u64) << 32 | bigtime_lo as u64) < ((bias_hi as u64) << 32 | bias_lo as u64);
    if cf {
        return 0;
    }
    let _ = borrow;
    let (eax, edx) = if edx >= nano {
        (0xFFFF_FFFFu32, nano - 1)
    } else {
        (eax, edx)
    };
    let dividend = ((edx as u64) << 32) | (eax as u64);
    (dividend / (nano as u64)) as u32
}

/// FASM-faithful host oracle for `xfs._.conv_bigtime_to_kos_epoch` — bias/clamp/div
/// mirrored from `xfs.asm`, then calendar via [`fasm_oracle_fs_time2bdfe`].
#[cfg(test)]
pub fn fasm_oracle_xfs_conv_bigtime_to_kos_epoch(bigtime_lo: u32, bigtime_hi: u32) -> BdfeTime {
    fasm_oracle_fs_time2bdfe(fasm_oracle_xfs_bigtime_to_secs(bigtime_lo, bigtime_hi))
}

/// FASM-faithful EXT Unix→secs oracle (`ext.inc` epoch-bits / sign / clamp).
///
/// Independently mirrors `and edx,3` / `test eax` / `dec edx` / `sub`/`sbb` /
/// `js`/`jnz` — not a call through [`ext_unix_to_secs`].
#[cfg(test)]
pub fn fasm_oracle_ext_unix_to_secs(i_time: u32, extra: u32) -> u32 {
    let offset = 978_307_200u32; // (365*31+8)*86400 — literal, not the const alias
    let mut edx = extra & 3;
    if (i_time as i32) < 0 {
        edx = edx.wrapping_sub(1);
    }
    let (eax, borrow) = i_time.overflowing_sub(offset);
    let edx = edx.wrapping_sub(if borrow { 1 } else { 0 });
    if (edx as i32) < 0 {
        return 0;
    }
    if edx != 0 {
        return 0xFFFF_FFFF;
    }
    eax
}

/// FASM-faithful host oracle for `ext_read_time` — epoch convert then calendar.
#[cfg(test)]
pub fn fasm_oracle_ext_read_time(i_time: u32, extra: u32) -> BdfeTime {
    fasm_oracle_fs_time2bdfe(fasm_oracle_ext_unix_to_secs(i_time, extra))
}
