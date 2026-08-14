//! Cut CX: `ipv4_output` — IPv4 header composition + output orchestration.
//!
//! Path B leaf. Route / ARP / `eth_output` / `loop_output` stay FASM callbacks
//! injected via [`Ipv4OutputCtx`]. Does **not** call `rust_ipv4_route`,
//! `ipv4_fragment`, or Cut E/F checksum helpers.
//!
//! Public FASM ABI (live callers, not the inverted banner):
//! `AL`=TTL, `AH`=protocol, `EBX`=device/0, `ECX`=payload len,
//! `EDX`=source IP, `EDI`=dest IP. Success `EAX`=buffer ZF=0; fail `EAX`=0 ZF=1.
//! Loopback OOM: legacy still writes the header and returns `EAX` from
//! `loop_output` (often 0) without taking `.eth_error`-style failure unwind.

/// Cut CX smoke / PRNG marker (`'IPV4'`).
pub const IPV4_OUTPUT_PRNG_SEED: u32 = 0x4950_5634;

/// Injected trampoline context (14 dwords = 56 bytes).
pub const IPV4_OUTPUT_CTX_SIZE: usize = 56;

/// FASM `cmp ecx, 65500` / `ja .too_large`.
pub const IPV4_OUTPUT_MAX_PAYLOAD: u32 = 65500;

/// `sizeof.IPv4_header`.
pub const IPV4_HEADER_LEN: u32 = 20;

/// `ETHER_PROTO_IPv4` in AX (`stosw` → on-wire `08 00`).
pub const ETHER_PROTO_IPV4: u16 = 0x0008;

/// `AF_INET4` passed to `loop_output` in EDI.
pub const AF_INET4: u32 = 2;

/// Version 4, IHL 5.
pub const VERSION_IHL_V4_20: u8 = 0x45;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ipv4OutputCtx {
    /// `AX` at entry: `AL`=TTL, `AH`=protocol.
    pub ttl_proto: u16,
    pub _pad: u16,
    /// `EBX` device pointer or 0.
    pub device_in: u32,
    /// `ECX` payload length (not including IPv4 header).
    pub payload_len: u32,
    /// `EDX` requested source IP (0 ⇒ route fills).
    pub source_in: u32,
    /// `EDI` original destination IP (header dest; not next-hop).
    pub dest_in: u32,
    /// Public `ipv4_route` (Cut AC trampoline).
    pub route: u32,
    /// FASM `arp_ip_to_mac`.
    pub arp: u32,
    /// FASM `eth_output`.
    pub eth_out: u32,
    /// FASM `loop_output`.
    pub loop_out: u32,
    /// `IPv4_packets_tx` base.
    pub packets_tx: u32,
    /// `net_device_list` base.
    pub net_devices: u32,
    /// EBX out (device pointer).
    pub out_device: u32,
    /// EDX out (frame size, or loopback leftover source).
    pub frame_size: u32,
    /// EDI out (IPv4 payload start).
    pub payload_ptr: u32,
}

#[cfg(all(target_arch = "x86", target_os = "none"))]
const _: () = assert!(core::mem::size_of::<Ipv4OutputCtx>() == IPV4_OUTPUT_CTX_SIZE);

/// Host-side callback hooks (kernel uses ctx function pointers).
pub struct Ipv4OutputHooks {
    pub state: *mut u8,
    pub route: unsafe fn(*mut u8, u32, u32, u32) -> (u32, u32, u32),
    pub arp: unsafe fn(*mut u8, u32, u32) -> (u32, u32),
    pub eth: unsafe fn(*mut u8, u16, u32, u32, &[u8; 6]) -> EthOut,
    pub loop_out: unsafe fn(*mut u8, u32) -> LoopOut,
    /// Host-width counter base (avoids u32 pointer truncation).
    pub packets_tx: *mut u32,
    pub net_devices: *const u32,
}

