//! Cut CB: `ahci_port_wait` — AHCI port TFD busy/DRQ poll with timer deadline.
//!
//! Matches `kernel/blkdev/ahci.inc` FASM leaf semantics:
//! * `deadline = timer_ticks + timeout` (unsigned wrap)
//! * Poll `[port + task_file_data]` until `(tfd & (BUSY|DRQ)) == 0`
//! * Loop while `timer_ticks < deadline` (unsigned `jb`)
//! * Return `0` success / `1` timeout
//!
//! MMIO and `timer_ticks` reads stay in FASM callbacks (reloc-free blob).

/// ATA task-file status bits (`kernel/blkdev/ahci.inc`).
pub const ATA_DEV_BUSY: u32 = 0x80;
pub const ATA_DEV_DRQ: u32 = 0x08;
pub const ATA_DEV_BUSY_DRQ_MASK: u32 = ATA_DEV_BUSY | ATA_DEV_DRQ;

/// Cut CB differential PRNG seed (`'CUTW'`).
pub const AHCI_PORT_WAIT_PRNG_SEED: u32 = 0x4355_5457;

/// Stdcall: read `[port + HBA_PORT.task_file_data]`.
pub type AhciReadTfdFn = unsafe extern "stdcall" fn(port: u32) -> u32;

/// Stdcall: read global `timer_ticks`.
pub type AhciReadTicksFn = unsafe extern "stdcall" fn() -> u32;

/// Independent FASM-flow oracle (not calling the Rust helper body).
#[inline(always)]
pub fn fasm_oracle_ahci_port_wait(
    mut read_tfd: impl FnMut() -> u32,
    mut read_ticks: impl FnMut() -> u32,
    timeout: u32,
) -> u32 {
    let start = read_ticks();
    let deadline = start.wrapping_add(timeout);
    loop {
        let tfd = read_tfd();
        if (tfd & ATA_DEV_BUSY_DRQ_MASK) == 0 {
            return 0;
        }
        if read_ticks() >= deadline {
            return 1;
        }
    }
}

/// FASM-faithful port busy-wait loop via injected readers.
#[inline(always)]
pub unsafe fn ahci_port_wait(
    read_tfd: AhciReadTfdFn,
    read_ticks: AhciReadTicksFn,
    port: u32,
    timeout: u32,
) -> u32 {
    let start = unsafe { read_ticks() };
    let deadline = start.wrapping_add(timeout);
    loop {
        let tfd = unsafe { read_tfd(port) };
        if (tfd & ATA_DEV_BUSY_DRQ_MASK) == 0 {
            return 0;
        }
        if unsafe { read_ticks() } >= deadline {
            return 1;
        }
    }
}

