"""Host-only QMP EXT SetFileInfo write/readback soak.

CoW EXT disk + firstapp EXTSOAK guest driver. Primary oracle:

  guest SetFileInfo → GetFileInfo → (embedded disk_sync) → host EXT inode parse

Does **not** migrate SetFileInfo or add production gates.

Exit classes:
  0  PASS
  2  boot / desktop / marker failure
  3  QEMU RESET
  4  QEMU shutdown / not running
  5  symbol-resolution failure
  6  guest report / marker mismatch
  7  host inode / metadata mismatch (sync persistence failure)
  8  timeout
  9  tooling error
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from common import LAST_IMAGE_MARKER, load_config, log, resolve, setup_logging  # noqa: E402
from ext_setfileinfo_oracle import (  # noqa: E402
    classify_log_side_effects,
    expected_primary_unix,
    extract_ext2_root_file,
    metadata_diff,
    parse_guest_report,
    read_inode_times,
    sha256_file,
)
from prepare_extsoak_image import prepare_ext_cow, prepare_extsoak_image  # noqa: E402
from build_extsoak import build_extsoak  # noqa: E402
from resolve_allocator_symbols import (  # noqa: E402
    SymbolResolveError,
    resolve_allocator_symbols,
)
from run_qemu import append_ide_image, build_qemu_argv, find_qemu  # noqa: E402

EXIT_PASS = 0
EXIT_DESKTOP = 2
EXIT_RESET = 3
EXIT_SHUTDOWN = 4
EXIT_SYMBOLS = 5
EXIT_GUEST = 6
EXIT_HOST = 7
EXIT_TIMEOUT = 8
EXIT_TOOLING = 9
EXIT_ESFI_LOG = 10
EXIT_DEBUGFS = 11


class SoakFailure(Exception):
    def __init__(self, code: int, cls: str, message: str):
        super().__init__(message)
        self.code = code
        self.cls = cls


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
    raise SoakFailure(EXIT_TOOLING, "qmp", f"QMP connect failed: {last_err}")


def qmp_recv_obj(sock: socket.socket) -> dict:
    buf = b""
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            raise SoakFailure(EXIT_SHUTDOWN, "shutdown", "QMP connection closed")
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
        if "error" in obj:
            raise SoakFailure(EXIT_TOOLING, "qmp", f"QMP error: {obj}")
        if "return" in obj:
            return obj


def drain_events(sock: socket.socket, resets: list[int], shutdowns: list[int]) -> None:
    sock.setblocking(False)
    try:
        while True:
            try:
                chunk = sock.recv(65536)
            except BlockingIOError:
                break
            if not chunk:
                shutdowns[0] += 1
                break
            for line in chunk.split(b"\n"):
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line.decode("utf-8"))
                except json.JSONDecodeError:
                    continue
                ev = obj.get("event")
                if ev == "RESET":
                    resets[0] += 1
                elif ev in ("SHUTDOWN", "POWERDOWN"):
                    shutdowns[0] += 1
    finally:
        sock.setblocking(True)


def parse_xp_dwords(text: str) -> list[int]:
    vals: list[int] = []
    for m in re.finditer(r"0x([0-9a-fA-F]+)", text):
        vals.append(int(m.group(1), 16))
    return vals


def xp_bytes(sock: socket.socket, pa: int, n: int) -> bytes:
    words = (n + 3) // 4
    cmd = f"xp /{max(1, words)}xw {pa:#x}"
    resp = qmp_exec(sock, {"execute": "human-monitor-command", "arguments": {"command-line": cmd}})
    text = resp.get("return") or ""
    dwords = parse_xp_dwords(text if isinstance(text, str) else str(text))
    out = bytearray()
    for w in dwords:
        out.extend(struct.pack("<I", w & 0xFFFFFFFF))
    return bytes(out[:n])


def scrape_msg_board(sock: socket.socket, symbols: dict[str, Any], nbytes: int = 8192) -> str:
    extras = symbols.get("extras") or {}
    mb = extras.get("msg_board_data")
    if not isinstance(mb, dict) or "physical_address" not in mb:
        return ""
    blob = xp_bytes(sock, int(mb["physical_address"]), nbytes)
    chars: list[str] = []
    for b in blob:
        if 32 <= b < 127 or b in (10, 13):
            chars.append(chr(b))
        elif b == 0:
            chars.append("\n")
    return "".join(chars)


def count_non_black_ppm(path: Path) -> int:
    data = path.read_bytes()
    if not data.startswith(b"P6"):
        return 0
    i = 2
    parts: list[int] = []
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
        parts.append(int(data[start:i]))
    w, h, _maxv = parts
    pixels = data[i + 1 :]
    nb = 0
    for j in range(0, min(len(pixels), w * h * 3), 3):
        if pixels[j] | pixels[j + 1] | pixels[j + 2]:
            nb += 1
    return nb


def wait_markers_and_desktop(
    sock: socket.socket,
    *,
    symbols: dict[str, Any],
    ppm: Path,
    wait_s: float,
    min_non_black: int,
    resets: list[int],
    shutdowns: list[int],
) -> dict[str, Any]:
    deadline = time.time() + wait_s
    saw_pass = False
    saw_fail = False
    saw_start = False
    board_excerpt = ""
    desktop = False
    best_nb = 0
    while time.time() < deadline:
        drain_events(sock, resets, shutdowns)
        if resets[0]:
            raise SoakFailure(EXIT_RESET, "reset", f"guest RESET x{resets[0]}")
        if shutdowns[0]:
            raise SoakFailure(EXIT_SHUTDOWN, "shutdown", "guest shut down early")
        try:
            board_excerpt = scrape_msg_board(sock, symbols)
        except Exception:  # noqa: BLE001
            board_excerpt = board_excerpt
        if "EXTSOAK START" in board_excerpt:
            saw_start = True
        if "EXTSOAK PASS" in board_excerpt:
            saw_pass = True
        if "EXTSOAK FAIL" in board_excerpt:
            saw_fail = True
        has_fin = "EXTSOAK FIN " in board_excerpt
        has_log = "EXTSOAK LOG" in board_excerpt and "LOGFAIL" not in board_excerpt
        if (saw_pass and has_fin) or saw_fail:
            try:
                qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
                if ppm.is_file():
                    best_nb = count_non_black_ppm(ppm)
                    desktop = best_nb >= min_non_black
            except Exception:  # noqa: BLE001
                pass
            # Give LAUNCHER a brief window if PASS already seen
            if saw_pass:
                t_end = time.time() + 25.0
                while time.time() < t_end and not desktop:
                    drain_events(sock, resets, shutdowns)
                    if resets[0]:
                        raise SoakFailure(EXIT_RESET, "reset", f"RESET after PASS x{resets[0]}")
                    try:
                        board_excerpt = scrape_msg_board(sock, symbols)
                    except Exception:  # noqa: BLE001
                        pass
                    try:
                        qmp_exec(
                            sock,
                            {"execute": "screendump", "arguments": {"filename": str(ppm)}},
                        )
                        if ppm.is_file():
                            best_nb = count_non_black_ppm(ppm)
                            desktop = best_nb >= min_non_black
                    except Exception:  # noqa: BLE001
                        pass
                    time.sleep(1.0)
            break
        time.sleep(0.5)
    else:
        raise SoakFailure(
            EXIT_TIMEOUT,
            "timeout",
            f"no EXTSOAK PASS/FAIL within {wait_s}s; board={board_excerpt[-400:]!r}",
        )
    markers = [ln.strip() for ln in board_excerpt.splitlines() if "EXTSOAK" in ln]
    return {
        "saw_start": saw_start,
        "saw_pass": saw_pass,
        "saw_fail": saw_fail,
        "desktop_reached": desktop,
        "non_black": best_nb,
        "markers": markers,
        "msg_board_excerpt": board_excerpt[-2000:] if board_excerpt else None,
    }


def extract_esfi_log_from_cow(
    cow: Path, out_dir: Path, *, expected_run_id: int | None
) -> dict[str, Any]:
    """Extract durable ``ESFI.LOG`` from the EXT CoW (primary guest evidence file)."""
    host = out_dir / "ESFI.LOG"
    try:
        blob = extract_ext2_root_file(cow, "ESFI.LOG", max_bytes=512)
    except Exception as ex:  # noqa: BLE001
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-extraction",
            f"ESFI.LOG not readable from CoW: {ex}",
        ) from ex
    host.write_bytes(blob)
    parsed = parse_guest_report(blob)
    (out_dir / "guest-report.json").write_text(
        json.dumps(parsed, indent=2) + "\n", encoding="utf-8"
    )
    if parsed.get("error"):
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-parse",
            f"ESFI.LOG unreadable/bad: {parsed}",
        )
    if expected_run_id is not None and parsed.get("run_id") != expected_run_id:
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-run-id",
            f"run_id mismatch: log=0x{parsed.get('run_id', 0):08X} "
            f"expected=0x{expected_run_id:08X}",
        )
    if parsed.get("target_inode_hint") not in (None, 12):
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-inode",
            f"target_inode_hint={parsed.get('target_inode_hint')}",
        )
    if parsed.get("path_tag") not in (None, "ROOT"):
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-path",
            f"path_tag={parsed.get('path_tag')!r}",
        )
    fd = parsed.get("flags_decode") or {}
    if not fd.get("pass"):
        raise SoakFailure(EXIT_ESFI_LOG, "esfi-log-pass", "FLAG_PASS clear in ESFI.LOG")
    if not fd.get("set_ok") or not fd.get("get2_ok") or not fd.get("get3_ok"):
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-markers",
            f"missing set/get markers in ESFI.LOG flags={fd}",
        )
    if not fd.get("log_ok"):
        raise SoakFailure(
            EXIT_ESFI_LOG,
            "esfi-log-persistence",
            "FLAG_LOG_OK clear — guest create/write of ESFI.LOG failed",
        )
    parsed["source"] = "ext_cow:/hd0/1/ESFI.LOG"
    parsed["host_path"] = str(host).replace("\\", "/")
    return parsed


def parse_board_hex_markers(board: str) -> dict[str, str]:
    """Parse ``EXTSOAK IMM <32hex>`` / ``EXTSOAK FIN <32hex>`` lines."""
    out: dict[str, str] = {}
    for kind in ("IMM", "FIN"):
        m = re.search(rf"EXTSOAK {kind} ([0-9a-fA-F]{{32}})", board)
        if m:
            hx = m.group(1).lower()
            out[f"{kind.lower()}_atime_hex"] = hx[0:16]
            out[f"{kind.lower()}_mtime_hex"] = hx[16:32]
    return out


def refresh_boot_run_id(boot_image: Path) -> int:
    """Rebuild/put EXTSOAK1 with a fresh run_id onto an existing boot CoW."""
    from prepare_image import ensure_kolibri_img

    cfg = load_config()
    bin_path, rid = build_extsoak()
    img_tool = ensure_kolibri_img(cfg["image"])
    subprocess.run(
        [str(img_tool), "put", str(boot_image), "EXTSOAK1", str(bin_path)],
        cwd=str(ROOT),
        check=True,
        capture_output=True,
        text=True,
    )
    return rid


def run_once(
    *,
    run_id: str,
    port: int,
    wait_s: float,
    min_non_black: int,
    prepare: bool,
    bus: str,
) -> dict[str, Any]:
    cfg = load_config()
    out_dir = resolve("dev_build/extsoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    run_dir = out_dir / f"run-{run_id}"
    run_dir.mkdir(parents=True, exist_ok=True)
    ppm = run_dir / "screen.ppm"

    result: dict[str, Any] = {
        "run_id": run_id,
        "ok": False,
        "failure_class": None,
        "qemu": {"resets": 0, "shutdowns": 0},
        "notes": [],
    }

    expected_run_id: int | None = None
    if prepare:
        boot_image, cow = prepare_extsoak_image()
        recipe_path = resolve("dev_build/extsoak/recipe.json")
        if recipe_path.is_file():
            expected_run_id = int(json.loads(recipe_path.read_text(encoding="utf-8")).get("run_id") or 0)
    else:
        if not LAST_IMAGE_MARKER.is_file():
            raise SoakFailure(EXIT_TOOLING, "tooling", "missing last_image.txt")
        boot_image = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
        ref = resolve("images/ext-image.img")
        cow = out_dir / "ext-cow.img"
        prepare_ext_cow(ref, cow)
        expected_run_id = refresh_boot_run_id(boot_image)

    result["boot_image"] = str(boot_image).replace("\\", "/")
    result["ext_cow"] = str(cow).replace("\\", "/")
    result["ext_cow_sha256_before"] = sha256_file(cow)
    result["reference_sha256"] = sha256_file(resolve("images/ext-image.img"))
    result["expected_run_id"] = expected_run_id
    result["expected_run_id_hex"] = (
        f"0x{expected_run_id:08X}" if expected_run_id is not None else None
    )

    before = read_inode_times(cow, "ROOT.TXT", try_debugfs=False)
    (run_dir / "before-inode.json").write_text(
        json.dumps(before, indent=2) + "\n", encoding="utf-8"
    )
    expected = expected_primary_unix()
    result["expected"] = expected
    result["before_inode"] = {
        "inode": before.get("inode"),
        "atime": before.get("atime"),
        "mtime": before.get("mtime"),
        "ctime": before.get("ctime"),
        "size": before.get("size"),
    }

    try:
        symbols = resolve_allocator_symbols()
    except SymbolResolveError as e:
        raise SoakFailure(EXIT_SYMBOLS, "symbols", str(e)) from e

    resets = [0]
    shutdowns = [0]
    proc: subprocess.Popen[bytes] | None = None
    sock: socket.socket | None = None

    try:
        qemu = find_qemu(cfg["qemu"]["executables"])
        qargs = build_qemu_argv(
            cfg,
            image_path=boot_image,
            disks=None,
            headless=True,
            use_testdisk=False,
            bus=bus,
        )
        append_ide_image(qargs, cow, 0)
        for i, a in enumerate(qargs):
            if a == "-qmp" and i + 1 < len(qargs):
                qargs[i + 1] = f"tcp:127.0.0.1:{port},server,nowait"

        proc = subprocess.Popen([str(qemu), *qargs], cwd=str(ROOT))
        sock = qmp_connect("127.0.0.1", port, timeout=30.0)
        greet = qmp_recv_obj(sock)
        if "QMP" not in greet:
            raise SoakFailure(EXIT_TOOLING, "qmp", f"unexpected greeting {greet}")
        qmp_exec(sock, {"execute": "qmp_capabilities"})

        wait_info = wait_markers_and_desktop(
            sock,
            symbols=symbols,
            ppm=ppm,
            wait_s=wait_s,
            min_non_black=min_non_black,
            resets=resets,
            shutdowns=shutdowns,
        )
        result["qemu"].update(wait_info)
        result["qemu"]["resets"] = resets[0]
        result["qemu"]["shutdowns"] = shutdowns[0]

        if wait_info.get("saw_fail") and not wait_info.get("saw_pass"):
            raise SoakFailure(EXIT_GUEST, "guest-marker", "EXTSOAK FAIL on msg_board")
        if not wait_info.get("saw_pass"):
            raise SoakFailure(EXIT_GUEST, "guest-marker", "EXTSOAK PASS not observed")

        # Clean ACPI-ish shutdown so disk flushes complete.
        try:
            qmp_exec(sock, {"execute": "system_powerdown"})
        except SoakFailure:
            pass
        # Wait for exit
        t0 = time.time()
        while time.time() - t0 < 30.0:
            drain_events(sock, resets, shutdowns)
            if proc.poll() is not None:
                break
            time.sleep(0.5)
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
            result["notes"].append("qemu terminated after powerdown timeout")

        if resets[0]:
            raise SoakFailure(EXIT_RESET, "reset", f"RESET during run x{resets[0]}")

        # Preserve final CoW under the run directory for artifact retention.
        cow_final = run_dir / "ext-cow.img"
        shutil.copy2(cow, cow_final)
        result["ext_cow_final"] = str(cow_final).replace("\\", "/")
        result["ext_cow_sha256_after"] = sha256_file(cow_final)

        guest = extract_esfi_log_from_cow(
            cow_final, run_dir, expected_run_id=expected_run_id
        )
        result["guest_report"] = guest
        board_src = "\n".join(wait_info.get("markers") or [])
        if not board_src.strip():
            board_src = wait_info.get("msg_board_excerpt") or ""
        board_hex = parse_board_hex_markers(board_src)
        result["board_hex"] = board_hex
        result["guest_evidence"] = "ESFI.LOG"

        exp_a = expected["atime_bdfe_hex"]
        exp_m = expected["mtime_bdfe_hex"]
        guest_ok = (
            guest.get("immediate_atime_hex") == exp_a
            and guest.get("immediate_mtime_hex") == exp_m
            and guest.get("final_atime_hex") == exp_a
            and guest.get("final_mtime_hex") == exp_m
            and guest.get("requested_atime_hex") == exp_a
            and guest.get("requested_mtime_hex") == exp_m
        )
        result["guest_vs_expected_bdfe"] = guest_ok
        if not guest_ok:
            raise SoakFailure(
                EXIT_GUEST,
                "guest-bdfe",
                "guest BDFE readback in ESFI.LOG != host expected encoding",
            )

        # Host inode after shutdown — independent persistence oracle (ROOT.TXT only)
        after = read_inode_times(cow_final, "ROOT.TXT", try_debugfs=True)
        (run_dir / "after-inode.json").write_text(
            json.dumps(after, indent=2) + "\n", encoding="utf-8"
        )
        result["after_inode"] = {
            "inode": after.get("inode"),
            "atime": after.get("atime"),
            "mtime": after.get("mtime"),
            "ctime": after.get("ctime"),
            "size": after.get("size"),
            "mode": after.get("mode"),
            "debugfs": after.get("debugfs"),
            "debugfs_status": after.get("debugfs_status"),
            "debugfs_probe": after.get("debugfs_probe"),
            "debugfs_crosscheck": after.get("debugfs_crosscheck"),
            "debugfs_error": after.get("debugfs_error"),
        }

        log_fx = classify_log_side_effects(cow_final)
        result["log_side_effects"] = log_fx
        if not log_fx.get("esfi_log_present"):
            raise SoakFailure(
                EXIT_ESFI_LOG,
                "esfi-log-missing",
                "ESFI.LOG not present on CoW after shutdown",
            )

        diff = metadata_diff(
            before,
            after,
            expected_atime=int(expected["atime_unix"]),
            expected_mtime=int(expected["mtime_unix"]),
            log_side_effects=log_fx,
        )
        result["metadata_diff"] = diff
        (run_dir / "metadata-diff.json").write_text(
            json.dumps(diff, indent=2) + "\n", encoding="utf-8"
        )
        if not diff.get("ok"):
            raise SoakFailure(
                EXIT_HOST,
                "host-inode",
                f"on-disk inode times mismatch: {diff}",
            )

        dbg_status = after.get("debugfs_status")
        result["debugfs"] = {
            "status": dbg_status,
            "probe": after.get("debugfs_probe"),
            "stat": after.get("debugfs"),
            "crosscheck": after.get("debugfs_crosscheck"),
            "error": after.get("debugfs_error"),
        }
        if dbg_status == "ok":
            xc = after.get("debugfs_crosscheck") or {}
            if not xc.get("ok"):
                raise SoakFailure(
                    EXIT_DEBUGFS,
                    "debugfs-mismatch",
                    f"mini vs debugfs mismatch: {xc}",
                )
            result["debugfs_class"] = "PASS"
        elif dbg_status == "DEBUGFS_CROSSCHECK_UNAVAILABLE":
            result["debugfs_class"] = "DEBUGFS_CROSSCHECK_UNAVAILABLE"
            result["notes"].append(
                "debugfs unavailable (Docker daemon stopped / no native debugfs); "
                "primary host mini parser still PASS"
            )
        else:
            raise SoakFailure(
                EXIT_DEBUGFS,
                "debugfs-error",
                f"debugfs status={dbg_status}: {after.get('debugfs_error')}",
            )

        result["ok"] = True
        result["decision_note"] = (
            "guest ESFI.LOG + host mini parser PASS"
            + (
                "; debugfs PASS"
                if result.get("debugfs_class") == "PASS"
                else "; DEBUGFS_CROSSCHECK_UNAVAILABLE"
            )
        )
        return result
    finally:
        if sock is not None:
            try:
                sock.close()
            except OSError:
                pass
        if proc is not None and proc.poll() is None:
            proc.kill()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
        # keep ppm for evidence; copy recipe
        recipe = resolve("dev_build/extsoak/recipe.json")
        if recipe.is_file():
            shutil.copy2(recipe, run_dir / "recipe.json")
        (run_dir / "result.json").write_text(
            json.dumps(result, indent=2) + "\n", encoding="utf-8"
        )


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--port", type=int, default=4450)
    ap.add_argument("--wait", type=float, default=120.0)
    ap.add_argument("--min-non-black", type=int, default=50000)
    ap.add_argument("--bus", default="ide", choices=("ide", "ahci"))
    ap.add_argument("--repeats", type=int, default=3, help="mutation runs after prepare")
    ap.add_argument(
        "--no-prepare",
        action="store_true",
        help="skip prepare_extsoak_image on first run (still refresh EXT CoW)",
    )
    ap.add_argument("--run-id", default=None, help="single-run id (skips repeat loop)")
    args = ap.parse_args(argv)
    setup_logging(args.verbose)

    summary: dict[str, Any] = {
        "schema": 1,
        "task": "ext_setfileinfo_oracle",
        "production_changes": "NONE",
        "runs": [],
        "ok": False,
    }
    out_dir = resolve("dev_build/extsoak")
    out_dir.mkdir(parents=True, exist_ok=True)

    try:
        if args.run_id:
            r = run_once(
                run_id=args.run_id,
                port=args.port,
                wait_s=args.wait,
                min_non_black=args.min_non_black,
                prepare=not args.no_prepare,
                bus=args.bus,
            )
            summary["runs"].append(r)
        else:
            # First run prepares boot image + CoW; subsequent runs refresh CoW only.
            for i in range(max(1, args.repeats)):
                rid = f"{int(time.time())}-{i + 1}"
                r = run_once(
                    run_id=rid,
                    port=args.port + i,
                    wait_s=args.wait,
                    min_non_black=args.min_non_black,
                    prepare=(i == 0 and not args.no_prepare),
                    bus=args.bus,
                )
                summary["runs"].append(r)
                if not r.get("ok"):
                    break
        summary["ok"] = all(r.get("ok") for r in summary["runs"]) and bool(summary["runs"])
        summary["reset_total"] = sum(int(r.get("qemu", {}).get("resets") or 0) for r in summary["runs"])
        summary["expected"] = expected_primary_unix()
    except SoakFailure as e:
        summary["ok"] = False
        summary["failure_class"] = e.cls
        summary["error"] = str(e)
        log.error("%s: %s", e.cls, e)
        (out_dir / "summary.json").write_text(
            json.dumps(summary, indent=2) + "\n", encoding="utf-8"
        )
        return e.code
    except Exception as e:  # noqa: BLE001
        summary["ok"] = False
        summary["failure_class"] = "tooling"
        summary["error"] = repr(e)
        log.exception("extsoak failed")
        (out_dir / "summary.json").write_text(
            json.dumps(summary, indent=2) + "\n", encoding="utf-8"
        )
        return EXIT_TOOLING

    (out_dir / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf-8"
    )
    log.info("extsoak summary ok=%s runs=%s", summary["ok"], len(summary["runs"]))
    return EXIT_PASS if summary["ok"] else EXIT_HOST


if __name__ == "__main__":
    raise SystemExit(main())
