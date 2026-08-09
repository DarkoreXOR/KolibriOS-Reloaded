# Unsafe Boundary

Goal: concentrate `unsafe` in auditable layers — not “zero unsafe”.

## Allowed unsafe zones

| Zone | Examples | Review bar |
|------|----------|------------|
| CPU / arch | CR*, descriptors, IRET, CLI | Small functions, no alloc |
| MMIO | `Volatile` ptrs to devices | Typed wrappers per device |
| User pointers | probe + copy_in/out | Must check AS + rights |
| Legacy ABI | write shim structs at fixed VA | Behind `compatibility` |
| Context switch | stack swap | Pair with formal invariants |
| Page tables | PTE writes | Hold AS lock / UP CLI |
| DMA | phys contiguous buffers | Sync with device |
| Interrupt handlers | top half only | No lock that sleeps |
| FFI / asm | `extern "C"` to asm | Explicit ABI comments |

## Forbidden outside zones

- Random `&mut` to `'static` kernel globals without lock wrapper
- Trusting app pointers in FS/GUI without validation
- Safe wrappers that lie about invariants

## Pattern

```rust
// safe API
pub fn write_user(as: &AddressSpace, va: u32, bytes: &[u8]) -> Result<(), Error> {
    // unsafe confined inside as.copy_to_user
}
```
