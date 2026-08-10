#!/usr/bin/env python3
"""Create the deterministic 128 MiB exFAT regression-test disk for KolibriOS/QEMU.

This fixture is used exclusively to exercise the KolibriOS exFAT filesystem
implementation (including Rust migration cuts that touch exFAT) under QEMU.

Usage (from repository root):

    python tools/testdata/create_exfat_test_image.py
    python tools/testdata/create_exfat_test_image.py --force

Requires: FATtools (see tools/testdata/requirements.txt).

Output (default):

    tools/testdata/kolibrios-exfat-test.img
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
from pathlib import Path

SIZE_BYTES = 128 * 1024 * 1024  # exactly 128 MiB
DEFAULT_REL = Path("tools/testdata/kolibrios-exfat-test.img")

# Deterministic ASCII contents. LARGE.TXT is sized to span many clusters.
README = (
    "KolibriOS exFAT regression-test fixture\n"
    "Purpose: deterministic secondary IDE disk for QEMU kernel tests.\n"
    "Filesystem: exFAT\n"
    "Size: 128 MiB\n"
    "Do not use as a boot disk.\n"
)

ROOT_TXT = "ROOT.TXT: file in the volume root for enumeration tests.\n"
EMPTY_TXT = b""  # intentionally empty

DATA_ONE = "DATA/ONE.TXT: first file in DATA/.\n"
DATA_TWO = "DATA/TWO.TXT: second file in DATA/.\n"
DATA_THREE = "DATA/THREE.TXT: third file in DATA/.\n"

FILE_A1 = "TEST/A/FILE_A1.TXT: nested under TEST/A/.\n"
FILE_A2 = "TEST/A/FILE_A2.TXT: sibling of FILE_A1.TXT.\n"
FILE_B1 = "TEST/B/FILE_B1.TXT: nested under TEST/B/.\n"
FILE_B2 = "TEST/B/FILE_B2.TXT: sibling of FILE_B1.TXT.\n"

# ~256 KiB of repeating lines — well above typical 4 KiB exFAT clusters on 128 MiB.
_LARGE_LINE = (
    "LARGE.TXT line {:05d}: "
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 "
    "kolibrios-exfat-regression-fixture\n"
)
LARGE_LINES = 4096  # 4096 * ~88 bytes ≈ 360 KiB


def repo_root_from_script() -> Path:
    # tools/testdata/create_exfat_test_image.py → repo root
    return Path(__file__).resolve().parents[2]


def require_fattools() -> None:
    try:
        import FATtools  # noqa: F401
    except ImportError:
        req = Path(__file__).resolve().parent / "requirements.txt"
        sys.stderr.write(
            "ERROR: Python package FATtools is required to build the exFAT test image.\n"
            f"Install with:\n  python -m pip install -r {req}\n"
        )
        raise SystemExit(2)


def large_payload() -> bytes:
    body = "".join(_LARGE_LINE.format(i) for i in range(LARGE_LINES))
    # Append a stable trailer with a checksum of the body so corruption is obvious.
    digest = hashlib.sha256(body.encode("ascii")).hexdigest()
    trailer = f"END LARGE.TXT sha256={digest} lines={LARGE_LINES}\n"
    return (body + trailer).encode("ascii")


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

    data = fs.mkdir("DATA")
    write_file(data, "ONE.TXT", DATA_ONE)
    write_file(data, "TWO.TXT", DATA_TWO)
    write_file(data, "THREE.TXT", DATA_THREE)

    test = fs.mkdir("TEST")
    a = test.mkdir("A")
    write_file(a, "FILE_A1.TXT", FILE_A1)
    write_file(a, "FILE_A2.TXT", FILE_A2)
    b = test.mkdir("B")
    write_file(b, "FILE_B1.TXT", FILE_B1)
    write_file(b, "FILE_B2.TXT", FILE_B2)

    large = fs.mkdir("LARGE")
    write_file(large, "LARGE.TXT", large_payload())


def verify_image(path: Path) -> None:
    size = path.stat().st_size
    if size != SIZE_BYTES:
        raise SystemExit(f"ERROR: image size {size} != {SIZE_BYTES}")

    with open(path, "rb") as f:
        boot = f.read(11)
    if boot[3:11] != b"EXFAT   ":
        raise SystemExit(f"ERROR: missing exFAT OEM name at offset 3: {boot!r}")

    from FATtools.Volume import vopen, vclose

    fs = vopen(str(path), "rb")
    try:
        root_names = set(fs.listdir())
        expected_root = {"README.TXT", "ROOT.TXT", "EMPTY.TXT", "DATA", "TEST", "LARGE"}
        if root_names != expected_root:
            raise SystemExit(f"ERROR: root listing {sorted(root_names)} != {sorted(expected_root)}")

        data = fs.opendir("DATA")
        if set(data.listdir()) != {"ONE.TXT", "TWO.TXT", "THREE.TXT"}:
            raise SystemExit(f"ERROR: DATA/ listing unexpected: {data.listdir()}")

        test = fs.opendir("TEST")
        a = test.opendir("A")
        b = test.opendir("B")
        if set(a.listdir()) != {"FILE_A1.TXT", "FILE_A2.TXT"}:
            raise SystemExit(f"ERROR: TEST/A/ listing unexpected: {a.listdir()}")
        if set(b.listdir()) != {"FILE_B1.TXT", "FILE_B2.TXT"}:
            raise SystemExit(f"ERROR: TEST/B/ listing unexpected: {b.listdir()}")

        large_dir = fs.opendir("LARGE")
        if set(large_dir.listdir()) != {"LARGE.TXT"}:
            raise SystemExit(f"ERROR: LARGE/ listing unexpected: {large_dir.listdir()}")

        empty = fs.open("EMPTY.TXT")
        try:
            empty_data = empty.read()
        finally:
            empty.close()
        if empty_data not in (b"", None):
            # FATtools may return b'' for empty; reject non-empty.
            if empty_data:
                raise SystemExit(f"ERROR: EMPTY.TXT not empty ({len(empty_data)} bytes)")

        large_f = large_dir.open("LARGE.TXT")
        try:
            large_data = large_f.read()
        finally:
            large_f.close()
        expected_large = large_payload()
        if large_data != expected_large:
            raise SystemExit(
                f"ERROR: LARGE.TXT content mismatch "
                f"(got {len(large_data)} bytes, expected {len(expected_large)})"
            )
    finally:
        vclose(fs)


def create_image(out: Path, force: bool) -> None:
    require_fattools()
    from FATtools.Volume import vopen, vclose
    from FATtools.mkfat import exfat_mkfs

    out = out.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)

    if out.exists() and not force:
        if out.stat().st_size == SIZE_BYTES:
            with open(out, "rb") as f:
                marker = f.read(11)[3:11]
            if marker == b"EXFAT   ":
                print(f"exFAT test image already present: {out}")
                print("Re-run with --force to recreate.")
                verify_image(out)
                print("Verification OK.")
                return
        print(f"Existing image looks invalid; recreating: {out}")

    tmp = out.with_suffix(out.suffix + ".tmp")
    if tmp.exists():
        tmp.unlink()

    print(f"Creating {SIZE_BYTES} byte raw image: {tmp}")
    with open(tmp, "wb") as f:
        f.truncate(SIZE_BYTES)

    print("Formatting whole-disk exFAT (no partition table; KolibriOS .notmbr path)...")
    disk = vopen(str(tmp), "r+b", "disk")
    try:
        exfat_mkfs(disk, disk.size)
    finally:
        vclose(disk)

    print(f"Populating deterministic tree...")
    fs = vopen(str(tmp), "r+b")
    try:
        populate(fs)
    finally:
        vclose(fs)

    if out.exists():
        out.unlink()
    os.replace(tmp, out)
    print(f"Wrote {out}")

    verify_image(out)
    print("Verification OK:")
    print(f"  path:       {out}")
    print(f"  size:       {SIZE_BYTES} bytes (128 MiB)")
    print("  filesystem: exFAT (whole-disk)")
    print("  layout:     README/ROOT/EMPTY + DATA/ + TEST/A|B/ + LARGE/LARGE.TXT")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help=f"output image path (default: <repo>/{DEFAULT_REL.as_posix()})",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="recreate even if a valid image already exists",
    )
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="only verify an existing image; do not create",
    )
    args = parser.parse_args()

    root = repo_root_from_script()
    out = args.output if args.output is not None else root / DEFAULT_REL
    if not out.is_absolute():
        out = (Path.cwd() / out).resolve()

    if args.verify_only:
        require_fattools()
        verify_image(out)
        print(f"Verification OK: {out}")
        return 0

    create_image(out, force=args.force)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
