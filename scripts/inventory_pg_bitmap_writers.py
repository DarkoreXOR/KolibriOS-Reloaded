#!/usr/bin/env python3
"""Generate Stage-4 physical bitmap writer inventory (host-only research).

Scans kernel/ for references to sys_pgmap / pages_free / page_start and
classifies boot vs runtime writers. Does not modify production code.

Usage:
  python scripts/inventory_pg_bitmap_writers.py
  python scripts/inventory_pg_bitmap_writers.py --out docs/migration/stage4-bitmap-writers.json
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from common import PROJECT_ROOT

STATES = ("sys_pgmap", "pages_free", "page_start", "page_end")

# Heuristic write patterns (instruction left-hand / BTS/BTR / stos).
WRITE_HINTS = re.compile(
    r"\b(mov|and|or|xor|add|sub|inc|dec|adc|sbb|xchg|bts|btr|btc|stos[bwd]?|rep\s+stos)",
    re.I,
)


def classify_file(path: Path) -> str:
    rel = path.as_posix()
    if "init.inc" in rel:
        return "boot"
    if "/rust/" in rel or rel.startswith("kernel/rust/"):
        return "diagnostic"
    return "runtime"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--out",
        type=Path,
        default=PROJECT_ROOT / "docs" / "migration" / "stage4-bitmap-writers.json",
    )
    args = ap.parse_args()

    kernel = PROJECT_ROOT / "kernel"
    records: list[dict] = []

    for path in sorted(kernel.rglob("*")):
        if path.suffix.lower() not in {".inc", ".asm"}:
            continue
        try:
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        rel = path.relative_to(PROJECT_ROOT).as_posix()
        for i, line in enumerate(lines, 1):
            stripped = line.strip()
            if not stripped or stripped.startswith(";"):
                continue
            code = stripped.split(";")[0]
            for state in STATES:
                if state not in code and f"pg_data.{state}" not in code:
                    # pages_free lives as pg_data.pages_free
                    if state == "pages_free" and "pages_free" not in code:
                        continue
                    if state != "pages_free" and state not in code:
                        continue
                if state == "pages_free" and "pages_free" not in code:
                    continue
                if state != "pages_free" and state not in code:
                    continue

                is_write = bool(WRITE_HINTS.search(code))
                # Address-only uses (sub ebx, sys_pgmap) are not bitmap stores.
                role = "read_or_address"
                must_move = False
                phase = classify_file(path)
                write_kind = "none"

                if state == "sys_pgmap":
                    if re.search(r"\bbts\b", code, re.I):
                        is_write, write_kind, role = True, "bts", "bitmap_free_bit"
                        must_move = phase == "runtime"
                    elif re.search(r"\bbtr\b", code, re.I):
                        is_write, write_kind, role = True, "btr", "bitmap_alloc_bit"
                        must_move = phase == "runtime"
                    elif re.search(r"\brep\s+stos|\bstos", code, re.I):
                        is_write, write_kind, role = True, "stos", "bitmap_bulk_clear"
                        must_move = phase == "runtime"
                    elif re.search(r"mov\s+.*sys_pgmap|sys_pgmap\s*-", code, re.I):
                        role = "address_calc_or_boot_fill"
                        if phase == "boot" and re.search(r"stos|mov\s+\[", code, re.I):
                            is_write, write_kind, role = True, "boot_init", "boot_bitmap_init"
                    elif "[" in code and re.search(r"mov\s+.*\[", code, re.I):
                        # indirect via register holding sys_pgmap
                        pass

                if state == "pages_free" or "pages_free" in code:
                    if re.search(
                        r"(mov|dec|sub|add|adc|inc)\s+.*pages_free|pages_free.*\]",
                        code,
                        re.I,
                    ):
                        if re.search(r"cmp\s+.*pages_free|pages_free.*cmp", code, re.I):
                            role = "read_compare"
                        elif re.search(r"push\s+.*pages_free|pop\s+.*pages_free", code, re.I):
                            role = "diagnostic_save_restore"
                            phase = "diagnostic"
                        elif re.search(r"mov\s+\[.*pages_free", code, re.I) or re.search(
                            r"dec\s+\[.*pages_free|sub\s+\[.*pages_free|adc\s+\[.*pages_free|add\s+\[.*pages_free",
                            code,
                            re.I,
                        ):
                            is_write = True
                            write_kind = "counter_update"
                            role = "pages_free_write"
                            must_move = phase == "runtime"
                        elif re.search(r"mov\s+\w+,\s*\[.*pages_free", code, re.I):
                            role = "read"

                if state == "page_start" and "page_start" in code:
                    if re.search(r"mov\s+\[.*page_start", code, re.I):
                        is_write, write_kind, role = True, "cursor_store", "page_start_write"
                        must_move = phase in {"runtime", "boot"}
                        if phase == "boot":
                            must_move = False
                    elif re.search(r"mov\s+\w+,\s*\[.*page_start|cmp\s+\[.*page_start", code, re.I):
                        role = "read"

                if state == "page_end" and "page_end" in code:
                    if re.search(r"mov\s+\[.*page_end", code, re.I):
                        is_write, write_kind, role = True, "boot_bound", "page_end_write"
                        phase = "boot"
                        must_move = False
                    else:
                        role = "read"

                # release_pages: bts [edx] with edx=sys_pgmap
                if re.search(r"\bbts\s+\[edx\]", code, re.I) and rel.endswith("memory.inc"):
                    is_write, write_kind = True, "bts_indirect"
                    role = "release_pages_bitmap"
                    state = "sys_pgmap"
                    must_move = True
                    phase = "runtime"

                # alloc_pages rep stosb clears map bytes (edi walks the map)
                if re.search(r"\brep\s+stosb", code, re.I) and rel.endswith("memory.inc"):
                    # context: only the alloc_pages body
                    is_write, write_kind = True, "rep_stosb"
                    role = "alloc_pages_clear_ff_run"
                    state = "sys_pgmap"
                    must_move = True
                    phase = "runtime"

                records.append(
                    {
                        "state": state if state in STATES else "sys_pgmap",
                        "file": rel,
                        "line": i,
                        "code": code.strip()[:200],
                        "phase": phase,
                        "is_write": bool(is_write),
                        "write_kind": write_kind,
                        "role": role,
                        "direct": "indirect" not in write_kind,
                        "must_move_behind_rust_api": bool(must_move),
                        "notes": "",
                    }
                )

    # Deduplicate identical line hits for multiple state tokens
    uniq: dict[tuple, dict] = {}
    for r in records:
        key = (r["file"], r["line"], r["state"], r["write_kind"], r["role"])
        uniq[key] = r
    records = list(uniq.values())
    records.sort(key=lambda r: (r["file"], r["line"], r["state"]))

    writers = [r for r in records if r["is_write"]]
    runtime_w = [r for r in writers if r["phase"] == "runtime"]
    boot_w = [r for r in writers if r["phase"] == "boot"]
    diag_w = [r for r in writers if r["phase"] == "diagnostic"]

    # Hand-audited summary (authoritative for ownership decision)
    audited_runtime_writers = [
        {
            "symbol": "alloc_page",
            "file": "kernel/core/memory.inc",
            "states": ["sys_pgmap", "pages_free", "page_start"],
            "write_kinds": ["btr", "dec/mov pages_free", "mov page_start"],
            "must_move_behind_rust_api": True,
            "notes": "CLI; OOM forces pages_free=1; scan miss leaves pages_free unchanged",
        },
        {
            "symbol": "alloc_pages",
            "file": "kernel/core/memory.inc",
            "states": ["sys_pgmap", "pages_free"],
            "write_kinds": ["rep stosb", "sub pages_free"],
            "must_move_behind_rust_api": True,
            "notes": "Does NOT update page_start; ceil(N/8)*8 charged to pages_free",
        },
        {
            "symbol": "free_page",
            "file": "kernel/core/memory.inc",
            "states": ["sys_pgmap", "pages_free", "page_start"],
            "write_kinds": ["bts", "adc pages_free", "maybe mov page_start"],
            "must_move_behind_rust_api": True,
            "notes": "Double-free: bts old=1 → cmc/adc adds 0",
        },
        {
            "symbol": "release_pages",
            "file": "kernel/core/memory.inc",
            "states": ["sys_pgmap", "pages_free"],
            "write_kinds": ["bts [edx]", "mov pages_free"],
            "must_move_behind_rust_api": True,
            "notes": "Tracks page_start in EBX but NEVER stores it; PTE clear/invlpg/mutex stay FASM",
        },
    ]

    audited_boot_writers = [
        {
            "symbol": "init_page_map",
            "file": "kernel/init.inc",
            "states": ["sys_pgmap", "pages_free", "page_start", "page_end"],
            "must_move_behind_rust_api": False,
            "notes": "Boot-only; may hand off initialized bitmap to Rust ownership later",
        }
    ]

    audited_non_writers = [
        {
            "symbol": "sysfn_getfreemem / sysfn_meminfo / taskman / disk_cache / getcache",
            "states": ["pages_free"],
            "role": "read_only",
            "must_move_behind_rust_api": False,
        },
        {
            "symbol": "test_app_header smoke",
            "file": "kernel/rust/test_app_header.inc",
            "states": ["pages_free"],
            "role": "diagnostic_save_restore",
            "must_move_behind_rust_api": False,
            "notes": "Push/mov/pop around ABI smoke only",
        },
    ]

    out = {
        "schema": "stage4-bitmap-writers/v1",
        "generated_by": "scripts/inventory_pg_bitmap_writers.py",
        "authority": "Hand-audited runtime writer list is authoritative for ownership; raw_hits are heuristic.",
        "summary": {
            "raw_hits": len(records),
            "raw_write_hits": len(writers),
            "raw_runtime_write_hits": len(runtime_w),
            "raw_boot_write_hits": len(boot_w),
            "raw_diagnostic_write_hits": len(diag_w),
            "audited_runtime_bitmap_writers": len(audited_runtime_writers),
            "audited_boot_writers": len(audited_boot_writers),
            "unresolved_suspicious": [],
            "sole_runtime_writer_feasible_after_release_pages_split": True,
            "sole_runtime_writer_notes": (
                "Yes: only alloc_page, alloc_pages, free_page, and release_pages "
                "mutate sys_pgmap/pages_free/(page_start) at runtime. After routing "
                "release_pages bitmap BTS through a Rust free/bulk-free API, Rust can "
                "be the sole runtime writer. Boot init_page_map stays FASM. "
                "Diagnostic smokes save/restore pages_free only."
            ),
        },
        "audited_runtime_writers": audited_runtime_writers,
        "audited_boot_writers": audited_boot_writers,
        "audited_non_writers": audited_non_writers,
        "release_pages_split": {
            "fasm_orchestration": [
                "mutex_lock/unlock pg_data.mutex",
                "xchg PTE with 0 via page_tabs",
                "invlpg on linear addresses",
                "loop over VA range",
            ],
            "future_rust_bitmap_primitive": [
                "bts polarity free of phys page indices from present PTEs",
                "pages_free updates",
                "NOTE: must preserve legacy quirk of NOT updating page_start",
            ],
            "divergence_from_free_page": "release_pages does not store page_start; free_page does",
        },
        "raw_hits": records,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(out, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {args.out}")
    print(
        "audited runtime writers:",
        len(audited_runtime_writers),
        "sole_runtime_feasible:",
        out["summary"]["sole_runtime_writer_feasible_after_release_pages_split"],
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
