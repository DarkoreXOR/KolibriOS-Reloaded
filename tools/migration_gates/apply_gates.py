#!/usr/bin/env python3
"""Apply USE_RUST_* migration gates from project/build.toml.

Focused utility: reads CONFIG_DATA, rewrites gate assignments in-place.
Not an orchestrator — call from Rhai via process::run.

Usage:
  python tools/migration_gates/apply_gates.py project/build.toml
"""

from __future__ import annotations

import sys
from pathlib import Path


def load_toml(path: Path) -> dict:
    try:
        import tomllib
    except ImportError:
        try:
            import tomli as tomllib  # type: ignore
        except ImportError:
            # Minimal fallback: only enough for [[rust.migrations]] via a tiny parser
            # Prefer stdlib tomllib (3.11+) or tomli.
            sys.stderr.write(
                "error: need Python 3.11+ (tomllib) or the tomli package to parse TOML\n"
            )
            sys.exit(2)
    with path.open("rb") as f:
        return tomllib.load(f)


def apply_gate(repo: Path, gate: str, gate_file: str, enabled: bool) -> None:
    want = 1 if enabled else 0
    path = repo / gate_file
    if not path.is_file():
        raise SystemExit(f"error: gate_file missing for {gate}: {path}")
    text = path.read_text(encoding="utf-8")
    found = False
    changed = False
    out_lines: list[str] = []
    for line in text.splitlines(keepends=True):
        raw = line.rstrip("\r\n")
        newline = line[len(raw) :]
        trimmed = raw.strip()
        is_assign = False
        if not trimmed.startswith(";") and trimmed.startswith(gate):
            rest = trimmed[len(gate) :].lstrip()
            if rest.startswith("="):
                is_assign = True
        if is_assign:
            found = True
            indent = raw[: len(raw) - len(raw.lstrip())]
            expected = f"{gate} = {want}"
            if trimmed == expected:
                out_lines.append(raw + newline)
            else:
                changed = True
                out_lines.append(indent + expected + newline)
        else:
            out_lines.append(line)
    if not found:
        raise SystemExit(f"error: gate `{gate}` not found in {gate_file}")
    if changed:
        path.write_text("".join(out_lines), encoding="utf-8")
        print(f"  gate {gate} → {want} ({gate_file})")


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    cfg_path = Path(sys.argv[1]).resolve()
    if not cfg_path.is_file():
        print(f"error: config not found: {cfg_path}", file=sys.stderr)
        return 1
    # Repo root: walk up until kernel/kernel.asm exists, else parent of project/
    repo = cfg_path.parent
    if (repo / "kernel" / "kernel.asm").is_file():
        pass
    elif (repo.parent / "kernel" / "kernel.asm").is_file():
        repo = repo.parent
    else:
        # project/build.toml → repo is parent of project/
        if cfg_path.parent.name == "project":
            repo = cfg_path.parent.parent

    data = load_toml(cfg_path)
    migrations = data.get("rust", {}).get("migrations", [])
    print(f"  migration gates: {len(migrations)} registered")
    for m in migrations:
        apply_gate(repo, m["gate"], m["gate_file"], bool(m.get("enabled", False)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
