# ABI Completeness Audit (Adversarial)

This document **audits** prior reverse-engineering docs against live sources.
It is not a rewrite of those docs; it records confirmations, contradictions,
extensions, and newly found consumers.

Evidence policy: [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Methodology

For each claimed HARD ABI surface:

| Outcome | Meaning |
|---------|---------|
| CONFIRMED | Source matches prior claim |
| CONTRADICTED | Source falsifies or narrows prior claim |
| EXTENDED | Additional requirements beyond prior docs |
| UNDOCUMENTED_CONSUMER | Extra path that observes the surface |

---

## Critical corrections to prior suite

### C1. Application direct access to `SLOT_BASE` / `window_data` — **CONTRADICTED (for ring3)**

**Prior claim:** treat slot/window fixed VAs as “INTERNAL used externally / shim until audit” for applications.

**LOCAL FACT:** Kernel PDE flags for the high half are built with `PG_SWR` (**no `PG_USER`**) in upstream `init_mem` (`docs/_upstream/init.inc:136–174`). `create_process` copies those PDEs verbatim (`taskman.inc:337–339`).

**Consequence:** Ring-3 applications **cannot** dereference `0x80090000` / `0x80001000` via flat `DS`/`ES` without `#PF`.

**Still HARD for apps:** slot **numbers**, TID/PID semantics, and syscall 9 buffer — not the raw `APPDATA` VA.

**Still accidental for ring-0 drivers:** drivers may still read kernel globals (no paging barrier).

**Migration:** Prefer **not** freezing `SLOT_BASE` VA for apps. Keep syscall 9 exact. Optional ring-0 driver audit separately.

### C2. Direct framebuffer via `GS` — **EXTENDED (under-documented HARD)**

**LOCAL FACT:** `sysfuncs.txt:1759–1761`, `:2729–2757` document **direct graphics access through selector `gs`** without syscalls; function 61 returns mode parameters.

**LOCAL FACT:** LFB PDE patch uses `PG_UWR` (`video/framebuffer.inc` / `set_framebuffer` path with user-writable mapping). GDT `graph_data_l` is a CPL3 data segment (`data32.inc:318–324`).

**Classification:** **HARD ABI** for applications.

**Migration:** Must preserve GS→LFB (or equivalent documented direct access) and fn 61.

### C3. `f68.31` copies `SRV` fields into user memory — **EXTENDED accidental ABI**

**LOCAL FACT:** `memory.inc` ~1280–1301 copies name, list links, `base`, `entry`, `srv_proc` into a user buffer.

**Classification:** **LEGACY/ACCIDENTAL ABI** (apps can observe kernel pointers).

**Migration:** Preserve copy layout or break known callers; treat as HARD until app audit.

### C4. Driver version gate — **EXTENDED**

**LOCAL FACT:** `dll.inc:9–12` — `DRV_COMPAT=5`, `DRV_CURRENT=6`, `DRV_VERSION` packed.

**Migration:** PE `START` / version negotiation must keep accepting current drivers.

### C5. `DISKFUNC.strucsize` versioning — **CONFIRMED + EXTENDED**

**LOCAL FACT:** `disk.inc:36–41` documents size field for forward-compatible callback tables.

**Migration:** Accept older smaller `strucsize`; do not require full modern table.

### C6. `SRV.magic` value — **EXTENDED (comment vs code)**

**LOCAL FACT:** Code compares `' SRV'` (leading space) in `dll.inc:52`, `92`. Comment in `const.inc` says `'SRV '` — **trust code**.

### C7. Syscall eflags “always preserved” — **CONTRADICTED in part**

**LOCAL FACT:** `sysfuncs.txt:12–13` claim all regs including eflags preserved.
**LOCAL FACT:** `sys_apm` writes CF into stacked eflags (`kernel.asm` APM path); fn 49 docs require CF.
**LOCAL FACT:** SYSENTER path does not restore user eflags via `iretd`.

### C8. Syscall entry only `int 0x40` — **CONTRADICTED as complete description**

**LOCAL FACT:** `sysenter_entry` and `syscall_entry` also dispatch (`syscall.inc`). Undocumented in `sysfuncs.txt` but live.

### C9. Function 58 — **DOC vs CODE conflict**

**LOCAL FACT:** `sysfuncs.txt` still references fn 58 LBA API; `servetable2[58]=undefined_syscall`.

**Classification:** Documented-but-dead; Rust should keep returning −1.

### C10. App header `MENUET01`/`MENUET02` — **EXTENDED**

**LOCAL FACT:** Banner check and version=2 DLL autoload in `taskman.inc` — not fully captured in prior `application-abi.md`.

---

## Compatibility surface register

| Interface | Consumer | Source | Evidence | Prior docs | Class | Confidence | Migration consequence |
|-----------|----------|--------|----------|------------|-------|------------|----------------------|
| `int 0x40` + EAX | apps | `syscall.inc:i40`, `sysfuncs.txt:10–13` | CONFIRMED | syscall-abi | HARD | High | Keep forever |
| SYSENTER entry | apps (stubs) | `syscall.inc:16–42`, `kernel.asm` MSRs | UNDOCUMENTED_CONSUMER | noted weak | HARD (undocumented) | Med | Keep asm stub + EBP stack convention |
| AMD SYSCALL entry | apps | `syscall.inc:68–91` | UNDOCUMENTED_CONSUMER | noted | HARD (undocumented) | Med | Keep |
| `servetable2` numbers | apps | `syscall.inc:98–183` | CONFIRMED | syscalls.yaml | HARD | High | Exact numbers |
| Undefined → EAX=−1 | apps | `kernel.asm:undefined_syscall` | CONFIRMED | yes | HARD | High | |
| Fn9 `process_information` 0x4C | apps | `const.inc`, `kernel.asm` validate sizeof | CONFIRMED; docs recommend 1KB | yes | HARD (0x4C layout) | High | Exact layout; padding beyond may grow later |
| Slot index semantics | apps | `sysfuncs.txt:277–376` | CONFIRMED | yes | HARD | High | Slots from 1; IDLE=1; OS=2 |
| TID monotonic / unique | apps | `sysfuncs.txt:362–367` | CONFIRMED as documented contract | weak | HARD/BEHAVIORAL | Med | Do not reuse TIDs |
| GS LFB access + fn61 | apps | `sysfuncs.txt:2729+`, GDT graph | EXTENDED | under-documented | HARD | High | Preserve GS graphics model |
| Event bit masks | apps | `const.inc:EVENT_*`, fn10/11/40 | CONFIRMED | yes | HARD | High | |
| Dual event systems (bits vs EVENT objects) | apps/drivers | `gui/event.inc`, fn68.14 | CONFIRMED | yes | HARD | High | Both façades |
| IPC buffer in user VA | apps | fn60, `APPDATA.ipc_*` | CONFIRMED | yes | HARD | High | |
| Named shmem 68.22/23 | apps | `heap.inc` SMEM | CONFIRMED | weak | HARD | High | |
| User heap 68.11–13,20 | apps | `memory.inc` f68 | CONFIRMED | yes | HARD | High | |
| Driver load 68.16/21 | apps | `dll.inc`, `peload.inc` | CONFIRMED | yes | HARD | High | `/sys/drivers/NAME.sys` |
| IOCTL 68.17 struct | apps | `IOCTL` 6 dwords | CONFIRMED | yes | HARD | High | stdcall to `srv_proc` |
| SRV handle = kernel ptr | apps | get_service returns eax | EXTENDED | opaque handle | HARD (as opaque) | High | Must round-trip; peeking blocked by paging |
| f68.31 SRV dump | apps | `memory.inc` copy | EXTENDED | missing | LEGACY/ACCIDENTAL | High | Preserve or audit callers |
| MENUET01/02 header | apps | `taskman.inc:211–219` | EXTENDED | partial | HARD | High | Exact banner/fields |
| version=2 DLL autoload | apps | `taskman.inc:877+` | EXTENDED | missing | HARD | Med | |
| User `< OS_BASE` | apps | paging, `is_region_userspace` | CONFIRMED | yes | HARD | High | Many checks use `OS_BASE` |
| Kernel PDEs without U | apps | upstream init_mem PG_SWR | CONTRADICTS prior “apps poke slots” | fixed-addresses | INTERNAL (apps) | High | Do not require slot VA for apps |
| `SLOT_BASE` layout | kernel/drivers | `APPDATA` 256 | CONFIRMED size | data-structures | INTERNAL / ACCIDENTAL (ring0) | High | Free to move if drivers audited |
| `window_data` | kernel | WDATA 128 | CONFIRMED | yes | INTERNAL (apps) | High | Same |
| KEY/BTN buffers | kernel/legacy | fixed VA | CONFIRMED exist | yes | UNKNOWN if apps map | Low | Syscalls mediate; raw VA likely INTERNAL |
| Boot `0x10000` + header | bootloader | `bootbios.inc` | CONFIRMED | yes | HARD | High | |
| `boot_data` @ `0x9000` | bootloader | `const.inc` | CONFIRMED | yes | HARD | High | |
| `AX='KL'` handshake | bootloader | `loader_doc.txt` | CONFIRMED | yes | HARD | High | |
| KERNEL PE exports | drivers | `exports.inc` + `export.inc` OS_BASE-relative | CONFIRMED | yes | HARD | High | Names + stdcall |
| `LFBAddress` last export cell | drivers | `exports.inc:156–160` | CONFIRMED | yes | HARD | High | Must remain last; writable cell |
| `RegService`/`GetService` | drivers/apps | `dll.inc` | CONFIRMED | yes | HARD | High | |
| `DiskAdd`+`DISKFUNC` | drivers | `disk.inc`, `drivers_api.txt` | CONFIRMED | yes | HARD | High | Optional NULLs; strucsize |
| `AttachIntHandler` | drivers | `irq.inc:66` | CONFIRMED | yes | HARD | High | stdcall; eax≠0 handled |
| IRQ handler ABI | drivers | `irq.inc:240–245` | EXTENDED | weak | HARD | High | `push data; call handler; test eax` |
| USB RegUSBDriver | drivers | `dll.inc`, `usbapi.txt` | CONFIRMED | yes | HARD | High | |
| NetRegDev / EthInput | drivers | exports | CONFIRMED | yes | HARD | High | |
| TimerHS | drivers | exports + docs | CONFIRMED | yes | HARD | High | |
| DRV_ENTRY/EXIT ±1 | drivers | `const.inc`, `load_pe_driver` | CONFIRMED | yes | HARD | High | |
| DRV_COMPAT/CURRENT | drivers | `dll.inc:9–12` | EXTENDED | missing | HARD | High | |
| UP CLI locking | drivers | spin macros | CONFIRMED | yes | BEHAVIORAL | High | Don’t assume SMP locks |
| Fn58 LBA | apps | docs vs undefined | DOC CONFLICT | — | HARD return −1 | High | Keep dead |
| APM CF return | apps | fn49 | CONFIRMED exception | weak | HARD | Med | |
| Scheduler quanta | apps | sched.inc | BEHAVIORAL | yes | BEHAVIORAL | Med | Match soak tests |
| WinMap / fn34 | apps | `_display.win_map` | CONFIRMED | weak | HARD (via syscall) | Med | |

Kinds of ABI found:

- **Documented ABI** — `sysfuncs.txt`, `drivers_api.txt`, `loader_doc.txt`, `usbapi.txt`
- **Undocumented ABI** — SYSENTER/SYSCALL entries; export address relativity; IRQ eax convention
- **Accidental ABI** — f68.31 pointer leak; ring0 global poking
- **Leaked internals** — SRV field dump; LFB VA convention `0xFE000000`

---

## Confidence notes

No application or `.sys` binaries are in this workspace. Classifications that depend on “wild” consumers remain **UNKNOWN** pending binary corpus scan. Prefer **over-preserving** undocumented-but-implemented entry points (SYSENTER).
