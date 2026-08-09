# Compatibility Testing

## Goal

Prove Rust (or hybrid) matches **observable** FASM behavior.

## Differential pattern

```text
same test case
   ├─ FASM kernel.mnt  → trace/log/screenshot/hash
   └─ Rust/hybrid      → compare
```

## Suites

| Suite | Method |
|-------|--------|
| Boot | Multi-loader: FAT12, ext loader, UEFI if available; check `boot_data` end-state |
| Syscall | User stub exercising each non-undefined number; nested 18/68/70/74–77 |
| Process | Launch apps, threads (51), exit (−1), syscall 9 fields |
| Memory | Resize (64), PF on guard, shared mem |
| FS | 70/80 ops on FAT/ramdisk/ISO; execute |
| GUI | Draw window, buttons, redraw, mouse/key events |
| Drivers | Load stock `.sys`, IOCTL smoke, DiskAdd devices |
| Net | loopback socket pair, ping if NIC |
| Sched | Spin threads + priorities; watchdog |
| IPC | clipboard, IPC send, futex/pipe via 77 |

## Harness sketch

1. QEMU + serial logging (`msg board` / debug).  
2. Tiny Kolibri test apps (asm/C) checked into future `tests/`.  
3. Compare: register return traces, filesystem side effects on ramdisk, optional framebuffer CRC.  
4. Gate merges on golden diffs.

## Pass criteria

- No HARD ABI mismatches.
- BEHAVIORAL within documented tolerances (timing windows).
- Zero PF panics on suite.

## Gaps (**UNKNOWN** without runtime)

- Exact stock app list for this export
- Hardware-only drivers (AHCI quirks)
