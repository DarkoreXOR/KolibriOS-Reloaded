#!/usr/bin/env python3
"""Create or populate a deterministic EXT2 regression-test disk image.

Usage (from repository root):

    python tools/mkfs_utils/create_ext_image.py --size 64M
    python tools/mkfs_utils/create_ext_image.py --size 64M -o images/ext-image.img
    python tools/mkfs_utils/create_ext_image.py --force

Backends (tried in order):
    1. Native Linux: losetup + mkfs.ext2 + mount (requires root / CAP_SYS_ADMIN)
    2. Docker privileged container with e2fsprogs (Windows / when no local mkfs)

KolibriOS ``ext2_create_partition`` probes whole-disk SB at partition LBA 2
(byte 1024), requires 512-byte sectors, magic 0xEF53, incompatible features
subseteq 0x22C2, and ``blocksTotal_hi == 0``. Plain EXT2 (no 64bit / metadata_csum
/ orphan_file) satisfies that contract.
"""

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import tempfile
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
    expected_root_names,
    large_payload,
)


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[2]


def parse_size(text: str) -> int:
    m = re.fullmatch(r"(\d+(?:\.\d+)?)([KMG]?)", text.strip().upper())
    if not m:
        raise SystemExit(f"ERROR: invalid size `{text}` (use 64M, 128M, …)")
    value = float(m.group(1))
    mult = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3}[m.group(2)]
    size = int(round(value * mult))
    if size <= 0:
        raise SystemExit(f"ERROR: size must be positive, got `{text}`")
    return size


def ext_sb_ok(path: Path) -> bool:
    """True if image has EXT magic at classic superblock offset 1024."""
    try:
        with open(path, "rb") as f:
            f.seek(1024)
            sb = f.read(0x60)
    except OSError:
        return False
    if len(sb) < 0x3A:
        return False
    magic = struct.unpack_from("<H", sb, 0x38)[0]
    return magic == 0xEF53


def write_tree(root: Path) -> None:
    (root / "README.TXT").write_text(README, encoding="ascii")
    (root / "ROOT.TXT").write_text(ROOT_TXT, encoding="ascii")
    (root / "EMPTY.TXT").write_bytes(EMPTY_TXT)
    (root / "TINY.BIN").write_bytes(SMALL_BIN)

    data = root / "DATA"
    data.mkdir(exist_ok=True)
    (data / "ONE.TXT").write_text(DATA_ONE, encoding="ascii")
    (data / "TWO.TXT").write_text(DATA_TWO, encoding="ascii")
    (data / "THREE.TXT").write_text(DATA_THREE, encoding="ascii")

    nested = root / "NESTED"
    (nested / "A").mkdir(parents=True, exist_ok=True)
    (nested / "B").mkdir(parents=True, exist_ok=True)
    (nested / "A" / "FILE_A1.TXT").write_text(NESTED_A1, encoding="ascii")
    (nested / "A" / "FILE_A2.TXT").write_text(NESTED_A2, encoding="ascii")
    (nested / "B" / "FILE_B1.TXT").write_text(NESTED_B1, encoding="ascii")
    (nested / "B" / "FILE_B2.TXT").write_text(NESTED_B2, encoding="ascii")

    large = root / "LARGE"
    large.mkdir(exist_ok=True)
    (large / "LARGE.TXT").write_bytes(large_payload())

    spaces = root / "FILES WITH SPACES"
    spaces.mkdir(exist_ok=True)
    (spaces / "HELLO WORLD.TXT").write_text(SPACE_CONTENT, encoding="ascii")


def verify_tree(root: Path) -> None:
    names = {p.name for p in root.iterdir()}
    missing = expected_root_names() - names
    if missing:
        raise SystemExit(f"ERROR: missing root entries after populate: {sorted(missing)}")
    readme = (root / "README.TXT").read_text(encoding="ascii")
    if "KolibriOS filesystem regression-test fixture" not in readme:
        raise SystemExit("ERROR: README.TXT content mismatch")
    if (root / "TINY.BIN").read_bytes() != SMALL_BIN:
        raise SystemExit("ERROR: TINY.BIN content mismatch")
    if (root / "LARGE" / "LARGE.TXT").read_bytes() != large_payload():
        raise SystemExit("ERROR: LARGE.TXT content mismatch")


