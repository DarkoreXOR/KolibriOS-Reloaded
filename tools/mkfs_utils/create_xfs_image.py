#!/usr/bin/env python3
"""Create or populate a deterministic XFS regression-test disk image.

Usage (from repository root):

    python tools/mkfs_utils/create_xfs_image.py --size 1G
    python tools/mkfs_utils/create_xfs_image.py --size 1G -o images/xfs-image.img
    python tools/mkfs_utils/create_xfs_image.py --force   # reformat + populate

Backends (tried in order):
    1. Native Linux: losetup + mkfs.xfs + mount (requires root / CAP_SYS_ADMIN)
    2. Docker privileged container with xfsprogs (Windows / when no local xfs)

The default output is ``images/xfs-image.img``. Prefer reusing an already-valid
XFS image and only rewriting the fixture tree unless ``--force`` is passed.
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
        raise SystemExit(f"ERROR: invalid size `{text}` (use 128M, 1G, …)")
    value = float(m.group(1))
    mult = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3}[m.group(2)]
    size = int(round(value * mult))
    if size <= 0:
        raise SystemExit(f"ERROR: size must be positive, got `{text}`")
    return size


def xfs_sb_ok(path: Path) -> bool:
    try:
        with open(path, "rb") as f:
            sb = f.read(512)
    except OSError:
        return False
    if len(sb) < 112 or sb[0:4] != b"XFSB":
        return False
    blocksize = struct.unpack(">I", sb[4:8])[0]
    return blocksize in (512, 1024, 2048, 4096, 8192, 16384, 32768, 65536)


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


def populate_native(path: Path, size_bytes: int, *, force: bool) -> None:
    if os.geteuid() != 0:  # type: ignore[attr-defined]
        raise PermissionError("native XFS populate requires root")
    if force or not path.is_file() or not xfs_sb_ok(path):
        path.parent.mkdir(parents=True, exist_ok=True)
        # Sparse allocate then format.
        with open(path, "wb") as f:
            f.truncate(size_bytes)
        subprocess.check_call(["mkfs.xfs", "-f", "-m", "crc=1,finobt=1", str(path)])
    elif path.stat().st_size != size_bytes:
        # Keep existing FS; size change requires --force.
        print(
            f"NOTE: keeping existing image size {path.stat().st_size} "
            f"(requested {size_bytes}); pass --force to reformat",
            file=sys.stderr,
        )

    loop = subprocess.check_output(["losetup", "-f", "--show", str(path)], text=True).strip()
    mnt = Path(tempfile.mkdtemp(prefix="kolibri-xfs-"))
    try:
        subprocess.check_call(["mount", "-t", "xfs", loop, str(mnt)])
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
            "ERROR: Docker is required to populate XFS on this host "
            "(no local mkfs.xfs/losetup)."
        )
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file():
        with open(path, "wb") as f:
            f.truncate(size_bytes)

    script = Path(__file__).resolve().parent / "docker_populate_xfs.sh"
    if not script.is_file():
        raise SystemExit(f"ERROR: missing helper script {script}")

    # Mount the images directory so the host path stays writable.
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
    print("+", " ".join(cmd), flush=True)
    subprocess.check_call(cmd)


def verify_image(path: Path) -> None:
    if not xfs_sb_ok(path):
        raise SystemExit(f"ERROR: XFS superblock missing/invalid: {path}")
    print(f"OK: XFS image {path} ({path.stat().st_size} bytes)")


def main(argv: list[str] | None = None) -> None:
    root = repo_root_from_script()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", default="1G", help="Image size (default 1G)")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=root / "images" / "xfs-image.img",
        help="Output image path",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Reformat with mkfs.xfs before populating",
    )
    parser.add_argument(
        "--backend",
        choices=("auto", "native", "docker"),
        default="auto",
        help="Populate backend (default: auto)",
    )
    args = parser.parse_args(argv)
    size_bytes = parse_size(args.size)
    out: Path = args.output
    if not out.is_absolute():
        out = (Path.cwd() / out).resolve()

    backend = args.backend
    if backend == "auto":
        if platform.system() == "Linux" and have_cmd("mkfs.xfs") and have_cmd("losetup"):
            backend = "native"
        else:
            backend = "docker"

    print(f"XFS populate backend={backend} out={out} size={size_bytes} force={args.force}")
    if backend == "native":
        try:
            populate_native(out, size_bytes, force=args.force)
        except PermissionError:
            print("NOTE: native populate needs root; falling back to Docker", file=sys.stderr)
            populate_docker(out, size_bytes, force=args.force)
    else:
        populate_docker(out, size_bytes, force=args.force)

    verify_image(out)


if __name__ == "__main__":
    main()
