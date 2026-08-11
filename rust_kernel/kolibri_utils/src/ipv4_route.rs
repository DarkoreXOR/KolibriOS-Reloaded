//! Cut AC: `ipv4_route` — IPv4 on-link / gateway / broadcast egress selection.
//!
//! Matches `kernel/network/IPv4.inc` FASM leaf semantics:
//! * `EBX == 0`: auto-route (broadcast / on-link / gateway / loopback)
//! * `EBX != 0`: `net_ptr_to_num4`-style device-list scan, then on-link vs gateway
//! * Tables are `NET_DEVICES_MAX` (16) dwords each; index is byte offset `edi = n*4`
//! * Broadcast auto-route picks first active link without IP, else first with link
//! * Explicit-device path overwrites source IP from `IPv4_address[edi]`
//! * Auto-route fills source IP from `IPv4_address[edi]` only when incoming source is 0
//!
//! Table bases and `net_device_list` are passed explicitly so the Rust blob
//! stays reloc-free. `NET_DEVICE.link_state` is read at offset 24.

/// `NET_DEVICES_MAX` from `kernel/network/stack.inc`.
pub const NET_DEVICES_MAX: usize = 16;

/// Byte span of one device-indexed dword table (`4 * NET_DEVICES_MAX`).
pub const TABLE_BYTES: usize = 4 * NET_DEVICES_MAX;

/// Offset of `NET_DEVICE.link_state` within the device structure.
pub const OFF_LINK_STATE: usize = 24;

/// IPv4 limited broadcast (`0xffffffff`).
pub const IPV4_BROADCAST: u32 = 0xffff_ffff;

/// Cut AC differential PRNG seed (`'CUTC'`).
pub const IPV4_ROUTE_PRNG_SEED: u32 = 0x4355_5443;

/// Result triple matching FASM `EAX` / `EDX` / `EDI` outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ipv4RouteResult {
    /// Destination IP (may be rewritten to gateway) or `0` on fail (`EAX`).
    pub dest_ip: u32,
    /// Source IP (`EDX`).
    pub source_ip: u32,
    /// Device index × 4, or `0xffffffff` on device-ptr fail (`EDI`).
    pub device_idx4: u32,
}

#[inline(always)]
unsafe fn read_u32(base: *const u32, idx4: u32) -> u32 {
    // idx4 is a byte offset into a dword table (0, 4, 8, ...).
    let p = unsafe { (base as *const u8).add(idx4 as usize) as *const u32 };
    unsafe { *p }
}

#[inline(always)]
unsafe fn device_link_state(net_device_list: *const u32, idx4: u32) -> Option<(u32, u32)> {
    let dev = unsafe { read_u32(net_device_list, idx4) };
    if dev == 0 {
        return None;
    }
    let link = unsafe { *((dev as *const u8).add(OFF_LINK_STATE) as *const u32) };
    Some((dev, link))
}

/// Inline device-list scan used by the explicit-device route path.
///
/// Mirrors FASM `net_ptr_to_num4` (Cut AY owns the public leaf). Returns byte
/// offset `n*4`, or `None` if missing / null device.
#[inline(always)]
pub unsafe fn find_device_idx4(device_ptr: u32, net_device_list: *const u32) -> Option<u32> {
    if device_ptr == 0 {
        return None;
    }
    let mut edi = 0u32;
    for _ in 0..NET_DEVICES_MAX {
        if unsafe { read_u32(net_device_list, edi) } == device_ptr {
            return Some(edi);
        }
        edi = edi.wrapping_add(4);
    }
    None
}

