# Application ABI

What an existing KolibriOS application expects from the kernel.

## Binary format

**LOCAL FACT:** Menuet/Kolibri style header `APP_HEADER_01_` (`taskman.inc`):

- Banner/version identifying format
- `start` entry EIP
- `i_end` end of image
- `mem_size` initial address space size
- `stack_top` user ESP
- `i_param`, `i_icon` optional pointers

Loaded by FS execute (syscall 70 operation 7) via `fs_execute`.

## Address space

- User VA `0 .. 0x7FFFFFFF`
- App typically based at `0`
- Kernel high half not usable
- Resize via syscall 64 / related 68 heap helpers

## Thread / process

- First thread created with process
- Additional threads: syscall 51
- PID/TID reported by syscall 9 (`process_information`)
- Exit: EAX=−1 / syscall 255

## Syscall invocation

- **HARD:** `int 0x40`, number in EAX, args in other GPRs
- Event masks: syscall 40; wait/check: 10/11/23
- Event bits: `EVENT_REDRAW`, `KEY`, `BUTTON`, `MOUSE`, `IPC`, `NETWORK`, … (`const.inc`)

## GUI

Observable behaviors apps rely on:

- Window create/draw (0), buttons (8/17), redraw brackets (12)
- Client vs window coordinates, skins, shapes
- Mouse (37), keyboard (2), screen size (14)
- Z-order / focus via syscall 18 subfunctions
- Window messages (72)

Timing of redraw events and input queues is **BEHAVIORAL ABI**.

## Files

- LFN API syscall 70; Unicode 80
- Path conventions `/sys`, `/rd`, `/hd*`, dynamic disk names
- Execute from FS
- CWD syscall 30

## IPC / sync

- IPC buffers syscall 60
- Clipboard 54
- POSIX subset 77 (pipes, futexes, …)

## Network

- 74 device, 75 socket, 76 protocols
- `EVENT_NETWORK` wakeups

## Debug / misc

- Msg board 63
- Debug services 69
- PCI 62
- Port reserve 46

## Memory observability

Official: syscall 9 fields, resize syscalls.
Unofficial risk: direct reads of `SLOT_BASE` / window arrays — treat as compatibility hazard.

## Drivers from apps

Syscall 68 load/IOCTL — apps load `.sys` under `/sys/drivers/`.

## Must preserve for app compatibility

1. Syscall numbers + register convention + buffer layouts in `sysfuncs.txt`
2. App header load semantics
3. Event bit meanings and delivery
4. FS path + 70/80 operations
5. Window/button/mouse/key behaviors
6. Network socket semantics
7. 2 GiB user / high kernel split

Details: [`syscall-abi.md`](syscall-abi.md), [`external-contract.md`](external-contract.md).
