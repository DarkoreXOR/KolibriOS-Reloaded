# Persistent filesystem regression images

Stable paths used by `scripts/mkfs.py` and `scripts/run_qemu.py --disk`:

| Filesystem | Path | Guest path (hybrid IDE) | Guest path (stock reference) |
|------------|------|-------------------------|------------------------------|
| exFAT | `images/exfat-image.img` | `/hd0/1` | `/bd0/1` |
| NTFS | `images/ntfs-image.img` | `/hd1/1` | `/bd1/1` |

Create or reuse images:

```bash
python scripts/mkfs.py              # both at 128M
python scripts/mkfs.py exfat 128M
python scripts/mkfs.py ntfs 128M    # Windows: needs Admin for --force recreate
python scripts/regression.py
```

On Windows, recreating NTFS (`--force`) requires an **Administrator** shell
(diskpart). Reuse of an already-valid populated image does not.

Images are gitignored (regenerated deterministically). Do not delete them
casually during active regression work — use `mkfs.py … --force` to recreate.

`scripts/clean.py` never removes this directory.

- Hybrid kernel (`scripts/run_qemu.py` / `regression.py`): Eolite **Devices** →
  **`/hd0/1`**, **`/hd1/1`** (IDE). Use `--bus ahci` only on AHCI-capable
  builds (`/sdN/1`).
- Stock reference (`scripts/reference_qemu.py --disk …`): disks show as
  **`/bd0/1`**, **`/bd1/1`** (BIOS disks; script injects `biosdisks=on`).
  Stock may not mount exFAT/NTFS — use the hybrid path to browse those FS.
