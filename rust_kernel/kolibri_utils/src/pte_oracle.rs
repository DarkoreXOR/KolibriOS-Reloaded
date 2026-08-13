//! Stage-4 host-only PTE / `page_tabs` oracle (research — not a production cut).
//!
//! Independent state model for KolibriOS recursive page-table window semantics.
//! Does **not** claim production ownership and is not wired to any `USE_RUST_*` gate.
//!
//! Two layers:
//!
//! 1. [`IndependentPageMap`] — map from VPN → entry (hardware PTE **or** soft
//!    heap descriptor). Not a transcription of `map_page`.
//! 2. [`FasmPteEmu`] — instruction-faithful model of the common store shape
//!    `(phys | flags) & valid_mask` into a flat PTE array + recorded `invlpg` set.
//!
//! **Limitation:** does not execute assembled FASM under Unicorn; does not model
//! CR3 switches, multi-address-space process PDTs, or real TLB hardware. Soft
//! heap `MEM_BLOCK_*` tags are modeled because they share `page_tabs` cells.
//!
//! Seed: `'PTEO'` (`0x5054_454F`). See
//! `docs/migration/stage4-pte-ownership-design.md`.

#![cfg(test)]

/// PRNG seed for Stage-4 PTE differential (`'PTEO'`).
pub const PTE_ORACLE_PRNG_SEED: u32 = 0x5054_454F;

pub const PG_READ: u32 = 0x001;
pub const PG_WRITE: u32 = 0x002;
pub const PG_USER: u32 = 0x004;
pub const PG_UNMAP: u32 = 0x000;
pub const PG_SWR: u32 = PG_WRITE | PG_READ;
pub const PG_UWR: u32 = PG_USER | PG_WRITE | PG_READ;
pub const PG_UR: u32 = PG_USER | PG_READ;

pub const MEM_BLOCK_RESERVED: u32 = 0x02;
pub const MEM_BLOCK_FREE: u32 = 0x04;
pub const MEM_BLOCK_USED: u32 = 0x08;
pub const MEM_BLOCK_DONT_FREE: u32 = 0x10;

