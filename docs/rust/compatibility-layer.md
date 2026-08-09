# Compatibility Layer

```text
legacy KolibriOS ABI  →  compatibility/  →  modern Rust kernel internals
```

## Responsibilities

1. Syscall register decode/encode + number table.
2. `#[repr(C)]` clones of HARD structures (`process_information`, `IOCTL`, `boot_data`, …).
3. PE export table named `KERNEL` with stable symbols pointing at Rust implementations (or FASM during hybrid).
4. Optional fixed-VA shims: map `SLOT_BASE`/`window_data`/buffers to mirror views of Rust objects.
5. Emulate undocumented quirks (error codes, sysenter stack convention).
6. Isolate “ugly” forever from core.

## Preserve vs adapt

| Item | Strategy |
|------|----------|
| Syscall numbers | Preserve in compat dispatcher |
| `APPDATA` | Adapter object; optional mirror pages |
| Driver exports | Stable symbols; internals free |
| Recursive page_tabs | Reimplement or keep VA during hybrid |
| GUI bit events | Façade over window server |
| Leaked addresses | Capability replacement long-term; shim short-term |

## Raw pointers

Still supported at boundary: user pointers validated (`UserSlice`), IOCTL buffers, IPC maps — never trusted in core without checks.

## Testing

Every compat façade gets differential tests vs FASM kernel ([`../testing/compatibility-testing.md`](../testing/compatibility-testing.md)).
