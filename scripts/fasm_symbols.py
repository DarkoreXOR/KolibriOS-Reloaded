"""Parse FASM ``-s`` symbolic information dumps (``.fas``).

Format reference: ``tools/fasm/TOOLS/FAS.TXT`` (signature ``1A736166h``).
"""

from __future__ import annotations

import argparse
import json
import struct
from pathlib import Path
from typing import Any


FAS_SIGNATURE = 0x1A736166
SYMBOL_SIZE = 32


class FasError(ValueError):
    pass


def parse_fas_symbols(path: Path | str) -> dict[str, dict[str, Any]]:
    """Return ``{name: {value, flags, size, type}}`` for defined symbols.

    When duplicate names exist, the last defined entry wins (FASM multipass).
    """
    data = Path(path).read_bytes()
    if len(data) < 32:
        raise FasError(f"FAS too short: {path}")
    sig = struct.unpack_from("<I", data, 0)[0]
    if sig != FAS_SIGNATURE:
        raise FasError(f"bad FAS signature {sig:#x} in {path}")

    hdr_len = struct.unpack_from("<H", data, 6)[0]
    if hdr_len < 32:
        raise FasError(f"FAS header too short ({hdr_len}) in {path}")

    strings_off, strings_len = struct.unpack_from("<II", data, 16)
    sym_off, sym_len = struct.unpack_from("<II", data, 24)
    prep_off, prep_len = struct.unpack_from("<II", data, 32)

    if sym_off + sym_len > len(data):
        raise FasError(f"symbols table out of range in {path}")
    if strings_off + strings_len > len(data):
        raise FasError(f"strings table out of range in {path}")
    if prep_off + prep_len > len(data):
        raise FasError(f"preprocessed source out of range in {path}")

    out: dict[str, dict[str, Any]] = {}
    off = sym_off
    end = sym_off + sym_len
    while off + SYMBOL_SIZE <= end:
        raw = data[off : off + SYMBOL_SIZE]
        value = struct.unpack_from("<Q", raw, 0)[0]
        flags = struct.unpack_from("<H", raw, 8)[0]
        size = raw[10]
        vtype = raw[11]
        name_field = struct.unpack_from("<I", raw, 24)[0]
        off += SYMBOL_SIZE

        if (flags & 1) == 0:
            continue
        # Skip assembly-time variables that are not address labels when bit1 set
        # and negative-value flag (bit9 of flags word / byte[9] bit1 in dumper).
        # Keep all defined labels; callers filter by name.

        if name_field & 0x80000000:
            n = name_field & 0x7FFFFFFF
            p = strings_off + n
            if p >= len(data):
                continue
            z = data.find(b"\x00", p)
            if z < 0:
                continue
            name = data[p:z].decode("ascii", "replace")
        elif name_field == 0:
            name = "@@"
        else:
            p = prep_off + (name_field & 0x7FFFFFFF)
            if p >= len(data):
                continue
            ln = data[p]
            name = data[p + 1 : p + 1 + ln].decode("ascii", "replace")

        out[name] = {
            "value": value,
            "flags": flags,
            "size": size,
            "type": vtype,
        }
    return out


def lookup_symbol(symbols: dict[str, dict[str, Any]], *names: str) -> dict[str, Any]:
    for name in names:
        if name in symbols:
            hit = dict(symbols[name])
            hit["name"] = name
            return hit
    raise KeyError(f"symbol not found; tried {list(names)}")


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("fas", type=Path)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--grep", default=None, help="substring filter on symbol names")
    args = ap.parse_args(argv)
    syms = parse_fas_symbols(args.fas)
    if args.grep:
        syms = {k: v for k, v in syms.items() if args.grep in k}
    if args.json:
        printable = {
            k: {**v, "value": f"0x{v['value']:X}"} for k, v in sorted(syms.items())
        }
        print(json.dumps(printable, indent=2))
    else:
        for name, v in sorted(syms.items()):
            print(f"{name}: 0x{v['value']:08X}")


if __name__ == "__main__":
    main()
