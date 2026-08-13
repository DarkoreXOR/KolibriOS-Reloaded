"""Stage-4 free_page vs release_pages page_start A/B experiment.

Builds disposable PE ``asoakdrv_ab.asm``, installs on CoW image as firstapp
payload, samples ``pages_free`` / ``page_start`` / bitmap digest around guest
markers via QMP ``xp``.

Does **not** modify production allocator code.
"""

from __future__ import annotations

import argparse
import json
import socket
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from build_allocsoak import build_allocsoak  # noqa: E402
from common import LAST_IMAGE_MARKER, load_config, log, resolve, run_cmd, setup_logging  # noqa: E402
from prepare_allocsoak_image import patch_firstapp_in_kernel_mnt  # noqa: E402
from prepare_image import ensure_kolibri_img, prepare_image  # noqa: E402
from qmp_allocator_soak import (  # noqa: E402
    count_non_black_ppm,
    drain_events,
    qmp_connect,
    qmp_exec,
    qmp_recv_obj,
    sample_allocator,
    scrape_msg_board,
)
from resolve_allocator_symbols import resolve_allocator_symbols  # noqa: E402
from run_qemu import build_qemu_argv, find_qemu  # noqa: E402

SEED = 0x5047424D


def build_asoakdrv_ab() -> Path:
    cfg = load_config()
    fasm = resolve(cfg["kernel"]["fasm"])
    src = resolve("tools/allocsoak/asoakdrv_ab.asm")
    out_dir = resolve("dev_build/allocsoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / "ASOAKDRV"
    if out.is_file():
        out.unlink()
    run_cmd([fasm, src, out], what="FASM asoakdrv_ab")
    log.info("asoakdrv_ab built: %s (%s bytes)", out, out.stat().st_size)
    return out


def prepare_ab_image(*, delete: bool | None = None) -> dict[str, Any]:
    cfg = load_config()
    image_path = prepare_image(cfg, delete=delete)
    drv = build_asoakdrv_ab()
    loader = build_allocsoak()
    img_tool = ensure_kolibri_img(cfg["image"])
    run_cmd([img_tool, "put", image_path, "ASOAKDRV", drv], what="put ASOAKDRV")
    run_cmd([img_tool, "put", image_path, "ALLOCSOK", loader], what="put ALLOCSOK")
    patch_info = patch_firstapp_in_kernel_mnt(image_path, img_tool)
    recipe = {
        "schema": 1,
        "mode": "release_free_ab",
        "seed": f"{SEED:#010x}",
        "image": str(image_path).replace("\\", "/"),
        "asoakdrv": str(drv).replace("\\", "/"),
        "firstapp_patch": patch_info,
        "abi": {
            "FreePage": "EAX=phys page; may lower page_start",
            "ReleasePages": "EAX=lin base, ECX=count; BTS bitmap; local page_start candidate discarded",
            "KernelAlloc": "stdcall size → mapped lin (uses alloc_page internally)",
        },
    }
    path = resolve("dev_build/allocsoak/release-free-ab-recipe.json")
    path.write_text(json.dumps(recipe, indent=2) + "\n", encoding="utf-8")
    return recipe


def _digest(sample: dict[str, Any] | None) -> str | None:
    if not sample:
        return None
    d = sample.get("bitmap_digest") or {}
    return d.get("digest")


def _pair_delta(before: dict[str, Any] | None, after: dict[str, Any] | None) -> dict[str, Any]:
    if not before or not after:
        return {"ok": False, "error": "missing sample"}
    return {
        "pages_free_before": before.get("pages_free"),
        "pages_free_after": after.get("pages_free"),
        "pages_free_delta": (
            after["pages_free"] - before["pages_free"]
            if isinstance(before.get("pages_free"), int)
            and isinstance(after.get("pages_free"), int)
            else None
        ),
        "page_start_before": before.get("page_start"),
        "page_start_after": after.get("page_start"),
        "page_start_changed": before.get("page_start") != after.get("page_start"),
        "page_start_delta": (
            after["page_start"] - before["page_start"]
            if isinstance(before.get("page_start"), int)
            and isinstance(after.get("page_start"), int)
            else None
        ),
        "digest_before": _digest(before),
        "digest_after": _digest(after),
        "digest_changed": _digest(before) != _digest(after),
    }


def classify_ab(free_case: dict[str, Any], rel_case: dict[str, Any]) -> dict[str, Any]:
    """Interpret observed deltas against legacy expectations."""
    notes: list[str] = []
    free_ps = free_case.get("page_start_changed")
    rel_ps = rel_case.get("page_start_changed")
    free_pf = free_case.get("pages_free_delta")
    rel_pf = rel_case.get("pages_free_delta")

    # free_page of a below-cursor page should increase pages_free by 1 and
    # typically lower page_start. release_pages should increase pages_free by 1
    # (if PTE was present) and leave page_start unchanged.
    expected = {
        "free_page_pages_free_delta": 1,
        "release_pages_pages_free_delta": 1,
        "free_page_page_start_changes": True,
        "release_pages_page_start_unchanged": True,
    }

    free_pf_ok = free_pf == 1
    rel_pf_ok = rel_pf == 1
    free_ps_ok = free_ps is True
    rel_ps_ok = rel_ps is False

    if not free_pf_ok:
        notes.append(f"free_page pages_free_delta={free_pf} (expected +1)")
    if not rel_pf_ok:
        notes.append(f"release_pages pages_free_delta={rel_pf} (expected +1)")
    if free_ps is False:
        notes.append(
            "free_page did not change page_start "
            "(possible if freed dword was not below cursor)"
        )
    if rel_ps is True:
        notes.append("release_pages CHANGED page_start — contradicts design hypothesis")

    proven = free_pf_ok and rel_pf_ok and free_ps_ok and rel_ps_ok
    partial = (free_pf_ok or rel_pf_ok) and not proven
    if proven:
        decision = "RELEASE/FREE PAGE_START DIFFERENCE PROVEN"
    elif free_case.get("ok") is False or rel_case.get("ok") is False:
        decision = "RELEASE/FREE EXPERIMENT BLOCKED"
    elif partial or (free_ps_ok != rel_ps_ok):
        decision = "RELEASE/FREE EXPERIMENT PARTIAL"
    else:
        decision = "RELEASE/FREE EXPERIMENT BLOCKED"

    return {
        "decision": decision,
        "expected": expected,
        "free_page_pages_free_ok": free_pf_ok,
        "release_pages_pages_free_ok": rel_pf_ok,
        "free_page_page_start_changed": free_ps,
        "release_pages_page_start_changed": rel_ps,
        "difference_observed": free_ps is True and rel_ps is False,
        "notes": notes,
    }


def run_one(*, port: int, wait_s: float, sample_interval: float, digest_bytes: int, artifact: Path, disks: list[str]) -> dict[str, Any]:
    recipe = prepare_ab_image(delete=None)
    image_path = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
    symbols = resolve_allocator_symbols()
    cfg = load_config()
    qemu = find_qemu(cfg["qemu"]["executables"])
    out_dir = resolve("dev_build/allocsoak")
    ppm = out_dir / "release-free-ab.ppm"
    if ppm.is_file():
        ppm.unlink()

    result: dict[str, Any] = {
        "schema": 1,
        "experiment": "release_free_page_start_ab",
        "seed": f"{SEED:#010x}",
        "recipe": recipe,
        "image": str(image_path).replace("\\", "/"),
        "symbols": {
            "pages_free": symbols["pages_free"],
            "page_start": symbols["page_start"],
            "sys_pgmap": symbols["sys_pgmap"],
        },
        "legacy": {
            "free_page": "EAX=phys; BTS; adc pages_free; maybe mov page_start if freed dword < cursor",
            "release_pages": "EAX=lin,ECX=count; mutex; clear PTE+invlpg; BTS; update local ebx; NEVER store page_start",
        },
    }

    resets = [0]
    shutdowns = [0]
    sock: socket.socket | None = None
    proc: subprocess.Popen[bytes] | None = None
    early_samples: list[dict[str, Any]] = []
    tagged: list[dict[str, Any]] = []

    try:
        qargs = build_qemu_argv(cfg, image_path=image_path, disks=disks or None, headless=True)
        for i, a in enumerate(qargs):
            if a == "-qmp" and i + 1 < len(qargs):
                qargs[i + 1] = f"tcp:127.0.0.1:{port},server,nowait"
        proc = subprocess.Popen([str(qemu), *qargs], cwd=str(ROOT))
        sock = qmp_connect("127.0.0.1", port, timeout=45.0)
        greet = qmp_recv_obj(sock)
        if "QMP" not in greet:
            raise RuntimeError(f"bad greeting {greet}")
        qmp_exec(sock, {"execute": "qmp_capabilities"})

        # Latch FIRST host sample after each marker appears. Do not keep
        # overwriting after the Delay window — later FreePage(fillers) /
        # KernelAlloc would contaminate free_after / rel_after.
        deadline = time.time() + wait_s
        last_board = ""
        latched: dict[str, dict[str, Any]] = {}
        latch_keys = (
            ("FREE BEFORE", "free_before"),
            ("FREE AFTER", "free_after"),
            ("REL BEFORE", "rel_before"),
            ("REL AFTER", "rel_after"),
        )
        pe_done = False
        while time.time() < deadline:
            drain_events(sock, resets, shutdowns)
            if resets[0] or shutdowns[0]:
                break
            try:
                sample = sample_allocator(sock, symbols, digest_max_bytes=digest_bytes)
                early_samples.append(sample)
            except Exception:  # noqa: BLE001
                sample = None

            try:
                board = scrape_msg_board(sock, symbols) or ""
            except Exception:  # noqa: BLE001
                board = last_board
            if board != last_board:
                last_board = board
            for needle, key in latch_keys:
                if key in latched:
                    continue
                if needle not in board:
                    continue
                # Fresh xp on first sight of each marker (avoid shared snap).
                try:
                    snap = sample_allocator(sock, symbols, digest_max_bytes=digest_bytes)
                except Exception:  # noqa: BLE001
                    snap = sample
                if snap is None:
                    continue
                latched[key] = snap
                tagged.append({"phase": key, "sample": snap, "ts": time.time()})
            if "ALLOCSOK PASS" in board or "ALLOCSOK FAIL" in board:
                pe_done = True

            if pe_done and len(latched) >= 4:
                # Brief settle then stop — desktop optional for this A/B.
                time.sleep(0.5)
                break

            if ppm.exists():
                try:
                    ppm.unlink()
                except OSError:
                    pass
            try:
                qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
                time.sleep(0.15)
                _w, _h, nb = count_non_black_ppm(ppm)
                if nb >= 100000 and pe_done and len(latched) >= 4:
                    break
            except Exception:  # noqa: BLE001
                pass
            time.sleep(sample_interval)

        board_text = last_board
        markers = [ln.strip() for ln in board_text.splitlines() if "ALLOCSOK" in ln]
        result["allocsok_markers"] = markers
        result["tagged_sample_count"] = len(tagged)
        result["latched_phases"] = sorted(latched.keys())

        free_before = latched.get("free_before")
        free_after = latched.get("free_after")
        rel_before = latched.get("rel_before")
        rel_after = latched.get("rel_after")

        free_case = _pair_delta(free_before, free_after)
        free_case["ok"] = free_before is not None and free_after is not None
        rel_case = _pair_delta(rel_before, rel_after)
        rel_case["ok"] = rel_before is not None and rel_after is not None

        result["free_page_case"] = free_case
        result["release_pages_case"] = rel_case
        result["classification"] = classify_ab(free_case, rel_case)

        pe_pass = any(m.startswith("ALLOCSOK PASS") for m in markers)
        status = qmp_exec(sock, {"execute": "query-status"})
        st = status.get("return", {}).get("status")
        final = sample_allocator(sock, symbols, digest_max_bytes=digest_bytes)
        if ppm.exists():
            try:
                ppm.unlink()
            except OSError:
                pass
        qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
        time.sleep(0.2)
        w, h, nb = count_non_black_ppm(ppm)

        result["qemu"] = {
            "status": st,
            "resets": resets[0],
            "shutdowns": shutdowns[0],
            "non_black": nb,
            "desktop_reached": nb >= 100000,
        }
        result["final"] = final
        result["result"] = {
            "passed": bool(
                pe_pass
                and resets[0] == 0
                and shutdowns[0] == 0
                and result["classification"]["decision"]
                == "RELEASE/FREE PAGE_START DIFFERENCE PROVEN"
            ),
            "pe_pass": pe_pass,
            "decision": result["classification"]["decision"],
        }
        artifact.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        log.info(
            "AB decision=%s free_ps_changed=%s rel_ps_changed=%s → %s",
            result["classification"]["decision"],
            free_case.get("page_start_changed"),
            rel_case.get("page_start_changed"),
            artifact,
        )
        return result
    finally:
        if sock is not None:
            try:
                sock.close()
            except OSError:
                pass
        if proc is not None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=4575)
    ap.add_argument("--wait", type=float, default=90.0)
    ap.add_argument("--sample-interval", type=float, default=0.15)
    ap.add_argument("--digest-bytes", type=int, default=4096)
    ap.add_argument("--disk", action="append", default=[])
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--out-dir", default="dev_build/allocsoak")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args(argv)
    setup_logging(args.verbose)

    out_dir = resolve(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    summary: dict[str, Any] = {
        "schema": 1,
        "seed": f"{SEED:#010x}",
        "runs": [],
        "decision": None,
    }

    total = 1 + max(0, args.repeats)
    for i in range(total):
        name = "release-free-page-ab.json" if i == 0 else f"release-free-page-ab-run{i+1}.json"
        art = out_dir / name
        try:
            run = run_one(
                port=args.port,
                wait_s=args.wait,
                sample_interval=args.sample_interval,
                digest_bytes=args.digest_bytes,
                artifact=art,
                disks=list(args.disk),
            )
        except Exception as ex:  # noqa: BLE001
            log.error("run %s failed: %s", i + 1, ex)
            summary["runs"].append({"error": repr(ex), "artifact": str(art).replace("\\", "/")})
            summary["decision"] = "RELEASE/FREE EXPERIMENT BLOCKED"
            break
        summary["runs"].append(
            {
                "artifact": str(art).replace("\\", "/"),
                "decision": run["classification"]["decision"],
                "pe_pass": run["result"]["pe_pass"],
                "qemu_resets": run["qemu"]["resets"],
                "free_page": {
                    "page_start_changed": run["free_page_case"].get("page_start_changed"),
                    "pages_free_delta": run["free_page_case"].get("pages_free_delta"),
                    "ok": run["free_page_case"].get("ok"),
                },
                "release_pages": {
                    "page_start_changed": run["release_pages_case"].get("page_start_changed"),
                    "pages_free_delta": run["release_pages_case"].get("pages_free_delta"),
                    "ok": run["release_pages_case"].get("ok"),
                },
            }
        )
        if run["qemu"]["resets"] or run["qemu"]["shutdowns"]:
            summary["decision"] = "RELEASE/FREE EXPERIMENT BLOCKED"
            summary["boundary_reason"] = "RESET/shutdown"
            break

    decisions = [r.get("decision") for r in summary["runs"] if r.get("decision")]
    if summary.get("decision") is None:
        if decisions and all(d == "RELEASE/FREE PAGE_START DIFFERENCE PROVEN" for d in decisions):
            summary["decision"] = "RELEASE/FREE PAGE_START DIFFERENCE PROVEN"
        elif any(d == "RELEASE/FREE PAGE_START DIFFERENCE PROVEN" for d in decisions):
            summary["decision"] = "RELEASE/FREE EXPERIMENT PARTIAL"
            summary["boundary_reason"] = "not all repeats fully proven"
        elif any(d == "RELEASE/FREE EXPERIMENT PARTIAL" for d in decisions):
            summary["decision"] = "RELEASE/FREE EXPERIMENT PARTIAL"
        else:
            summary["decision"] = "RELEASE/FREE EXPERIMENT BLOCKED"

    out = out_dir / "release-free-page-ab-summary.json"
    # Also write canonical name pointing at first run content merge
    out.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    # Ensure release-free-page-ab.json exists (first run artifact)
    print(json.dumps({"decision": summary["decision"], "summary": str(out)}, indent=2))
    log.info("SUMMARY %s → %s", summary["decision"], out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
