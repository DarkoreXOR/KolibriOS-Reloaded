# Cut AC Implementation — `ipv4_route`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ac-plan.md`](cut-ac-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ipv4_route` |
| Source | [`kernel/network/IPv4.inc`](../../kernel/network/IPv4.inc) |
| Callers | 4 live (`ipv4_output`, `ipv4_output_raw`, `udp.inc` connect, `tcp_usreq.inc` connect) |
| Rust symbol | `rust_ipv4_route` |
| Pure helper | `kolibri_utils::ipv4_route` |
| Subsystem | Network / IPv4 routing |

---

## Candidate comparison (post-AB audit)

| Candidate | Outcome |
|-----------|---------|
| `ipv4_route` | **Selected** — first IPv4 on-link/gateway/broadcast routing class (Stage-5 foothold) |
| `is_protective_mbr` | Deferred #2 — GPT protective-MBR ZF validate; pairs Z without cloning |
| `ntfs_test_bootsec` | Deferred #3 — FS bootsec+CF; mild validate-shape overlap with Z |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |
| `socket_check` / `get_pg_addr` / `net_ptr_to_num4` | Deferred — list membership / Stage-4 / thin scan |

---

## Legacy ABI

FASM leaf in `IPv4.inc` (retained under `USE_RUST_IPV4_ROUTE=0`):

```text
call / ret
in:  EAX = dest IP; EBX = device ptr or 0; EDX = source IP
out: EAX = dest IP (may become gateway) or 0 on fail
     EDX = source IP
     EDI = device number × 4 (or -1 on device-ptr fail)
destroyed: EBX, ECX
preserves: ESI, EBP (untouched)
flags: unspecified
```

Critical quirks retained:

* Auto-route (`EBX=0`): broadcast → on-link → gateway (skip loopback) → loopback
* Explicit device: inlines `net_ptr_to_num4`; overwrites source from `IPv4_address[edi]`
* Auto-route fills source from table only when incoming `EDX==0`
* On-link requires `(addr & subnet) ≠ 0`
* `ipv4_output_raw` caller quirk left untouched (not part of this leaf)

---

## Rust ABI

```text
stdcall rust_ipv4_route(
  dest, device, source,
  ipv4_address, ipv4_subnet, ipv4_gateway, net_device_list,
  source_out, device_idx_out
) -> EAX
ret 36
```

Trampoline injects `IPv4_*` / `net_device_list` bases and out-slots for EDX/EDI; preserves ESI/EBP.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `ipv4_route.rs` + `ffi.rs` section `.text.rust_ipv4_route` |
| Extract | `extract_reloc_free_text.py` → `rust_ipv4_route.bin` |
| Embed | `kernel/rust/ipv4_route.inc` `file` directive |
| Trampoline | `IPv4.inc` under `USE_RUST_IPV4_ROUTE` |
| Gate | `USE_RUST_IPV4_ROUTE` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ipv4_route` |
| Blob/object size | 2176 bytes |
| Relocations | 0 |
| SHA-256 | `723DBA72B08030A24493306F492C1CDB9875EC0D523E5C3A4125A6435A790B26` |

Note: size reflects LLVM loop unrolling over `NET_DEVICES_MAX=16`; still reloc-free with trailing `ret 36`.

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs Rust | **PASS** |
| Named vectors | on-link source fill; source preserve; gateway skip loopback; loopback fallback; `(addr&subnet)==0` skip; broadcast no-IP prefer; broadcast link-with-IP; broadcast loopback fallback; explicit on/off-link/broadcast; unknown device fail; null list slot |
| Boundary | dest/source 0 / max; broadcast `0xffffffff`; empty gateway table |
| PRNG | 50 000 vectors, seed `0x43555443` (`'CUTC'`) |
| Host tests | **300/300** cargo tests |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ipv4_route_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CAC` hang) |
| Vectors | Direct `rust_*` on-link/gateway/broadcast/explicit/fail + public trampoline with ESI/EBP canaries after planting live `IPv4_*` / `net_device_list` |
| Marker | `rust_ipv4_route_smoke_result = 'IPRT'` on success |

---

## QEMU validation

Kernels built with Cuts A–AB production gates intact (`USE_RUST_UTF8TO16=1`, etc.).

Images: fresh CoW from reference + `KERNEL.MNT` replace (Cut AB final image was not present on disk; live tree + AB gates are authoritative).

| Gate | Setting | Desktop | Network NIC |
|------|---------|---------|-------------|
| OFF | `USE_RUST_IPV4_ROUTE=0` | **OK** (QMP `running` + screendump `tmp_images/cut-ac-off.ppm`, 779380 non-black samples) | e1000 + user net present |
| ON | `USE_RUST_IPV4_ROUTE=1` | **OK** (QMP `running` + screendump `tmp_images/cut-ac-on.ppm`, 288000 non-black samples) | e1000 + user net present |

Smoke (ON): **PASS** (no `0xDEAD0CAC`; boot continued).

Real subsystem soak: **NOT AVAILABLE** — no active UDP/TCP connect stimulus was applied to prove live `ipv4_output` / connect callers beyond boot smoke. Boot smoke does exercise the public `ipv4_route` symbol on planted tables (ABI path), which is recorded under ABI smoke above.

Production image: `tmp_images/cut-ac-final.img`.

---

## Production gate

```text
USE_RUST_IPV4_ROUTE = 1
```

Rollback: `USE_RUST_IPV4_ROUTE = 0` (or `enabled = false` in `tools/build/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/ipv4_route.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-ipv4-route.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_ipv4_route.bin` (generated)
* `kernel/rust/ipv4_route.inc` (new)
* `kernel/network/IPv4.inc` (trampoline + gate)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `tools/build/config.toml`
* `tools/build/README.md`
* `tools/build/src/config.rs` / `main.rs` / `Cargo.toml` (A–AC comments)
* `docs/migration/cut-ac-plan.md`
* `docs/migration/cut-ac-implementation.md`
* `docs/migration/migration-plan.md`
* `tmp_images/README.md`

---

## Known limitations

* Blob is large (2176 B) due to unrolled 16-slot loops; functionally reloc-free.
* Host differential uses injectable link-state lookup for broadcast (avoids truncating 64-bit pointers); production path reads `NET_DEVICE.link_state` at offset 24 through real device pointers.
* Does not migrate `net_ptr_to_num4` as a separate cut (inlined into the device path only).
* Does not fix `ipv4_output_raw`’s undocumented register setup.
* Flags after return are unspecified.
* `memmove` / Stage-4 VA→PA / `is_protective_mbr` / `ntfs_test_bootsec` remain deferred.
* Live packet/connect soak not claimed without a connect stimulus.
