"""Verify host tools and paths from project/build.toml."""

from __future__ import annotations

import argparse

from common import find_qemu, load_config, log, resolve, setup_logging, which


def doctor() -> None:
    cfg = load_config()
    failed: list[str] = []

    def check(label: str, ok: bool) -> None:
        if ok:
            log.info("OK  %s", label)
        else:
            log.error("MISSING  %s", label)
            failed.append(label)

    check("cargo", which("cargo") is not None)
    check("python", which("python") is not None or which("python3") is not None)
    check("kernel asm", resolve(cfg["kernel"]["asm"]).is_file())
    check("FASM", resolve(cfg["kernel"]["fasm"]).is_file())
    check("base image", resolve(cfg["image"]["base_image"]).is_file())
    check(
        "extract generic script",
        resolve(cfg["rust"]["extract"]["generic_script"]).is_file(),
    )
    check("kolibri_img manifest", resolve(cfg["image"]["tool_manifest"]).is_file())
    check(
        "migration_gates tool",
        resolve("tools/migration_gates/apply_gates.py").is_file(),
    )

    qemu_ok = False
    try:
        find_qemu(cfg["qemu"]["executables"])
        qemu_ok = True
    except SystemExit:
        qemu_ok = False
    check("QEMU", qemu_ok)

    if failed:
        raise SystemExit(
            "ERROR: doctor found missing tools or paths: " + ", ".join(failed)
        )
    log.info("doctor: all checks passed")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    doctor()


if __name__ == "__main__":
    main()
