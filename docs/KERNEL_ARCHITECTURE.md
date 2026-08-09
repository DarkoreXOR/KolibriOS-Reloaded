# KolibriOS Kernel Architecture (Master Index)

Entry point for future coding agents migrating this FASM kernel to freestanding Rust.

**Read first:** [`_meta/evidence-policy.md`](_meta/evidence-policy.md).  
**Do not modify `kernel/` while only documenting.**  
**Baseline:** [`kernel/init.inc`](../kernel/init.inc) restored (2026-08-09); Cut A complete — see [`migration/cut-a-final-architecture.md`](migration/cut-a-final-architecture.md). Historical init notes: [`_meta/upstream-init-diff.md`](_meta/upstream-init-diff.md).

---

## 1. What KolibriOS kernel is

A **32-bit x86** monolithic kernel written in **FASM**, producing flat binary **`kernel.mnt`**. It serves Menuet-descended applications via **`int 0x40`** syscalls and loads **PE `.sys` drivers** that import a **`KERNEL`** export table. User address space is the low 2 GiB; kernel is mapped at **`OS_BASE = 0x80000000`**.

## 2. How it boots

Loader places image at **`0x10000`** → optional 16-bit UI (`bootbios.inc`) → `B32` → CPU/memory init → paging → `high_code` subsystem init → `sti` → `osloop`.

Details: [`architecture/boot-sequence.md`](architecture/boot-sequence.md).

## 3. How it is built

`fasm kernel.asm → bin/kernel.mnt` (`make lang=en_US`). No ELF link. See [`architecture/build-system.md`](architecture/build-system.md).

## 4. Memory model

User `< 0x80000000`; kernel high half; 4 KiB pages; recursive PT at `0xFDC00000`; heap at `0x80800000`; LFB at `0xFE000000`. See [`architecture/memory-model.md`](architecture/memory-model.md), [`compatibility/fixed-addresses.md`](compatibility/fixed-addresses.md).

## 5. Process / thread model

`PROC` = address space; `APPDATA` (256 B slots at `0x80090000`) = schedulable thread; windows via `WDATA`. See [`architecture/process-model.md`](architecture/process-model.md), [`architecture/scheduler.md`](architecture/scheduler.md).

## 6. Kernel subsystems

| Area | Doc |
|------|-----|
| Memory / paging | `architecture/subsystem-memory.md`, `subsystem-paging.md` |
| Syscalls | `subsystem-syscalls.md` |
| FS / storage | `subsystem-filesystem.md`, `subsystem-storage.md` |
| GUI / input | `subsystem-gui.md`, `subsystem-input.md` |
| Network | `subsystem-network.md` |
| Drivers | `subsystem-drivers.md` |
| Sync/IPC | `subsystem-sync-ipc.md` |
| Timers / buses / debug | `subsystem-timers.md`, `subsystem-buses.md`, `subsystem-debug.md` |
| x86 / IRQ / switch | `architecture/x86.md`, `interrupts.md`, `context-switching.md` |

## 7. Important structures

[`architecture/data-structures.md`](architecture/data-structures.md), [`architecture/object-relationships.md`](architecture/object-relationships.md).

Key: `APPDATA`, `PROC`, `WDATA`, `process_information` (**HARD**), `SRV`, `IOCTL`, `boot_data`, `DISK`/`DISKFUNC`.

## 8. Syscall ABI

[`compatibility/syscall-abi.md`](compatibility/syscall-abi.md), [`compatibility/syscalls.yaml`](compatibility/syscalls.yaml). Primary app entry: **`int 0x40`**, EAX=number.

## 9. Driver ABI

[`compatibility/driver-abi.md`](compatibility/driver-abi.md), [`compatibility/driver-dependencies.md`](compatibility/driver-dependencies.md), exports in `kernel/core/exports.inc`.

## 10. Fixed addresses

[`compatibility/fixed-addresses.md`](compatibility/fixed-addresses.md).

## 11. Important assembly

[`rust/permanent-assembly.md`](rust/permanent-assembly.md) — boot, IRQ/syscall entry, context switch primitive, descriptors, ports, FPU save.

## 12. Dependency graph

[`architecture/dependency-graph.md`](architecture/dependency-graph.md).

## 13. Compatibility boundary

[`compatibility/compatibility-boundary.md`](compatibility/compatibility-boundary.md).  
**Complete external contract:** [`compatibility/external-contract.md`](compatibility/external-contract.md).  
**Inventory:** [`compatibility/abi-inventory.yaml`](compatibility/abi-inventory.yaml).  
**Must-preserve checklist:** [`compatibility/KNOWN_COMPATIBILITY_SURFACES.md`](compatibility/KNOWN_COMPATIBILITY_SURFACES.md).

### Adversarial audit (second pass)

| Doc | Purpose |
|-----|---------|
| [`compatibility/abi-audit.md`](compatibility/abi-audit.md) | Confirm/contradict/extend prior HARD claims |
| [`compatibility/application-memory-contract.md`](compatibility/application-memory-contract.md) | What apps can really see in memory |
| [`compatibility/driver-binary-contract.md`](compatibility/driver-binary-contract.md) | Full `.sys`↔kernel binary contract |
| [`compatibility/driver-contract.yaml`](compatibility/driver-contract.yaml) | Machine-readable driver contract |
| [`compatibility/fixed-address-audit.md`](compatibility/fixed-address-audit.md) | Per-address relocate/emulate decisions |
| [`compatibility/syscall-audit.md`](compatibility/syscall-audit.md) | sysfuncs vs code discrepancies |
| [`compatibility/structure-abi.md`](compatibility/structure-abi.md) | Exact sizes/offsets |

## 14. Architectural problems

[`architecture/legacy-problems.md`](architecture/legacy-problems.md), [`architecture/pointer-dependencies.md`](architecture/pointer-dependencies.md).

## 15. Target Rust architecture

[`rust/target-architecture.md`](rust/target-architecture.md).

## 16. Compatibility layer

[`rust/compatibility-layer.md`](rust/compatibility-layer.md), [`rust/unsafe-boundary.md`](rust/unsafe-boundary.md).

## 17. Migration strategy

[`migration/migration-plan.md`](migration/migration-plan.md).  
**Dependency cuts / order:** [`migration/boundaries.md`](migration/boundaries.md).  
**Risks:** [`migration/risk-register.md`](migration/risk-register.md).

## 18. Testing strategy

[`testing/compatibility-testing.md`](testing/compatibility-testing.md).

## 19. Known unknowns

- Exact upstream revision match for this tree (beyond mirrored `init.inc`).
- Whether shipping apps poke `SLOT_BASE` / `window_data` directly.
- Full wild driver import surface beyond `exports.inc`.
- Sysenter user stub canonical form (not in this tree).
- SMP safety beyond experimental AP sleep path.
- Complete teardown ordering edge cases.
- Behavioral tolerances for scheduler/GUI timing.

Mark runtime work: **UNKNOWN — requires runtime investigation**.

---

## Quick answers to the two migration questions

1. **External contract:** boot protocol + syscall/app binary + driver PE/exports/structs + address-space split + behavioral GUI/FS/net — see `external-contract.md`.
2. **Dependency cuts:** pure utils → easy syscalls → alloc exports → FS/net islands → scheduler policy → process create → GUI → PE/exports → boot — see `boundaries.md`.

## Source inventory / repo layout

[`_meta/source-inventory.md`](_meta/source-inventory.md).  
**Filesystem layout (FASM vs Rust, FASM/QEMU/image rules):** [`_meta/project-structure.md`](_meta/project-structure.md).