/// FASM-faithful IPv4 route selection.
///
/// # Safety
/// `ipv4_address` / `ipv4_subnet` / `ipv4_gateway` / `net_device_list` must each
/// be readable for [`TABLE_BYTES`] bytes. Non-null device pointers in the list
/// must expose a readable `link_state` dword at [`OFF_LINK_STATE`].
#[inline(always)]
pub unsafe fn ipv4_route(
    dest_ip: u32,
    device_ptr: u32,
    source_ip: u32,
    ipv4_address: *const u32,
    ipv4_subnet: *const u32,
    ipv4_gateway: *const u32,
    net_device_list: *const u32,
) -> Ipv4RouteResult {
    unsafe {
        ipv4_route_inner(
            dest_ip,
            device_ptr,
            source_ip,
            ipv4_address,
            ipv4_subnet,
            ipv4_gateway,
            net_device_list,
            |idx4| device_link_state(net_device_list, idx4).map(|(_, link)| link),
        )
    }
}

/// Core router with injectable link-state lookup (broadcast path).
///
/// `link_at(idx4)` returns `Some(link_state)` when the device slot is present
/// (non-null in `net_device_list`), or `None` when the slot is empty.
#[inline(always)]
unsafe fn ipv4_route_inner(
    mut dest_ip: u32,
    device_ptr: u32,
    source_ip: u32,
    ipv4_address: *const u32,
    ipv4_subnet: *const u32,
    ipv4_gateway: *const u32,
    net_device_list: *const u32,
    link_at: impl Fn(u32) -> Option<u32>,
) -> Ipv4RouteResult {
    if device_ptr != 0 {
        return unsafe {
            ipv4_route_got_device(
                dest_ip,
                device_ptr,
                source_ip,
                ipv4_address,
                ipv4_subnet,
                ipv4_gateway,
                net_device_list,
            )
        };
    }

    // Broadcast does not need gateway
    if dest_ip == IPV4_BROADCAST {
        return unsafe { ipv4_route_broadcast(dest_ip, source_ip, ipv4_address, link_at) };
    }

    // Check for on-link
    let mut edi = 0u32;
    while edi < TABLE_BYTES as u32 {
        let addr = unsafe { read_u32(ipv4_address, edi) };
        let subnet = unsafe { read_u32(ipv4_subnet, edi) };
        let masked_local = addr & subnet;
        if masked_local != 0 {
            let masked_dest = dest_ip & subnet;
            if masked_local == masked_dest {
                return unsafe { got_it(dest_ip, source_ip, edi, ipv4_address) };
            }
        }
        edi = edi.wrapping_add(4);
    }

    // no on-link match, find first device with a gateway (skip loopback)
    edi = 4;
    while edi < TABLE_BYTES as u32 {
        if unsafe { read_u32(ipv4_gateway, edi) } != 0 {
            dest_ip = unsafe { read_u32(ipv4_gateway, edi) };
            return unsafe { got_it(dest_ip, source_ip, edi, ipv4_address) };
        }
        edi = edi.wrapping_add(4);
    }

    // fall-back to loopback device
    unsafe { got_it(dest_ip, source_ip, 0, ipv4_address) }
}

#[inline(always)]
unsafe fn got_it(
    dest_ip: u32,
    mut source_ip: u32,
    edi: u32,
    ipv4_address: *const u32,
) -> Ipv4RouteResult {
    if source_ip == 0 {
        source_ip = unsafe { read_u32(ipv4_address, edi) };
    }
    Ipv4RouteResult {
        dest_ip,
        source_ip,
        device_idx4: edi,
    }
}

#[inline(always)]
unsafe fn ipv4_route_broadcast(
    dest_ip: u32,
    source_ip: u32,
    ipv4_address: *const u32,
    link_at: impl Fn(u32) -> Option<u32>,
) -> Ipv4RouteResult {
    // first non-loopback device with active link and no IP
    let mut edi = 4u32;
    while edi < TABLE_BYTES as u32 {
        if let Some(link) = link_at(edi) {
            if link != 0 && unsafe { read_u32(ipv4_address, edi) } == 0 {
                return unsafe { got_it(dest_ip, source_ip, edi, ipv4_address) };
            }
        }
        edi = edi.wrapping_add(4);
    }

    // else first non-loopback with active link (any IP)
    edi = 4;
    while edi < TABLE_BYTES as u32 {
        if let Some(link) = link_at(edi) {
            if link != 0 {
                return unsafe { got_it(dest_ip, source_ip, edi, ipv4_address) };
            }
        }
        edi = edi.wrapping_add(4);
    }

    // loopback fall-back
    unsafe { got_it(dest_ip, source_ip, 0, ipv4_address) }
}

