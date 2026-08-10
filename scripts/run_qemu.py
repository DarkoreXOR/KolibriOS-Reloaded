"""Launch QEMU with the last packaged boot image (and optional FS disks)."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any, Mapping, Sequence

from common import (
    LAST_IMAGE_MARKER,
    PROJECT_ROOT,
    find_qemu,
    load_config,
    log,
    qemu_opt_path,
    resolve,
    run_interactive,
    setup_logging,
)

_HD_FLAGS = ("-hda", "-hdb", "-hdc", "-hdd")


def _disk_path_arg(img: Path) -> str:
    """Path for a standalone QEMU argv token (`-hda PATH`)."""
    try:
        return str(img.resolve().relative_to(PROJECT_ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return str(img)


def append_ide_image(qargs: list[str], img: Path, index: int) -> None:
    if not img.is_file():
        raise SystemExit(f"ERROR: disk image missing: {img}")
    if index >= len(_HD_FLAGS):
        raise SystemExit("ERROR: too many disks (max 4 IDE drives via -hda..-hdd)")
    flag = _HD_FLAGS[index]
    path = _disk_path_arg(img)
    qargs.extend([flag, path])
    log.info("Attaching %s → /hd%s/1 : %s", flag, index, path)


def append_ahci_image(qargs: list[str], img: Path, index: int, ahci_added: bool) -> bool:
    if not img.is_file():
        raise SystemExit(f"ERROR: disk image missing: {img}")
    if not ahci_added:
        qargs.extend(["-device", "ahci,id=kolibri_ahci"])
        ahci_added = True
    drive_id = f"fsdisk{index}"
    safe = qemu_opt_path(img)
    qargs.extend(
        [
            "-drive",
            f"if=none,id={drive_id},file={safe},format=raw",
            "-device",
            f"ide-hd,drive={drive_id},bus=kolibri_ahci.{index}",
        ]
    )
    log.info("Attaching AHCI port %s → /sd%s/1 : %s", index, index, img)
    return ahci_added


def attach_disk_images(
    qargs: list[str],
    images: Sequence[Path],
    *,
    bus: str = "ide",
) -> None:
    bus = bus.lower()
    if bus not in ("ide", "ahci"):
        raise SystemExit(f"ERROR: unknown disk bus {bus!r} (expected ide or ahci)")
    ahci_added = False
    for index, img in enumerate(images):
        if bus == "ide":
            append_ide_image(qargs, Path(img), index)
        else:
            ahci_added = append_ahci_image(qargs, Path(img), index, ahci_added)


def resolve_named_disks(names: Sequence[str]) -> list[Path]:
    images: list[Path] = []
    for name in names:
        img = resolve(f"images/{name}-image.img")
        if not img.is_file():
            raise SystemExit(
                f"ERROR: disk image missing for {name}: {img}\n"
                f"Create with: python scripts/mkfs.py {name} …"
            )
        images.append(img)
    return images


def build_qemu_argv(
    cfg: Mapping[str, Any],
    *,
    image_path: Path,
    disks: Sequence[str] | None = None,
    memory: str | None = None,
    serial: bool = False,
    debug: bool = False,
    headless: bool = False,
    extra_args: Sequence[str] | None = None,
    use_testdisk: bool = True,
    bus: str = "ide",
) -> list[str]:
    """Build QEMU argv.

    Disks default to IDE (`-hda`/`-hdb` → guest `/hd0/1`, `/hd1/1`) so the
    stock reference floppy and hybrid builds both see them. Use bus='ahci'
    for `/sdN/1` on kernels with AHCI support.
    """
    qemu_cfg = cfg["qemu"]
    testdisk = cfg.get("testdisk") or {}
    disks = list(disks or [])

    qargs: list[str] = ["-fda", str(image_path)]
    qargs.extend(str(a) for a in qemu_cfg.get("args") or [])
    if extra_args:
        qargs.extend(str(a) for a in extra_args)

    if memory:
        qargs.extend(["-m", memory])
    if serial:
        qargs.extend(["-serial", "stdio"])
    if debug:
        resolve("dev_build").mkdir(parents=True, exist_ok=True)
        qargs.extend(
            ["-d", "int,cpu_reset", "-D", str(resolve("dev_build/qemu-debug.log"))]
        )
    if headless:
        qargs.extend(str(a) for a in qemu_cfg.get("headless_extra_args") or [])

    if disks:
        attach_disk_images(qargs, resolve_named_disks(disks), bus=bus)
    elif use_testdisk and testdisk.get("enabled"):
        td = resolve(testdisk["image"])
        if td.is_file():
            attach_disk_images(qargs, [td], bus=bus)
        else:
            log.warning(
                "testdisk enabled but missing: %s — create with: "
                "python scripts/mkfs.py exfat 128M",
                td,
            )
    elif use_testdisk:
        log.warning("no --disk and testdisk disabled — booting floppy only")

    return qargs


def run_qemu(
    *,
    disks: Sequence[str] | None = None,
    memory: str | None = None,
    serial: bool = False,
    debug: bool = False,
    headless: bool = False,
    bus: str = "ide",
    cfg: Mapping[str, Any] | None = None,
) -> int:
    cfg = cfg or load_config()
    mnt = resolve("kernel/bin/kernel.mnt")
    if not LAST_IMAGE_MARKER.is_file():
        raise SystemExit(
            "ERROR: no fresh image marker (dev_build/last_image.txt) — "
            "run: python scripts/prepare_image.py"
        )
    if not mnt.is_file():
        raise SystemExit(
            "ERROR: refusing to launch QEMU — kernel.mnt missing; "
            "run: python scripts/build.py"
        )

    raw = LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip()
    image_path = resolve(raw)
    if not image_path.is_file():
        raise SystemExit(f"ERROR: recorded image missing: {image_path}")

    qemu = find_qemu(cfg["qemu"]["executables"])
    qargs = build_qemu_argv(
        cfg,
        image_path=image_path,
        disks=disks,
        memory=memory,
        serial=serial,
        debug=debug,
        headless=headless,
        bus=bus,
    )

    guest = "/hd0/1, /hd1/1" if bus == "ide" else "/sd0/1, /sd1/1"
    log.info("Starting QEMU (FS disks appear in Eolite as %s, …)", guest)
    log.info("qemu %s", " ".join(qargs))
    code = run_interactive([qemu, *qargs], what="QEMU")
    if code != 0:
        raise SystemExit(f"ERROR: QEMU run failed: exit {code}")

    if (cfg.get("cleanup") or {}).get("delete_image_on_success"):
        log.info("Cleanup: removing %s", image_path)
        image_path.unlink(missing_ok=True)
    return code


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--disk",
        action="append",
        default=[],
        metavar="TYPE",
        help="Attach images/TYPE-image.img (repeatable: exfat, ntfs)",
    )
    parser.add_argument(
        "--bus",
        choices=("ide", "ahci"),
        default="ide",
        help="Disk bus: ide → /hdN/1 (default, works on stock reference); ahci → /sdN/1",
    )
    parser.add_argument("--memory", default=None, help="Override QEMU -m size")
    parser.add_argument("--serial", action="store_true")
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    run_qemu(
        disks=args.disk,
        memory=args.memory,
        serial=args.serial,
        debug=args.debug,
        headless=args.headless,
        bus=args.bus,
    )


if __name__ == "__main__":
    main()
