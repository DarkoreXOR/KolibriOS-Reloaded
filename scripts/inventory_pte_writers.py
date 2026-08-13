#!/usr/bin/env python3
"""Generate Stage-4 page_tabs / PTE writer inventory (host-only research).

Scans kernel/ for page_tabs / app_page_tabs / invlpg / CR3 / pte_valid_mask
references and emits heuristic hits. Hand-audited writer list in the JSON
authority section is authoritative for ownership decisions.

Does not modify production code.

Usage:
  python scripts/inventory_pte_writers.py
  python scripts/inventory_pte_writers.py --out docs/migration/stage4-pte-writers.json
"""

from __future__ import annotations

import argparse
import json
import re
from collections import Counter
from pathlib import Path

from common import PROJECT_ROOT

STATE_TOKENS = (
    "page_tabs",
    "app_page_tabs",
    "pte_valid_mask",
    "invlpg",
    "cr3",
)

WRITE_HINTS = re.compile(
    r"\b(mov|and|or|xor|add|sub|inc|dec|adc|sbb|xchg|bts|btr|btc|stos[bwd]?|rep\s+stos)",
    re.I,
)

# Destination-looking stores into page table windows.
PTE_STORE = re.compile(
    r"(mov|xchg|or|and|xor)\s+.*(page_tabs|app_page_tabs)",
    re.I,
)
PTE_STORE_BRACKET = re.compile(
    r"(mov|xchg|or|and|xor)\s+\[.*(page_tabs|app_page_tabs|[a-z]{2,3}\s*\+\s*.*\*.*4)",
    re.I,
)
INVLPG = re.compile(r"\binvlpg\b", re.I)
CR3_WRITE = re.compile(r"\bmov\s+cr3\b", re.I)
CR3_READ = re.compile(r"\bmov\s+\w+\s*,\s*cr3\b", re.I)


def classify_file(path: Path) -> str:
    rel = path.as_posix().replace("\\", "/")
    if "init.inc" in rel or "/boot/" in rel:
        return "boot"
    if "/rust/" in rel or rel.startswith("kernel/rust/"):
        return "diagnostic"
    if "test_" in path.name.lower():
        return "diagnostic"
    return "runtime"


