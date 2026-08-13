"""Build a disposable CoW boot image with PE allocator soak driver.

Steps (reference image never mutated):
  1. prepare_image (kernel replace on CoW)
  2. build_asoakdrv → ``dev_build/allocsoak/ASOAKDRV`` (PE)
  3. build_allocsoak → ``dev_build/allocsoak/ALLOCSOK`` (MENUET loader)
  4. ``kolibri_img put`` both onto the CoW root
  5. Patch ``KERNEL.MNT`` ``/sys/LAUNCHER\\0`` → ``/sys/ALLOCSOK\\0``

Loader (ALLOCSOK) calls syscall 68.21 on ``/sys/ASOAKDRV``, which runs the
AllocPage/FreePage/AllocPages soak inside driver START, then starts LAUNCHER.

Writes ``dev_build/last_image.txt`` and ``dev_build/allocsoak/recipe.json``.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from build_allocsoak import build_allocsoak
from build_asoakdrv import build_asoakdrv
from common import LAST_IMAGE_MARKER, load_config, log, resolve, run_cmd, setup_logging
from prepare_image import ensure_kolibri_img, prepare_image

FIRSTAPP_FROM = b"/sys/LAUNCHER\x00"
FIRSTAPP_TO = b"/sys/ALLOCSOK\x00"
assert len(FIRSTAPP_FROM) == len(FIRSTAPP_TO)


def patch_firstapp_in_kernel_mnt(image: Path, img_tool: Path) -> dict:
    """Extract KERNEL.MNT, patch firstapp path, replace on image.

    Idempotent: if already redirected to ALLOCSOK, leave as-is.
    """
    tmp_dir = resolve("dev_build/allocsoak")
    tmp_dir.mkdir(parents=True, exist_ok=True)
    host_mnt = tmp_dir / "KERNEL.MNT.patched"
    if host_mnt.is_file():
        host_mnt.unlink()
    run_cmd(
        [img_tool, "extract", image, "KERNEL.MNT", host_mnt],
        what="extract KERNEL.MNT",
    )
    data = bytearray(host_mnt.read_bytes())
    already = data.count(FIRSTAPP_TO)
    count = data.count(FIRSTAPP_FROM)
    if count == 0 and already > 0:
        return {
            "patched_occurrences": 0,
            "already_patched_occurrences": already,
            "from": FIRSTAPP_FROM.decode("ascii", "replace"),
            "to": FIRSTAPP_TO.decode("ascii", "replace"),
            "kernel_mnt_host": str(host_mnt).replace("\\", "/"),
        }
    if count == 0:
        raise SystemExit(
            "ERROR: /sys/LAUNCHER\\0 not found in KERNEL.MNT — cannot redirect firstapp"
        )
    data = data.replace(FIRSTAPP_FROM, FIRSTAPP_TO)
    host_mnt.write_bytes(data)
    run_cmd(
        [img_tool, "replace", image, "KERNEL.MNT", host_mnt],
        what="replace patched KERNEL.MNT",
    )
    return {
        "patched_occurrences": count,
        "already_patched_occurrences": already,
        "from": FIRSTAPP_FROM.decode("ascii", "replace"),
        "to": FIRSTAPP_TO.decode("ascii", "replace"),
        "kernel_mnt_host": str(host_mnt).replace("\\", "/"),
    }


def prepare_allocsoak_image(*, delete: bool | None = None) -> Path:
    cfg = load_config()
    image_path = prepare_image(cfg, delete=delete)
    drv_bin = build_asoakdrv()
    alloc_bin = build_allocsoak()
    img_tool = ensure_kolibri_img(cfg["image"])

    run_cmd(
        [img_tool, "put", image_path, "ASOAKDRV", drv_bin],
        what="kolibri_img put ASOAKDRV",
    )
    run_cmd(
        [img_tool, "put", image_path, "ALLOCSOK", alloc_bin],
        what="kolibri_img put ALLOCSOK",
    )
    patch_info = patch_firstapp_in_kernel_mnt(image_path, img_tool)

    recipe = {
        "schema": 2,
        "mode": "pe_driver",
        "image": str(image_path).replace("\\", "/"),
        "allocsoak_bin": str(alloc_bin).replace("\\", "/"),
        "asoakdrv_bin": str(drv_bin).replace("\\", "/"),
        "seed": "0x5047424D",
        "seed_ascii": "PGBM",
        "pressure_target": 512,
        "max_ledger": 2048,
        "max_oom_extra": 256,
        "firstapp_patch": patch_info,
        "public_interface": {
            "pe_exports": ["AllocPage", "FreePage", "AllocPages"],
            "load": "syscall 68.21 /sys/ASOAKDRV",
            "evidence": "SysMsgBoardStr ALLOCSOK markers (msg_board_data via QMP xp)",
            "desktop": "ALLOCSOK starts /sys/LAUNCHER after driver returns",
            "note": "START is cdecl; returns 0 so load_pe_driver frees image after soak",
        },
        "abi": {
            "AllocPage": "0-arg call/ret; EAX=phys page or 0 (gcc/plain); updates page_start",
            "FreePage": "EAX=phys page address; call/ret; may lower page_start; double-free BTS polarity",
            "AllocPages": "stdcall(count pages); count rounded up to multiple of 8; EAX=phys base or 0; page_start unchanged",
            "PE_fixups": "data fixups required — Kolibri maps away from ImageBase 0x400000",
        },
    }
    recipe_path = resolve("dev_build/allocsoak/recipe.json")
    recipe_path.write_text(json.dumps(recipe, indent=2) + "\n", encoding="utf-8")
    log.info("allocsoak recipe: %s", recipe_path)
    log.info("Boot image (allocsoak PE): %s", image_path)
    assert LAST_IMAGE_MARKER.is_file()
    return image_path


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--no-delete", action="store_true")
    args = ap.parse_args(argv)
    setup_logging(args.verbose)
    prepare_allocsoak_image(delete=False if args.no_delete else None)


if __name__ == "__main__":
    main()
