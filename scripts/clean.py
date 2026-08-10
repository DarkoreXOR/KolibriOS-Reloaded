"""Remove disposable build artifacts. Never touches images/."""

from __future__ import annotations

import argparse
import shutil
from common import PROJECT_ROOT, log, setup_logging


def clean(*, full: bool = False) -> None:
    """Clean artifacts.

    full=False (default): remove contents of dev_build/ except README.md.
    full=True: remove build/ and dev_build/ entirely (images/ preserved).
    """
    if full:
        log.info("Cleaning build artifacts (full)")
        for name in ("build", "dev_build"):
            path = PROJECT_ROOT / name
            if path.exists():
                shutil.rmtree(path)
                log.info("removed %s/", name)
        log.info("Clean completed (images/ preserved)")
        return

    log.info("Cleaning dev_build/*")
    root = PROJECT_ROOT / "dev_build"
    if not root.is_dir():
        log.info("dev_build/ absent — nothing to clean")
        return
    removed = 0
    for child in list(root.iterdir()):
        if child.name == "README.md":
            continue
        if child.is_file() or child.is_symlink():
            child.unlink()
        else:
            shutil.rmtree(child)
        removed += 1
        log.info("removed %s", child.relative_to(PROJECT_ROOT))
    log.info("dev_build clean: %s entries removed (README preserved)", removed)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--full",
        action="store_true",
        help="Remove build/ and dev_build/ entirely (default: only dev_build/*)",
    )
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    clean(full=args.full)


if __name__ == "__main__":
    main()
