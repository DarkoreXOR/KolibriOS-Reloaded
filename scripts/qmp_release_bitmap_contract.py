"""Disposable §19 release-bitmap ABI smoke (RBPB) — no production call-out.

Builds ``asoakdrv_rbpb.asm``, installs on CoW as firstapp PE payload, waits for
``RBPB PASS`` via QMP msg-board scrape.
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
    scrape_msg_board,
)
from resolve_allocator_symbols import resolve_allocator_symbols  # noqa: E402
from run_qemu import build_qemu_argv, find_qemu  # noqa: E402

SEED = 0x5047424D


def build_asoakdrv_rbpb() -> Path:
    cfg = load_config()
    fasm = resolve(cfg["kernel"]["fasm"])
    src = resolve("tools/allocsoak/asoakdrv_rbpb.asm")
    out_dir = resolve("dev_build/allocsoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / "ASOAKDRV"
    if out.is_file():
        out.unlink()
    run_cmd([fasm, src, out], what="FASM asoakdrv_rbpb")
    log.info("asoakdrv_rbpb built: %s (%s bytes)", out, out.stat().st_size)
    return out


def prepare_rbpb_image(*, delete: bool | None = None) -> dict[str, Any]:
    cfg = load_config()
    image_path = prepare_image(cfg, delete=delete)
    drv = build_asoakdrv_rbpb()
    loader = build_allocsoak()
    img_tool = ensure_kolibri_img(cfg["image"])
    run_cmd([img_tool, "put", image_path, "ASOAKDRV", drv], what="put ASOAKDRV")
    run_cmd([img_tool, "put", image_path, "ALLOCSOK", loader], what="put ALLOCSOK")
    patch_info = patch_firstapp_in_kernel_mnt(image_path, img_tool)
    return {
        "schema": 1,
        "mode": "rbpb_abi_smoke",
        "seed": f"{SEED:#010x}",
        "image": str(image_path).replace("\\", "/"),
        "asoakdrv": str(drv).replace("\\", "/"),
        "firstapp_patch": patch_info,
        "note": "Test-only FASM shim for §19 ABI; no production release_pages change",
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=4580)
    ap.add_argument("--wait", type=float, default=60.0)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--out", default="dev_build/allocsoak/release-bitmap-contract-smoke.json")
    args = ap.parse_args(argv)
    setup_logging(args.verbose)

    recipe = prepare_rbpb_image()
    image_path = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
    symbols = resolve_allocator_symbols()
    cfg = load_config()
    qemu = find_qemu(cfg["qemu"]["executables"])
    out_path = resolve(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    ppm = resolve("dev_build/allocsoak/rbpb-smoke.ppm")
    if ppm.is_file():
        ppm.unlink()

    resets = [0]
    shutdowns = [0]
    sock: socket.socket | None = None
    proc: subprocess.Popen[bytes] | None = None
    board = ""
    result: dict[str, Any] = {
        "schema": 1,
        "experiment": "release_bitmap_contract_abi_smoke",
        "seed": f"{SEED:#010x}",
        "marker": "RBPB",
        "recipe": recipe,
        "abi": {
            "entry": "plain call / ret 0",
            "input": "EAX = page_index",
            "output": "EAX = delta {0,1}",
            "preserved": ["EBX", "ECX", "EDX", "ESI", "EDI", "EBP"],
            "clobbered": ["EAX", "EFLAGS"],
            "flags": "not a public contract",
            "df": "unchanged",
            "page_start": "canary never written by shim",
        },
    }

    try:
        qargs = build_qemu_argv(cfg, image_path=image_path, disks=None, headless=True)
        for i, a in enumerate(qargs):
            if a == "-qmp" and i + 1 < len(qargs):
                qargs[i + 1] = f"tcp:127.0.0.1:{args.port},server,nowait"
        proc = subprocess.Popen([str(qemu), *qargs], cwd=str(ROOT))
        sock = qmp_connect("127.0.0.1", args.port, timeout=45.0)
        greet = qmp_recv_obj(sock)
        if "QMP" not in greet:
            raise RuntimeError(f"bad greeting {greet}")
        qmp_exec(sock, {"execute": "qmp_capabilities"})

        deadline = time.time() + args.wait
        pe_pass = False
        pe_fail = False
        while time.time() < deadline:
            drain_events(sock, resets, shutdowns)
            if resets[0] or shutdowns[0]:
                break
            try:
                board = scrape_msg_board(sock, symbols) or board
            except Exception:  # noqa: BLE001
                pass
            if "RBPB PASS" in board:
                pe_pass = True
                break
            if "RBPB FAIL" in board:
                pe_fail = True
                break
            time.sleep(0.2)

        if ppm.exists():
            try:
                ppm.unlink()
            except OSError:
                pass
        try:
            qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
            time.sleep(0.2)
            _w, _h, nb = count_non_black_ppm(ppm)
        except Exception:  # noqa: BLE001
            nb = 0
        status = qmp_exec(sock, {"execute": "query-status"})
        st = status.get("return", {}).get("status")

        markers = [ln.strip() for ln in board.splitlines() if "RBPB" in ln]
        result["markers"] = markers
        result["qemu"] = {
            "status": st,
            "resets": resets[0],
            "shutdowns": shutdowns[0],
            "non_black": nb,
        }
        result["result"] = {
            "passed": bool(pe_pass and resets[0] == 0 and shutdowns[0] == 0 and not pe_fail),
            "rbpb_pass": pe_pass,
            "rbpb_fail": pe_fail,
        }
        out_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        log.info(
            "RBPB passed=%s markers=%s → %s",
            result["result"]["passed"],
            markers,
            out_path,
        )
        print(json.dumps({"passed": result["result"]["passed"], "artifact": str(out_path)}, indent=2))
        return 0 if result["result"]["passed"] else 1
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
        if ppm.is_file():
            try:
                ppm.unlink()
            except OSError:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
