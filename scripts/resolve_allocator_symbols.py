"""Resolve Stage-4 allocator globals to guest VA/PA for QMP ``xp``.

Reads FASM ``kernel/bin/kernel.fas`` (produced by ``assemble_kernel.py`` with
``-s``). Physical address uses the Kolibri linear kernel map:

    PA = VA - OS_BASE   (OS_BASE = 0x80000000)

Fails clearly if symbols are missing — never invents addresses.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from common import load_config, resolve  # noqa: E402
from fasm_symbols import FasError, lookup_symbol, parse_fas_symbols  # noqa: E402

OS_BASE = 0x80000000

# PG_DATA.pages_free offset (const.inc struct PG_DATA).
PG_DATA_PAGES_FREE_OFF = 12
PG_DATA_PAGES_COUNT_OFF = 8
PG_DATA_PAGES_FAULTS_OFF = 16
PG_DATA_PAGEMAP_SIZE_OFF = 20


class SymbolResolveError(RuntimeError):
    pass


def va_to_pa(va: int) -> int:
    if va < OS_BASE:
        raise SymbolResolveError(
            f"refusing PA mapping for VA {va:#x} (< OS_BASE {OS_BASE:#x})"
        )
    return va - OS_BASE


def default_fas_path(cfg: dict[str, Any] | None = None) -> Path:
    cfg = cfg or load_config()
    # Prefer FAS beside the kernel output.
    out = resolve(cfg["kernel"]["output"])
    cand = out.with_suffix(".fas")
    return cand


def resolve_allocator_symbols(fas_path: Path | None = None) -> dict[str, Any]:
    cfg = load_config()
    fas = fas_path or default_fas_path(cfg)
    if not fas.is_file():
        raise SymbolResolveError(
            f"FASM symbols dump missing: {fas}\n"
            "Rebuild the kernel so assemble_kernel.py emits -s "
            f"({fas.name})."
        )
    try:
        symbols = parse_fas_symbols(fas)
    except FasError as e:
        raise SymbolResolveError(str(e)) from e

    def pack(name: str, *alts: str, offset: int = 0) -> dict[str, Any]:
        try:
            hit = lookup_symbol(symbols, name, *alts)
        except KeyError as e:
            raise SymbolResolveError(str(e)) from e
        va = int(hit["value"]) + offset
        if va > 0xFFFFFFFF:
            raise SymbolResolveError(f"{name}: value out of 32-bit range {va:#x}")
        va &= 0xFFFFFFFF
        pa = va_to_pa(va)
        return {
            "symbol": hit["name"],
            "virtual_address": va,
            "physical_address": pa,
            "link_value": int(hit["value"]),
            "offset_applied": offset,
            "resolution_source": str(fas).replace("\\", "/"),
            "fasm_flags": hit["flags"],
            "os_base": OS_BASE,
            "pa_rule": "VA - OS_BASE",
        }

    # Prefer fully-qualified struct field if present.
    try:
        pages_free = pack("pg_data.pages_free")
    except SymbolResolveError:
        pages_free = pack("pg_data", offset=PG_DATA_PAGES_FREE_OFF)
        pages_free["symbol"] = "pg_data+12(pages_free)"

    page_start = pack("page_start")
    sys_pgmap = pack("sys_pgmap")

    # Optional helpers for digest sizing / cross-checks.
    extras: dict[str, Any] = {}
    for key, names, off in (
        ("pages_count", ("pg_data.pages_count",), PG_DATA_PAGES_COUNT_OFF),
        ("pages_faults", ("pg_data.pages_faults",), PG_DATA_PAGES_FAULTS_OFF),
        ("pagemap_size", ("pg_data.pagemap_size",), PG_DATA_PAGEMAP_SIZE_OFF),
        ("page_end", ("page_end",), 0),
        ("pg_data", ("pg_data",), 0),
        ("msg_board_data", ("msg_board_data",), 0),
        ("msg_board_count", ("msg_board_count",), 0),
    ):
        try:
            if names[0] == "page_end" or names[0] == "pg_data":
                extras[key] = pack(names[0])
            else:
                try:
                    extras[key] = pack(names[0])
                except SymbolResolveError:
                    extras[key] = pack("pg_data", offset=off)
                    extras[key]["symbol"] = f"pg_data+{off}({key})"
        except SymbolResolveError:
            extras[key] = {"error": f"unresolved:{names[0]}"}

    return {
        "schema": 1,
        "fas": str(fas).replace("\\", "/"),
        "pages_free": pages_free,
        "page_start": page_start,
        "sys_pgmap": sys_pgmap,
        "extras": extras,
    }


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fas", type=Path, default=None)
    ap.add_argument("-o", "--output", type=Path, default=None)
    args = ap.parse_args(argv)
    try:
        result = resolve_allocator_symbols(args.fas)
    except SymbolResolveError as e:
        raise SystemExit(f"ERROR: {e}") from e
    text = json.dumps(result, indent=2)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