#[derive(Clone, Copy)]
pub struct EthOut {
    pub buf: u32,
    pub device: u32,
    pub frame_size: u32,
    pub header: *mut u8,
    pub zf: bool,
}

#[derive(Clone, Copy)]
pub struct LoopOut {
    pub buf: u32,
    pub device: u32,
    pub header: *mut u8,
}

/// Independent RFC 1071 IPv4 header checksum (not FASM `ipv4_checksum`, not Cut E/F).
#[inline(always)]
pub fn ipv4_header_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < header.len() {
        let hi = if i == 10 { 0 } else { header[i] };
        let lo = if i == 10 { 0 } else { header[i + 1] };
        sum = sum.wrapping_add(((hi as u32) << 8) | (lo as u32));
        i += 2;
    }
    if i < header.len() {
        let b = if i == 10 { 0 } else { header[i] };
        sum = sum.wrapping_add((b as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let mut csum = (!sum) as u16;
    if csum == 0 {
        csum = 0xffff;
    }
    csum
}

/// Write the 20-byte IPv4 header `ipv4_output` produces after buffer alloc.
#[inline(always)]
pub fn write_ipv4_header(dst: &mut [u8], ttl: u8, proto: u8, src: u32, dest: u32, payload_len: u32) {
    let total = (IPV4_HEADER_LEN.wrapping_add(payload_len) as u16).to_be_bytes();
    dst[0] = VERSION_IHL_V4_20;
    dst[1] = 0;
    dst[2] = total[0];
    dst[3] = total[1];
    dst[4] = 0;
    dst[5] = 0;
    dst[6] = 0;
    dst[7] = 0;
    dst[8] = ttl;
    dst[9] = proto;
    dst[10] = 0;
    dst[11] = 0;
    dst[12..16].copy_from_slice(&src.to_le_bytes());
    dst[16..20].copy_from_slice(&dest.to_le_bytes());
    let csum = ipv4_header_checksum(&dst[..20]);
    let cb = csum.to_be_bytes();
    dst[10] = cb[0];
    dst[11] = cb[1];
}

#[inline(always)]
unsafe fn write_ipv4_header_ptr(hdr: *mut u8, ttl: u8, proto: u8, src: u32, dest: u32, payload_len: u32) {
    if hdr.is_null() {
        return;
    }
    // Reloc-free: no memcpy — write fields with unaligned stores.
    let total = IPV4_HEADER_LEN.wrapping_add(payload_len) as u16;
    unsafe {
        hdr.write(VERSION_IHL_V4_20);
        hdr.add(1).write(0);
        (hdr.add(2) as *mut u16).write_unaligned(total.to_be());
        (hdr.add(4) as *mut u16).write_unaligned(0);
        (hdr.add(6) as *mut u16).write_unaligned(0);
        hdr.add(8).write(ttl);
        hdr.add(9).write(proto);
        (hdr.add(10) as *mut u16).write_unaligned(0);
        (hdr.add(12) as *mut u32).write_unaligned(src);
        (hdr.add(16) as *mut u32).write_unaligned(dest);
        let mut sum: u32 = 0;
        let mut i = 0usize;
        while i < 20 {
            let hi = if i == 10 { 0 } else { hdr.add(i).read() };
            let lo = if i == 10 { 0 } else { hdr.add(i + 1).read() };
            sum = sum.wrapping_add(((hi as u32) << 8) | (lo as u32));
            i += 2;
        }
        while (sum >> 16) != 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        let mut csum = (!sum) as u16;
        if csum == 0 {
            csum = 0xffff;
        }
        (hdr.add(10) as *mut u16).write_unaligned(csum.to_be());
    }
}

#[inline(always)]
unsafe fn inc_packets_tx_raw(base: u32, dev_idx4: u32) {
    if base == 0 {
        return;
    }
    unsafe {
        let p = (base as usize).wrapping_add(dev_idx4 as usize) as *mut u32;
        p.write_unaligned(p.read_unaligned().wrapping_add(1));
    }
}

#[inline(always)]
unsafe fn load_device_raw(net_devices: u32, dev_idx4: u32) -> u32 {
    if net_devices == 0 {
        return 0;
    }
    unsafe {
        let p = (net_devices as usize).wrapping_add(dev_idx4 as usize) as *const u32;
        p.read_unaligned()
    }
}

#[inline(always)]
unsafe fn inc_packets_tx(base: u32, dev_idx4: u32, hooks: Option<&Ipv4OutputHooks>) {
    if let Some(h) = hooks {
        if !h.packets_tx.is_null() {
            unsafe {
                let p = h.packets_tx.byte_add(dev_idx4 as usize);
                p.write(p.read().wrapping_add(1));
            }
            return;
        }
    }
    unsafe { inc_packets_tx_raw(base, dev_idx4) }
}

#[inline(always)]
unsafe fn load_device(net_devices: u32, dev_idx4: u32, hooks: Option<&Ipv4OutputHooks>) -> u32 {
    if let Some(h) = hooks {
        if !h.net_devices.is_null() {
            unsafe {
                return h.net_devices.byte_add(dev_idx4 as usize).read();
            }
        }
    }
    unsafe { load_device_raw(net_devices, dev_idx4) }
}

#[inline(always)]
unsafe fn invoke_route(fn_ptr: u32, dest: u32, device: u32, source: u32) -> (u32, u32, u32) {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        let mut edx_r: u32;
        let mut edi_r: u32;
        unsafe {
            core::arch::asm!(
                "push esi",
                "push ebp",
                "call edi",
                "pop ebp",
                "pop esi",
                in("edi") fn_ptr,
                in("eax") dest,
                in("ebx") device,
                in("edx") source,
                lateout("eax") eax_r,
                lateout("edx") edx_r,
                lateout("ebx") _,
                lateout("ecx") _,
                lateout("edi") edi_r,
            );
        }
        (eax_r, edx_r, edi_r)
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, dest, device, source);
        (0, 0, 0)
    }
}

