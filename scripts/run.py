"""Build -> package -> QEMU (default developer pipeline)."""

from __future__ import annotations

import argparse

from build import build
from common import setup_logging
from prepare_image import prepare_image
from run_qemu import run_qemu


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", default="dev", help="Build mode (default: dev)")
    parser.add_argument("--release", action="store_true", help="Shorthand for --mode release")
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument(
        "--disk",
        action="append",
        default=[],
        metavar="TYPE",
        help="Attach images/TYPE-image.img|.iso (repeatable)",
    )
    parser.add_argument(
        "--bus",
        choices=("ide", "ahci"),
        default="ide",
        help="HD bus: ide -> /hdN/1 (default); ahci -> /sdN/1",
    )
    parser.add_argument("--memory", default=None)
    parser.add_argument("--serial", action="store_true")
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)

    mode = "release" if args.release else args.mode
    build(mode=mode, skip_tests=args.skip_tests)
    prepare_image()
    run_qemu(
        disks=args.disk or None,
        memory=args.memory,
        serial=args.serial,
        debug=args.debug,
        headless=args.headless,
        bus=args.bus,
    )


if __name__ == "__main__":
    main()
