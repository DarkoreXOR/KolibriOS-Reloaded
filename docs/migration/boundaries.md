# Migration Boundaries (Dependency Cuts)

> **Q2 answer:** viable boundaries between legacy FASM and future Rust, and the order to migrate them.

## Cut principle

A **viable cut** is a surface where:

1. Arguments/results are already marshalled (syscall, stdcall export, boot struct), and
2. The trampoline restores any **caller-observable** legacy register / flag behavior that existing in-kernel callers rely on (even if undocumented) — see Cut D `strncmp` / EDX in [`cut-d-implementation.md`](cut-d-implementation.md), and
3. Failure can fall back to FASM without bricking boot.

## Viable cuts (ranked early → late)

```mermaid
flowchart LR
  subgraph early [Early cuts]
    util[Pure utils CRC unicode string]
    proto[Isolated protocol parsers]
  end
  subgraph mid [Mid cuts]
    heap[Allocator behind exports]
    posix[POSIX 77 subset]
    netproto[Net protocols behind socket API]
    fsplugin[Individual FS plugins]
  end
  subgraph late [Late cuts]
    task[taskman process create]
    sched[scheduler policy]
    gui[GUI window server]
    peload[PE driver loader plus exports]
    boot[Boot memory paging]
  end
  early --> mid --> late
```

### Cut A — Pure functions (EARLY)

- **Boundary:** stdcall/C callable from FASM.
- **Examples:** `crc`, `unicode` helpers, string ops already exported.
- **FASM deps:** none beyond call.
- **Risk:** low.
- **Done:** differential unit tests vs FASM.

### Cut B — Memory allocator implementation (MID)

- **Boundary:** exports `AllocPage`/`KernelAlloc`/… keep symbols; body in Rust.
- **Prerequisite:** phys bitmap init still FASM (or Rust owns phys after init cut).
- **Risk:** medium (fragmentation behavioral).
- **Rollback:** relink FASM allocators.

### Cut C — Syscall handler bodies one-by-one (MID)

- **Boundary:** `servetable2` slot points to Rust `extern "C"` after asm `pushad` frame decode.
- **Best first:** clock/date, msg board, pure query calls.
- **Avoid first:** 0/7/12 GUI, 70 FS, 68 drivers, 51 threads.
- **Risk:** per-call.
- **Rollback:** restore FASM function pointer.

### Cut D — FS plugin behind `FileSystem` ops (MID)

- **Boundary:** `fs_add` registration; implement one FS in Rust.
- **Prerequisite:** disk layer still FASM.
- **Risk:** medium.

### Cut E — Network protocols (MID–LATE)

- **Boundary:** keep `sys_socket` / `NetRegDev` in FASM; move TCP/UDP processing.
- **Risk:** timing/races medium-high.

### Cut F — Scheduler policy (LATE)

- **Boundary:** `find_next_task` only; keep `do_change_task` asm.
- **Risk:** high behavioral.

### Cut G — Process/thread creation (LATE)

- **Boundary:** `create_process`/`fs_execute` orchestration in Rust calling memory/sched primitives.
- **Risk:** high.

### Cut H — GUI (LATE)

- **Boundary:** syscall GUI → Rust window server; keep LFB blit hot paths carefully.
- **Risk:** very high (apps sensitive).

### Cut I — Driver loader + export directory (LATE)

- **Boundary:** entire PE load/export remain stable; reimplement in Rust as a unit.
- **Risk:** very high (all `.sys`).

### Cut J — Boot/paging bring-up (LATE / flag day)

- **Boundary:** loader still drops at `0x10000`; Rust owns from `B32` or UEFI entry.
- **Risk:** extreme; do when hybrid proven.

## Non-cuts (do not split naively)

- Mid-`do_change_task` (esp swap vs CR3) — keep one asm owner.
- Half of `SRV` without export table.
- GUI event bits without window redraw path.
- USB ISR without USB thread model.
- GS/LFB path without fn61 / graphics GDT (HARD app graphics).
- SYSENTER stub without matching EBP/stack convention.

## Audit-driven cut adjustments (2026-08)

Findings from [`../compatibility/abi-audit.md`](../compatibility/abi-audit.md):

1. **Do not prioritize freezing `SLOT_BASE`/`window_data` VAs for apps** — CPL3 cannot access them (`PG_SWR` kernel PDEs). Migrating slot storage early is safer than assumed.
2. **Add explicit Cut C0 — preserve syscall entry asm** (`i40`/`sysenter_entry`/`syscall_entry`) before swapping handler bodies.
3. **Add Cut H0 — GS/LFB contract tests** before GUI rewrite; graphics HARD ABI is broader than draw syscalls.
4. **Driver cut I must include** `DISKFUNC.strucsize`, IRQ EAX convention, `DRV_*` version, `' SRV'` magic, `LFBAddress` last-export rule — not just symbol names.
5. **Corpus scan** of apps/`.sys` remains a gate before deleting ACCIDENTAL shims (68.31 dumps, ring0 global peeks).

## Recommended migration order

1. Tooling: restore `init.inc`, build `kernel.mnt`, smoke boot in QEMU.  
2. Cut A utils.  
3. Compat test harness (differential) **including GS fn61 and SYSENTER if stub available**.  
4. Cut C0 entry stubs frozen; Cut C easy syscalls.  
5. Cut B allocators behind exports.  
6. Cut D one FS.  
7. Cut E network pieces.  
8. Cut F scheduler policy (**APPDATA storage may move** if stride/semantics preserved).  
9. Cut G process create (`MENUET01/02` tests).  
10. Cut H0 graphics contract; Cut H GUI.  
11. Cut I drivers/exports (**full binary contract**).  
12. Cut J boot; delete FASM.

## Hybrid link sketch

**LOCAL FACT (Phase C probe):** Rust emits ELF32 into `libkolibri_utils.a`; reloc-free functions are extracted to a raw blob and embedded with FASM `file`. See [`phase-c-integration.md`](phase-c-integration.md).

**LOCAL FACT (Cut A complete):** `rust_crc_32`, `rust_unicode_utf16_encode`, `rust_unicode_cp866_encode`, and `rust_unicode_utf8_decode` are reloc-free and use the same extract + `file` path ([`cut-a-final-architecture.md`](cut-a-final-architecture.md)). CP866 required a stack-local mapping table to avoid `.rodata`/GOTOFF; UTF-8 was rewritten to the same reloc-free discipline.

**LOCAL FACT (Cut B complete):** `rust_cp866_to_upper` (`cp866toUpper`) is also reloc-free (71 B, 0 relocs) via the same path ([`cut-b-implementation.md`](cut-b-implementation.md)). Note: this “Cut B” is the next pure-util migration, **not** the allocator cut named “Cut B” elsewhere in this document.

**LOCAL FACT (Cuts A–BD complete, 2026-08-12):** Stage 2/3/5 production envelope through Cut BD (`tcp_outflags`) — see [`migration-plan.md`](migration-plan.md) and durable inventory [`migration-todo.md`](migration-todo.md). Prefer `scripts/` + `project/build.toml` blob/migration registry over manual per-cut scripts for day-to-day builds. Post-BC audit: Path A rejected; thin P sibling / Stage-4 address math / PE leftovers deferred; Cut BD is Path B (TCP state→flags table; `tcp_output` send path).

**INFERENCE (future functions):** anything that reintroduces `.rodata`/GOTOFF/cross-section refs needs either a reloc-free rewrite or `rust-lld` at the FASM placement VMA — evaluate per function. Avoid Rust `match` jump tables in freestanding leaves (Cut AZ).
