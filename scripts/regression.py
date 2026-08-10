"""Filesystem regression: ensure FS images, build, package, QEMU with AHCI disks."""

from __future__ import annotations

import argparse

from build import build
from common import setup_logging
from mkfs import mkfs
from prepare_image import prepare_image
from run_qemu import run_qemu


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument("--memory", default=None)
    parser.add_argument("--serial", action="store_true")
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)

    mkfs()
    build(mode="dev", skip_tests=args.skip_tests)
    prepare_image()
    run_qemu(
        disks=["exfat", "ntfs"],
        memory=args.memory,
        serial=args.serial,
        debug=args.debug,
        headless=args.headless,
    )


if __name__ == "__main__":
    main()