#[inline(always)]
unsafe fn invoke_arp(fn_ptr: u32, ip: u32, dev_idx4: u32) -> (u32, u32) {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        let mut ebx_r: u32;
        unsafe {
            core::arch::asm!(
                "push esi",
                "push ebp",
                "push edi",
                "call edx",
                "pop edi",
                "pop ebp",
                "pop esi",
                in("edx") fn_ptr,
                in("eax") ip,
                in("edi") dev_idx4,
                lateout("eax") eax_r,
                lateout("ebx") ebx_r,
                lateout("ecx") _,
                lateout("edx") _,
            );
        }
        (eax_r, ebx_r)
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, ip, dev_idx4);
        (0xffff_0000, 0)
    }
}

#[repr(C)]
struct EthAsmIn {
    fptr: u32,
    proto: u32,
    device: u32,
    total: u32,
    mac: u32,
    out: u32,
}

#[repr(C)]
struct EthAsmOut {
    eax: u32,
    ebx: u32,
    edx: u32,
    edi: u32,
    zf: u8,
    _pad: [u8; 3],
}

#[inline(always)]
unsafe fn invoke_eth(
    fn_ptr: u32,
    proto: u16,
    device: u32,
    total: u32,
    mac: &[u8; 6],
) -> (u32, u32, u32, *mut u8, bool) {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        // Pack args/results in memory so LLVM does not exhaust i686 GPRs
        // (REG-018: capture ZF immediately via `setz` into the out slot).
        let mut out = EthAsmOut {
            eax: 0,
            ebx: 0,
            edx: 0,
            edi: 0,
            zf: 0,
            _pad: [0; 3],
        };
        let inp = EthAsmIn {
            fptr: fn_ptr,
            proto: proto as u32,
            device,
            total,
            mac: mac.as_ptr() as u32,
            out: core::ptr::addr_of_mut!(out) as u32,
        };
        unsafe {
            core::arch::asm!(
                "push esi",
                "push ebp",
                "push ebx",
                "mov esi, {inp}",
                "push dword ptr [esi + 20]",
                "mov eax, [esi + 4]",
                "mov ebx, [esi + 8]",
                "mov ecx, [esi + 12]",
                "mov edx, [esi + 16]",
                "call dword ptr [esi]",
                "mov ebp, [esp]",
                "mov [ebp], eax",
                "mov [ebp + 4], ebx",
                "mov [ebp + 8], edx",
                "mov [ebp + 12], edi",
                "setz [ebp + 16]",
                "add esp, 4",
                "pop ebx",
                "pop ebp",
                "pop esi",
                inp = in(reg) core::ptr::addr_of!(inp) as u32,
                lateout("eax") _,
                lateout("ecx") _,
                lateout("edx") _,
                lateout("edi") _,
            );
        }
        (out.eax, out.ebx, out.edx, out.edi as *mut u8, out.zf != 0)
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, proto, device, total, mac);
        (0, 0, 0, core::ptr::null_mut(), true)
    }
}

