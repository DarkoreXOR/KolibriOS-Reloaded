# Subsystem: Block Storage

**Responsibilities:** `DISK` objects, partitions, caches, ATA/AHCI/floppy/ramdisk/CD.  
**Files:** `blkdev/*`, `detect/init_ata.inc`.  
**Public:** `DiskAdd`/`DiskDel`/`DiskMediaChanged` (**HARD**).  
**Init:** after PCI; before USB/apps.
