# Migration Risk Register

Updated after adversarial ABI audit.

| Risk | Subsystem | Prob | Impact | Detect | Mitigate | Fallback |
|------|-----------|------|--------|--------|----------|----------|
| Break GS/LFB direct access | graphics/apps | Med | High | Fn61+GS tests | Keep graph GDT + U LFB map | Documented syscall-only would break apps |
| SYSENTER stub mismatch | syscall | Med | High | Exercise SYSENTER | Keep asm entry + EBP convention | int 0x40 only (may break libc) |
| `SRV.magic` wrong (`'SRV '` vs `' SRV'`) | drivers | Low | High | IOCTL fail | Match code magic | |
| Omit `DISKFUNC.strucsize` compat | drivers | Med | High | Old driver load | Accept smaller tables | |
| IRQ handler EAX convention wrong | drivers | Med | High | IRQ storm/fail | Match `test eax` | |
| f68.31 pointer dump change | apps | Low | Med | App uses 68.31 | Preserve copy layout | |
| Assume apps poke `SLOT_BASE` | false risk | — | — | CPL3 load must PF | **Do not over-shim** | Wastes effort |
| Ring0 driver peeks slots | drivers | Med | Med | `.sys` disasm | Optional layout shim | |
| Syscall doc ≠ code (58, 68.15/25, 77 labels) | syscall | High | Med | Golden vs **code** | Code-primary tests | |
| MENUET02/DLL autoload missed | apps | Med | High | Launch MENUET02 | Implement version=2 path | |
| PE export not last=`LFBAddress` | drivers | Low | High | Driver LFB use | Enforce export order | |
| Export addr not `OS_BASE`-relative | drivers | Med | Crit | All `.sys` fail | Match `export.inc` | |
| Scheduler timing | sched | Med | High | Soak | Match initially | Revert FASM |
| GUI redraw races | gui | High | High | Visual tests | Migrate late | FASM GUI |
| Page map / recursive PT | memory | Med | Crit | PF stress | Careful cut | FASM memory |
| Boot `boot_data` mismatch | boot | Low | Crit | Loader matrix | Freeze struct | |
| DMA24 assumptions | drivers | Med | High | Device tests | Keep API | |
| UP CLI vs SMP | sync | Low now | High later | Don't enable SMP | Document UP | |
| Corrupted local `init.inc` | build | — | — | Assemble | **Resolved** 2026-08-09 — restored from upstream `944d74f01` | |
| Network races | net | Med | Med | socket tests | Migrate late | |
| Fn9 buffer only 0x4C validated | apps | Low | Low | Pass 1KB buffers | Fill 0x4C; don't require 1KB | |

## Highest priority watchlist

1. GS/LFB + fn61 (newly elevated)  
2. Driver PE export relativity + full binary contract  
3. SYSENTER/SYSCALL entries  
4. GUI/event timing  
5. Code-vs-docs syscall edges  
6. ~~Restoring buildable `init.inc` baseline~~ (done)  
7. Ring0 accidental coupling (not CPL3 slot VA)  
8. Phase C hybrid link of Rust `staticlib` into flat `kernel.mnt`