#[inline(always)]
unsafe fn ipv4_route_got_device(
    mut dest_ip: u32,
    device_ptr: u32,
    _source_ip_in: u32,
    ipv4_address: *const u32,
    ipv4_subnet: *const u32,
    ipv4_gateway: *const u32,
    net_device_list: *const u32,
) -> Ipv4RouteResult {
    let Some(edi) = (unsafe { find_device_idx4(device_ptr, net_device_list) }) else {
        // .fail: xor eax,eax; edi left at -1 from net_ptr_to_num4
        return Ipv4RouteResult {
            dest_ip: 0,
            source_ip: _source_ip_in,
            device_idx4: 0xffff_ffff,
        };
    };

    // Explicit device: source IP always taken from the device address table.
    let source_ip = unsafe { read_u32(ipv4_address, edi) };

    // Broadcast does not need gateway
    if dest_ip != IPV4_BROADCAST {
        let local = unsafe { read_u32(ipv4_address, edi) } & unsafe { read_u32(ipv4_subnet, edi) };
        let remote = dest_ip & unsafe { read_u32(ipv4_subnet, edi) };
        if remote != local {
            dest_ip = unsafe { read_u32(ipv4_gateway, edi) };
        }
    }

    Ipv4RouteResult {
        dest_ip,
        source_ip,
        device_idx4: edi,
    }
}

