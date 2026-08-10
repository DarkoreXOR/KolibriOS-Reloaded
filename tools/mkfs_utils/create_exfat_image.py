#!/usr/bin/env python3
"""Create a deterministic exFAT regression-test disk image.

Usage (from repository root):

    python tools/mkfs_utils/create_exfat_image.py --size 4M
    python tools/mkfs_utils/create_exfat_image.py --size 128M -o images/exfat-image.img
    python tools/mkfs_utils/create_exfat_image.py --size 4M --force

Requires: FATtools (see requirements.txt).
"""

from __future__ import annotations

import argparse
import os
import re
import struct
import sys
from pathlib import Path

from test_tree import (
    DATA_ONE,
    DATA_THREE,
    DATA_TWO,
    EMPTY_TXT,
    NESTED_A1,
    NESTED_A2,
    NESTED_B1,
    NESTED_B2,
    README,
    ROOT_TXT,
    SMALL_BIN,
    SPACE_CONTENT,
    SPACE_NAME,
    expected_root_names,
    large_payload,
)


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[2]


def parse_size(text: str) -> int:
    m = re.fullmatch(r"(\d+(?:\.\d+)?)([KMG]?)", text.strip().upper())
    if not m:
        raise SystemExit(f"ERROR: invalid size `{text}` (use 4M, 128M, 4096, …)")
    value = float(m.group(1))
    mult = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3}[m.group(2)]
    size = int(round(value * mult))
    if size <= 0:
        raise SystemExit(f"ERROR: size must be positive, got `{text}`")
    return size


def require_fattools() -> None:
    try:
        import FATtools  # noqa: F401
    except ImportError:
        req = Path(__file__).resolve().parent / "requirements.txt"
        sys.stderr.write(
            "ERROR: Python package FATtools is required.\n"
            f"Install: python -m pip install -r {req}\n"
        )
        raise SystemExit(2)


def write_file(dir_obj, name: str, data: bytes | str) -> None:
    if isinstance(data, str):
        data = data.encode("ascii")
    handle = dir_obj.create(name)
    try:
        if data:
            handle.write(data)
    finally:
        handle.close()


def populate(fs) -> None:
    write_file(fs, "README.TXT", README)
    write_file(fs, "ROOT.TXT", ROOT_TXT)
    write_file(fs, "EMPTY.TXT", EMPTY_TXT)
    write_file(fs, "TINY.BIN", SMALL_BIN)

    data = fs.mkdir("DATA")
    write_file(data, "ONE.TXT", DATA_ONE)
    write_file(data, "TWO.TXT", DATA_TWO)
    write_file(data, "THREE.TXT", DATA_THREE)

    nested = fs.mkdir("NESTED")
    a = nested.mkdir("A")
    write_file(a, "FILE_A1.TXT", NESTED_A1)
    write_file(a, "FILE_A2.TXT", NESTED_A2)
    b = nested.mkdir("B")
    write_file(b, "FILE_B1.TXT", NESTED_B1)
    write_file(b, "FILE_B2.TXT", NESTED_B2)

    large = fs.mkdir("LARGE")
    write_file(large, "LARGE.TXT", large_payload())

    spaces = fs.mkdir("FILES WITH SPACES")
    write_file(spaces, "HELLO WORLD.TXT", SPACE_CONTENT)


def verify_image(path: Path, size_bytes: int) -> None:
    if path.stat().st_size != size_bytes:
        raise SystemExit(f"ERROR: image size {path.stat().st_size} != {size_bytes}")

    with open(path, "rb") as f:
        boot = f.read(512)
    if boot[3:11] != b"EXFAT   ":
        raise SystemExit(f"ERROR: missing exFAT OEM name: {boot[:11]!r}")
    part_off = struct.unpack_from("<Q", boot, 64)[0]
    if part_off != 0:
        raise SystemExit(
            f"ERROR: PartitionOffset={part_off} (want 0 for whole-disk Kolibri images)"
        )

    from FATtools.Volume import vclose, vopen

    fs = vopen(str(path), "rb")
    try:
        root_names = set(fs.listdir())
        if root_names != expected_root_names():
            raise SystemExit(
                f"ERROR: root listing {sorted(root_names)} != {sorted(expected_root_names())}"
            )
    finally:
        vclose(fs)