const DEFAULT_VALID_MASK: u32 = 0xFFFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryKind {
    /// Hardware-shaped PTE (present bit may be 0 for unmap / soft tags).
    Hardware = 0,
    /// Explicit soft heap descriptor (present clear; MEM_BLOCK_* in low bits).
    SoftHeap = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageEntry {
    pub raw: u32,
    pub kind: EntryKind,
}

impl PageEntry {
    pub fn empty() -> Self {
        Self {
            raw: 0,
            kind: EntryKind::Hardware,
        }
    }

    pub fn present(self) -> bool {
        self.raw & PG_READ != 0
    }

    pub fn writable(self) -> bool {
        self.raw & PG_WRITE != 0
    }

    pub fn user(self) -> bool {
        self.raw & PG_USER != 0
    }

    pub fn frame(self) -> u32 {
        self.raw & !0xFFF
    }
}

/// Independent VPN → entry model + expected TLB invalidate VPN set.
#[derive(Clone, Debug)]
pub struct IndependentPageMap {
    /// Sparse map: VPN → entry.
    pub entries: std::collections::BTreeMap<u32, PageEntry>,
    pub valid_mask: u32,
    /// VPNs that must have been invalidated since last clear.
    pub expected_invlpg: std::collections::BTreeSet<u32>,
}

impl IndependentPageMap {
    pub fn new() -> Self {
        Self {
            entries: std::collections::BTreeMap::new(),
            valid_mask: DEFAULT_VALID_MASK,
            expected_invlpg: std::collections::BTreeSet::new(),
        }
    }

    pub fn map_page(&mut self, lin: u32, phys: u32, flags: u32) {
        let vpn = lin >> 12;
        let raw = (phys | flags) & self.valid_mask;
        self.entries.insert(
            vpn,
            PageEntry {
                raw,
                kind: EntryKind::Hardware,
            },
        );
        self.expected_invlpg.insert(vpn);
    }

    pub fn unmap_page(&mut self, lin: u32) {
        let vpn = lin >> 12;
        self.entries.insert(vpn, PageEntry::empty());
        self.expected_invlpg.insert(vpn);
    }

    /// Soft heap store (present bit must remain clear for true soft tags).
    pub fn store_soft(&mut self, vpn: u32, raw: u32) {
        debug_assert!(raw & PG_READ == 0, "soft heap tags must not set present");
        self.entries.insert(
            vpn,
            PageEntry {
                raw,
                kind: EntryKind::SoftHeap,
            },
        );
        // Soft metadata updates do not always invlpg in FASM; leave caller choice.
    }

    pub fn get(&self, vpn: u32) -> PageEntry {
        self.entries.get(&vpn).copied().unwrap_or_else(PageEntry::empty)
    }

    pub fn clear_invlpg(&mut self) {
        self.expected_invlpg.clear();
    }

    pub fn digest(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for (&vpn, e) in &self.entries {
            h ^= u64::from(vpn);
            h = h.wrapping_mul(0x100000001b3);
            h ^= u64::from(e.raw);
            h = h.wrapping_mul(0x100000001b3);
            h ^= u64::from(e.kind as u8);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

/// Flat PTE array emulator matching `map_page` / `unmap` store shape.
#[derive(Clone, Debug)]
pub struct FasmPteEmu {
    pub tabs: Vec<u32>,
    pub valid_mask: u32,
    pub invlpg: std::collections::BTreeSet<u32>,
}

impl FasmPteEmu {
    pub fn with_slots(n: usize) -> Self {
        Self {
            tabs: vec![0; n],
            valid_mask: DEFAULT_VALID_MASK,
            invlpg: std::collections::BTreeSet::new(),
        }
    }

    pub fn map_page(&mut self, lin: u32, phys: u32, flags: u32) {
        let vpn = (lin >> 12) as usize;
        assert!(vpn < self.tabs.len());
        self.tabs[vpn] = (phys | flags) & self.valid_mask;
        self.invlpg.insert(lin >> 12);
    }

    pub fn unmap_page(&mut self, lin: u32) {
        let vpn = (lin >> 12) as usize;
        assert!(vpn < self.tabs.len());
        self.tabs[vpn] = 0;
        self.invlpg.insert(lin >> 12);
    }

    pub fn xchg_clear(&mut self, lin: u32) -> u32 {
        let vpn = (lin >> 12) as usize;
        let old = self.tabs[vpn];
        self.tabs[vpn] = 0;
        self.invlpg.insert(lin >> 12);
        old
    }

    pub fn store_raw(&mut self, vpn: u32, raw: u32) {
        let i = vpn as usize;
        assert!(i < self.tabs.len());
        self.tabs[i] = raw;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct XorShift32(u32);
    impl XorShift32 {
        fn next_u32(&mut self) -> u32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.0 = x;
            x
        }
    }

    #[test]
    fn pteo_map_unmap_roundtrip() {
        let mut ind = IndependentPageMap::new();
        let mut emu = FasmPteEmu::with_slots(1024);
        let lin = 0x0004_0000u32;
        let phys = 0x0010_0000u32;
        ind.map_page(lin, phys, PG_UWR);
        emu.map_page(lin, phys, PG_UWR);
        assert_eq!(ind.get(lin >> 12).raw, emu.tabs[(lin >> 12) as usize]);
        assert!(ind.get(lin >> 12).present());
        assert!(ind.get(lin >> 12).user());
        assert!(ind.get(lin >> 12).writable());
        ind.unmap_page(lin);
        emu.unmap_page(lin);
        assert_eq!(ind.get(lin >> 12).raw, 0);
        assert_eq!(emu.tabs[(lin >> 12) as usize], 0);
        assert_eq!(ind.expected_invlpg, emu.invlpg);
    }

    #[test]
    fn pteo_remap_replaces_frame() {
        let mut ind = IndependentPageMap::new();
        let lin = 0x0080_0000u32;
        ind.map_page(lin, 0x1000, PG_SWR);
        ind.map_page(lin, 0x2000, PG_SWR);
        assert_eq!(ind.get(lin >> 12).frame(), 0x2000);
    }

    #[test]
    fn pteo_unmap_absent_is_zero() {
        let mut ind = IndependentPageMap::new();
        ind.unmap_page(0x00A0_0000);
        assert_eq!(ind.get(0x00A0_0000 >> 12).raw, 0);
    }

    #[test]
    fn pteo_soft_heap_reserved_not_present() {
        let mut ind = IndependentPageMap::new();
        let vpn = 0x100u32;
        ind.store_soft(vpn, MEM_BLOCK_RESERVED);
        let e = ind.get(vpn);
        assert!(!e.present());
        assert_eq!(e.raw, MEM_BLOCK_RESERVED);
        // Fault path treats bit1 as "reserved for usage"
        assert_eq!(e.raw & PG_WRITE, PG_WRITE);
    }

    #[test]
    fn pteo_valid_mask_applied() {
        let mut ind = IndependentPageMap::new();
        ind.valid_mask = 0xFFFF_F007; // drop some high attribute bits
        ind.map_page(0x1000, 0xABCDE000, 0xFFFF);
        assert_eq!(ind.get(1).raw, (0xABCDE000u32 | 0xFFFF) & 0xFFFF_F007);
    }

    #[test]
    fn pteo_release_xchg_polarity() {
        let mut emu = FasmPteEmu::with_slots(64);
        emu.map_page(0x2000, 0x3000, PG_SWR);
        let old = emu.xchg_clear(0x2000);
        assert_eq!(old & PG_READ, PG_READ);
        assert_eq!(emu.tabs[2], 0);
        let old2 = emu.xchg_clear(0x2000);
        assert_eq!(old2, 0);
    }

    #[test]
    fn pteo_randomized_50k_map_unmap() {
        let mut rng = XorShift32(PTE_ORACLE_PRNG_SEED);
        let slots = 4096usize;
        let mut ind = IndependentPageMap::new();
        let mut emu = FasmPteEmu::with_slots(slots);
        for _ in 0..50_000 {
            let vpn = rng.next_u32() % (slots as u32);
            let lin = vpn << 12;
            let op = rng.next_u32() % 5;
            match op {
                0 => {
                    let phys = (rng.next_u32() & 0x000F_F000) + 0x0010_0000;
                    let flags = [PG_SWR, PG_UWR, PG_UR, PG_READ, PG_UNMAP][(rng.next_u32() % 5) as usize];
                    ind.map_page(lin, phys, flags);
                    emu.map_page(lin, phys, flags);
                }
                1 => {
                    ind.unmap_page(lin);
                    emu.unmap_page(lin);
                }
                2 => {
                    let _ = emu.xchg_clear(lin);
                    ind.unmap_page(lin);
                }
                3 => {
                    // Soft tag — independent only tracks kind; emu stores raw.
                    let tag = [MEM_BLOCK_FREE, MEM_BLOCK_USED, MEM_BLOCK_RESERVED]
                        [(rng.next_u32() % 3) as usize];
                    ind.store_soft(vpn, tag);
                    emu.store_raw(vpn, tag);
                    assert_eq!(ind.get(vpn).raw, emu.tabs[vpn as usize]);
                    continue;
                }
                _ => {
                    // Remap same VPN
                    let phys = (rng.next_u32() & 0x000F_F000) + 0x0020_0000;
                    ind.map_page(lin, phys, PG_UWR);
                    emu.map_page(lin, phys, PG_UWR);
                }
            }
            assert_eq!(
                ind.get(vpn).raw,
                emu.tabs[vpn as usize],
                "vpn={vpn} digest_ind={:#x}",
                ind.digest()
            );
        }
    }
}
