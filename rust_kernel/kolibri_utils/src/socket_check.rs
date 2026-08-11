//! Cut AS: `socket_check` — lock-free socket-list membership.
//!
//! Matches `kernel/network/socket.inc` FASM leaf semantics:
//! * Reject null candidate immediately → `0`
//! * Start at `*[net_sockets + SOCKET.NextPtr]` (offset 0)
//! * Walk via each node's `NextPtr` until match or null
//! * First exact pointer match → return that pointer
//! * End of list → `0`
//!
//! Legacy also sets ZF from the final `test eax, eax` (FASM trampoline).
//! `net_sockets` is passed explicitly so the Rust blob stays reloc-free.
//! No tables / `.rodata` / mutex (unlike `socket_check_port`).

/// Offset of `SOCKET.NextPtr` within a socket (and the sentinel head).
pub const OFF_NEXT_PTR: usize = 0;

/// Cut AS differential PRNG seed (`'CUTS'`).
pub const SOCKET_CHECK_PRNG_SEED: u32 = 0x4355_5453;

/// Pure FASM-flow walk: `node` is the first list element (`sentinel.NextPtr`).
///
/// `load_next(addr)` returns `*(addr + NextPtr)` for a live socket node.
#[inline(always)]
pub fn socket_check_from_first(
    candidate: u32,
    mut node: u32,
    mut load_next: impl FnMut(u32) -> u32,
) -> u32 {
    // test eax, eax / jz .error
    if candidate == 0 {
        return 0;
    }
    // FASM: ebx = net_sockets; then loop mov ebx,[ebx+NextPtr] / …
    // Equivalent entry: node = first NextPtr already loaded.
    loop {
        // or ebx, ebx / jz .done
        if node == 0 {
            break;
        }
        // cmp ebx, eax / jz .done (fall through)
        if node == candidate {
            break;
        }
        // mov ebx, [ebx + SOCKET.NextPtr]
        node = load_next(node);
    }
    // mov eax, ebx
    node
}

/// FASM-faithful socket-pointer membership check.
///
/// Returns the candidate pointer when it appears in the list headed by
/// `net_sockets`, otherwise `0`.
///
/// # Safety
/// `net_sockets` must be readable as a sentinel with a valid singly-linked
/// `NextPtr` chain of socket nodes (or terminate with null). Production
/// callers hold the list without this leaf taking `socket_mutex`.
#[inline(always)]
pub unsafe fn socket_check(candidate: u32, net_sockets: *const u8) -> u32 {
    // mov ebx, net_sockets / mov ebx, [ebx + SOCKET.NextPtr]  (first iter)
    let first = unsafe { read_u32_le(net_sockets.add(OFF_NEXT_PTR)) };
    socket_check_from_first(candidate, first, |addr| unsafe {
        read_u32_le((addr as *const u8).add(OFF_NEXT_PTR))
    })
}

/// Pointer-form wrapper for the FFI boundary (same as [`socket_check`]).
///
/// # Safety
/// Same as [`socket_check`].
#[inline(always)]
pub unsafe fn socket_check_ptr(candidate: u32, net_sockets: *const u8) -> u32 {
    unsafe { socket_check(candidate, net_sockets) }
}

