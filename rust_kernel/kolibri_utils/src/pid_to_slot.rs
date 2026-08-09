//! Cut AA: `pid_to_slot` — process TID → slot-index linear walk.
//!
//! Matches `kernel/core/taskman.inc` FASM leaf semantics:
//! * Skip slot 0; start at offset `sizeof.APPDATA` (256)
//! * Bound = `thread_count << 8` (BSF sizeof.APPDATA = 8)
//! * Loop while `ecx <= ebx` as **signed** (`jle`)
//! * Skip entries with `APPDATA.state == TSTATE_FREE` (9)
//! * Match dword `APPDATA.tid == pid`
//! * Found → `ecx >> 8` (slot index); miss → 0
//!
//! `SLOT_BASE` and `thread_count` are passed explicitly so the Rust blob
//! stays reloc-free. No tables / `.rodata`.

/// `sizeof.APPDATA` (asserted in `kernel/const.inc`).
pub const APPDATA_SIZE: u32 = 256;

/// Bit-scan of `APPDATA_SIZE` (`BSF` → shift amount for × / ÷).
pub const APPDATA_SIZE_SHIFT: u32 = 8;

/// Offset of `APPDATA.tid` within a slot.
pub const OFF_TID: usize = 112;

/// Offset of `APPDATA.state` within a slot.
pub const OFF_STATE: usize = 124;

/// `TSTATE_FREE` — empty slot sentinel.
pub const TSTATE_FREE: u8 = 9;

/// Cut AA differential PRNG seed (`'CUTA'`).
pub const PID_TO_SLOT_PRNG_SEED: u32 = 0x4355_5441;

/// FASM-faithful TID → slot lookup.
///
/// Returns slot index (`1..=thread_count` when found) or `0` if missing.
///
/// # Safety
/// `slot_base` must be readable for at least
/// `(thread_count.wrapping_shl(APPDATA_SIZE_SHIFT) as usize) + APPDATA_SIZE`
/// bytes of APPDATA layout (or the loop must terminate earlier). Production
/// callers keep `thread_count ≤ max_processes` (255).
#[inline(always)]
pub unsafe fn pid_to_slot(pid: u32, slot_base: *const u8, thread_count: u32) -> u32 {
    // mov ebx, [thread_count] / shl ebx, BSF sizeof.APPDATA
    let ebx = thread_count.wrapping_shl(APPDATA_SIZE_SHIFT);
    // skip first process: mov ecx, sizeof.APPDATA
    let mut ecx = APPDATA_SIZE;

    // cmp ecx, ebx / jle .loop  — signed bound
    while (ecx as i32) <= (ebx as i32) {
        let entry = unsafe { slot_base.add(ecx as usize) };
        // cmp [SLOT_BASE+ecx+APPDATA.state], TSTATE_FREE / jz .endloop
        let state = unsafe { *entry.add(OFF_STATE) };
        if state != TSTATE_FREE {
            // cmp [SLOT_BASE+ecx+APPDATA.tid], eax / jz .pid_found
            let tid = unsafe { read_u32_le(entry.add(OFF_TID)) };
            if tid == pid {
                // shr ecx, BSF sizeof.APPDATA
                return ecx >> APPDATA_SIZE_SHIFT;
            }
        }
        // add ecx, sizeof.APPDATA
        ecx = ecx.wrapping_add(APPDATA_SIZE);
    }
    0
}

