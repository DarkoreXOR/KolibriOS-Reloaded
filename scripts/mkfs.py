"""Create or reuse persistent filesystem images under images/."""

from __future__ import annotations

import argparse
import platform

from common import find_python, log, resolve, run_cmd, setup_logging

SCRIPTS = {
    "exfat": "tools/mkfs_utils/create_exfat_image.py",
    "ntfs": "tools/mkfs_utils/create_ntfs_image.py",
    "xfs": "tools/mkfs_utils/create_xfs_image.py",
    "ext": "tools/mkfs_utils/create_ext_image.py",
}

DEFAULTS = (("exfat", "128M"), ("ntfs", "128M"), ("xfs", "1G"), ("ext", "64M"))


def mkfs_one(fs: str, size: str, *, force: bool = False) -> None:
    fs = fs.lower()
    if fs not in SCRIPTS:
        raise SystemExit(
            f"ERROR: unsupported filesystem {fs!r} (expected exfat, ntfs, xfs, or ext)"
        )
    script = resolve(SCRIPTS[fs])
    py = find_python()
    argv = [py, script, "--size", size]
    if force:
        argv.append("--force")
    # Windows NTFS needs Administrator diskpart; Linux uses mkfs.ntfs+loop.
    if fs == "ntfs" and platform.system() == "Windows":
        argv.append("--use-diskpart")
        log.info(
            "NTFS on Windows uses diskpart "
            "(UAC prompt if this shell is not already Administrator)"
        )
    # XFS / EXT on Windows use Docker + mkfs tools (create_*_image.py auto-selects).
    if fs == "xfs" and platform.system() == "Windows":
        log.info("XFS populate uses Docker privileged + xfsprogs on Windows")
    if fs == "ext" and platform.system() == "Windows":
        log.info("EXT populate uses Docker privileged + e2fsprogs on Windows")
    log.info("mkfs %s %s", fs, size)
    run_cmd(argv, what=f"mkfs {fs}")
    log.info("mkfs %s completed", fs)


def mkfs(
    fs: str | None = None,
    size: str | None = None,
    *,
    force: bool = False,
) -> None:
    if fs is None:
        log.info("Creating default exFAT, NTFS, and XFS images under images/")
        for name, default_size in DEFAULTS:
            mkfs_one(name, default_size, force=force)
        return
    if size is None:
        size = dict(DEFAULTS).get(fs.lower(), "128M")
    mkfs_one(fs, size, force=force)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fs", nargs="?", help="Filesystem: exfat, ntfs, xfs, or ext")
    parser.add_argument("size", nargs="?", help="Size, e.g. 128M / 1G (default per FS)")
    parser.add_argument("--force", action="store_true", help="Recreate even if valid")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    mkfs(args.fs, args.size, force=args.force)


if __name__ == "__main__":
    main()
