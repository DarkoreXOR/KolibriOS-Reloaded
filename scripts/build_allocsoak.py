"""Assemble disposable ``tools/allocsoak/allocsoak.asm`` with FASM."""

from __future__ import annotations

import argparse
from pathlib import Path

from common import load_config, log, resolve, run_cmd, setup_logging


def build_allocsoak() -> Path:
    cfg = load_config()
    fasm = resolve(cfg["kernel"]["fasm"])
    src = resolve("tools/allocsoak/allocsoak.asm")
    out_dir = resolve("dev_build/allocsoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / "ALLOCSOK"
    if not src.is_file():
        raise SystemExit(f"ERROR: allocsoak source missing: {src}")
    if not fasm.is_file():
        raise SystemExit(f"ERROR: FASM missing: {fasm}")
    if out.is_file():
        out.unlink()
    run_cmd([fasm, src, out], what="FASM allocsoak")
    if not out.is_file():
        raise SystemExit(f"ERROR: allocsoak binary missing: {out}")
    log.info("allocsoak built: %s (%s bytes)", out, out.stat().st_size)
    return out


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args(argv)
    setup_logging(args.verbose)
    build_allocsoak()


if __name__ == "__main__":
    main()
