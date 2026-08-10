"""Boot the immutable reference floppy with -snapshot (no rebuild).

Optional disks: --disk exfat --disk ntfs.

The stock reference kernel leaves PCI/IDE ``/hd*`` support thin and keeps
**BIOS disks off by default**. Attached QEMU ``-hda``/``-hdb`` images are
exposed through BIOS int 13h as guest ``/bd0/1``, ``/bd1/1`` once BIOS disks
are enabled.

When ``--disk`` (or ``--testdisk``) is used, this script builds a disposable
CoW under ``dev_build/`` and injects ``config.ini`` with ``biosdisks=on`` so
the boot menu does not need a manual ``[b]`` toggle. The read-only reference
``kolibrios-*.img`` is never modified.

Stock reference may still lack exFAT (and possibly NTFS) drivers — browse
filesystem contents with the hybrid kernel via ``python scripts/regression.py``
(guest ``/hd0/1``, ``/hd1/1``).
"""

from __future__ import annotations

import argparse
from typing import Any, Mapping, Sequence

from common import find_qemu, load_config, log, resolve, run_cmd, run_interactive, setup_logging
from prepare_image import ensure_kolibri_img
from run_qemu import build_qemu_argv

REF_DISK_COW = "dev_build/reference-biosdisks.img"
REF_CONFIG_HOST = "dev_build/reference-config.ini"


def prepare_reference_with_biosdisks(cfg: Mapping[str, Any]) -> Path:
    """CoW the reference floppy and ensure config.ini enables BIOS disks."""
    image = cfg["image"]
    base = resolve(image["base_image"])
    if not base.is_file():
        raise SystemExit(f"ERROR: reference image missing: {base}")

    cow = resolve(REF_DISK_COW)
    cow.parent.mkdir(parents=True, exist_ok=True)
    img_tool = ensure_kolibri_img(image)

    if not cow.is_file() or cow.stat().st_size != base.stat().st_size:
        if cow.is_file():
            cow.unlink()
        log.info("Creating reference CoW for BIOS disks: %s", cow)
        run_cmd([img_tool, "cow", base, cow], what="kolibri_img cow")

    cfg_host = resolve(REF_CONFIG_HOST)
    cfg_host.write_text("biosdisks=on\n", encoding="ascii")
    run_cmd(
        [img_tool, "put", cow, "config.ini", cfg_host],
        what="kolibri_img put config.ini",
    )
    cfg_host.unlink(missing_ok=True)
    return cow


def reference_qemu(
    *,
    disks: Sequence[str] | None = None,
    memory: str | None = None,
    serial: bool = False,
    debug: bool = False,
    headless: bool = False,
    use_testdisk: bool = False,
    bus: str = "ide",
    cfg: Mapping[str, Any] | None = None,
) -> int:
    cfg = cfg or load_config()
    base = resolve(cfg["image"]["base_image"])
    if not base.is_file():
        raise SystemExit(f"ERROR: reference image missing: {base}")

    disks = list(disks or [])
    attach = bool(disks) or use_testdisk
    # Stock reference: enable BIOS disks via config.ini on a disposable CoW.
    image_path = prepare_reference_with_biosdisks(cfg) if attach else base

    qemu = find_qemu(cfg["qemu"]["executables"])
    qargs = build_qemu_argv(
        cfg,
        image_path=image_path,
        disks=disks,
        memory=memory,
        serial=serial,
        debug=debug,
        headless=headless,
        extra_args=cfg["qemu"].get("reference_extra_args") or [],
        use_testdisk=use_testdisk,
        bus=bus,
    )

    log.info("Starting QEMU (reference snapshot): %s", image_path)
    if attach:
        log.info(
            "FS disks appear in Eolite as /bd0/1, /bd1/1, … "
            "(stock BIOS disks; not /hd*). "
            "For exFAT/NTFS browsing use: python scripts/regression.py"
        )
    log.info("qemu %s", " ".join(qargs))
    code = run_interactive([qemu, *qargs], what="QEMU ref")
    if code != 0:
        raise SystemExit(f"ERROR: QEMU ref failed: exit {code}")
    return code


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--disk",
        action="append",
        default=[],
        metavar="TYPE",
        help="Attach images/TYPE-image.img|.iso (repeatable: exfat, ntfs, iso9660)",
    )
    parser.add_argument(
        "--bus",
        choices=("ide", "ahci"),
        default="ide",
        help="Disk bus for QEMU attach (IDE recommended for stock BIOS disks)",
    )
    parser.add_argument(
        "--testdisk",
        action="store_true",
        help="Attach [testdisk] from project/build.toml when no --disk is given",
    )
    parser.add_argument("--memory", default=None, help="Override QEMU -m size")
    parser.add_argument("--serial", action="store_true")
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    reference_qemu(
        disks=args.disk or None,
        memory=args.memory,
        serial=args.serial,
        debug=args.debug,
        headless=args.headless,
        use_testdisk=args.testdisk,
        bus=args.bus,
    )


if __name__ == "__main__":
    main()