#[inline(always)]
unsafe fn invoke_loop(fn_ptr: u32, total: u32) -> (u32, u32, *mut u8) {
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        let mut eax_r: u32;
        let mut ebx_r: u32;
        let mut edi_r: u32;
        unsafe {
            core::arch::asm!(
                "push esi",
                "push ebp",
                "call edx",
                "pop ebp",
                "pop esi",
                in("edx") fn_ptr,
                in("ecx") total,
                in("edi") AF_INET4,
                lateout("eax") eax_r,
                lateout("ebx") ebx_r,
                lateout("ecx") _,
                lateout("edx") _,
                lateout("edi") edi_r,
            );
        }
        (eax_r, ebx_r, edi_r as *mut u8)
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        let _ = (fn_ptr, total);
        (0, 0, core::ptr::null_mut())
    }
}

#[inline(always)]
fn fail(ctx: &mut Ipv4OutputCtx) -> u32 {
    ctx.out_device = 0;
    ctx.frame_size = 0;
    ctx.payload_ptr = 0;
    0
}

/// Production entry used by the stdcall trampoline.
#[inline(always)]
pub unsafe fn ipv4_output_ptr(ctx: *mut Ipv4OutputCtx) -> u32 {
    // Freestanding blob path: no host hooks (keeps `.text.rust_ipv4_output` lean).
    #[cfg(all(target_arch = "x86", target_os = "none"))]
    {
        unsafe { ipv4_output_kernel(ctx) }
    }
    #[cfg(not(all(target_arch = "x86", target_os = "none")))]
    {
        unsafe { ipv4_output_with_hooks(ctx, None) }
    }
}

