# exFAT regression-test disk (KolibriOS / QEMU)

This directory holds the **persistent secondary disk fixture** used to
regression-test the KolibriOS exFAT filesystem implementation (including Rust
migration cuts that touch exFAT helpers) under QEMU.

It is **not** a boot disk. The KolibriOS kernel continues to boot from the
usual floppy / CoW test image (`-fda`). This image is attached as an **extra**
IDE hard disk on every normal development QEMU boot.

## Image

| Property | Value |
|----------|-------|
| Path | `tools/testdata/kolibrios-exfat-test.img` |
| Size | **exactly 128 MiB** (`134217728` bytes) |
| Filesystem | **exFAT** (whole-disk / superfloppy — no MBR partition table) |
| QEMU role | First IDE HDD (`if=ide,index=0`) → KolibriOS `hd0` / `/hd0/1/` |
| Committed? | **No** — generated locally (see `.gitignore`) |

Whole-disk exFAT matches KolibriOS `disk_scan_partitions` “not an MBR” path
(one partition covering the media), which is how many removable exFAT volumes
appear.

## Layout

```text
/
├── README.TXT
├── ROOT.TXT
├── EMPTY.TXT          (0 bytes)
├── DATA/
│   ├── ONE.TXT
│   ├── TWO.TXT
│   └── THREE.TXT
├── TEST/
│   ├── A/
│   │   ├── FILE_A1.TXT
│   │   └── FILE_A2.TXT
│   └── B/
│       ├── FILE_B1.TXT
│       └── FILE_B2.TXT
└── LARGE/
    └── LARGE.TXT      (~360 KiB deterministic ASCII; multi-cluster reads)
```

All file contents are fixed ASCII (no random data).

## Recreate

Host dependency (once per machine):

```powershell
python -m pip install -r tools/testdata/requirements.txt
```

Preferred (via orchestrator):

```powershell
cargo run --manifest-path tools/build/Cargo.toml -- testdisk
cargo run --manifest-path tools/build/Cargo.toml -- testdisk --force
```

Or call the generator directly:

```powershell
python tools/testdata/create_exfat_test_image.py
python tools/testdata/create_exfat_test_image.py --force
python tools/testdata/create_exfat_test_image.py --verify-only
```

The QEMU stages (`run` / `qemu` / `ref`) **auto-create** the image when it is
missing or invalid (`[testdisk]` in `tools/build/config.toml`).

## QEMU integration

Configured in [`../build/config.toml`](../build/config.toml) under `[testdisk]`:

```text
-drive file=tools/testdata/kolibrios-exfat-test.img,format=raw,if=ide,index=0,media=disk
```

Normal boot (builds kernel + attaches both floppy and exFAT disk):

```powershell
cargo run --manifest-path tools/build/Cargo.toml -- run
```

Reference floppy boot (also attaches the exFAT disk):

```powershell
cargo run --manifest-path tools/build/Cargo.toml -- ref
```

Boot order remains floppy (`-boot a`). The exFAT disk does not replace the
boot medium.

## How to verify in the guest

1. Start QEMU with `run` or `ref` as above.
2. After the desktop appears, open **EOLITE**.
3. In the **Devices** pane, confirm **Hard disk `/hd0/1`** is listed.
   - A/B check: without the testdisk attached, `/hd0/1` is absent (only
     `/sys`, `/fd/1`, `/tmp0/1`, `/cd2/1`).
4. Open `/hd0/1` and confirm `README.TXT`, `DATA/`, `TEST/`, `LARGE/LARGE.TXT`,
   and `EMPTY.TXT`.

If `/hd0/1` is missing, the IDE disk was not detected — check that QEMU was
launched by `kolibri_build` (so `[testdisk]` drive args are present) and that
the image verifies:

```powershell
python tools/testdata/create_exfat_test_image.py --verify-only
```
## Purpose

Regression-test fixture for the **KolibriOS Rust kernel / exFAT filesystem**
work: root enumeration, nested directories, empty/small/large files, and
multi-cluster reads against a stable on-disk layout.
