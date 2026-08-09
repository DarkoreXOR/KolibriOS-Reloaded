# Driver Dependencies on Kernel Internals

Distinguishes documented API vs accidental coupling.

## Documented / intentional API (OK)

- KERNEL PE exports by name
- `RegService` / `GetService` / `ServiceHandler`
- `DiskAdd` + `DISKFUNC` layout
- USB registration APIs (`usbapi.txt`)
- Event exports (`events_subsystem.txt`)
- TimerHS / CancelTimerHS
- IOCTL struct

## Likely / observed internal couplings (hazard)

| Dependency | Class | Notes |
|------------|-------|-------|
| Reading `LFBAddress` export cell | exported symbol + fixed VA | Must keep cell semantics |
| Assuming `OS_BASE` and phys=virt−OS_BASE for kernel addrs | accidental / convention | Common Kolibri pattern |
| Direct CLI for critical sections matching kernel | behavioral | |
| Assuming uniprocessor / no true spinlocks | behavioral | |
| Touching `SLOT_BASE` / current task structs | internal layout used externally | High risk if present |
| Relying on exact `delay_hs` tick rate | behavioral | |
| PE import ordinal vs name | **UNKNOWN** without driver corpus | Prefer names |
| Assuming heap addresses in `0x80800000+` | accidental | |
| IRQ vector numbers after PIC remap | behavioral / hardware | |

## In-tree driver-like PE loads

**LOCAL FACT:** `high_code` loads e.g. video Intel driver string `szVidintel`, PS/2 mouse `szPS2MDriver` via `load_pe_driver`.

## Recommendation

Treat **entire export table + Disk/USB/Net registration structs** as HARD ABI.
Treat any driver that walks kernel globals outside exports as broken-but-must-shim until rewritten.

**UNKNOWN — requires runtime investigation:** scan `/sys/drivers/*.sys` import tables and relocations against kernel symbols/addresses.
