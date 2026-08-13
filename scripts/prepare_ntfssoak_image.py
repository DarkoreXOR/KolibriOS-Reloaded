"""Build a disposable CoW boot image + CoW NTFS disk for SetFileInfo soak.

Steps (reference images never mutated):
  1. prepare_image (kernel replace on boot CoW)
  2. build_ntfssoak → ``dev_build/ntfssoak/NTFSOAK1``
  3. ``kolibri_img put`` NTFSOAK1 onto the boot CoW
  4. Patch ``KERNEL.MNT`` ``/sys/LAUNCHER\\0`` → ``/sys/NTFSOAK1\\0``
  5. Copy ``images/ntfs-image.img`` → ``dev_build/ntfssoak/ntfs-cow.img``

Writes ``dev_build/last_image.txt`` and ``dev_build/ntfssoak/recipe.json``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from build_ntfssoak import build_ntfssoak
from common import LAST_IMAGE_MARKER, load_config, log, resolve, run_cmd, setup_logging
from prepare_image import ensure_kolibri_img, prepare_image

FIRSTAPP_FROM = b"/sys/LAUNCHER\x00"
FIRSTAPP_TO = b"/sys/NTFSOAK1\x00"
assert len(FIRSTAPP_FROM) == len(FIRSTAPP_TO)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def patch_firstapp_in_kernel_mnt(image: Path, img_tool: Path) -> dict:
    tmp_dir = resolve("dev_build/ntfssoak")
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
    }


def ensure_ntfs_reference(
    *,
    allow_minimal: bool = False,
    force_minimal: bool = False,
    recreate: bool = False,
) -> Path:
    """Return NTFS reference for soak CoW.

    Soak minimal fixtures live under ``dev_build/ntfssoak/`` only — never
    overwrite ``images/ntfs-image.img`` (shared ``--disk ntfs`` regression disk).
    """
    persistent = resolve("images/ntfs-image.img")
    minimal = resolve("dev_build/ntfssoak/ntfs-minimal-reference.img")

    if (
        not force_minimal
        and persistent.is_file()
        and persistent.stat().st_size > 0
    ):
        return persistent

    if allow_minimal or force_minimal:
        if recreate or not minimal.is_file() or minimal.stat().st_size == 0:
            log.info(
                "Creating disposable NTFS soak fixture at %s "
                "(corrected ntfs_minimal FILE/USA layout)",
                minimal,
            )
            minimal.parent.mkdir(parents=True, exist_ok=True)
            from tools.mkfs_utils.ntfs_minimal import format_minimal_ntfs
            from tools.mkfs_utils.test_tree import EMPTY_TXT, README, ROOT_TXT

            soak_files = {
                "README.TXT": README,
                "ROOT.TXT": ROOT_TXT,
                "EMPTY.TXT": EMPTY_TXT,
                "NSFI.LOG": b"\x00" * 256,
            }
            format_minimal_ntfs(minimal, 128 * 1024 * 1024, soak_files)
        return minimal

    raise SystemExit(
        "ERROR: images/ntfs-image.img missing.\n"
        "  Create a mountable reference: python scripts/mkfs.py ntfs --force "
        "(Windows: run elevated for diskpart)\n"
        "  Or soak-only minimal fixture: prepare_ntfssoak_image.py --force-minimal"
    )


def prepare_ntfs_cow(ref: Path, cow: Path) -> dict:
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


def prepare_ntfssoak_image(
    *,
    delete: bool | None = None,
    allow_minimal: bool = False,
    force_minimal: bool = False,
    control: bool = False,
) -> tuple[Path, Path]:
    cfg = load_config()
    out_dir = resolve("dev_build/ntfssoak")
    out_dir.mkdir(parents=True, exist_ok=True)

    ref = ensure_ntfs_reference(
        allow_minimal=allow_minimal or force_minimal,
        force_minimal=force_minimal,
        recreate=force_minimal,
    )
    cow = out_dir / "ntfs-cow.img"
    cow_info = prepare_ntfs_cow(ref, cow)

    image_path = prepare_image(cfg, delete=delete)
    ntfssoak_bin, run_id = build_ntfssoak(control=control)
    img_tool = ensure_kolibri_img(cfg["image"])

    run_cmd(
        [img_tool, "put", image_path, "NTFSOAK1", ntfssoak_bin],
        what="kolibri_img put NTFSOAK1",
    )
    patch_info = patch_firstapp_in_kernel_mnt(image_path, img_tool)

    from ntfs_setfileinfo_oracle import expected_primary_filetimes, parse_root_index_times, preflight_ntfs_soak_image

    baseline = parse_root_index_times(cow, "ROOT.TXT")
    expected = expected_primary_filetimes()
    preflight = preflight_ntfs_soak_image(cow, "ROOT.TXT")
    if not preflight.get("ok"):
        raise SystemExit(f"ERROR: NTFS soak image failed host preflight: {json.dumps(preflight, indent=2)}")

    recipe = {
        "schema": 1,
        "mode": "menuet_ntfs_setfileinfo",
        "boot_image": str(image_path).replace("\\", "/"),
        "ntfssoak_bin": str(ntfssoak_bin).replace("\\", "/"),
        "control": bool(control),
        "run_id": run_id,
        "run_id_hex": f"0x{run_id:08X}",
        "firstapp_patch": patch_info,
        "ntfs_disk": cow_info,
        "preflight": {
            "ok": preflight.get("ok"),
            "mft0_usa_offset": (preflight.get("mft0_walk") or {}).get("usa_offset"),
            "mft0_first_attr": (preflight.get("mft0_walk") or {}).get("first_attr"),
            "mft0_walk_ok": (preflight.get("mft0_walk") or {}).get("ok"),
            "target_mft_record": (preflight.get("target_index") or {}).get("mft_record"),
        },
        "target": {
            "guest_path": "/hd0/1/ROOT.TXT",
            "file_name": "ROOT.TXT",
            "mft_record_baseline": baseline.get("mft_record"),
            "baseline_accessed": baseline.get("file_accessed"),
            "baseline_modified": baseline.get("file_modified"),
        },
        "mutation": {
            "field": "$I30 index fileAccessed + fileModified (BDFE atime/mtime)",
            "note": "ntfs_SetFileInfo mutates parent directory index entry, not file MFT $STANDARD_INFORMATION",
            "expected": expected,
        },
        "abi": {
            "syscall": 70,
            "get": "subfn 5, 40-byte BDFE out",
            "set": "subfn 6, 32-byte buffer; preserve attrs+ctime from GetFileInfo; patch +16/+24",
            "sync": "ntfs_SetFileInfo → writeRecord → ntfsDone → disk_sync",
        },
        "public_interface": {
            "evidence_file": "/hd0/1/NSFI.LOG",
            "msg_board": "NTFSOAK START|SET|PASS|FAIL|LOG|IMM|FIN",
        },
    }
    recipe_path = out_dir / "recipe.json"
    recipe_path.write_text(json.dumps(recipe, indent=2) + "\n", encoding="utf-8")
    (out_dir / "baseline-index.json").write_text(
        json.dumps(baseline, indent=2) + "\n", encoding="utf-8"
    )
    log.info("ntfssoak boot image: %s", image_path)
    log.info("ntfssoak NTFS CoW: %s", cow)
    log.info("recipe: %s", recipe_path)
    return image_path, cow


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--delete", action="store_true", default=None)
    ap.add_argument(
        "--allow-minimal",
        action="store_true",
        help="allow ntfs_minimal.py image if images/ntfs-image.img is missing",
    )
    ap.add_argument(
        "--force-minimal",
        action="store_true",
        help="always recreate the disposable ntfs_minimal soak fixture (ignore images/ntfs-image.img)",
    )
    args = ap.parse_args(argv)
    setup_logging(args.verbose)
    prepare_ntfssoak_image(
        delete=args.delete,
        allow_minimal=args.allow_minimal,
        force_minimal=args.force_minimal,
    )


if __name__ == "__main__":
    main()