def exfat_boot_checksum(boot_region: bytes) -> int:
    """Volume boot checksum over the first 11 sectors (exFAT spec)."""
    checksum = 0
    for index, byte in enumerate(boot_region):
        if index in (106, 107, 112):
            continue
        checksum = (
            (0x80000000 if (checksum & 1) else 0) + (checksum >> 1) + byte
        ) & 0xFFFFFFFF
    return checksum


def normalize_whole_disk_exfat(path: Path) -> None:
    """FATtools often sets PartitionOffset=2048 on raw disks; Kolibri expects 0."""
    with open(path, "r+b") as f:
        main = bytearray(f.read(12 * 512))
        if main[3:11] != b"EXFAT   ":
            raise SystemExit(f"ERROR: not an exFAT boot sector: {path}")
        struct.pack_into("<Q", main, 64, 0)
        packed = struct.pack("<I", exfat_boot_checksum(main[: 11 * 512]))
        main[11 * 512 : 12 * 512] = packed * (512 // 4)
        f.seek(0)
        f.write(main)
        if path.stat().st_size >= 24 * 512:
            f.seek(12 * 512)
            backup = bytearray(f.read(12 * 512))
            if backup[3:11] == b"EXFAT   ":
                struct.pack_into("<Q", backup, 64, 0)
                packed = struct.pack("<I", exfat_boot_checksum(backup[: 11 * 512]))
                backup[11 * 512 : 12 * 512] = packed * (512 // 4)
                f.seek(12 * 512)
                f.write(backup)


def create_image(out: Path, size_bytes: int, force: bool) -> str:
    require_fattools()
    from FATtools.Volume import vclose, vopen
    from FATtools.mkfat import exfat_mkfs

    out = out.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)

    if out.exists() and not force:
        if out.stat().st_size == size_bytes:
            with open(out, "rb") as f:
                if f.read(11)[3:11] == b"EXFAT   ":
                    verify_image(out, size_bytes)
                    print(f"reused: {out}")
                    return "reused"
        print(f"Existing image invalid or wrong size; recreating: {out}")

    tmp = out.with_suffix(out.suffix + ".tmp")
    if tmp.exists():
        tmp.unlink()

    print(f"Creating {size_bytes} byte raw exFAT image: {tmp}")
    with open(tmp, "wb") as f:
        f.truncate(size_bytes)

    disk = vopen(str(tmp), "r+b", "disk")
    try:
        exfat_mkfs(disk, disk.size)
    finally:
        vclose(disk)

    normalize_whole_disk_exfat(tmp)

    fs = vopen(str(tmp), "r+b")
    try:
        populate(fs)
    finally:
        vclose(fs)

    if out.exists():
        out.unlink()
    os.replace(tmp, out)
    verify_image(out, size_bytes)
    print(f"created: {out}")
    return "created"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help="output path (default: images/exfat-image.img)",
    )
    parser.add_argument(
        "--size",
        required=True,
        help="image size (e.g. 4M, 128M, 4096)",
    )
    parser.add_argument("--force", action="store_true", help="recreate even if valid")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()

    size_bytes = parse_size(args.size)
    root = repo_root_from_script()
    out = args.output if args.output else root / "images" / "exfat-image.img"
    if not out.is_absolute():
        out = (Path.cwd() / out).resolve()

    if args.verify_only:
        require_fattools()
        verify_image(out, size_bytes)
        print(f"verify OK: {out}")
        return 0

    outcome = create_image(out, size_bytes, args.force)
    print(f"outcome: {outcome}")
    print(f"  size: {size_bytes} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
