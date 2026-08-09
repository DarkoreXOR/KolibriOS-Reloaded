//! Cut G: `fsCalculateTime` — BDFE datetime → seconds since 2001-01-01.
//!
//! Matches `kernel/fs/fs_common.inc` FASM leaf semantics for the production
//! domain (year &lt; 3025 so `BH` pollution after `shr ebx,2` does not apply).
//! Month tables are stack-materialized so the freestanding FFI section stays
//! reloc-free (no `.rodata` absolute loads).

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
}