def nearest_symbol(lines: list[str], idx: int) -> str:
    """Walk upward for a rough enclosing label/proc."""
    for j in range(idx, -1, -1):
        s = lines[j].strip()
        if not s or s.startswith(";"):
            continue
        m = re.match(r"^(?:align\s+\d+\s*$)", s, re.I)
        if m:
            continue
        m = re.match(r"^proc\s+(\w+)", s, re.I)
        if m:
            return m.group(1)
        m = re.match(r"^([A-Za-z_][\w.]*)\s*:", s)
        if m:
            name = m.group(1)
            if not name.startswith("."):
                return name
        if s.lower().startswith("endp"):
            continue
    return "?"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--out",
        type=Path,
        default=PROJECT_ROOT / "docs" / "migration" / "stage4-pte-writers.json",
    )
    ap.add_argument(
        "--hits-only",
        action="store_true",
        help="Emit only heuristic hits (no audited authority section).",
    )
    args = ap.parse_args()

    kernel = PROJECT_ROOT / "kernel"
    hits: list[dict] = []

    for path in sorted(kernel.rglob("*")):
        if path.suffix.lower() not in {".inc", ".asm"}:
            continue
        # Skip huge listing dumps / upstream mirrors if any
        rel = path.relative_to(PROJECT_ROOT).as_posix()
        if "/_upstream/" in rel or "listing.inc" in rel:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        lines = text.splitlines()
        for i, line in enumerate(lines, 1):
            stripped = line.strip()
            if not stripped or stripped.startswith(";"):
                continue
            code = stripped.split(";")[0]
            low = code.lower()

            tokens_hit = [t for t in STATE_TOKENS if t in low]
            # Also catch mov cr3 without token list miss
            if not tokens_hit and not CR3_WRITE.search(code) and not CR3_READ.search(code):
                # Heuristic: stores through edi/esi after lea … page_tabs
                continue
            if not tokens_hit:
                if CR3_WRITE.search(code):
                    tokens_hit = ["cr3"]
                elif CR3_READ.search(code):
                    tokens_hit = ["cr3"]
                else:
                    continue

            phase = classify_file(path)
            is_write = False
            write_kind = "none"
            role = "read_or_address"
            must_move = False

            if INVLPG.search(code):
                is_write = True  # TLB side effect
                write_kind = "invlpg"
                role = "tlb_invalidate"
                must_move = phase == "runtime"
            elif CR3_WRITE.search(code):
                is_write = True
                write_kind = "mov_cr3"
                role = "cr3_switch"
                must_move = False  # process ownership — never auto-move
            elif any(t in ("page_tabs", "app_page_tabs") for t in tokens_hit):
                if PTE_STORE.search(code) or (
                    "[" in code and WRITE_HINTS.search(code) and re.search(r"mov\s+\[|xchg\s+", code, re.I)
                ):
                    is_write = True
                    if re.search(r"\bxchg\b", code, re.I):
                        write_kind = "xchg"
                        role = "pte_clear_or_swap"
                    elif re.search(r"\bor\b", code, re.I):
                        write_kind = "or"
                        role = "pte_set_flags"
                    elif re.search(r"\band\b", code, re.I):
                        write_kind = "and"
                        role = "pte_mask_flags"
                    else:
                        write_kind = "mov"
                        role = "pte_store"
                    must_move = phase == "runtime"
                elif "lea" in low and ("page_tabs" in low or "app_page_tabs" in low):
                    role = "address_calc"
                elif WRITE_HINTS.search(code) and "[" in code:
                    # Possible indirect store via register previously loaded with page_tabs
                    is_write = True
                    write_kind = "maybe_indirect"
                    role = "suspicious_indirect"
                    must_move = False
                else:
                    role = "read_or_address"
            elif "pte_valid_mask" in tokens_hit:
                if re.search(r"mov\s+\[?\s*pte_valid_mask", code, re.I):
                    is_write = True
                    write_kind = "mov"
                    role = "mask_init"
                    must_move = False
                else:
                    role = "mask_read"

            hits.append(
                {
                    "file": rel,
                    "line": i,
                    "symbol_guess": nearest_symbol(lines, i - 1),
                    "phase": phase,
                    "tokens": tokens_hit,
                    "is_write": is_write,
                    "write_kind": write_kind,
                    "role": role,
                    "must_move_behind_rust_api": must_move,
                    "code": code.strip()[:160],
                }
            )

    # Also scan for stores that use page_tabs only via register (second pass cues)
    # Collect lines with mov [reg*4+…] near page_tabs LEA — already covered partially.

    write_hits = [h for h in hits if h["is_write"]]
    runtime_writes = [h for h in write_hits if h["phase"] == "runtime"]
    boot_writes = [h for h in write_hits if h["phase"] == "boot"]
    diag_writes = [h for h in write_hits if h["phase"] == "diagnostic"]
    invlpg_hits = [h for h in hits if h["write_kind"] == "invlpg"]
    cr3_hits = [h for h in hits if h["write_kind"] == "mov_cr3"]
    suspicious = [h for h in hits if h["role"] == "suspicious_indirect"]

    by_file = Counter(h["file"] for h in write_hits)
    by_symbol = Counter(h["symbol_guess"] for h in write_hits)

    payload = {
        "schema": "stage4-pte-writers/v1",
        "generated_by": "scripts/inventory_pte_writers.py",
        "authority": (
            "Hand-audited runtime writer list in audited_* sections is authoritative "
            "for ownership; raw_hits are heuristic and may include address LEAs / reads."
        ),
        "summary_heuristic": {
            "raw_hits": len(hits),
            "raw_write_hits": len(write_hits),
            "raw_runtime_write_hits": len(runtime_writes),
            "raw_boot_write_hits": len(boot_writes),
            "raw_diagnostic_write_hits": len(diag_writes),
            "raw_invlpg_hits": len(invlpg_hits),
            "raw_cr3_write_hits": len(cr3_hits),
            "raw_suspicious_indirect": len(suspicious),
            "write_hits_by_file": dict(by_file.most_common()),
            "write_hits_by_symbol_guess": dict(by_symbol.most_common(40)),
        },
        "raw_hits": hits,
    }

    if not args.hits_only:
        # Audited authority is filled by the research pass in the same file after
        # generation, or merged below from a sibling audited blob if present.
        audited_path = (
            PROJECT_ROOT
            / "docs"
            / "migration"
            / "stage4-pte-writers.audited.json"
        )
        if audited_path.exists():
            audited = json.loads(audited_path.read_text(encoding="utf-8"))
            payload.update(audited)
        else:
            payload["audited_runtime_writers"] = []
            payload["audited_boot_writers"] = []
            payload["audited_non_writers"] = []
            payload["unresolved_suspicious"] = [
                f"{h['file']}:{h['line']} {h['code']}" for h in suspicious[:50]
            ]
            payload["summary"] = {
                "note": "Run research merge to populate audited_* authority sections.",
                "heuristic_only": True,
            }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {args.out} ({len(hits)} hits, {len(write_hits)} write-class)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
