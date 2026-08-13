"""Apply migration gates and assemble kernel.mnt with FASM."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any, Mapping

from common import (
    CONFIG_PATH,
    find_python,
    load_config,
    log,
    resolve,
    run_cmd,
    setup_logging,
)


def assemble_kernel(cfg: Mapping[str, Any] | None = None) -> Path:
    cfg = cfg or load_config()
    rust = cfg["rust"]
    kernel = cfg["kernel"]
    out_dir = resolve(rust["out_dir"])

    for blob in rust["blobs"]:
        blob_path = out_dir / blob["out"]
        if not blob_path.is_file():
            raise SystemExit(
                f"ERROR: Rust blob missing before kernel assemble: {blob_path}\n"
                "Run: python scripts/build_rust.py"
            )

    log.info("Applying migration gates")
    py = find_python()
    gates = resolve("tools/migration_gates/apply_gates.py")
    run_cmd([py, gates, str(CONFIG_PATH)], what="apply_gates")

    fasm = resolve(kernel["fasm"])
    asm = resolve(kernel["asm"])
    out = resolve(kernel["output"])
    lang = kernel["lang"]
    mem = kernel["memory_kib"]

    if not asm.is_file():
        raise SystemExit(f"ERROR: kernel asm missing: {asm}")
    if not fasm.is_file():
        raise SystemExit(f"ERROR: FASM missing: {fasm}")

    out.parent.mkdir(parents=True, exist_ok=True)
    if out.is_file():
        out.unlink()
    # Host-only symbol dump for QMP allocator soak (VA→PA resolution).
    fas = out.with_suffix(".fas")
    if fas.is_file():
        fas.unlink()

    lang_path = resolve("kernel/lang.inc")
    try:
        # FASM expects exactly: `lang fix en_US` + newline.
        lang_path.write_text(f"lang fix {lang}\n", encoding="ascii")
        log.info("Assembling kernel with FASM")
        run_cmd(
            [fasm, "-m", str(mem), asm, out, "-s", fas],
            what="FASM assemble",
        )
    finally:
        if lang_path.is_file():
            lang_path.unlink()

    if not out.is_file():
        raise SystemExit(f"ERROR: kernel artifact missing after assemble: {out}")
    if not fas.is_file():
        raise SystemExit(f"ERROR: FASM symbols dump missing after assemble: {fas}")

    log.info("Kernel built: %s (symbols: %s)", out, fas)
    return out


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    assemble_kernel()


if __name__ == "__main__":
    main()
