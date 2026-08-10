# KolibriOS project automation (plain Python)

From the repository root (or any directory — scripts resolve the project root):

```bash
python scripts/doctor.py
python scripts/build.py
python scripts/build.py --release
python scripts/prepare_image.py
python scripts/run_qemu.py
python scripts/run_qemu.py --disk exfat --disk ntfs
python scripts/run.py
python scripts/regression.py
python scripts/reference_qemu.py
python scripts/reference_qemu.py --disk exfat --disk ntfs
python scripts/mkfs.py
python scripts/mkfs.py exfat 128M
python scripts/clean.py
python scripts/clean.py --full
```

With `--disk` on the **stock reference**, look in Eolite for **`/bd0/1`** /
**`/bd1/1`** (BIOS disks). For exFAT/NTFS browsing use the hybrid kernel:

```bash
python scripts/regression.py
```

Configuration: `project/build.toml`.

Focused utilities remain under `tools/` (`kolibri_img`, `mkfs_utils`, `migration_gates`, …).

Run unit tests:

```bash
python -m unittest scripts.tests.test_scripts
```

or:

```bash
python scripts/tests/test_scripts.py
```
