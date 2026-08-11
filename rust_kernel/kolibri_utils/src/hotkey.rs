//! Cut BE: `hotkey_do_test` — HID hotkey field match against `kb_state`.
//!
//! Matches `kernel/hid/keyboard.inc` FASM leaf semantics:
//! * `CL ∈ {0,2,4}` selects Shift / Control / Alt 2-bit field from `kb_state`
//! * doubled `CL` selects the matching nibble from the hotkey `funcs` dword
//! * nibble ≥ 5 → fail (CF set)
//! * otherwise run inlined `hotkey_test0..4` on the 2-bit modifier field
//! * predicate returns 1 → pass (CF clear); 0 → fail (CF set)
//!
//! Reloc-free: predicates are inlined and selected branchlessly (no jump
//! table / `.rodata` — Cut BD `.flaglist` + Cut AZ lessons).

/// Number of defined hotkey predicates (`hotkey_tests_num`).
pub const HOTKEY_TESTS_NUM: u8 = 5;

/// Cut BE differential PRNG seed (`CUBE`).
pub const HOTKEY_DO_TEST_PRNG_SEED: u32 = 0x4355_4245;

/// Inlined `hotkey_test0`: `test al,al` / `setz al` — neither modifier.
#[inline(always)]
fn hotkey_test0(al: u8) -> u8 {
    u8::from(al == 0)
}

/// Inlined `hotkey_test1`: `test al,al` / `setnp al` — odd parity of AL.
///
/// Domain after `and al,3` is `{0,1,2,3}`; odd parity ≡ `{1,2}`.
#[inline(always)]
fn hotkey_test1(al: u8) -> u8 {
    u8::from(al == 1 || al == 2)
}

/// Inlined `hotkey_test2`: `cmp al,3` / `setz` — both modifiers.
#[inline(always)]
fn hotkey_test2(al: u8) -> u8 {
    u8::from(al == 3)
}

/// Inlined `hotkey_test3`: `cmp al,1` / `setz` — left-only.
#[inline(always)]
fn hotkey_test3(al: u8) -> u8 {
    u8::from(al == 1)
}

/// Inlined `hotkey_test4`: `cmp al,2` / `setz` — right-only.
#[inline(always)]
fn hotkey_test4(al: u8) -> u8 {
    u8::from(al == 2)
}

/// Branchless select among `hotkey_test0..4` (avoids LLVM jump tables).
#[inline(always)]
fn run_predicate(test_id: u8, field: u8) -> u8 {
    let t0 = hotkey_test0(field);
    let t1 = hotkey_test1(field);
    let t2 = hotkey_test2(field);
    let t3 = hotkey_test3(field);
    let t4 = hotkey_test4(field);
    u8::from(test_id == 0)
        .wrapping_mul(t0)
        .wrapping_add(u8::from(test_id == 1).wrapping_mul(t1))
        .wrapping_add(u8::from(test_id == 2).wrapping_mul(t2))
        .wrapping_add(u8::from(test_id == 3).wrapping_mul(t3))
        .wrapping_add(u8::from(test_id == 4).wrapping_mul(t4))
}

/// Run hotkey field match.
///
/// # Arguments
/// * `funcs` — dword at hotkey node `+4` (packed test-id nibbles).
/// * `kb_state` — current modifier bitmap (`[kb_state]`).
/// * `cl` — field select (`0` / `2` / `4`); only the low 8 bits matter.
///
/// # Returns
/// `0` if the test **passes** (legacy CF clear), nonzero if it **fails**
/// (legacy CF set). Matches FASM `cmp al,1` / `.fail: stc` polarity used by
/// `jc` / `jnc` callers in the hotkey loop.
#[inline(always)]
pub fn hotkey_do_test(funcs: u32, kb_state: u32, cl: u32) -> u32 {
    let cl8 = cl as u8;
    // FASM: mov edx,[kb_state] / shr edx,cl
    let shifted_state = kb_state.wrapping_shr(u32::from(cl8));
    // FASM: add cl,cl  (then used as shift count for funcs)
    let cl_doubled = cl8.wrapping_add(cl8);
    // FASM: mov eax,[eax+4] / shr eax,cl / and eax,15
    let test_id = ((funcs.wrapping_shr(u32::from(cl_doubled))) & 15) as u8;
    if test_id >= HOTKEY_TESTS_NUM {
        return 1; // .fail → stc
    }
    // FASM: xchg eax,edx / and al,3 / call [hotkey_tests+edx*4]
    let field = (shifted_state as u8) & 3;
    let pred = run_predicate(test_id, field);
    // FASM: cmp al,1 → CF clear iff al==1
    u32::from(pred != 1)
}

