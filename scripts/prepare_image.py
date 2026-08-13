"""Package a disposable CoW boot image with the built kernel."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
from typing import Any, Mapping

from common import (
    LAST_IMAGE_MARKER,
    PROJECT_ROOT,
    load_config,
    log,
    read_mode_marker,
    resolve,
    run_cmd,
    setup_logging,
    utc_stamp,
    which,
)


def ensure_kolibri_img(image: Mapping[str, Any]) -> Path:
    tool_bin = resolve(image["tool_bin"])
    alt = resolve("tools/kolibri_img/target/release/kolibri_img")
    if tool_bin.is_file():
        return tool_bin
    if alt.is_file():
        return alt
    # Unix may omit .exe
    if tool_bin.suffix.lower() == ".exe":
        no_ext = tool_bin.with_suffix("")
        if no_ext.is_file():
            return no_ext

    log.info("Building kolibri_img (release)")
    cargo = which("cargo")
    if cargo is None:
        raise SystemExit("ERROR: 'cargo' not found on PATH")
    # Cursor/sandbox may set CARGO_TARGET_DIR to a shared cache; force the
    # tool-local target so project/build.toml tool_bin stays correct.
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(resolve("tools/kolibri_img/target"))
    run_cmd(
        [
            cargo,
            "build",
            "--release",
            "--manifest-path",
            resolve(image["tool_manifest"]),
        ],
        env=env,
        what="kolibri_img build",
    )
    for cand in (tool_bin, alt, tool_bin.with_suffix("")):
        if cand.is_file():
            return cand
    raise SystemExit("ERROR: kolibri_img binary missing after build")


def resolve_output_dir(cfg: Mapping[str, Any], image: Mapping[str, Any]) -> Path:
    out_dir = resolve(image["output_dir"])
    mode = read_mode_marker(cfg)
    if mode:
        mc = (cfg.get("modes") or {}).get(mode) or {}
        if mc.get("image_output_dir"):
            out_dir = resolve(mc["image_output_dir"])
            log.info("prepare_image mode=%s output_dir=%s", mode, out_dir)
    return out_dir


def prepare_image(
    cfg: Mapping[str, Any] | None = None,
    *,
    delete: bool | None = None,
) -> Path:
    cfg = cfg or load_config()
    image = cfg["image"]
    kernel = resolve(cfg["kernel"]["output"])

    if not kernel.is_file():
        raise SystemExit(
            f"ERROR: kernel artifact missing; cannot package image: {kernel}\n"
            "Run: python scripts/build.py"
        )
    base = resolve(image["base_image"])
    if not base.is_file():
        raise SystemExit(f"ERROR: base/reference image missing: {base}")

    img_tool = ensure_kolibri_img(image)
    out_dir = resolve_output_dir(cfg, image)
    out_dir.mkdir(parents=True, exist_ok=True)

    fname = image["filename_pattern"].replace("{timestamp}", utc_stamp())
    image_path = out_dir / fname

    log.info("Creating disk image %s", image_path)
    run_cmd([img_tool, "cow", base, image_path], what="kolibri_img cow")

    do_delete = bool(image.get("delete_before_replace_enabled", True))
    if delete is not None:
        do_delete = delete
    if do_delete:
        for del_path in image.get("delete_before_replace") or []:
            log.info("delete %s (skip if missing)", del_path)
            run_cmd(
                [img_tool, "delete", "--ignore-missing", image_path, del_path],
                what=f"kolibri_img delete {del_path}",
            )
    else:
        log.info("skip CoW deletes (delete_before_replace disabled)")

    fat_name = image["kernel_fat_name"]
    log.info("replace %s", fat_name)
    run_cmd(
        [img_tool, "replace", image_path, fat_name, kernel],
        what="kolibri_img replace",
    )

    if not image_path.is_file():
        raise SystemExit(f"ERROR: test image missing after packaging: {image_path}")

    LAST_IMAGE_MARKER.parent.mkdir(parents=True, exist_ok=True)
    # Store repo-relative path when possible for portability.
    try:
        rel = image_path.resolve().relative_to(PROJECT_ROOT)
        LAST_IMAGE_MARKER.write_text(str(rel).replace("\\", "/"), encoding="utf-8")
    except ValueError:
        LAST_IMAGE_MARKER.write_text(str(image_path), encoding="utf-8")

    log.info("Boot image prepared: %s", image_path)
    return image_path


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verbose", action="store_true")
    parser.add_argument(
        "--no-delete",
        action="store_true",
        help="Do not delete authorized free-space paths on the CoW copy",
    )
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    prepare_image(delete=False if args.no_delete else None)


if __name__ == "__main__":
    main()
