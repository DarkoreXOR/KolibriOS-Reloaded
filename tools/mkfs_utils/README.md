# mkfs_utils

Reusable filesystem image generators invoked by `scripts/mkfs.py`.

| Script | Output |
|--------|--------|
| `create_exfat_image.py` | `./images/exfat-image.img` |
| `create_ntfs_image.py` | `./images/ntfs-image.img` |
| `create_xfs_image.py` | `./images/xfs-image.img` |

Default sizes via `scripts/mkfs.py`: exFAT/NTFS **128M**, XFS **1G**.

## exFAT

Requires Python **FATtools**:

```bash
python -m pip install -r tools/mkfs_utils/requirements.txt
python scripts/mkfs.py exfat 128M
```

Creates a whole-disk exFAT volume (`PartitionOffset=0`) and populates the
regression tree. Verified by listing the root after create.

## NTFS

| Platform | Backend |
|----------|---------|
| Linux | `losetup` + `mkfs.ntfs` (whole-disk), then populate |
| Windows | **diskpart** (Administrator) — MBR + one NTFS partition, then populate |

`scripts/mkfs.py ntfs` passes `--use-diskpart` on Windows automatically. Run the
shell elevated when recreating with `--force`.

The pure-Python `--allow-minimal` path is experimental and usually unmountable
in Kolibri; do not use it for regression.

```bash
python scripts/mkfs.py ntfs 128M
python scripts/mkfs.py ntfs 128M --force   # needs Admin on Windows
```

## XFS

| Platform | Backend |
|----------|---------|
| Linux (root) | `losetup` + `mkfs.xfs` + mount |
| Windows / no root | Docker privileged + Alpine `xfsprogs` (`docker_populate_xfs.sh`) |

```bash
python scripts/mkfs.py xfs 1G
python scripts/mkfs.py xfs 1G --force   # reformat then populate
python scripts/run_qemu.py --disk xfs
```

Without `--force`, an existing `XFSB` image is reused and only the fixture tree
is rewritten. Docker Desktop must be running on Windows.

Each image contains a deterministic regression tree: root files, nested
directories, empty/small/large files, and a filename with spaces.
