"""Assemble disposable ``tools/extsoak/extsoak.asm`` with FASM."""

from __future__ import annotations

import argparse
import struct
import time
from pathlib import Path

from common import load_config, log, resolve, run_cmd, setup_logging

RUN_ID_PLACEHOLDER = 0xDEADBEEF


def patch_run_id(binary: Path, run_id: int | None = None) -> int:
    """Replace the ``0xDEADBEEF`` run_id placeholder in EXTSOAK1 with ``run_id``."""
    rid = int(run_id if run_id is not None else (int(time.time()) & 0xFFFFFFFF))
    data = bytearray(binary.read_bytes())
    needle = struct.pack("<I", RUN_ID_PLACEHOLDER)
    replacement = struct.pack("<I", rid & 0xFFFFFFFF)
    count = data.count(needle)
    if count == 0:
        raise SystemExit(
            f"ERROR: run_id placeholder 0x{RUN_ID_PLACEHOLDER:08X} not found in {binary}"
        )
    data = data.replace(needle, replacement, 1)
    binary.write_bytes(data)
    log.info("patched run_id=0x%08X in %s (matches=%s)", rid, binary.name, count)
    return rid & 0xFFFFFFFF


def build_extsoak(*, run_id: int | None = None) -> tuple[Path, int]:
    cfg = load_config()
    fasm = resolve(cfg["kernel"]["fasm"])
    src = resolve("tools/extsoak/extsoak.asm")
    out_dir = resolve("dev_build/extsoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / "EXTSOAK1"
    if not src.is_file():
        raise SystemExit(f"ERROR: extsoak source missing: {src}")
    if not fasm.is_file():
        raise SystemExit(f"ERROR: FASM missing: {fasm}")
    if out.is_file():
        out.unlink()
    run_cmd([fasm, src, out], what="FASM extsoak")
    if not out.is_file():
        raise SystemExit(f"ERROR: extsoak binary missing: {out}")
    rid = patch_run_id(out, run_id)
    log.info("extsoak built: %s (%s bytes) run_id=0x%08X", out, out.stat().st_size, rid)
    return out, rid


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--run-id", type=lambda s: int(s, 0), default=None)
    args = ap.parse_args(argv)
    setup_logging(args.verbose)
    build_extsoak(run_id=args.run_id)


if __name__ == "__main__":
    main()
