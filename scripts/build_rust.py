"""Build freestanding kolibri_utils staticlib and extract reloc-free blobs."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
from typing import Any, Mapping

from common import (
    PROJECT_ROOT,
    find_python,
    load_config,
    log,
    resolve,
    run_cmd,
    setup_logging,
    which,
)

ARCHIVE_REL = Path("i686-kolibri-none") / "release" / "libkolibri_utils.a"


def _mode_overlay(cfg: Mapping[str, Any], rust: Mapping[str, Any], mode: str | None):
    run_host = bool(rust.get("run_host_tests", True))
    force = bool(rust.get("force_recompile_staticlib", True))
    if mode:
        mc = (cfg.get("modes") or {}).get(mode) or {}
        if "run_host_tests" in mc:
            run_host = bool(mc["run_host_tests"])
        if "force_recompile_staticlib" in mc:
            force = bool(mc["force_recompile_staticlib"])
        log.info("build_rust mode=%s", mode)
    return run_host, force


def _extract_blob(
    py: Path,
    archive: Path,
    out_dir: Path,
    blob: Mapping[str, Any],
    generic: Path,
    probe: Path,
) -> None:
    out = out_dir / blob["out"]
    kind = blob["kind"]
    log.info("extract %s", blob["out"])
    if kind == "generic":
        run_cmd(
            [
                py,
                generic,
                "--archive",
                archive,
                "--section",
                blob["section"],
                "--symbol",
                blob["symbol"],
                "--expect-ret-imm",
                str(blob["expect_ret_imm"]),
                "--out",
                out,
            ],
            what=f"blob extract {blob['out']}",
        )
    elif kind == "probe":
        run_cmd(
            [py, probe, "--archive", archive, "--out", out],
            what=f"probe extract {blob['out']}",
        )
    else:
        raise SystemExit(f"ERROR: unknown blob kind: {kind}")
    if not out.is_file():
        raise SystemExit(f"ERROR: expected blob missing after extract: {out}")


def build_rust(
    *,
    mode: str | None = None,
    skip_tests: bool = False,
    cfg: Mapping[str, Any] | None = None,
) -> None:
    cfg = cfg or load_config()
    rust = cfg["rust"]
    extract = rust["extract"]

    workspace = resolve(rust["workspace"])
    pkg = rust["package"]
    target_json = resolve(rust["target_json"])
    target_dir = resolve(rust["cargo_target_dir"])
    out_dir = resolve(rust["out_dir"])
    toolchain = rust["toolchain"]

    if not workspace.is_dir():
        raise SystemExit(f"ERROR: rust workspace missing: {workspace}")
    if not target_json.is_file():
        raise SystemExit(f"ERROR: freestanding target JSON missing: {target_json}")

    cargo = which("cargo")
    if cargo is None:
        raise SystemExit("ERROR: 'cargo' not found on PATH")

    run_host, force = _mode_overlay(cfg, rust, mode)
    archive = target_dir / ARCHIVE_REL
    # Host `cargo test` uses a separate target dir so a stuck/hung test binary
    # under `target/debug/deps/` cannot block later builds (LNK1104 file lock).
    host_target_dir = target_dir / "host-test"

    log.info("Building Rust components")

    env = {"CARGO_TARGET_DIR": str(target_dir)}
    if run_host and not skip_tests:
        log.info("host tests: cargo test -p %s", pkg)
        run_cmd(
            [cargo, "test", "-p", pkg],
            cwd=workspace,
            env={**env, "CARGO_TARGET_DIR": str(host_target_dir)},
            what="cargo test",
        )
    elif skip_tests:
        log.info("host tests: skipped")
    else:
        log.info("host tests: disabled by config/mode")

    if force and archive.is_file():
        log.info("invalidate %s", archive)
        archive.unlink()

    if rust.get("clear_rustflags"):
        env = {**env}
        # Clear RUSTFLAGS for freestanding build.
        cleared = os.environ.copy()
        cleared.pop("RUSTFLAGS", None)
        cleared["CARGO_TARGET_DIR"] = str(target_dir)
        build_env = cleared
    else:
        build_env = env

    log.info("freestanding staticlib (release, %s)", toolchain)
    run_cmd(
        [
            cargo,
            f"+{toolchain}",
            "build",
            "-Z",
            "build-std=core,compiler_builtins",
            "-Z",
            "json-target-spec",
            "-p",
            pkg,
            "--release",
            "--target",
            str(target_json),
        ],
        cwd=workspace,
        env=build_env,
        what="freestanding cargo build",
    )

    if not archive.is_file():
        raise SystemExit(f"ERROR: freestanding archive missing: {archive}")

    out_dir.mkdir(parents=True, exist_ok=True)
    py = find_python(extract.get("python", "python"))
    generic = resolve(extract["generic_script"])
    probe = resolve(extract["probe_script"])

    for blob in rust["blobs"]:
        _extract_blob(py, archive, out_dir, blob, generic, probe)

    log.info("Rust blobs ready")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", default=None, help="Build mode from [modes.*]")
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    setup_logging(args.verbose)
    build_rust(mode=args.mode, skip_tests=args.skip_tests)


if __name__ == "__main__":
    main()