/// Pointer-form wrapper for the FFI boundary.
///
/// # Safety
/// Same as [`ipv4_route`].
#[inline(always)]
pub unsafe fn ipv4_route_ptr(
    dest_ip: u32,
    device_ptr: u32,
    source_ip: u32,
    ipv4_address: *const u32,
    ipv4_subnet: *const u32,
    ipv4_gateway: *const u32,
    net_device_list: *const u32,
) -> Ipv4RouteResult {
    unsafe {
        ipv4_route(
            dest_ip,
            device_ptr,
            source_ip,
            ipv4_address,
            ipv4_subnet,
            ipv4_gateway,
            net_device_list,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Host-safe tables: `list[i]` holds a synthetic device token (not a real
    /// pointer). Broadcast link-state is supplied via `link_states[]` into
    /// `ipv4_route_inner`, matching production `device_link_state` semantics
    /// without truncating 64-bit host pointers to `u32`.
    struct Tables {
        address: [u32; NET_DEVICES_MAX],
        subnet: [u32; NET_DEVICES_MAX],
        gateway: [u32; NET_DEVICES_MAX],
        link_states: [u32; NET_DEVICES_MAX],
        list: [u32; NET_DEVICES_MAX],
    }

    impl Tables {
        fn new() -> Self {
            Self {
                address: [0; NET_DEVICES_MAX],
                subnet: [0; NET_DEVICES_MAX],
                gateway: [0; NET_DEVICES_MAX],
                link_states: [0; NET_DEVICES_MAX],
                list: core::array::from_fn(|i| 0xA000_0001 + i as u32),
            }
        }

        unsafe fn route(&self, dest: u32, device: u32, source: u32) -> Ipv4RouteResult {
            let links = &self.link_states;
            let list = &self.list;
            unsafe {
                ipv4_route_inner(
                    dest,
                    device,
                    source,
                    self.address.as_ptr(),
                    self.subnet.as_ptr(),
                    self.gateway.as_ptr(),
                    self.list.as_ptr(),
                    |idx4| {
                        let i = (idx4 / 4) as usize;
                        if list[i] == 0 {
                            None
                        } else {
                            Some(links[i])
                        }
                    },
                )
            }
        }
    }

    fn oracle(
        mut dest_ip: u32,
        device_ptr: u32,
        mut source_ip: u32,
        address: &[u32; NET_DEVICES_MAX],
        subnet: &[u32; NET_DEVICES_MAX],
        gateway: &[u32; NET_DEVICES_MAX],
        list: &[u32; NET_DEVICES_MAX],
        link_states: &[u32; NET_DEVICES_MAX],
    ) -> Ipv4RouteResult {
        let read = |tab: &[u32; NET_DEVICES_MAX], idx4: u32| tab[(idx4 / 4) as usize];
        let link = |idx4: u32| {
            let i = (idx4 / 4) as usize;
            if list[i] == 0 {
                None
            } else {
                Some(link_states[i])
            }
        };
        let got_it = |dest: u32, mut src: u32, edi: u32| {
            if src == 0 {
                src = read(address, edi);
            }
            Ipv4RouteResult {
                dest_ip: dest,
                source_ip: src,
                device_idx4: edi,
            }
        };

        if device_ptr != 0 {
            let mut found = None;
            for i in 0..NET_DEVICES_MAX {
                if list[i] == device_ptr {
                    found = Some((i as u32) * 4);
                    break;
                }
            }
            let Some(edi) = found else {
                return Ipv4RouteResult {
                    dest_ip: 0,
                    source_ip,
                    device_idx4: 0xffff_ffff,
                };
            };
            source_ip = read(address, edi);
            if dest_ip != IPV4_BROADCAST {
                let local = read(address, edi) & read(subnet, edi);
                let remote = dest_ip & read(subnet, edi);
                if remote != local {
                    dest_ip = read(gateway, edi);
                }
            }
            return Ipv4RouteResult {
                dest_ip,
                source_ip,
                device_idx4: edi,
            };
        }

        if dest_ip == IPV4_BROADCAST {
            let mut edi = 4u32;
            while edi < TABLE_BYTES as u32 {
                if let Some(ls) = link(edi) {
                    if ls != 0 && read(address, edi) == 0 {
                        return got_it(dest_ip, source_ip, edi);
                    }
                }
                edi += 4;
            }
            edi = 4;
            while edi < TABLE_BYTES as u32 {
                if let Some(ls) = link(edi) {
                    if ls != 0 {
                        return got_it(dest_ip, source_ip, edi);
                    }
                }
                edi += 4;
            }
            return got_it(dest_ip, source_ip, 0);
        }

        let mut edi = 0u32;
        while edi < TABLE_BYTES as u32 {
            let masked_local = read(address, edi) & read(subnet, edi);
            if masked_local != 0 {
                let masked_dest = dest_ip & read(subnet, edi);
                if masked_local == masked_dest {
                    return got_it(dest_ip, source_ip, edi);
                }
            }
            edi += 4;
        }

        edi = 4;
        while edi < TABLE_BYTES as u32 {
            if read(gateway, edi) != 0 {
                dest_ip = read(gateway, edi);
                return got_it(dest_ip, source_ip, edi);
            }
            edi += 4;
        }

        got_it(dest_ip, source_ip, 0)
    }

    fn check(t: &Tables, dest: u32, device: u32, source: u32) {
        let got = unsafe { t.route(dest, device, source) };
        let exp = oracle(
            dest,
            device,
            source,
            &t.address,
            &t.subnet,
            &t.gateway,
            &t.list,
            &t.link_states,
        );
        assert_eq!(got, exp, "dest={dest:#x} device={device:#x} source={source:#x}");
    }

    #[test]
    fn on_link_match_fills_source_when_zero() {
        let mut t = Tables::new();
        t.address[0] = 0x0100_007f;
        t.subnet[0] = 0x0000_00ff;
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        t.gateway[1] = 0x0100_000a;
        t.link_states[1] = 1;
        check(&t, 0x0300_000a, 0, 0);
        let r = unsafe { t.route(0x0300_000a, 0, 0) };
        assert_eq!(r.device_idx4, 4);
        assert_eq!(r.source_ip, 0x0200_000a);
        assert_eq!(r.dest_ip, 0x0300_000a);
    }

    #[test]
    fn on_link_preserves_nonzero_source() {
        let mut t = Tables::new();
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        check(&t, 0x0300_000a, 0, 0xAABB_CCDD);
        let r = unsafe { t.route(0x0300_000a, 0, 0xAABB_CCDD) };
        assert_eq!(r.source_ip, 0xAABB_CCDD);
    }

    #[test]
    fn off_link_uses_first_gateway_skipping_loopback() {
        let mut t = Tables::new();
        t.address[0] = 0x0100_007f;
        t.subnet[0] = 0x0000_00ff;
        t.gateway[0] = 0xDEAD_BEEF;
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        t.gateway[1] = 0x0100_000a;
        check(&t, 0x0100_0008, 0, 0);
        let r = unsafe { t.route(0x0100_0008, 0, 0) };
        assert_eq!(r.dest_ip, 0x0100_000a);
        assert_eq!(r.device_idx4, 4);
    }

    #[test]
    fn no_gateway_falls_back_to_loopback() {
        let mut t = Tables::new();
        t.address[0] = 0x0100_007f;
        t.subnet[0] = 0x0000_00ff;
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        check(&t, 0x0100_0008, 0, 0);
        let r = unsafe { t.route(0x0100_0008, 0, 0) };
        assert_eq!(r.device_idx4, 0);
        assert_eq!(r.dest_ip, 0x0100_0008);
        assert_eq!(r.source_ip, 0x0100_007f);
    }

    #[test]
    fn skip_on_link_when_addr_and_subnet_zero() {
        let mut t = Tables::new();
        t.address[0] = 0;
        t.subnet[0] = 0xffff_ffff;
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        check(&t, 0x0300_000a, 0, 0);
        let r = unsafe { t.route(0x0300_000a, 0, 0) };
        assert_eq!(r.device_idx4, 4);
    }

    #[test]
    fn broadcast_prefers_link_without_ip() {
        let mut t = Tables::new();
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        t.link_states[1] = 1;
        t.address[2] = 0;
        t.link_states[2] = 1;
        check(&t, IPV4_BROADCAST, 0, 0);
        let r = unsafe { t.route(IPV4_BROADCAST, 0, 0) };
        assert_eq!(r.device_idx4, 8);
        assert_eq!(r.dest_ip, IPV4_BROADCAST);
    }

    #[test]
    fn broadcast_falls_back_to_link_with_ip() {
        let mut t = Tables::new();
        t.address[1] = 0x0200_000a;
        t.link_states[1] = 1;
        t.address[2] = 0;
        t.link_states[2] = 0;
        check(&t, IPV4_BROADCAST, 0, 0);
        let r = unsafe { t.route(IPV4_BROADCAST, 0, 0) };
        assert_eq!(r.device_idx4, 4);
    }

    #[test]
    fn broadcast_no_link_falls_back_loopback() {
        let mut t = Tables::new();
        t.address[0] = 0x0100_007f;
        t.link_states[0] = 0xffff_ffff;
        t.link_states[1] = 0;
        check(&t, IPV4_BROADCAST, 0, 0);
        let r = unsafe { t.route(IPV4_BROADCAST, 0, 0) };
        assert_eq!(r.device_idx4, 0);
    }

    #[test]
    fn explicit_device_on_link() {
        let mut t = Tables::new();
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        t.gateway[1] = 0x0100_000a;
        let dev = t.list[1];
        check(&t, 0x0300_000a, dev, 0x1111_1111);
        let r = unsafe { t.route(0x0300_000a, dev, 0x1111_1111) };
        assert_eq!(r.device_idx4, 4);
        assert_eq!(r.dest_ip, 0x0300_000a);
        assert_eq!(r.source_ip, 0x0200_000a);
    }

    #[test]
    fn explicit_device_off_link_rewrites_gateway() {
        let mut t = Tables::new();
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        t.gateway[1] = 0x0100_000a;
        let dev = t.list[1];
        check(&t, 0x0100_0008, dev, 0);
        let r = unsafe { t.route(0x0100_0008, dev, 0) };
        assert_eq!(r.dest_ip, 0x0100_000a);
        assert_eq!(r.source_ip, 0x0200_000a);
    }

    #[test]
    fn explicit_device_broadcast_skips_gateway() {
        let mut t = Tables::new();
        t.address[1] = 0x0200_000a;
        t.subnet[1] = 0x0000_00ff;
        t.gateway[1] = 0x0100_000a;
        let dev = t.list[1];
        check(&t, IPV4_BROADCAST, dev, 0);
        let r = unsafe { t.route(IPV4_BROADCAST, dev, 0) };
        assert_eq!(r.dest_ip, IPV4_BROADCAST);
        assert_eq!(r.source_ip, 0x0200_000a);
    }

    #[test]
    fn explicit_device_unknown_fails() {
        let t = Tables::new();
        check(&t, 0x0100_0008, 0xDEAD_BEEF, 0xCAFE_BABE);
        let r = unsafe { t.route(0x0100_0008, 0xDEAD_BEEF, 0xCAFE_BABE) };
        assert_eq!(r.dest_ip, 0);
        assert_eq!(r.source_ip, 0xCAFE_BABE);
        assert_eq!(r.device_idx4, 0xffff_ffff);
    }

    #[test]
    fn null_list_slot_skipped_in_broadcast() {
        let mut t = Tables::new();
        t.list[1] = 0;
        t.address[2] = 0;
        t.link_states[2] = 1;
        check(&t, IPV4_BROADCAST, 0, 0);
        let r = unsafe { t.route(IPV4_BROADCAST, 0, 0) };
        assert_eq!(r.device_idx4, 8);
    }

    #[test]
    fn named_vector_matrix_matches_oracle() {
        let mut t = Tables::new();
        t.address[0] = 0x0100_007f;
        t.subnet[0] = 0x0000_00ff;
        t.address[1] = 0x0200_00c0;
        t.subnet[1] = 0x00ff_ffff;
        t.gateway[1] = 0x0100_00c0;
        t.link_states[1] = 1;
        t.address[2] = 0;
        t.link_states[2] = 1;
        let dests = [0u32, 1, IPV4_BROADCAST, 0x0300_00c0, 0x0100_0008, 0x7f00_0001];
        let sources = [0u32, 1, 0xAABB_CCDD, 0xffff_ffff];
        for &d in &dests {
            for &s in &sources {
                check(&t, d, 0, s);
                check(&t, d, t.list[1], s);
                check(&t, d, 0x1111_2222, s);
            }
        }
    }

    #[test]
    fn prng_corpus_matches_oracle() {
        let mut rng = IPV4_ROUTE_PRNG_SEED;
        let next = |r: &mut u32| {
            let mut x = *r;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *r = x;
            x
        };
        for _ in 0..50_000 {
            let mut t = Tables::new();
            for i in 0..NET_DEVICES_MAX {
                t.address[i] = next(&mut rng);
                t.subnet[i] = next(&mut rng);
                t.gateway[i] = next(&mut rng);
                t.link_states[i] = next(&mut rng);
                if next(&mut rng) & 0x7 == 0 {
                    t.list[i] = 0;
                }
            }
            let dest = next(&mut rng);
            let source = next(&mut rng);
            let device = match next(&mut rng) & 0x3 {
                0 => 0,
                1 => t.list[(next(&mut rng) as usize) % NET_DEVICES_MAX],
                _ => next(&mut rng) | 1,
            };
            check(&t, dest, device, source);
        }
    }

    #[test]
    fn link_state_offset_matches_net_device() {
        assert_eq!(OFF_LINK_STATE, 24);
        #[repr(C)]
        struct Dev {
            _pad: [u32; 6],
            link_state: u32,
        }
        assert_eq!(core::mem::offset_of!(Dev, link_state), OFF_LINK_STATE);
    }
}
