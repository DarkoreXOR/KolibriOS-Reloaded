# Cut AY Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-ay-implementation.md`](cut-ay-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AY** migrates NIC device-list pointer → index×4 —
> `net_ptr_to_num4` in `kernel/network/stack.inc`.  
> Cuts A–AX remain complete and must not be redone. Do not start Cut AZ.

---

## Post-AX migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EAX+EBX+ECX+EDX+ESI+EBP**; **EDI** is the result (do not restore); TCP send keeps EAX→socket across the call |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not an FS cut |
| Cut D / AS / AU | Network stdcall | Inject `net_device_list` + `NET_DEVICES_MAX`; do **not** claim network Path A |
| Cut AX | NTFS MCB encode | Complete; do not extend to NTFS write ownership |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** — complementary leaves; FASM owns mount/AG/inode/dir/I/O |
| I + AX NTFS MCB Path A? | **No** — encode/decode ≠ FRS/bitmap/space ownership |
| AV + AHCI wait/IRQ Path A? | **No** — controller/DMA/IRQ stay FASM |
| AC/M/V/AS/AU + this leaf Path A? | **No** — device-index leaf ≠ stack/mutex/alloc ownership |
| Y + AT + `rebase_coff` / `get_proc_ex` Path A? | **No** — loader orchestration stays FASM |
| AQ / X+AR Path A? | **No** — unchanged rejects |
| Strongest remaining leaf? | **Yes** — `net_ptr_to_num4` (device-list ptr→index×4) |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| XFS Path A / next XFS leaf | Fatigue after AW; leaves ≠ ownership |
| NTFS MCB beyond AX | Ownership incomplete |
| `exFAT_get_sector` / `getInodeLocation` / `fat_get_sector` | AW address-math sibling ban; EXT no `--disk ext` |
| `rebase_coff` | Y mutate anti-cluster |
| `get_proc_ex` | PE ban after AT unless stronger; strncmp-loop novelty thin |
| `bdfe_to_fat_time` / `fat_date_to_bdfe` | Easy calendar / AO pair ban |
| `usb_td_to_virt` | Calls `get_pg_addr` in loop — awkward reloc-free; weak USB soak |
| AS + `socket_check_port` / `socket_num_to_ptr` | Mutex + AS anti-cluster |
| AHCI wait / endian / sig | Impure / trivial |
| `strnlen` / `coff_get_align` / `iso9660_copy_name` | Thin / glue |
| Ban-list FS/Unicode / H+blit / thin sysfn | Unchanged rejects |

### Ranked top candidates (post-AX)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `net_ptr_to_num4` | Network / NIC list | ~12 | **Device ptr→index×4** | Excellent | Med (packet-hot) | desktop net | **SELECT** |
| `get_proc_ex` | PE / DLL | 1 | Export name→VA | Excellent | Low | `.sys` load | #2 (PE ban stretch) |
| `bdfe_to_fat_time` | FAT/exFAT | ~5 | AO pack inverse | Excellent | Low | `--disk exfat` | Reject (calendar/AO) |
| `usb_td_to_virt` | USB | HC table | TD phys→virt | Good | Med | Weak USB | Defer (AQ compose) |
| `rebase_coff` | PE / DLL | 1 | DIR32 rebase | Good | Med | Rare | Defer (Y anti-cluster) |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: net_ptr_to_num4
Source: kernel/network/stack.inc
Subsystem: Network NIC device-list index resolve
Stage: Stage-5 foothold (device-index leaf; stack ownership stays FASM)
Why selected:
    Post-AX audit: Path A rejected everywhere. Address-math / XFS / PE /
    calendar / USB leftovers lose to ban-list or weak soak. Strongest remaining
    leaf is net_ptr_to_num4: new semantic class vs AS (socket ZF) and AU
    (fragment slot) — linear scan of net_device_list returning index×4 or −1,
    with ~12 production callers and reloc-free inject of list base + max.
Why this is a genuine migration boundary:
    Deterministic null-reject + fixed-width pointer table walk; EDI = byte
    offset into net_device_list (or −1). Distinct from socket membership and
    fragment keyed scan without claiming stack ownership.
Why Path A / Path B:
    Path B — one device-index leaf. Device register/remove, mutex, protocol
    walks, and IPv4 tables remain FASM.
Regression risks:
    REG-001: EAX live across TCP send (socket*); EDX live across ipv4/arp
    input (packet header); ECX legacy-preserved; EBX = device in/out;
    EDI is OUTPUT (callers that need prior EDI push/pop themselves).
CPU/interrupt-state risks:
    None in leaf — no cli/sti; no locks; pure table walk.
Shared-state risks:
    Read-only scan of net_device_list; list ownership stays FASM.
Concurrency/locking risks:
    None in leaf (callers assume stable device ptr for the scan window).
Required differential tests:
    Independent FASM-flow oracle; null; empty; hit slot 0/mid/last; hole
    (null slots); miss; full 16; 50k PRNG seed 0x43555459 ('CUTY').
Required ABI tests:
    Marker NPT4; synthetic list + live net_device_list; EAX/EBX/ECX/EDX/ESI/EBP
    canaries; EDI = index×4 / −1.
Required A/B tests:
    Gate OFF vs ON desktop; optional network path if stock image exercises it.
Required real subsystem validation:
    Packet-hot path reaches net_ptr_to_num4 on any NIC I/O; stock desktop may
    only lightly exercise net — report PARTIAL/NOT AVAILABLE honestly.
Rejected alternatives:
    get_proc_ex; bdfe_to_fat_time; usb_td_to_virt; rebase_coff;
    XFS/AHCI/NTFS/PE Path A; AO/AN/address-math ban-list.
Expected legacy ABI:
    register call; in EBX=device ptr; out EDI=index×4 or −1;
    preserves EAX/EBX/ECX/EDX/ESI/EBP; clobbers flags; ret 0.
Expected Rust ABI:
    stdcall rust_net_ptr_to_num4(device, list_base, max) → EAX=index×4/−1;
    ret 12; trampoline injects net_device_list + NET_DEVICES_MAX;
    mov edi,eax; restores EAX/EBX/ECX/EDX/ESI/EBP.
Differential-testing strategy:
    Independent oracle mirroring FASM null/scan/sub; 50k PRNG 'CUTY'.
ABI-risk assessment:
    High — packet-hot + REG-001 EAX on TCP send; trampoline must restore EAX.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with list inject and **EAX+EBX+ECX+EDX+ESI+EBP**
preserve; `USE_RUST_NET_PTR_TO_NUM4` rollback.

---

## Out of scope

* Claiming Path A for networking / sockets / IPv4
* Migrating `net_ptr_to_num` wrapper / `socket_num_to_ptr` / `get_proc_ex`
* Migrating `usb_td_to_virt` / `rebase_coff` / `bdfe_to_fat_time`
* Beginning Cut AZ
