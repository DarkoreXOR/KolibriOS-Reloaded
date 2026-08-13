"""Assemble disposable ``tools/ntfssoak/ntfssoak.asm`` with FASM."""

from __future__ import annotations

import argparse
import struct
import time
from pathlib import Path

from common import load_config, log, resolve, run_cmd, setup_logging

RUN_ID_PLACEHOLDER = 0xDEADBEEF


CONTROL_PLACEHOLDER = 0xC0A1C0A1


def patch_run_id(binary: Path, run_id: int | None = None) -> int:
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


def patch_control_mode(binary: Path, control: bool) -> None:
    data = bytearray(binary.read_bytes())
    needle = struct.pack("<I", CONTROL_PLACEHOLDER)
    replacement = struct.pack("<I", 1 if control else 0)
    count = data.count(needle)
    if count == 0:
        raise SystemExit(
            f"ERROR: control placeholder 0x{CONTROL_PLACEHOLDER:08X} not found in {binary}"
        )
    data = data.replace(needle, replacement, 1)
    binary.write_bytes(data)
    log.info("patched control=%s in %s", int(control), binary.name)


def build_ntfssoak(*, run_id: int | None = None, control: bool = False) -> tuple[Path, int]:
    cfg = load_config()
    fasm = resolve(cfg["kernel"]["fasm"])
    src = resolve("tools/ntfssoak/ntfssoak.asm")
    out_dir = resolve("dev_build/ntfssoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / "NTFSOAK1"
    if not src.is_file():
        raise SystemExit(f"ERROR: ntfssoak source missing: {src}")
    if not fasm.is_file():
        raise SystemExit(f"ERROR: FASM missing: {fasm}")
    if out.is_file():
        out.unlink()
    run_cmd([fasm, src, out], what="FASM ntfssoak")
    if not out.is_file():
        raise SystemExit(f"ERROR: ntfssoak binary missing: {out}")
    rid = patch_run_id(out, run_id)
    patch_control_mode(out, control)
    log.info("ntfssoak built: %s (%s bytes) run_id=0x%08X", out, out.stat().st_size, rid)
    return out, rid


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--run-id", type=lambda s: int(s, 0), default=None)
    args = ap.parse_args(argv)
    setup_logging(args.verbose)
    build_ntfssoak(run_id=args.run_id)


if __name__ == "__main__":
    main()
