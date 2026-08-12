# Persistent filesystem regression images

Stable paths used by `scripts/mkfs.py` and `scripts/run_qemu.py --disk`:

| Filesystem | Path | Guest path (hybrid IDE) | Guest path (stock reference) |
|------------|------|-------------------------|------------------------------|
| exFAT | `images/exfat-image.img` | `/hd0/1` | `/bd0/1` |
| NTFS | `images/ntfs-image.img` | `/hd1/1` | `/bd1/1` |
| XFS | `images/xfs-image.img` | `/hdN/1` (order of `--disk`; skips IDE index 2 when ISO is attached) | `/bdN/1` |
| EXT2 | `images/ext-image.img` | `/hdN/1` | `/bdN/1` |
| ISO9660 | `images/iso9660-image.iso` | `/cdN` (ATAPI `-cdrom`) | `/cdN` if BIOS sees ATAPI |

Create or reuse images:

```bash
python scripts/mkfs.py              # exfat + ntfs + xfs + ext (defaults)
python scripts/mkfs.py exfat 128M
python scripts/mkfs.py ntfs 128M    # Windows: needs Admin for --force recreate
python scripts/mkfs.py xfs 1G       # Windows: Docker+xfsprogs; reuses existing SB unless --force
python scripts/mkfs.py ext 64M      # Windows: Docker+e2fsprogs; plain EXT2 for Kolibri
python scripts/run_qemu.py --disk xfs
python scripts/run_qemu.py --disk ext
python scripts/run_qemu.py --disk iso9660
python scripts/regression.py
```

On Windows, recreating NTFS (`--force`) requires an **Administrator** shell
(diskpart). Reuse of an already-valid populated image does not.

XFS populate on Windows uses a privileged Docker container with `xfsprogs`
(`tools/mkfs_utils/create_xfs_image.py`). Without `--force`, an existing
`XFSB` image is kept and only the regression tree is rewritten.

EXT populate on Windows uses a privileged Docker container with `e2fsprogs`
(`tools/mkfs_utils/create_ext_image.py`). Images are plain EXT2 (SB @ 1024,
1024-byte blocks, 128-byte inodes) so KolibriOS `ext2_create_partition`
accepts them (incompatible-feature mask + `blocksTotal_hi==0`).

ISO9660: provide `images/iso9660-image.iso` (`.img` also accepted). There is no
`mkfs.py iso9660` generator yet. `--disk iso9660` attaches via QEMU `-cdrom`
(ATAPI, 2048-byte sectors). Attaching as a hard disk does **not** work —
`iso9660_create_partition` requires `SectorSize == 2048`. In Eolite look under
**Devices** for **`/cd0`**, **`/cd1`**, or **`/cd2`** (slot depends on IDE
layout alongside `-hda`/`-hdb`).

Images under `images/*.img` are gitignored (regenerated deterministically). Do
not delete them casually during active regression work — use `mkfs.py … --force`
to recreate. `*.iso` fixtures may be committed or kept locally as needed.

`scripts/clean.py` never removes this directory.

- Hybrid kernel (`scripts/run_qemu.py` / `regression.py`): Eolite **Devices** →
  **`/hd0/1`**, **`/hd1/1`**, … (IDE). Use `--bus ahci` only on AHCI-capable
  builds (`/sdN/1`).
- Stock reference (`scripts/reference_qemu.py --disk …`): disks show as
  **`/bd0/1`**, **`/bd1/1`**, … (BIOS disks; script injects `biosdisks=on`).
  Stock may not mount exFAT/NTFS/XFS/ISO — use the hybrid path to browse those FS.
