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

# QEMU maps bare `-cdrom` to IDE secondary master (same slot as `-hdc`,
# if=ide,index=2). Hard disks attached alongside `--disk iso9660` must skip
# that unit or QEMU exits with "drive with bus=1, unit=0 (index=2) exists".
_IDE_CDROM_INDEX = 2

# Named `--disk TYPE` → `images/TYPE-image.<ext>`. Most FS disks use `.img`;
# ISO9660 fixtures are commonly shipped as `.iso`.
_NAMED_DISK_SUFFIXES: dict[str, tuple[str, ...]] = {
    "iso9660": (".iso", ".img"),
}
_DEFAULT_DISK_SUFFIXES: tuple[str, ...] = (".img", ".iso")

# KolibriOS ISO9660 requires ATAPI 2048-byte sectors (`iso9660_create_partition`
# rejects SectorSize!=2048). Hard-disk `-hdX` is always 512 → must use `-cdrom`
# so the guest sees `/cdN` via the legacy ATAPI path.
_CDROM_DISK_TYPES = frozenset({"iso9660"})


def _disk_path_arg(img: Path) -> str:
    """Path for a standalone QEMU argv token (`-hda PATH` / `-cdrom PATH`)."""
    try:
        return str(img.resolve().relative_to(PROJECT_ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return str(img)


def append_ide_image(qargs: list[str], img: Path, index: int) -> None:
    if not img.is_file():
        raise SystemExit(f"ERROR: disk image missing: {img}")
    if index >= len(_HD_FLAGS):
        raise SystemExit(
            "ERROR: too many IDE hard disks (max 4, or 3 when --disk iso9660 "
            "occupies -hdc / index 2)"
        )
    flag = _HD_FLAGS[index]
    # Explicit format=raw avoids QEMU "probing guessed raw" warnings and
    # block-0 write restrictions on recent QEMU.
    path = qemu_opt_path(img)
    qargs.extend(
        [
            "-drive",
            f"file={path},format=raw,if=ide,index={index},media=disk",
        ]
    )
    log.info("Attaching %s (index %s) → /hd%s/1 : %s", flag, index, index, path)


def append_cdrom_image(qargs: list[str], img: Path) -> None:
    """Attach as IDE ATAPI CD-ROM (guest `/cdN`, 2048-byte sectors).

    Uses `-cdrom` (IDE index 2 / secondary master) so Kolibri sees ATAPI.
    """
    if not img.is_file():
        raise SystemExit(f"ERROR: disk image missing: {img}")
    if "-cdrom" in qargs:
        raise SystemExit("ERROR: only one --disk iso9660 / -cdrom is supported")
    path = _disk_path_arg(img)
    qargs.extend(["-cdrom", path])
    log.info(
        "Attaching -cdrom (IDE index %s) → /cdN (ATAPI ISO9660) : %s",
        _IDE_CDROM_INDEX,
        path,
    )


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
    """Attach hard-disk images only (512-byte sectors). Use attach_named_disks for --disk."""
    bus = bus.lower()
    if bus not in ("ide", "ahci"):
        raise SystemExit(f"ERROR: unknown disk bus {bus!r} (expected ide or ahci)")
    ahci_added = False
    for index, img in enumerate(images):
        if bus == "ide":
            append_ide_image(qargs, Path(img), index)
        else:
            ahci_added = append_ahci_image(qargs, Path(img), index, ahci_added)


def validate_named_disks(
    names: Sequence[str],
    *,
    create_ntfs_if_missing: bool = False,
) -> list[Path]:
    """Resolve disk paths or exit with mkfs hints (call before an expensive build)."""
    missing: list[str] = []
    resolved: list[Path] = []
    for name in names:
        suffixes = _NAMED_DISK_SUFFIXES.get(name, _DEFAULT_DISK_SUFFIXES)
        found: Path | None = None
        for suf in suffixes:
            cand = resolve(f"images/{name}-image{suf}")
            if cand.is_file():
                found = cand
                break
        if found is None:
            missing.append(name)
        else:
            resolved.append(found)
    if create_ntfs_if_missing and "ntfs" in missing:
        log.info(
            "NTFS image missing; creating images/ntfs-image.img "
            "(Administrator UAC prompt if needed)"
        )
        from mkfs import mkfs_one

        mkfs_one("ntfs", "128M")
        cand = resolve("images/ntfs-image.img")
        if cand.is_file():
            missing = [n for n in missing if n != "ntfs"]
            resolved.append(cand)
    if missing:
        lines = [
            "ERROR: missing regression disk image(s): "
            + ", ".join(missing),
            "",
        ]
        for name in missing:
            if name == "iso9660":
                lines.append(
                    f"  {name}: place images/{name}-image.iso (no mkfs generator yet)"
                )
            elif name == "ntfs":
                lines.append(
                    f"  {name}: python scripts/mkfs.py ntfs --force "
                    "(Windows: approve the UAC prompt for diskpart)"
                )
                soak = resolve("dev_build/ntfssoak/ntfs-minimal-reference.img")
                if soak.is_file():
                    lines.append(
                        f"    Note: soak-only minimal fixture at {soak} is NOT mountable "
                        "— do not copy it to images/ for --disk ntfs"
                    )
            else:
                lines.append(f"  {name}: python scripts/mkfs.py {name} --force")
        lines.append("")
        lines.append(
            "Workaround: omit missing types, e.g. drop --disk ntfs until the image exists."
        )
        raise SystemExit("\n".join(lines))
    return resolved


def resolve_named_disk(name: str) -> Path:
    suffixes = _NAMED_DISK_SUFFIXES.get(name, _DEFAULT_DISK_SUFFIXES)
    for suf in suffixes:
        cand = resolve(f"images/{name}-image{suf}")
        if cand.is_file():
            return cand
    tried = ", ".join(f"images/{name}-image{s}" for s in suffixes)
    hint = (
        f"Place the image at images/{name}-image.iso (or .img)."
        if name == "iso9660"
        else f"Create with: python scripts/mkfs.py {name} …"
    )
    raise SystemExit(f"ERROR: disk image missing for {name} (tried {tried})\n{hint}")


def resolve_named_disks(names: Sequence[str]) -> list[Path]:
    return [resolve_named_disk(name) for name in names]


def _next_ide_hd_index(index: int, *, reserve_cdrom_slot: bool) -> int:
    """Advance past the IDE unit reserved by QEMU `-cdrom` when needed."""
    if reserve_cdrom_slot and index == _IDE_CDROM_INDEX:
        return index + 1
    return index


def attach_named_disks(
    qargs: list[str],
    names: Sequence[str],
    *,
    bus: str = "ide",
) -> None:
    """Attach `--disk TYPE` images; ISO9660 → `-cdrom`, others → IDE/AHCI HD."""
    bus = bus.lower()
    if bus not in ("ide", "ahci"):
        raise SystemExit(f"ERROR: unknown disk bus {bus!r} (expected ide or ahci)")
    reserve_cdrom = any(n in _CDROM_DISK_TYPES for n in names)
    hd_index = 0
    ahci_added = False
    for name in names:
        img = resolve_named_disk(name)
        if name in _CDROM_DISK_TYPES:
            # Always IDE ATAPI: Kolibri /cdN path (SATAPI incomplete).
            append_cdrom_image(qargs, img)
            continue
        if bus == "ide":
            hd_index = _next_ide_hd_index(hd_index, reserve_cdrom_slot=reserve_cdrom)
            append_ide_image(qargs, img, hd_index)
            hd_index += 1
        else:
            ahci_added = append_ahci_image(qargs, img, hd_index, ahci_added)
            hd_index += 1


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

    `--disk iso9660` attaches as `-cdrom` (guest `/cdN` via ATAPI). Attaching
    an ISO as a hard disk fails: SectorSize 512 ≠ CDBlockSize 2048.
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
        attach_named_disks(qargs, disks, bus=bus)
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

    guest_hd = "/hdN/1" if bus == "ide" else "/sdN/1"
    has_cd = bool(disks and any(n in _CDROM_DISK_TYPES for n in disks))
    if has_cd:
        log.info(
            "Starting QEMU (HD → %s; ISO9660 → /cdN via ATAPI)",
            guest_hd,
        )
    else:
        log.info("Starting QEMU (FS disks appear in Eolite as %s, …)", guest_hd)
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
        help="Attach images/TYPE-image.img|.iso (repeatable: exfat, ntfs, xfs, iso9660→cdrom)",
    )
    parser.add_argument(
        "--bus",
        choices=("ide", "ahci"),
        default="ide",
        help="HD bus: ide → /hdN/1 (default); ahci → /sdN/1. iso9660 always uses -cdrom",
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
