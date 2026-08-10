# exFAT regression-test disk (KolibriOS / QEMU)

This directory holds generators and notes for the **persistent secondary disk
fixture** used to regression-test KolibriOS exFAT under QEMU.

The live image used by project scripts is:

```text
images/exfat-image.img
```

It is **not** a boot disk. The kernel boots from the usual floppy / CoW test
image (`-fda`). The exFAT image is attached as an **IDE** disk
(`/hd0/1` in Eolite).

## Image (via scripts)

| Property | Value |
|----------|-------|
| Path | `images/exfat-image.img` |
| Typical size | 128 MiB for soak tests (`python scripts/mkfs.py exfat 128M`) |
| Filesystem | **exFAT** (whole-disk / superfloppy — no MBR partition table) |
| QEMU role | IDE `-hda` → KolibriOS `/hd0/1` |
| Committed? | **No** — generated locally (see `.gitignore`) |

## Recreate

```bash
python -m pip install -r tools/mkfs_utils/requirements.txt
python scripts/mkfs.py exfat 128M
python scripts/mkfs.py exfat 128M --force
```

Or call generators under `tools/mkfs_utils/` / `tools/testdata/` directly.

## QEMU

```bash
python scripts/run.py
python scripts/run_qemu.py --disk exfat
python scripts/regression.py
```

## How to verify in the guest

1. Start QEMU with `python scripts/run.py` or `python scripts/regression.py`.
2. After the desktop appears, open **EOLITE**.
3. In **Devices**, confirm **`/hd0/1`** (and `/hd1/1` for NTFS under regression).
4. Open the volume and confirm `README.TXT`, `DATA/`, nested dirs, etc.