#[inline(always)]
pub unsafe fn ahci_port_wait_ptr(
    read_tfd: AhciReadTfdFn,
    read_ticks: AhciReadTicksFn,
    port: u32,
    timeout: u32,
) -> u32 {
    unsafe { ahci_port_wait(read_tfd, read_ticks, port, timeout) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    static MOCK_TFD: AtomicU32 = AtomicU32::new(0);
    static MOCK_TICKS: AtomicU32 = AtomicU32::new(0);

    unsafe extern "stdcall" fn mock_read_tfd(_port: u32) -> u32 {
        MOCK_TFD.load(Ordering::Relaxed)
    }

    unsafe extern "stdcall" fn mock_read_ticks() -> u32 {
        MOCK_TICKS.load(Ordering::Relaxed)
    }

    fn run_case(tfd_fn: impl FnMut() -> u32, mut tick_fn: impl FnMut() -> u32, timeout: u32) -> u32 {
        fasm_oracle_ahci_port_wait(tfd_fn, tick_fn, timeout)
    }

    fn check_mock(timeout: u32, exp: u32) {
        let got =
            unsafe { ahci_port_wait(mock_read_tfd, mock_read_ticks, 0x1000, timeout) };
        assert_eq!(got, exp, "timeout={timeout}");
    }

    fn check_oracle(
        tfd_seq: &[u32],
        tick_seq: &[u32],
        timeout: u32,
        exp: u32,
    ) {
        let mut ti = 0usize;
        let mut tfd_i = 0usize;
        let got = fasm_oracle_ahci_port_wait(
            || {
                let v = tfd_seq[tfd_i.min(tfd_seq.len() - 1)];
                if tfd_i + 1 < tfd_seq.len() {
                    tfd_i += 1;
                }
                v
            },
            || {
                let v = tick_seq[ti.min(tick_seq.len() - 1)];
                if ti + 1 < tick_seq.len() {
                    ti += 1;
                }
                v
            },
            timeout,
        );
        assert_eq!(got, exp, "timeout={timeout} tfd={tfd_seq:?} ticks={tick_seq:?}");
    }

    #[test]
    fn idle_immediate_success() {
        MOCK_TFD.store(0, Ordering::Relaxed);
        MOCK_TICKS.store(100, Ordering::Relaxed);
        check_mock(1000, 0);
        check_oracle(&[0], &[100], 1000, 0);
    }

    #[test]
    fn busy_only_not_masked() {
        // DRQ-only (0x08) still waits; BUSY-only (0x80) waits; both clear → success.
        check_oracle(&[0x08, 0x80, 0x00], &[10, 10, 11], 50, 0);
        check_oracle(&[0x80, 0x00], &[5, 6], 100, 0);
    }

    #[test]
    fn timeout_when_stuck_busy() {
        check_oracle(&[0x88, 0x88, 0x88], &[0, 1, 2], 2, 1);
        MOCK_TFD.store(0x80, Ordering::Relaxed);
        unsafe extern "stdcall" fn adv_ticks() -> u32 {
            MOCK_TICKS.fetch_add(1, Ordering::Relaxed)
        }
        MOCK_TICKS.store(0, Ordering::Relaxed);
        let got = unsafe { ahci_port_wait(mock_read_tfd, adv_ticks, 0, 3) };
        assert_eq!(got, 1);
    }

    #[test]
    fn deadline_unsigned_wrap() {
        // start=0xFFFF_FFF0, timeout=32 → deadline=0x10; loop tick 5 < 0x10 → clear
        check_oracle(&[0x80, 0x00], &[0xFFFF_FFF0, 5], 32, 0);
        // Timeout when loop tick reaches deadline
        check_oracle(&[0x80, 0x80], &[0xFFFF_FFF0, 0xF, 0x10], 32, 1);
    }

    #[test]
    fn zero_timeout_times_out_if_busy() {
        check_oracle(&[0x80], &[7, 7], 0, 1);
        check_oracle(&[0x00], &[7], 0, 0);
    }

    #[test]
    fn edge_masks() {
        let mut t = 0u32;
        assert_eq!(
            run_case(|| 0x71, || { let v = t; t += 1; v }, 10),
            0
        );
        t = 0;
        assert_eq!(
            run_case(|| 0x88, || { let v = t; t += 1; v }, 0),
            1
        );
        t = 0;
        assert_eq!(
            run_case(|| 0x08, || { let v = t; t += 1; v }, 5),
            1
        );
    }

    #[test]
    fn prng_50k_cutw() {
        let mut state = AHCI_PORT_WAIT_PRNG_SEED;
        for _ in 0..50_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let start = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let timeout = state & 0xFF;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let stuck_tfd = (state & 0xFF) | 0x80;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let clear_after = (state & 0x7) + 1;

            let mut poll = 0u32;
            let mut tick = start;
            let exp = fasm_oracle_ahci_port_wait(
                || {
                    poll = poll.wrapping_add(1);
                    if poll > clear_after {
                        0
                    } else {
                        stuck_tfd
                    }
                },
                || {
                    let v = tick;
                    tick = tick.wrapping_add(1);
                    v
                },
                timeout,
            );

            poll = 0;
            tick = start;
            let got = fasm_oracle_ahci_port_wait(
                || {
                    poll = poll.wrapping_add(1);
                    if poll > clear_after {
                        0
                    } else {
                        stuck_tfd
                    }
                },
                || {
                    let v = tick;
                    tick = tick.wrapping_add(1);
                    v
                },
                timeout,
            );
            assert_eq!(got, exp);
        }
    }
}
