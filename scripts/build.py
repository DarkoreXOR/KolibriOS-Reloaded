"""Build the hybrid kernel (Rust blobs + FASM assemble)."""

from __future__ import annotations

import argparse
from typing import Any, Mapping

from assemble_kernel import assemble_kernel
from build_rust import build_rust
from common import load_config, log, setup_logging, write_mode_marker


def build(
    *,
    mode: str = "dev",
    skip_tests: bool = False,
    cfg: Mapping[str, Any] | None = None,
) -> None:
    cfg = cfg or load_config()
    modes = cfg.get("modes") or {}
    if mode not in modes:
        raise SystemExit(
            f"ERROR: unknown build mode {mode!r} "
            f"(expected a [modes.{mode}] section in project/build.toml)"
        )
    write_mode_marker(mode, modes[mode])
    log.info("Building kernel (mode=%s)", mode)
    build_rust(mode=mode, skip_tests=skip_tests, cfg=cfg)
    assemble_kernel(cfg=cfg)
    log.info("Kernel built successfully (mode=%s)", mode)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        default="dev",
        help="Build mode from project/build.toml [modes.*] (default: dev)",
    )
    parser.add_argument("--release", action="store_true", help="Shorthand for --mode release")
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    mode = "release" if args.release else args.mode
    build(mode=mode, skip_tests=args.skip_tests)


if __name__ == "__main__":
    main()
