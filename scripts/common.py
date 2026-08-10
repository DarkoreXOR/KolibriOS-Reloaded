"""Shared helpers for KolibriOS project automation scripts."""

from __future__ import annotations

import logging
import os
import shutil
import subprocess
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping, MutableMapping, Sequence

PROJECT_ROOT = Path(__file__).resolve().parents[1]
CONFIG_PATH = PROJECT_ROOT / "project" / "build.toml"
LAST_IMAGE_MARKER = PROJECT_ROOT / "dev_build" / "last_image.txt"
DEFAULT_MODE_MARKER = PROJECT_ROOT / "dev_build" / "build-mode.txt"

log = logging.getLogger("kolibri.scripts")


def setup_logging(verbose: bool = False) -> None:
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(levelname)s: %(message)s",
        stream=sys.stderr,
    )


def load_config(path: Path | None = None) -> dict[str, Any]:
    cfg_path = path or CONFIG_PATH
    if not cfg_path.is_file():
        raise SystemExit(f"ERROR: missing build config: {cfg_path}")
    with cfg_path.open("rb") as f:
        return tomllib.load(f)


def resolve(path: str | Path) -> Path:
    p = Path(path)
    if p.is_absolute():
        return p
    return (PROJECT_ROOT / p).resolve()


def which(name: str) -> Path | None:
    found = shutil.which(name)
    return Path(found) if found else None


def find_python(preferred: str = "python") -> Path:
    for cand in (preferred, "python3", "py"):
        p = which(cand)
        if p is not None:
            return p
    raise SystemExit(
        f"ERROR: Python not found on PATH (tried {preferred!r}, 'python3', 'py')"
    )


def find_qemu(executables: Sequence[str]) -> Path:
    for cand in executables:
        p = Path(cand)
        if p.is_file():
            return p
        w = which(cand)
        if w is not None:
            return w
    raise SystemExit(
        "ERROR: QEMU executable not found "
        "(see project/build.toml [qemu].executables)"
    )


def run_cmd(
    argv: Sequence[str | Path],
    *,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
    check: bool = True,
    what: str | None = None,
) -> subprocess.CompletedProcess[bytes]:
    cmd = [str(a) for a in argv]
    work = cwd or PROJECT_ROOT
    label = what or cmd[0]
    log.info("run: %s (cwd=%s)", " ".join(cmd), work)
    merged: MutableMapping[str, str] | None = None
    if env is not None:
        merged = os.environ.copy()
        merged.update(env)
    try:
        cp = subprocess.run(cmd, cwd=work, env=merged, check=False)
    except FileNotFoundError as e:
        raise SystemExit(f"ERROR: {label}: executable not found: {cmd[0]}") from e
    if check and cp.returncode != 0:
        raise SystemExit(
            f"ERROR: {label} failed.\n"
            f"Command: {' '.join(cmd)}\n"
            f"Working directory: {work}\n"
            f"Exit code: {cp.returncode}"
        )
    return cp


def run_interactive(
    argv: Sequence[str | Path],
    *,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
    what: str | None = None,
) -> int:
    """Run a long-lived process; terminate cleanly on Ctrl+C."""
    cmd = [str(a) for a in argv]
    work = cwd or PROJECT_ROOT
    label = what or cmd[0]
    log.info("run: %s (cwd=%s)", " ".join(cmd), work)
    merged: MutableMapping[str, str] | None = None
    if env is not None:
        merged = os.environ.copy()
        merged.update(env)
    proc = subprocess.Popen(cmd, cwd=work, env=merged)
    try:
        return proc.wait()
    except KeyboardInterrupt:
        log.warning("Interrupted — terminating %s (pid=%s)", label, proc.pid)
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        raise SystemExit(130) from None


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")


def read_mode_marker(cfg: Mapping[str, Any] | None = None) -> str | None:
    env_mode = os.environ.get("KOLIBRI_BUILD_MODE") or os.environ.get("ORCH_BUILD_MODE")
    if env_mode:
        return env_mode.strip()
    marker = DEFAULT_MODE_MARKER
    if cfg is not None:
        # Prefer configured marker from any mode section if present.
        modes = cfg.get("modes") or {}
        for mc in modes.values():
            if isinstance(mc, dict) and mc.get("mode_marker"):
                marker = resolve(mc["mode_marker"])
                break
    # Also accept legacy orch marker during transition.
    candidates = [marker, PROJECT_ROOT / "dev_build" / "orch-mode.txt"]
    for path in candidates:
        if path.is_file():
            return path.read_text(encoding="utf-8").strip() or None
    return None


def write_mode_marker(mode: str, mode_cfg: Mapping[str, Any]) -> Path:
    marker = resolve(mode_cfg.get("mode_marker") or "dev_build/build-mode.txt")
    marker.parent.mkdir(parents=True, exist_ok=True)
    marker.write_text(mode, encoding="utf-8")
    os.environ["KOLIBRI_BUILD_MODE"] = mode
    return marker


def qemu_opt_path(path: Path | str) -> str:
    """Path safe inside a QEMU `file=` option value."""
    p = Path(path)
    try:
        rel = p.resolve().relative_to(PROJECT_ROOT.resolve())
        return str(rel).replace("\\", "/")
    except ValueError:
        s = str(p.resolve()).replace("\\", "/")
        # Absolute Windows paths need an explicit file: prefix (drive-letter colon).
        if len(s) >= 2 and s[1] == ":":
            return f"file:{s}"
        return s