/// Kernel-only leaf (callbacks via ctx function pointers).
#[cfg(all(target_arch = "x86", target_os = "none"))]
#[inline(always)]
unsafe fn ipv4_output_kernel(ctx: *mut Ipv4OutputCtx) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    let ctx = unsafe { &mut *ctx };
    let ttl = ctx.ttl_proto as u8;
    let proto = (ctx.ttl_proto >> 8) as u8;
    let payload_len = ctx.payload_len;
    let orig_dest = ctx.dest_in;

    if payload_len > IPV4_OUTPUT_MAX_PAYLOAD {
        return fail(ctx);
    }

    let (next_hop, routed_src, dev_idx4) =
        unsafe { invoke_route(ctx.route, orig_dest, ctx.device_in, ctx.source_in) };
    if next_hop == 0 {
        return fail(ctx);
    }

    let total = payload_len.wrapping_add(IPV4_HEADER_LEN);

    if dev_idx4 == 0 {
        unsafe { inc_packets_tx_raw(ctx.packets_tx, 0) };
        let header_src = next_hop;
        let (buf, device, header) = unsafe { invoke_loop(ctx.loop_out, total) };
        unsafe {
            write_ipv4_header_ptr(header, ttl, proto, header_src, orig_dest, payload_len);
        }
        ctx.out_device = device;
        ctx.frame_size = routed_src;
        ctx.payload_ptr = (header as u32).wrapping_add(IPV4_HEADER_LEN);
        return buf;
    }

    let (arp_eax, arp_ebx) = unsafe { invoke_arp(ctx.arp, next_hop, dev_idx4) };
    if (arp_eax & 0xffff_0000) != 0 {
        return fail(ctx);
    }

    unsafe { inc_packets_tx_raw(ctx.packets_tx, dev_idx4) };

    let mut mac = [0u8; 6];
    mac[0] = arp_eax as u8;
    mac[1] = (arp_eax >> 8) as u8;
    let b = arp_ebx.to_le_bytes();
    mac[2] = b[0];
    mac[3] = b[1];
    mac[4] = b[2];
    mac[5] = b[3];

    let device = unsafe { load_device_raw(ctx.net_devices, dev_idx4) };
    let (buf, device_out, frame, header, zf) =
        unsafe { invoke_eth(ctx.eth_out, ETHER_PROTO_IPV4, device, total, &mac) };
    if zf {
        return fail(ctx);
    }

    unsafe {
        write_ipv4_header_ptr(header, ttl, proto, routed_src, orig_dest, payload_len);
    }
    ctx.out_device = device_out;
    ctx.frame_size = frame;
    ctx.payload_ptr = (header as u32).wrapping_add(IPV4_HEADER_LEN);
    buf
}