#[inline(always)]
unsafe fn read_u32_le(p: *const u8) -> u32 {
    let b = unsafe { core::slice::from_raw_parts(p, 4) };
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Independent FASM-flow oracle (HashMap arena; not derived from Rust).
    fn oracle(candidate: u32, first: u32, next_of: &HashMap<u32, u32>) -> u32 {
        if candidate == 0 {
            return 0;
        }
        let mut ebx = first;
        let mut guard = 0u32;
        loop {
            if ebx == 0 {
                break;
            }
            if ebx == candidate {
                break;
            }
            ebx = next_of.get(&ebx).copied().unwrap_or(0);
            guard += 1;
            if guard > 256 {
                break;
            }
        }
        ebx
    }

    fn run_vs_oracle(candidate: u32, first: u32, next_of: &HashMap<u32, u32>) {
        let got = socket_check_from_first(candidate, first, |addr| {
            next_of.get(&addr).copied().unwrap_or(0)
        });
        let exp = oracle(candidate, first, next_of);
        assert_eq!(
            got, exp,
            "mismatch candidate={candidate:#x} first={first:#x} got={got:#x} exp={exp:#x}"
        );
    }

    #[test]
    fn null_candidate_returns_zero() {
        let mut m = HashMap::new();
        m.insert(0xA0, 0xB0);
        m.insert(0xB0, 0);
        run_vs_oracle(0, 0xA0, &m);
        assert_eq!(
            socket_check_from_first(0, 0xA0, |a| m.get(&a).copied().unwrap_or(0)),
            0
        );
    }

    #[test]
    fn empty_list_miss() {
        let m = HashMap::new();
        run_vs_oracle(0xDEAD_BEEF, 0, &m);
        assert_eq!(
            socket_check_from_first(0xDEAD_BEEF, 0, |_| 0),
            0
        );
    }

    #[test]
    fn hit_first_middle_last() {
        let mut m = HashMap::new();
        m.insert(0x10, 0x20);
        m.insert(0x20, 0x30);
        m.insert(0x30, 0);
        run_vs_oracle(0x10, 0x10, &m);
        run_vs_oracle(0x20, 0x10, &m);
        run_vs_oracle(0x30, 0x10, &m);
        assert_eq!(
            socket_check_from_first(0x10, 0x10, |a| m[&a]),
            0x10
        );
        assert_eq!(
            socket_check_from_first(0x20, 0x10, |a| m[&a]),
            0x20
        );
        assert_eq!(
            socket_check_from_first(0x30, 0x10, |a| m[&a]),
            0x30
        );
    }

    #[test]
    fn miss_and_sentinel_not_in_list() {
        let mut m = HashMap::new();
        m.insert(0x10, 0x20);
        m.insert(0x20, 0);
        run_vs_oracle(0x1111_1111, 0x10, &m);
        // Sentinel address itself is never compared on first load path as a
        // member unless it also appears as a node pointer in the chain.
        run_vs_oracle(0x01, 0x10, &m);
        assert_eq!(
            socket_check_from_first(0x1111_1111, 0x10, |a| m.get(&a).copied().unwrap_or(0)),
            0
        );
    }

    #[test]
    fn single_node_hit_miss() {
        let mut m = HashMap::new();
        m.insert(0xAA, 0);
        run_vs_oracle(0xAA, 0xAA, &m);
        run_vs_oracle(0xAB, 0xAA, &m);
        assert_eq!(socket_check_from_first(0xAA, 0xAA, |a| m[&a]), 0xAA);
        assert_eq!(
            socket_check_from_first(0xAB, 0xAA, |a| m.get(&a).copied().unwrap_or(0)),
            0
        );
    }

    #[test]
    fn first_of_two_equal_queries_is_stable() {
        // List cannot contain duplicate addresses; querying the only match once.
        let mut m = HashMap::new();
        m.insert(0x40, 0x50);
        m.insert(0x50, 0);
        run_vs_oracle(0x40, 0x40, &m);
        run_vs_oracle(0x50, 0x40, &m);
    }

    #[test]
    fn prng_corpus_50k() {
        let mut state = SOCKET_CHECK_PRNG_SEED;
        fn next(s: &mut u32) -> u32 {
            let mut x = *s;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *s = x;
            x
        }

        for _ in 0..50_000 {
            let n = (next(&mut state) % 13) as usize; // 0..12 nodes
            let mut addrs = Vec::with_capacity(n);
            for i in 0..n {
                // Distinct non-zero fake addresses
                addrs.push(0x1000_0000 + (i as u32 + 1) * 0x20 + (next(&mut state) & 0xF));
            }
            // Ensure uniqueness
            addrs.sort_unstable();
            addrs.dedup();
            let n = addrs.len();

            let mut m = HashMap::new();
            let first = if n == 0 { 0 } else { addrs[0] };
            for i in 0..n {
                let nxt = if i + 1 < n { addrs[i + 1] } else { 0 };
                m.insert(addrs[i], nxt);
            }

            let query = if n > 0 && next(&mut state) & 3 != 0 {
                addrs[(next(&mut state) as usize) % n]
            } else if next(&mut state) & 7 == 0 {
                0
            } else {
                next(&mut state) | 1
            };
            run_vs_oracle(query, first, &m);
        }
    }
}
