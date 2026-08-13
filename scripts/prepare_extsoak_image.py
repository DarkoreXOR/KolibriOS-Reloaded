"""Build a disposable CoW boot image + CoW EXT disk for SetFileInfo soak.

Steps (reference images never mutated):
  1. prepare_image (kernel replace on boot CoW)
  2. build_extsoak → ``dev_build/extsoak/EXTSOAK``
  3. ``kolibri_img put`` EXTSOAK onto the boot CoW
  4. Patch ``KERNEL.MNT`` ``/sys/LAUNCHER\\0`` → ``/sys/EXTSOAK\\0``
  5. Copy ``images/ext-image.img`` → ``dev_build/extsoak/ext-cow.img``

Writes ``dev_build/last_image.txt`` and ``dev_build/extsoak/recipe.json``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path

from build_extsoak import build_extsoak
from common import LAST_IMAGE_MARKER, load_config, log, resolve, run_cmd, setup_logging
from prepare_image import ensure_kolibri_img, prepare_image

FIRSTAPP_FROM = b"/sys/LAUNCHER\x00"
FIRSTAPP_TO = b"/sys/EXTSOAK1\x00"
assert len(FIRSTAPP_FROM) == len(FIRSTAPP_TO)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def patch_firstapp_in_kernel_mnt(image: Path, img_tool: Path) -> dict:
    tmp_dir = resolve("dev_build/extsoak")
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


def ensure_ext_reference() -> Path:
    ref = resolve("images/ext-image.img")
    if ref.is_file() and ref.stat().st_size > 0:
        return ref
    log.info("Creating persistent EXT reference via scripts/mkfs.py ext")
    run_cmd(["python", "scripts/mkfs.py", "ext"], what="mkfs ext")
    if not ref.is_file():
        raise SystemExit(f"ERROR: EXT reference missing after mkfs: {ref}")
    return ref


def prepare_ext_cow(ref: Path, cow: Path) -> dict:
    cow.parent.mkdir(parents=True, exist_ok=True)
    if cow.is_file():
        cow.unlink()
    shutil.copy2(ref, cow)
    return {
        "reference": str(ref).replace("\\", "/"),
        "cow": str(cow).replace("\\", "/"),
        "reference_sha256": sha256_file(ref),
        "cow_sha256_initial": sha256_file(cow),
        "size_bytes": cow.stat().st_size,
    }


def prepare_extsoak_image(*, delete: bool | None = None) -> tuple[Path, Path]:
    cfg = load_config()
    out_dir = resolve("dev_build/extsoak")
    out_dir.mkdir(parents=True, exist_ok=True)

    ref = ensure_ext_reference()
    cow = out_dir / "ext-cow.img"
    cow_info = prepare_ext_cow(ref, cow)

    image_path = prepare_image(cfg, delete=delete)
    extsoak_bin, run_id = build_extsoak()
    img_tool = ensure_kolibri_img(cfg["image"])

    run_cmd(
        [img_tool, "put", image_path, "EXTSOAK1", extsoak_bin],
        what="kolibri_img put EXTSOAK1",
    )
    patch_info = patch_firstapp_in_kernel_mnt(image_path, img_tool)

    from ext_setfileinfo_oracle import expected_primary_unix, read_inode_times

    baseline = read_inode_times(cow, "ROOT.TXT", try_debugfs=False)
    expected = expected_primary_unix()

    recipe = {
        "schema": 2,
        "mode": "menuet_ext_setfileinfo",
        "boot_image": str(image_path).replace("\\", "/"),
        "extsoak_bin": str(extsoak_bin).replace("\\", "/"),
        "run_id": run_id,
        "run_id_hex": f"0x{run_id:08X}",
        "firstapp_patch": patch_info,
        "ext_disk": cow_info,
        "target": {
            "guest_path": "/hd0/1/ROOT.TXT",
            "file_name": "ROOT.TXT",
            "inode_baseline": baseline.get("inode"),
            "baseline_atime": baseline.get("atime"),
            "baseline_mtime": baseline.get("mtime"),
        },
        "mutation": {
            "field": "INODE.aTime + INODE.mTime",
            "note": "ext_SetFileInfo ignores attrs/ctime; only atime@+16 and mtime@+24",
            "expected": expected,
        },
        "abi": {
            "syscall": 70,
            "get": "subfn 5, 40-byte BDFE out",
            "set": "subfn 6, 32-byte buffer; EXT uses +16 atime / +24 mtime BDFE",
            "sync": "ext_SetFileInfo → writeInode → writeSuperblock → disk_sync",
        },
        "public_interface": {
            "evidence_file": "/hd0/1/ESFI.LOG",
            "evidence_note": "durable on EXT CoW (not floppy); excluded from ROOT.TXT target diff",
            "msg_board": "EXTSOAK START|SET|PASS|FAIL|LOG|IMM|FIN",
            "desktop": "EXTSOAK starts /sys/LAUNCHER after report write",
        },
        "ab_future": {
            "off": "legacy FASM ext_SetFileInfo (current)",
            "on": "future Rust — NOT gated in this task",
        },
    }
    recipe_path = out_dir / "recipe.json"
    recipe_path.write_text(json.dumps(recipe, indent=2) + "\n", encoding="utf-8")
    baseline_path = out_dir / "baseline-inode.json"
    baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
    log.info("extsoak boot image: %s", image_path)
    log.info("extsoak EXT CoW: %s", cow)
    log.info("recipe: %s", recipe_path)
    return image_path, cow


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument(
        "--delete",
        action="store_true",
        default=None,
        help="pass through to prepare_image delete behavior",
    )
    args = ap.parse_args(argv)
    setup_logging(args.verbose)
    prepare_extsoak_image(delete=args.delete)


if __name__ == "__main__":
    main()