def have_cmd(name: str) -> bool:
    return shutil.which(name) is not None


def mkfs_ext2_cmd(path: Path) -> list[str]:
    # Plain EXT2: Kolibri-compatible feature set (no 64bit / metadata_csum).
    return [
        "mkfs.ext2",
        "-F",
        "-b",
        "1024",
        "-I",
        "128",
        "-L",
        "kolibri",
        str(path),
    ]


def populate_native(path: Path, size_bytes: int, *, force: bool) -> None:
    if os.geteuid() != 0:  # type: ignore[attr-defined]
        raise PermissionError("native EXT populate requires root")
    if force or not path.is_file() or not ext_sb_ok(path):
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "wb") as f:
            f.truncate(size_bytes)
        subprocess.check_call(mkfs_ext2_cmd(path))
    elif path.stat().st_size != size_bytes:
        print(
            f"NOTE: keeping existing image size {path.stat().st_size} "
            f"(requested {size_bytes}); pass --force to reformat",
            file=sys.stderr,
        )

    loop = subprocess.check_output(["losetup", "-f", "--show", str(path)], text=True).strip()
    mnt = Path(tempfile.mkdtemp(prefix="kolibri-ext-"))
    try:
        subprocess.check_call(["mount", "-t", "ext2", loop, str(mnt)])
        for child in mnt.iterdir():
            if child.is_dir():
                shutil.rmtree(child)
            else:
                child.unlink()
        write_tree(mnt)
        verify_tree(mnt)
        subprocess.check_call(["sync"])
    finally:
        subprocess.call(["umount", str(mnt)])
        subprocess.call(["losetup", "-d", loop])
        shutil.rmtree(mnt, ignore_errors=True)


def populate_docker(path: Path, size_bytes: int, *, force: bool) -> None:
    if not have_cmd("docker"):
        raise SystemExit(
            "ERROR: Docker is required to populate EXT on this host "
            "(no local mkfs.ext2/losetup)."
        )
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file():
        with open(path, "wb") as f:
            f.truncate(size_bytes)

    script = Path(__file__).resolve().parent / "docker_populate_ext.sh"
    if not script.is_file():
        raise SystemExit(f"ERROR: missing helper script {script}")

    img_dir = path.parent
    img_name = path.name
    env = [
        "-e",
        f"IMG=/img/{img_name}",
        "-e",
        f"FORCE={'1' if force else '0'}",
        "-e",
        f"SIZE_BYTES={size_bytes}",
    ]
    cmd = [
        "docker",
        "run",
        "--rm",
        "--privileged",
        *env,
        "-v",
        f"{img_dir}:/img",
        "-v",
        f"{script}:/populate.sh:ro",
        "alpine:3.20",
        "sh",
        "/populate.sh",
    ]
    print("+", " ".join(cmd), file=sys.stderr)
    subprocess.check_call(cmd)
    if not ext_sb_ok(path):
        raise SystemExit(f"ERROR: EXT superblock missing after Docker populate: {path}")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", default="64M", help="Image size (default 64M)")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help="Output path (default images/ext-image.img)",
    )
    parser.add_argument("--force", action="store_true", help="Reformat even if valid")
    args = parser.parse_args(argv)

    size_bytes = parse_size(args.size)
    out = args.output or (repo_root_from_script() / "images" / "ext-image.img")

    if platform.system() == "Linux" and have_cmd("mkfs.ext2") and have_cmd("losetup"):
        try:
            populate_native(out, size_bytes, force=args.force)
            print(f"OK: {out} ({size_bytes} bytes, native)")
            return
        except PermissionError:
            print("NOTE: native EXT populate needs root; trying Docker", file=sys.stderr)

    populate_docker(out, size_bytes, force=args.force)
    print(f"OK: {out} ({out.stat().st_size} bytes, docker)")


if __name__ == "__main__":
    main()