/// Pointer-form wrapper for the FFI boundary (same as [`pid_to_slot`]).
///
/// # Safety
/// Same as [`pid_to_slot`].
#[inline(always)]
pub unsafe fn pid_to_slot_ptr(pid: u32, slot_base: *const u8, thread_count: u32) -> u32 {
    unsafe { pid_to_slot(pid, slot_base, thread_count) }
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Write dword LE into a synthetic APPDATA table (tests / helpers).
#[inline(always)]
pub fn write_u32_le(buf: &mut [u8], off: usize, val: u32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

/// Plant one APPDATA slot at `index` inside a synthetic table.
#[inline(always)]
pub fn plant_slot(table: &mut [u8], index: u32, tid: u32, state: u8) {
    let base = (index as usize) * (APPDATA_SIZE as usize);
    write_u32_le(table, base + OFF_TID, tid);
    table[base + OFF_STATE] = state;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent FASM-flow oracle (same signed jle / free-skip / first match).
    fn oracle(pid: u32, table: &[u8], thread_count: u32) -> u32 {
        let ebx = thread_count.wrapping_shl(APPDATA_SIZE_SHIFT);
        let mut ecx = APPDATA_SIZE;
        while (ecx as i32) <= (ebx as i32) {
            let base = ecx as usize;
            if base + OFF_STATE >= table.len() {
                break;
            }
            let state = table[base + OFF_STATE];
            if state != TSTATE_FREE {
                let tid = u32::from_le_bytes([
                    table[base + OFF_TID],
                    table[base + OFF_TID + 1],
                    table[base + OFF_TID + 2],
                    table[base + OFF_TID + 3],
                ]);
                if tid == pid {
                    return ecx >> APPDATA_SIZE_SHIFT;
                }
            }
            ecx = ecx.wrapping_add(APPDATA_SIZE);
        }
        0
    }

    fn run_vs_oracle(pid: u32, table: &mut [u8], thread_count: u32) {
        let got = unsafe { pid_to_slot(pid, table.as_ptr(), thread_count) };
        let exp = oracle(pid, table, thread_count);
        assert_eq!(
            got, exp,
            "mismatch pid={pid:#x} thread_count={thread_count} got={got} exp={exp}"
        );
    }

    fn blank_table(slots: usize) -> Vec<u8> {
        let mut t = vec![0u8; slots * APPDATA_SIZE as usize];
        for i in 0..slots {
            t[i * APPDATA_SIZE as usize + OFF_STATE] = TSTATE_FREE;
        }
        t
    }

    #[test]
    fn empty_and_zero_thread_count() {
        let mut t = blank_table(4);
        run_vs_oracle(1, &mut t, 0);
        run_vs_oracle(0, &mut t, 0);
        run_vs_oracle(0xFFFF_FFFF, &mut t, 0);
    }

    #[test]
    fn skips_slot_zero() {
        let mut t = blank_table(4);
        // Slot 0 would match if scanned — must be ignored.
        plant_slot(&mut t, 0, 0x1111, 0); // TSTATE_RUNNING
        plant_slot(&mut t, 1, 0x2222, 0);
        run_vs_oracle(0x1111, &mut t, 2);
        assert_eq!(unsafe { pid_to_slot(0x1111, t.as_ptr(), 2) }, 0);
        run_vs_oracle(0x2222, &mut t, 2);
        assert_eq!(unsafe { pid_to_slot(0x2222, t.as_ptr(), 2) }, 1);
    }

    #[test]
    fn finds_first_and_last_in_range() {
        let mut t = blank_table(8);
        plant_slot(&mut t, 1, 10, 0);
        plant_slot(&mut t, 2, 20, 0);
        plant_slot(&mut t, 3, 30, 0);
        run_vs_oracle(10, &mut t, 3);
        run_vs_oracle(20, &mut t, 3);
        run_vs_oracle(30, &mut t, 3);
        assert_eq!(unsafe { pid_to_slot(10, t.as_ptr(), 3) }, 1);
        assert_eq!(unsafe { pid_to_slot(30, t.as_ptr(), 3) }, 3);
        // thread_count=2 excludes slot 3
        assert_eq!(unsafe { pid_to_slot(30, t.as_ptr(), 2) }, 0);
        run_vs_oracle(30, &mut t, 2);
    }

    #[test]
    fn skips_free_slots_even_if_tid_matches() {
        let mut t = blank_table(4);
        plant_slot(&mut t, 1, 0xABCD, TSTATE_FREE);
        plant_slot(&mut t, 2, 0xABCD, 0);
        run_vs_oracle(0xABCD, &mut t, 2);
        assert_eq!(unsafe { pid_to_slot(0xABCD, t.as_ptr(), 2) }, 2);
    }

    #[test]
    fn first_match_wins() {
        let mut t = blank_table(5);
        plant_slot(&mut t, 1, 7, 0);
        plant_slot(&mut t, 2, 7, 0);
        plant_slot(&mut t, 3, 7, 0);
        run_vs_oracle(7, &mut t, 3);
        assert_eq!(unsafe { pid_to_slot(7, t.as_ptr(), 3) }, 1);
    }

    #[test]
    fn inclusive_bound_at_thread_count() {
        // Legacy jle includes offset == thread_count * 256 (slot index = thread_count).
        let mut t = blank_table(5);
        plant_slot(&mut t, 4, 0x44, 0);
        run_vs_oracle(0x44, &mut t, 4);
        assert_eq!(unsafe { pid_to_slot(0x44, t.as_ptr(), 4) }, 4);
        run_vs_oracle(0x44, &mut t, 3);
        assert_eq!(unsafe { pid_to_slot(0x44, t.as_ptr(), 3) }, 0);
    }

    #[test]
    fn missing_pid_returns_zero() {
        let mut t = blank_table(4);
        plant_slot(&mut t, 1, 1, 0);
        plant_slot(&mut t, 2, 2, 0);
        run_vs_oracle(3, &mut t, 2);
        assert_eq!(unsafe { pid_to_slot(3, t.as_ptr(), 2) }, 0);
        run_vs_oracle(0, &mut t, 2);
    }

    #[test]
    fn all_non_free_states_are_scanned() {
        // Any state != TSTATE_FREE is eligible (running, zombie, waiting, …).
        for state in [0u8, 1, 2, 3, 4, 5, 8, 10, 0xFF] {
            let mut t = blank_table(3);
            plant_slot(&mut t, 1, 0x55, state);
            run_vs_oracle(0x55, &mut t, 1);
            assert_eq!(
                unsafe { pid_to_slot(0x55, t.as_ptr(), 1) },
                1,
                "state={state}"
            );
        }
    }

    #[test]
    fn signed_jle_bound_large_thread_count() {
        // ebx = thread_count << 8; when high, signed compare still applies.
        // thread_count=0x0080_0000 → ebx=0x8000_0000 (negative as i32).
        // ecx starts at 256 > ebx as signed → loop never runs → 0.
        let mut t = blank_table(4);
        plant_slot(&mut t, 1, 99, 0);
        let tc = 0x0080_0000u32;
        run_vs_oracle(99, &mut t, tc);
        assert_eq!(unsafe { pid_to_slot(99, t.as_ptr(), tc) }, 0);
    }

    #[test]
    fn wrapping_shl_on_huge_thread_count() {
        // thread_count with top bits set: shl wraps like FASM.
        let mut t = blank_table(4);
        plant_slot(&mut t, 1, 1, 0);
        for tc in [0x0100_0000u32, 0xFFFF_FFFF, 0x8000_0001] {
            run_vs_oracle(1, &mut t, tc);
        }
    }

    #[test]
    fn boot_like_idle_os_layout() {
        // Mirrors early boot: thread_count=2, slots 1 and 2 live, slot 0 free.
        let mut t = blank_table(4);
        plant_slot(&mut t, 1, 1, 0); // IDLE-ish
        plant_slot(&mut t, 2, 2, 0); // OS-ish
        run_vs_oracle(1, &mut t, 2);
        run_vs_oracle(2, &mut t, 2);
        run_vs_oracle(3, &mut t, 2);
        assert_eq!(unsafe { pid_to_slot(1, t.as_ptr(), 2) }, 1);
        assert_eq!(unsafe { pid_to_slot(2, t.as_ptr(), 2) }, 2);
    }

    #[test]
    fn prng_corpus_50k() {
        // Deterministic PRNG corpus (`'CUTA'`).
        let mut state = PID_TO_SLOT_PRNG_SEED;
        fn next(s: &mut u32) -> u32 {
            // xorshift32
            let mut x = *s;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *s = x;
            x
        }

        const SLOTS: usize = 16;
        for _ in 0..50_000 {
            let mut t = blank_table(SLOTS);
            let thread_count = (next(&mut state) % 15) + 1; // 1..15
            let n_live = (next(&mut state) % thread_count) + 1;
            for i in 0..n_live {
                let idx = i + 1; // never plant only in 0 for live set
                if idx as u32 > thread_count {
                    break;
                }
                let tid = next(&mut state);
                let st = if next(&mut state) & 7 == 0 {
                    TSTATE_FREE
                } else {
                    // Any non-9 byte is live; map accidental 9 → 0.
                    let b = (next(&mut state) & 0xFF) as u8;
                    if b == TSTATE_FREE {
                        0
                    } else {
                        b
                    }
                };
                plant_slot(&mut t, idx, tid, st);
            }
            // Also optionally clobber slot 0
            if next(&mut state) & 1 != 0 {
                plant_slot(&mut t, 0, next(&mut state), 0);
            }
            let query = if next(&mut state) & 3 == 0 {
                // sometimes query a known planted tid
                let idx = ((next(&mut state) % n_live) + 1) as u32;
                let base = (idx as usize) * APPDATA_SIZE as usize;
                u32::from_le_bytes([
                    t[base + OFF_TID],
                    t[base + OFF_TID + 1],
                    t[base + OFF_TID + 2],
                    t[base + OFF_TID + 3],
                ])
            } else {
                next(&mut state)
            };
            run_vs_oracle(query, &mut t, thread_count);
        }
    }
}
