# Subsystem: Filesystem

**Responsibilities:** path resolve, LFN/Unicode syscalls, FS plugins, execute.  
**Files:** `fs/fs_lfn.inc` (+ fat/exfat/ntfs/ext/iso9660/xfs), `blkdev/disk.inc`.  
**Public:** syscalls 70/80; export `FS_Service`, `FsAdd`, read/write helpers.  
**Model:** not POSIX VFS — path → disk list → `FileSystem` ops.  
**Compat:** path strings + operation codes HARD.
