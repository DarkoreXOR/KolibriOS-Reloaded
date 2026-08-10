#!/usr/bin/env python3
"""Create a deterministic NTFS regression-test disk image.

Usage (from repository root):

    python tools/mkfs_utils/create_ntfs_image.py --size 8M
    python tools/mkfs_utils/create_ntfs_image.py --size 8M -o images/ntfs-image.img --force

Backends (tried in order):
  1. mkfs.ntfs on a loop device (Linux, requires root)
  2. Windows diskpart + Format-Volume (optional, requires elevation; --use-diskpart)
  3. Pure-Python minimal NTFS formatter (default on Windows without elevation)

Minimum practical size is 8M (NTFS metadata overhead).
"""

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

from ntfs_minimal import format_minimal_ntfs
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
    large_payload,
)


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[2]


def parse_size(text: str) -> int:
    m = re.fullmatch(r"(\d+(?:\.\d+)?)([KMG]?)", text.strip().upper())
    if not m:
        raise SystemExit(f"ERROR: invalid size `{text}`")
    value = float(m.group(1))
    mult = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3}[m.group(2)]
    size = int(round(value * mult))
    if size <= 0:
        raise SystemExit("ERROR: size must be positive")
    return size


def oem_ntfs_ok(path: Path) -> bool:
    with open(path, "rb") as f:
        boot = f.read(11)
    return boot[3:11] == b"NTFS    "


def find_mkfs_ntfs() -> str | None:
    for name in ("mkfs.ntfs", "mkntfs"):
        found = shutil.which(name)
        if found:
            return found
    return None


def regression_files() -> dict[str, bytes | str]:
    return {
        "README.TXT": README,
        "ROOT.TXT": ROOT_TXT,
        "EMPTY.TXT": EMPTY_TXT,
        "TINY.BIN": SMALL_BIN,
        "DATA/ONE.TXT": DATA_ONE,
        "DATA/TWO.TXT": DATA_TWO,
        "DATA/THREE.TXT": DATA_THREE,
        "NESTED/A/FILE_A1.TXT": NESTED_A1,
        "NESTED/A/FILE_A2.TXT": NESTED_A2,
        "NESTED/B/FILE_B1.TXT": NESTED_B1,
        "NESTED/B/FILE_B2.TXT": NESTED_B2,
        "LARGE/LARGE.TXT": large_payload(),
        SPACE_NAME: SPACE_CONTENT,
    }


def populate_tree(root: Path) -> None:
    for rel, data in regression_files().items():
        p = root / rel.replace("/", os.sep)
        p.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(data, str):
            p.write_text(data, encoding="ascii", newline="\n")
        else:
            p.write_bytes(data)


def create_ntfs_minimal(out: Path, size_bytes: int) -> None:
    if size_bytes < 8 * 1024 * 1024:
        size_bytes = 8 * 1024 * 1024
    format_minimal_ntfs(out, size_bytes, regression_files())
    if not oem_ntfs_ok(out):
        raise SystemExit(f"ERROR: minimal NTFS formatter produced invalid boot sector: {out}")


def create_ntfs_linux(out: Path, size_bytes: int) -> bool:
    mkfs = find_mkfs_ntfs()
    if not mkfs or platform.system() != "Linux":
        return False

    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "disk.img"
        with open(raw, "wb") as f:
            f.truncate(size_bytes)

        setup = subprocess.run(
            ["losetup", "-f", "--show", str(raw)],
            capture_output=True,
            text=True,
        )
        if setup.returncode != 0:
            return False
        loop = setup.stdout.strip()
        try:
            subprocess.run([mkfs, "-f", loop], check=True)
            mount = Path(tmp) / "mnt"
            mount.mkdir()
            subprocess.run(["mount", loop, str(mount)], check=True)
            try:
                populate_tree(mount)
            finally:
                subprocess.run(["umount", str(mount)], check=True)
        finally:
            subprocess.run(["losetup", "-d", loop], check=False)

        shutil.copy2(raw, out)
    return True


def run_diskpart(script: str) -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as f:
        f.write(script)
        script_path = f.name
    try:
        proc = subprocess.run(
            ["diskpart", "/s", script_path],
            capture_output=True,
            text=True,
        )
        if proc.returncode != 0:
            raise SystemExit(
                f"ERROR: diskpart failed (exit {proc.returncode})\n"
                f"{proc.stdout}\n{proc.stderr}"
            )
    finally:
        os.unlink(script_path)


def create_ntfs_windows_diskpart(out: Path, size_bytes: int) -> None:
    if size_bytes < 8 * 1024 * 1024:
        size_bytes = 8 * 1024 * 1024

    with tempfile.TemporaryDirectory() as tmp:
        vhd = Path(tmp) / "kolibri_ntfs.vhd"
        vhd_str = str(vhd)

        create_script = textwrap.dedent(
            f"""
            create vdisk file={vhd_str} maximum={size_bytes} type=fixed
            select vdisk file={vhd_str}
            attach vdisk
            create partition primary
            format fs=ntfs quick label=KOLIBRI
            assign letter=K
            """
        )
        run_diskpart(create_script)

        mount = Path("K:/")
        if not mount.exists():
            raise SystemExit("ERROR: could not mount NTFS volume at K:")

        populate_tree(mount)

        detach_script = textwrap.dedent(
            f"""
            select vdisk file={vhd_str}
            detach vdisk
            """
        )
        run_diskpart(detach_script)

        disk_bytes = vhd.stat().st_size - 512
        with open(vhd, "rb") as src, open(out, "wb") as dst:
            dst.write(src.read(disk_bytes))

    if not oem_ntfs_ok(out):
        raise SystemExit(f"ERROR: output does not look like NTFS: {out}")


def create_image(out: Path, size_bytes: int, force: bool, use_diskpart: bool) -> str:
    out = out.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)

    if out.exists() and not force:
        if out.stat().st_size >= 8 * 1024 * 1024 and oem_ntfs_ok(out):
            print(f"reused: {out}")
            return "reused"
        print(f"Existing image invalid; recreating: {out}")

    tmp = out.with_suffix(out.suffix + ".tmp")
    if tmp.exists():
        tmp.unlink()

    if create_ntfs_linux(tmp, size_bytes):
        pass
    elif use_diskpart and platform.system() == "Windows":
        try:
            create_ntfs_windows_diskpart(tmp, size_bytes)
        except (SystemExit, OSError) as e:
            print(f"diskpart backend failed ({e}); falling back to minimal NTFS", file=sys.stderr)
            create_ntfs_minimal(tmp, size_bytes)
    else:
        create_ntfs_minimal(tmp, size_bytes)

    if out.exists():
        out.unlink()
    os.replace(tmp, out)
    print(f"created: {out}")
    return "created"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-o", "--output", type=Path, default=None)
    parser.add_argument("--size", required=True)
    parser.add_argument("--force", action="store_true")
    parser.add_argument(
        "--use-diskpart",
        action="store_true",
        help="try Windows diskpart backend (requires administrator elevation)",
    )
    args = parser.parse_args()

    size_bytes = parse_size(args.size)
    root = repo_root_from_script()
    out = args.output if args.output else root / "images" / "ntfs-image.img"
    if not out.is_absolute():
        out = (Path.cwd() / out).resolve()

    outcome = create_image(out, size_bytes, args.force, args.use_diskpart)
    print(f"outcome: {outcome}")
    print(f"  size: {out.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