#[inline(always)]
pub unsafe fn ipv4_output_with_hooks(
    ctx: *mut Ipv4OutputCtx,
    hooks: Option<&Ipv4OutputHooks>,
) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    let ctx = unsafe { &mut *ctx };
    let ttl = ctx.ttl_proto as u8;
    let proto = (ctx.ttl_proto >> 8) as u8;
    let payload_len = ctx.payload_len;
    let orig_dest = ctx.dest_in;

    if payload_len > IPV4_OUTPUT_MAX_PAYLOAD {
        return fail(ctx);
    }

    let (next_hop, routed_src, dev_idx4) = if let Some(h) = hooks {
        unsafe { (h.route)(h.state, orig_dest, ctx.device_in, ctx.source_in) }
    } else {
        unsafe { invoke_route(ctx.route, orig_dest, ctx.device_in, ctx.source_in) }
    };
    if next_hop == 0 {
        return fail(ctx);
    }

    let total = payload_len.wrapping_add(IPV4_HEADER_LEN);

    if dev_idx4 == 0 {
        unsafe { inc_packets_tx(ctx.packets_tx, 0, hooks) };
        let header_src = next_hop;
        let (buf, device, header) = if let Some(h) = hooks {
            let o = unsafe { (h.loop_out)(h.state, total) };
            (o.buf, o.device, o.header)
        } else {
            unsafe { invoke_loop(ctx.loop_out, total) }
        };
        unsafe {
            write_ipv4_header_ptr(header, ttl, proto, header_src, orig_dest, payload_len);
        }
        ctx.out_device = device;
        ctx.frame_size = routed_src;
        ctx.payload_ptr = (header as u32).wrapping_add(IPV4_HEADER_LEN);
        return buf;
    }

    let (arp_eax, arp_ebx) = if let Some(h) = hooks {
        unsafe { (h.arp)(h.state, next_hop, dev_idx4) }
    } else {
        unsafe { invoke_arp(ctx.arp, next_hop, dev_idx4) }
    };
    if (arp_eax & 0xffff_0000) != 0 {
        return fail(ctx);
    }

    unsafe { inc_packets_tx(ctx.packets_tx, dev_idx4, hooks) };

    let mut mac = [0u8; 6];
    mac[0] = arp_eax as u8;
    mac[1] = (arp_eax >> 8) as u8;
    let b = arp_ebx.to_le_bytes();
    mac[2] = b[0];
    mac[3] = b[1];
    mac[4] = b[2];
    mac[5] = b[3];

    let device = unsafe { load_device(ctx.net_devices, dev_idx4, hooks) };
    let (buf, device_out, frame, header, zf) = if let Some(h) = hooks {
        let o = unsafe { (h.eth)(h.state, ETHER_PROTO_IPV4, device, total, &mac) };
        (o.buf, o.device, o.frame_size, o.header, o.zf)
    } else {
        unsafe { invoke_eth(ctx.eth_out, ETHER_PROTO_IPV4, device, total, &mac) }
    };
    if zf {
        return fail(ctx);
    }

    unsafe {
        write_ipv4_header_ptr(header, ttl, proto, routed_src, orig_dest, payload_len);
    }
    ctx.out_device = device_out;
    ctx.frame_size = frame;
    ctx.payload_ptr = (header as u32).wrapping_add(IPV4_HEADER_LEN);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipv4_output_oracle as oracle;

    #[test]
    fn ctx_layout_i686() {
        assert_eq!(core::mem::size_of::<Ipv4OutputCtx>(), 56);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, ttl_proto), 0);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, device_in), 4);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, payload_len), 8);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, source_in), 12);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, dest_in), 16);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, route), 20);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, arp), 24);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, eth_out), 28);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, loop_out), 32);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, packets_tx), 36);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, net_devices), 40);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, out_device), 44);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, frame_size), 48);
        assert_eq!(core::mem::offset_of!(Ipv4OutputCtx, payload_ptr), 52);
    }

    #[test]
    fn checksum_matches_oracle_and_live_udp() {
        let mut h = [0u8; 20];
        write_ipv4_header(&mut h, 128, 17, 0x0f02_000a, 0x0202_000a, 18);
        let o = oracle::build_ipv4_header(128, 17, 0x0f02_000a, 0x0202_000a, 18);
        assert_eq!(h, o);
        assert_eq!(&h[10..12], &0x22b7u16.to_be_bytes());
        assert_ne!(&h[10..12], &[0, 0]);
    }

    #[test]
    fn checksum_never_zero_ttl_sweep() {
        for ttl in 0..=255u8 {
            let mut h = [0u8; 20];
            write_ipv4_header(&mut h, ttl, 1, 0, 0, 0);
            assert_ne!([h[10], h[11]], [0, 0], "ttl={ttl}");
            assert_eq!(h, oracle::build_ipv4_header(ttl, 1, 0, 0, 0));
        }
    }

    #[test]
    fn checksum_payload_len_edges() {
        let mut h = [0u8; 20];
        write_ipv4_header(&mut h, 1, 0, 0, 0, 0);
        assert_eq!(u16::from_be_bytes([h[2], h[3]]), 20);
        write_ipv4_header(&mut h, 64, 17, 0x0100007f, 0x0100007f, IPV4_OUTPUT_MAX_PAYLOAD);
        assert_eq!(
            u16::from_be_bytes([h[2], h[3]]),
            (20 + IPV4_OUTPUT_MAX_PAYLOAD) as u16
        );
    }

    struct State {
        tx: [u32; 16],
        devices: [u32; 16],
        buf: [u8; 64],
        route_ok: bool,
        arp_ok: bool,
        eth_zf: bool,
        loopback: bool,
        loop_oom: bool,
        last_mac: [u8; 6],
        last_eth_total: u32,
        last_loop_total: u32,
        routed_src: u32,
        next_hop: u32,
    }

    unsafe fn hook_route(st: *mut u8, _dest: u32, _dev: u32, source: u32) -> (u32, u32, u32) {
        let s = unsafe { &mut *(st as *mut State) };
        if !s.route_ok {
            return (0, 0, 0);
        }
        let src_out = if source == 0 { s.routed_src } else { source };
        let idx = if s.loopback { 0 } else { 4 };
        (s.next_hop, src_out, idx)
    }

    unsafe fn hook_arp(st: *mut u8, _ip: u32, _idx: u32) -> (u32, u32) {
        let s = unsafe { &*(st as *mut State) };
        if !s.arp_ok {
            return (0xffff_ffff, 0);
        }
        (0x0000_5452, 0x5634_1200)
    }

    unsafe fn hook_eth(st: *mut u8, _p: u16, dev: u32, total: u32, mac: &[u8; 6]) -> EthOut {
        let s = unsafe { &mut *(st as *mut State) };
        s.last_mac = *mac;
        s.last_eth_total = total;
        EthOut {
            buf: 0xB100_0001,
            device: dev,
            frame_size: 60,
            header: s.buf.as_mut_ptr(),
            zf: s.eth_zf,
        }
    }

    unsafe fn hook_loop(st: *mut u8, total: u32) -> LoopOut {
        let s = unsafe { &mut *(st as *mut State) };
        s.last_loop_total = total;
        LoopOut {
            buf: if s.loop_oom { 0 } else { 0xB100_0010 },
            device: 0xB10C_B10C,
            header: s.buf.as_mut_ptr(),
        }
    }

    fn run(state: &mut State, ttl: u8, proto: u8, plen: u32, src: u32, dst: u32, device: u32) -> u32 {
        state.devices[1] = 0x0DEC_0DE0;
        let hooks = Ipv4OutputHooks {
            state: state as *mut State as *mut u8,
            route: hook_route,
            arp: hook_arp,
            eth: hook_eth,
            loop_out: hook_loop,
            packets_tx: state.tx.as_mut_ptr(),
            net_devices: state.devices.as_ptr(),
        };
        let mut ctx = Ipv4OutputCtx {
            ttl_proto: u16::from(ttl) | (u16::from(proto) << 8),
            _pad: 0,
            device_in: device,
            payload_len: plen,
            source_in: src,
            dest_in: dst,
            route: 1,
            arp: 1,
            eth_out: 1,
            loop_out: 1,
            packets_tx: 0,
            net_devices: 0,
            out_device: 0,
            frame_size: 0,
            payload_ptr: 0,
        };
        unsafe { ipv4_output_with_hooks(&mut ctx, Some(&hooks)) }
    }

    fn fresh() -> State {
        State {
            tx: [0; 16],
            devices: [0; 16],
            buf: [0; 64],
            route_ok: true,
            arp_ok: true,
            eth_zf: false,
            loopback: false,
            loop_oom: false,
            last_mac: [0; 6],
            last_eth_total: 0,
            last_loop_total: 0,
            routed_src: 0x0f02_000a,
            next_hop: 0x0202_000a,
        }
    }

    #[test]
    fn too_large_no_callbacks_no_inc() {
        let mut s = fresh();
        let eax = run(&mut s, 128, 17, 65501, 0, 0x08080808, 0);
        assert_eq!(eax, 0);
        assert_eq!(s.tx, [0; 16]);
    }

    #[test]
    fn no_route_no_inc() {
        let mut s = fresh();
        s.route_ok = false;
        let eax = run(&mut s, 128, 17, 8, 0, 0x08080808, 0);
        assert_eq!(eax, 0);
        assert_eq!(s.tx[1], 0);
    }

    #[test]
    fn arp_fail_no_inc() {
        let mut s = fresh();
        s.arp_ok = false;
        let eax = run(&mut s, 128, 17, 8, 0, 0x08080808, 0);
        assert_eq!(eax, 0);
        assert_eq!(s.tx[1], 0);
    }

    #[test]
    fn eth_zf_increments() {
        let mut s = fresh();
        s.eth_zf = true;
        let eax = run(&mut s, 128, 17, 8, 0, 0x08080808, 0);
        assert_eq!(eax, 0);
        assert_eq!(s.tx[1], 1);
    }

    #[test]
    fn success_header_dest_is_original_not_nexthop() {
        let mut s = fresh();
        s.next_hop = 0x0100000a;
        let dst = 0x08080808;
        let eax = run(&mut s, 128, 17, 18, 0, dst, 0);
        assert_ne!(eax, 0);
        assert_eq!(s.tx[1], 1);
        assert_eq!(s.last_eth_total, 38);
        let h = &s.buf[..20];
        assert_eq!(h[0], 0x45);
        assert_eq!(h[8], 128);
        assert_eq!(h[9], 17);
        assert_eq!(&h[12..16], &s.routed_src.to_le_bytes());
        assert_eq!(&h[16..20], &dst.to_le_bytes());
        assert_ne!(&h[16..20], &s.next_hop.to_le_bytes());
        let o = oracle::build_ipv4_header(128, 17, s.routed_src, dst, 18);
        assert_eq!(h, &o);
    }

    #[test]
    fn loopback_incs_slot0_source_is_nexthop() {
        let mut s = fresh();
        s.loopback = true;
        s.next_hop = 0x0100007f;
        let eax = run(&mut s, 64, 1, 0, 0, 0x0100007f, 0);
        assert_ne!(eax, 0);
        assert_eq!(s.tx[0], 1);
        assert_eq!(s.tx[1], 0);
        assert_eq!(s.last_loop_total, 20);
        let h = &s.buf[..20];
        assert_eq!(&h[12..16], &s.next_hop.to_le_bytes());
        assert_eq!(&h[16..20], &0x0100007fu32.to_le_bytes());
    }

    #[test]
    fn loopback_oom_continues_and_increments() {
        let mut s = fresh();
        s.loopback = true;
        s.loop_oom = true;
        s.next_hop = 0x0100007f;
        let eax = run(&mut s, 64, 1, 0, 0, 0x0100007f, 0);
        assert_eq!(eax, 0);
        assert_eq!(s.tx[0], 1);
        assert_eq!(s.buf[0], 0x45);
        assert_eq!(&s.buf[12..16], &s.next_hop.to_le_bytes());
    }

    #[test]
    fn al_ah_ttl_not_protocol() {
        let mut s = fresh();
        let _ = run(&mut s, 7, 17, 8, 0, 0x08080808, 0);
        assert_eq!(s.buf[8], 7);
        assert_eq!(s.buf[9], 17);
    }

    fn xor32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn prng_50k_matches_oracle_headers() {
        let mut state = IPV4_OUTPUT_PRNG_SEED;
        let mut next = || xor32(&mut state);
        for _ in 0..50_000u32 {
            let ttl = (next() & 0xff) as u8;
            let proto = (next() & 0xff) as u8;
            let plen = next() % 64;
            let src = next();
            let dst = next() | 1;
            if next() % 17 == 0 {
                let mut s = fresh();
                let eax = run(&mut s, ttl, proto, IPV4_OUTPUT_MAX_PAYLOAD + 1, src, dst, 0);
                assert_eq!(eax, 0);
                continue;
            }
            let mut s = fresh();
            s.routed_src = src;
            s.next_hop = dst;
            match next() % 20 {
                0 => s.route_ok = false,
                1 => s.arp_ok = false,
                2 => s.eth_zf = true,
                _ => {}
            }
            let eax = run(&mut s, ttl, proto, plen, src, dst, 0);
            if !s.route_ok || !s.arp_ok {
                assert_eq!(eax, 0);
                assert_eq!(s.tx[1], 0);
                continue;
            }
            if s.eth_zf {
                assert_eq!(eax, 0);
                assert_eq!(s.tx[1], 1);
                continue;
            }
            assert_ne!(eax, 0);
            let o = oracle::build_ipv4_header(ttl, proto, src, dst, plen);
            assert_eq!(&s.buf[..20], &o);
        }
    }
}
