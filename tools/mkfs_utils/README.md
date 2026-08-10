# mkfs_utils

Reusable filesystem image generators invoked by the orchestrator `mkfs` command.

| Script | Output |
|--------|--------|
| `create_exfat_image.py` | `./images/exfat-image.img` |
| `create_ntfs_image.py` | `./images/ntfs-image.img` |

## exFAT

Requires Python **FATtools**:

```powershell
python -m pip install -r tools/mkfs_utils/requirements.txt
```

```powershell
python tools/mkfs_utils/create_exfat_image.py --size 4M
```

## NTFS

On **Windows**, uses `diskpart` + `Format-Volume` (administrator privileges may be
required). Minimum practical size is **8M**.

On **Linux**, uses `losetup` + `mkfs.ntfs` (ntfs-3g, root required).

Prefer the orchestrator:

```powershell
cargo run --manifest-path orch/Cargo.toml -- mkfs ntfs 8M
cargo run --manifest-path orch/Cargo.toml -- mkfs exfat 4M
```

Each image contains a deterministic regression tree: root files, nested
directories, empty/small/large files, and a filename with spaces.