#[cfg(test)]
mod tests {
    use super::{hotkey_do_test, HOTKEY_DO_TEST_PRNG_SEED, HOTKEY_TESTS_NUM};

    /// Independent FASM-flow oracle (mirrors keyboard.inc hotkey_do_test).
    fn fasm_oracle(funcs: u32, kb_state: u32, cl: u8) -> u32 {
        let mut edx = kb_state;
        edx = edx.wrapping_shr(u32::from(cl));
        let cl2 = cl.wrapping_add(cl);
        let mut eax = funcs.wrapping_shr(u32::from(cl2)) & 15;
        if eax >= u32::from(HOTKEY_TESTS_NUM) {
            return 1;
        }
        let test_id = eax;
        eax = edx & 3;
        let al = if test_id == 0 {
            u32::from(eax == 0)
        } else if test_id == 1 {
            // setnp after test al,al: odd parity of low 8 bits (domain 0..=3)
            u32::from(eax == 1 || eax == 2)
        } else if test_id == 2 {
            u32::from(eax == 3)
        } else if test_id == 3 {
            u32::from(eax == 1)
        } else {
            u32::from(eax == 2)
        };
        // cmp al,1 → fail if al != 1
        u32::from(al != 1)
    }

    #[test]
    fn oracle_agrees_named_vectors() {
        // funcs nibbles: id0@bits0..3, id1@4..7, id2@8..11
        // CL=0 → nibble0; CL=2 → nibble1; CL=4 → nibble2
        let funcs = 0x0000_0210; // n0=0, n1=1, n2=2
                                 // kb_state: shift=0, ctrl=0, alt=0 → field0=0
        assert_eq!(hotkey_do_test(funcs, 0, 0), 0); // test0 on 0 → pass
        assert_eq!(fasm_oracle(funcs, 0, 0), 0);

        // field0=1 (LSHIFT) with test0 → fail
        assert_eq!(hotkey_do_test(funcs, 0b01, 0), 1);
        assert_eq!(fasm_oracle(funcs, 0b01, 0), 1);

        // CL=2, test1 (parity): field ctrl bits at 2..3; value 1 → odd → pass
        assert_eq!(hotkey_do_test(funcs, 0b0100, 2), 0);
        assert_eq!(fasm_oracle(funcs, 0b0100, 2), 0);

        // CL=4, test2 (both): alt bits 4..5 = 3 → pass
        assert_eq!(hotkey_do_test(funcs, 0b11_0000, 4), 0);
        assert_eq!(fasm_oracle(funcs, 0b11_0000, 4), 0);
    }

    #[test]
    fn all_predicates_on_fields_0_to_3() {
        for test_id in 0u8..5 {
            for field in 0u8..4 {
                let funcs = u32::from(test_id); // CL=0 uses low nibble
                let kb = u32::from(field);
                let got = hotkey_do_test(funcs, kb, 0);
                let exp = fasm_oracle(funcs, kb, 0);
                assert_eq!(got, exp, "id={test_id} field={field}");
            }
        }
    }

    #[test]
    fn out_of_range_nibble_fails() {
        for id in 5u8..=15 {
            let funcs = u32::from(id);
            assert_eq!(hotkey_do_test(funcs, 0, 0), 1);
            assert_eq!(fasm_oracle(funcs, 0, 0), 1);
        }
    }

    #[test]
    fn cl_field_select_0_2_4() {
        // Pack distinct ids into three nibbles: n0=3, n1=4, n2=0 → 0x043
        let funcs = 0x0000_0043u32;
        // Shift field=1 → test3 pass; ctrl=2 → test4 pass; alt=0 → test0 pass
        let kb = 0b00_10_01; // alt=0, ctrl=2, shift=1
        assert_eq!(hotkey_do_test(funcs, kb, 0), 0);
        assert_eq!(hotkey_do_test(funcs, kb, 2), 0);
        assert_eq!(hotkey_do_test(funcs, kb, 4), 0);
        assert_eq!(fasm_oracle(funcs, kb, 0), 0);
        assert_eq!(fasm_oracle(funcs, kb, 2), 0);
        assert_eq!(fasm_oracle(funcs, kb, 4), 0);
    }

    #[test]
    fn prng_50k_matches_oracle() {
        let mut state = HOTKEY_DO_TEST_PRNG_SEED;
        for _ in 0..50_000 {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let funcs = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let kb = state;
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let cl = match state % 3 {
                0 => 0u8,
                1 => 2,
                _ => 4,
            };
            assert_eq!(
                hotkey_do_test(funcs, kb, u32::from(cl)),
                fasm_oracle(funcs, kb, cl),
                "funcs={funcs:#x} kb={kb:#x} cl={cl}"
            );
        }
    }
}
