"""One-shot headless QEMU QMP desktop smoke for migration cuts.

Usage:
  python scripts/qmp_desktop_smoke.py [--ppm PATH] [--wait SECS] [--port PORT]
"""
from __future__ import annotations

import argparse
import json
import socket
import struct
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from common import LAST_IMAGE_MARKER, load_config, resolve  # noqa: E402
from run_qemu import build_qemu_argv, find_qemu  # noqa: E402
import subprocess


def qmp_connect(host: str, port: int, timeout: float = 30.0) -> socket.socket:
    deadline = time.time() + timeout
    last_err: Exception | None = None
    while time.time() < deadline:
        try:
            s = socket.create_connection((host, port), timeout=2.0)
            s.settimeout(10.0)
            return s
        except OSError as e:
            last_err = e
            time.sleep(0.25)
    raise SystemExit(f"QMP connect failed: {last_err}")


def qmp_recv_obj(sock: socket.socket) -> dict:
    buf = b""
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            raise SystemExit("QMP connection closed")
        buf += chunk
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            line = line.strip()
            if not line:
                continue
            return json.loads(line.decode("utf-8"))


def qmp_exec(sock: socket.socket, cmd: dict) -> dict:
    sock.sendall((json.dumps(cmd) + "\n").encode("utf-8"))
    while True:
        obj = qmp_recv_obj(sock)
        if "error" in obj or "return" in obj or obj.get("event"):
            if "error" in obj:
                raise SystemExit(f"QMP error: {obj}")
            if "return" in obj:
                return obj
            # ignore events


def count_non_black_ppm(path: Path) -> tuple[int, int, int]:
    data = path.read_bytes()
    if not data.startswith(b"P6"):
        raise SystemExit(f"not a P6 PPM: {path}")
    # Parse header: P6\nW H\n255\n
    parts = []
    i = 2
    while len(parts) < 3:
        while i < len(data) and data[i] in b" \t\r\n":
            i += 1
        if i < len(data) and data[i] == ord("#"):
            while i < len(data) and data[i] not in b"\n":
                i += 1
            continue
        start = i
        while i < len(data) and data[i] not in b" \t\r\n":
            i += 1
        parts.append(data[start:i].decode("ascii"))
    width, height, maxval = int(parts[0]), int(parts[1]), int(parts[2])
    if maxval != 255:
        raise SystemExit(f"unexpected maxval {maxval}")
    while i < len(data) and data[i] in b" \t\r":
        i += 1
    if i < len(data) and data[i] == 10:
        i += 1
    pixels = data[i:]
    need = width * height * 3
    if len(pixels) < need:
        raise SystemExit(f"PPM truncated: {len(pixels)} < {need}")
    non_black = 0
    for off in range(0, need, 3):
        if pixels[off] | pixels[off + 1] | pixels[off + 2]:
            non_black += 1
    return width, height, non_black


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ppm", default="dev_build/qmp-smoke.ppm")
    ap.add_argument("--wait", type=float, default=12.0)
    ap.add_argument("--port", type=int, default=4550)
    ap.add_argument("--disk", action="append", default=[])
    ap.add_argument("--bus", choices=("ide", "ahci"), default="ide")
    # Splash-class Kolibri boots (REG-015/016) sit around ~8k non-black pixels.
    # Desktop is ~779380. Default floor rejects splash-only without treating
    # the count as the sole success criterion (RESET/shutdown/status still apply).
    ap.add_argument("--min-non-black", type=int, default=100000)
    ap.add_argument(
        "--splash-max",
        type=int,
        default=20000,
        help="non-black at or below this is classified as splash-only (FAIL)",
    )
    args = ap.parse_args()

    cfg = load_config()
    if not LAST_IMAGE_MARKER.is_file():
        raise SystemExit("missing last_image.txt — run prepare_image.py")
    image_path = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
    qemu = find_qemu(cfg["qemu"]["executables"])
    qargs = build_qemu_argv(
        cfg,
        image_path=image_path,
        disks=args.disk or None,
        headless=True,
        bus=args.bus,
    )
    # Override QMP port if requested
    for i, a in enumerate(qargs):
        if a == "-qmp" and i + 1 < len(qargs):
            qargs[i + 1] = f"tcp:127.0.0.1:{args.port},server,nowait"

    ppm = resolve(args.ppm)
    ppm.parent.mkdir(parents=True, exist_ok=True)
    if ppm.exists():
        ppm.unlink()

    proc = subprocess.Popen([qemu, *qargs], cwd=str(ROOT))
    try:
        sock = qmp_connect("127.0.0.1", args.port)
        # greeting
        greet = qmp_recv_obj(sock)
        if "QMP" not in greet:
            raise SystemExit(f"unexpected QMP greeting: {greet}")
        qmp_exec(sock, {"execute": "qmp_capabilities"})
        deadline = time.time() + args.wait
        resets = 0
        sock.settimeout(1.0)
        while time.time() < deadline:
            try:
                obj = qmp_recv_obj(sock)
            except (TimeoutError, socket.timeout):
                continue
            if obj.get("event") == "RESET":
                resets += 1
                print(f"QMP RESET event #{resets}")
        sock.settimeout(10.0)
        if resets:
            raise SystemExit(f"guest reset {resets} time(s) during wait — boot loop")
        status = qmp_exec(sock, {"execute": "query-status"})
        st = status.get("return", {}).get("status")
        print(f"query-status: {st}")
        if st != "running":
            raise SystemExit(f"expected running, got {st}")
        # screendump
        qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
        # give filesystem a moment
        time.sleep(0.5)
        w, h, nb = count_non_black_ppm(ppm)
        print(f"screendump: {ppm} {w}x{h} non-black={nb} resets={resets}")
        if nb <= args.splash_max:
            kind = "black" if nb < 1000 else "splash-only"
            raise SystemExit(
                f"{kind} framebuffer (non-black={nb} <= splash-max={args.splash_max}) "
                "— desktop not reached"
            )
        if nb < args.min_non_black:
            raise SystemExit(
                f"framebuffer short of desktop floor "
                f"(non-black={nb} < min-non-black={args.min_non_black})"
            )
        print("desktop-reached")
        print("PASS")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


if __name__ == "__main__":
    main()
