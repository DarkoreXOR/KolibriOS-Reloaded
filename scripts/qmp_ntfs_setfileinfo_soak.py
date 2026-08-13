"""Host-only QMP NTFS SetFileInfo write/readback soak.

CoW NTFS disk + firstapp NTFSOAK guest driver. Primary oracle:

  guest SetFileInfo → GetFileInfo → (embedded disk_sync) → host $I30 index parse

Does **not** migrate SetFileInfo or add production gates.
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
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "scripts"))

from common import LAST_IMAGE_MARKER, load_config, log, resolve, setup_logging  # noqa: E402
from ntfs_setfileinfo_oracle import (  # noqa: E402
    classify_log_side_effects,
    expected_primary_filetimes,
    extract_ntfs_root_file,
    metadata_diff,
    metadata_diff_control,
    parse_file_mft_sidecar,
    parse_guest_report,
    parse_root_index_times,
    preflight_ntfs_soak_image,
    sha256_file,
    sidecar_unchanged,
    validate_root_usa,
)
from prepare_ntfssoak_image import (  # noqa: E402
    ensure_ntfs_reference,
    prepare_ntfs_cow,
    prepare_ntfssoak_image,
)
from build_ntfssoak import build_ntfssoak  # noqa: E402
from resolve_allocator_symbols import SymbolResolveError, resolve_allocator_symbols  # noqa: E402
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
EXIT_NSFI_LOG = 10
EXIT_USA = 11


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
            pass
        if "NTFSOAK START" in board_excerpt:
            saw_start = True
        if "NTFSOAK PASS" in board_excerpt:
            saw_pass = True
        if "NTFSOAK FAIL" in board_excerpt:
            saw_fail = True
        has_fin = "NTFSOAK FIN " in board_excerpt
        has_log = "NTFSOAK LOG" in board_excerpt  # matches LOG and LOGFAIL
        if saw_fail or (saw_pass and has_fin and has_log):
            try:
                qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
                if ppm.is_file():
                    best_nb = count_non_black_ppm(ppm)
                    desktop = best_nb >= min_non_black
            except Exception:  # noqa: BLE001
                pass
            break
        time.sleep(0.5)
    else:
        raise SoakFailure(
            EXIT_TIMEOUT,
            "timeout",
            f"no NTFSOAK PASS/FAIL within {wait_s}s; board={board_excerpt[-400:]!r}",
        )
    markers = [ln.strip() for ln in board_excerpt.splitlines() if "NTFSOAK" in ln]
    return {
        "saw_start": saw_start,
        "saw_pass": saw_pass,
        "saw_fail": saw_fail,
        "desktop_reached": desktop,
        "non_black": best_nb,
        "markers": markers,
        "msg_board_excerpt": board_excerpt[-2000:] if board_excerpt else None,
    }


def extract_nsfi_log_from_cow(
    cow: Path, out_dir: Path, *, expected_run_id: int | None, control: bool = False
) -> dict[str, Any]:
    host = out_dir / "NSFI.LOG"
    try:
        blob = extract_ntfs_root_file(cow, "NSFI.LOG", max_bytes=512)
    except Exception as ex:  # noqa: BLE001
        raise SoakFailure(
            EXIT_NSFI_LOG,
            "nsfi-log-extraction",
            f"NSFI.LOG not readable from CoW: {ex}",
        ) from ex
    host.write_bytes(blob)
    parsed = parse_guest_report(blob)
    (out_dir / "guest-report.json").write_text(
        json.dumps(parsed, indent=2) + "\n", encoding="utf-8"
    )
    if parsed.get("error"):
        raise SoakFailure(EXIT_NSFI_LOG, "nsfi-log-parse", f"NSFI.LOG unreadable: {parsed}")
    if expected_run_id is not None and parsed.get("run_id") != expected_run_id:
        raise SoakFailure(
            EXIT_NSFI_LOG,
            "nsfi-log-run-id",
            f"run_id mismatch: log=0x{parsed.get('run_id', 0):08X} "
            f"expected=0x{expected_run_id:08X}",
        )
    fd = parsed.get("flags_decode") or {}
    if not fd.get("pass"):
        raise SoakFailure(EXIT_NSFI_LOG, "nsfi-log-pass", "FLAG_PASS clear in NSFI.LOG")
    if control:
        if not fd.get("get1_ok") or not fd.get("get2_ok") or not fd.get("get3_ok"):
            raise SoakFailure(EXIT_NSFI_LOG, "nsfi-log-markers", f"control missing Get flags: {fd}")
        if fd.get("set_ok"):
            raise SoakFailure(EXIT_NSFI_LOG, "nsfi-log-control", "control run set FLAG_SET_OK")
    else:
        if not fd.get("set_ok") or not fd.get("get2_ok") or not fd.get("get3_ok"):
            raise SoakFailure(EXIT_NSFI_LOG, "nsfi-log-markers", f"missing flags in NSFI.LOG: {fd}")
    if not fd.get("log_ok"):
        raise SoakFailure(EXIT_NSFI_LOG, "nsfi-log-persistence", "FLAG_LOG_OK clear")
    parsed["source"] = "ntfs_cow:/hd0/1/NSFI.LOG"
    parsed["host_path"] = str(host).replace("\\", "/")
    return parsed


def refresh_boot_run_id(boot_image: Path, *, control: bool = False) -> int:
    from prepare_image import ensure_kolibri_img

    cfg = load_config()
    bin_path, rid = build_ntfssoak(control=control)
    img_tool = ensure_kolibri_img(cfg["image"])
    subprocess.run(
        [str(img_tool), "put", boot_image, "NTFSOAK1", str(bin_path)],
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
    allow_minimal: bool,
    force_minimal: bool = False,
    control: bool = False,
) -> dict[str, Any]:
    cfg = load_config()
    out_dir = resolve("dev_build/ntfssoak")
    out_dir.mkdir(parents=True, exist_ok=True)
    run_dir = out_dir / f"run-{run_id}"
    run_dir.mkdir(parents=True, exist_ok=True)
    ppm = run_dir / "screen.ppm"

    result: dict[str, Any] = {
        "run_id": run_id,
        "ok": False,
        "control": bool(control),
        "failure_class": None,
        "qemu": {"resets": 0, "shutdowns": 0},
        "notes": [],
    }

    expected_run_id: int | None = None
    if prepare:
        boot_image, cow = prepare_ntfssoak_image(
            allow_minimal=allow_minimal or force_minimal,
            force_minimal=force_minimal,
            control=control,
        )
        recipe_path = resolve("dev_build/ntfssoak/recipe.json")
        if recipe_path.is_file():
            expected_run_id = int(json.loads(recipe_path.read_text(encoding="utf-8")).get("run_id") or 0)
        ref = ensure_ntfs_reference(
            allow_minimal=allow_minimal or force_minimal,
            force_minimal=force_minimal,
            recreate=False,
        )
    else:
        if not LAST_IMAGE_MARKER.is_file():
            raise SoakFailure(EXIT_TOOLING, "tooling", "missing last_image.txt")
        boot_image = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
        ref = ensure_ntfs_reference(
            allow_minimal=allow_minimal or force_minimal,
            force_minimal=force_minimal,
            recreate=False,
        )
        cow = out_dir / "ntfs-cow.img"
        prepare_ntfs_cow(ref, cow)
        expected_run_id = refresh_boot_run_id(boot_image, control=control)

    result["boot_image"] = str(boot_image).replace("\\", "/")
    result["ntfs_cow"] = str(cow).replace("\\", "/")
    result["ntfs_cow_sha256_before"] = sha256_file(cow)
    result["reference"] = str(ref).replace("\\", "/")
    result["reference_sha256"] = sha256_file(ref)
    result["expected_run_id"] = expected_run_id

    preflight = preflight_ntfs_soak_image(cow, "ROOT.TXT")
    result["preflight"] = {
        "ok": preflight.get("ok"),
        "mft0_walk": preflight.get("mft0_walk"),
        "target_mft_record": (preflight.get("target_index") or {}).get("mft_record"),
        "volume": preflight.get("volume"),
    }
    (run_dir / "preflight.json").write_text(json.dumps(preflight, indent=2) + "\n", encoding="utf-8")
    if not preflight.get("ok"):
        raise SoakFailure(EXIT_TOOLING, "preflight", f"NTFS image failed host preflight: {preflight.get('mft0_walk')}")

    before = parse_root_index_times(cow, "ROOT.TXT")
    before_si = parse_file_mft_sidecar(cow, "ROOT.TXT")
    (run_dir / "before-index.json").write_text(json.dumps(before, indent=2) + "\n", encoding="utf-8")
    (run_dir / "before-sidecar.json").write_text(json.dumps(before_si, indent=2) + "\n", encoding="utf-8")
    expected = expected_primary_filetimes()
    result["expected"] = expected
    result["before_index"] = {
        "mft_record": before.get("mft_record"),
        "file_accessed": before.get("file_accessed"),
        "file_modified": before.get("file_modified"),
        "file_created": before.get("file_created"),
        "file_real_size": before.get("file_real_size"),
        "file_flags": before.get("file_flags"),
    }
    result["before_sidecar"] = {
        "mft_record": before_si.get("mft_record"),
        "standard_information": before_si.get("standard_information"),
        "file_name": before_si.get("file_name"),
        "walk": before_si.get("walk"),
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
            raise SoakFailure(EXIT_GUEST, "guest-marker", "NTFSOAK FAIL on msg_board")
        if not wait_info.get("saw_start"):
            raise SoakFailure(EXIT_GUEST, "firstapp-timeout", "NTFSOAK START not observed")
        if not wait_info.get("saw_pass"):
            raise SoakFailure(EXIT_GUEST, "guest-marker", "NTFSOAK PASS not observed")
        markers = wait_info.get("markers") or []
        if control:
            if any("NTFSOAK SET" in m for m in markers):
                raise SoakFailure(EXIT_GUEST, "control-mutated", "control run printed NTFSOAK SET")
            if not any("CTRL" in m for m in markers):
                raise SoakFailure(EXIT_GUEST, "control-marker", "NTFSOAK CTRL not observed")
        else:
            if not any("NTFSOAK SET" in m for m in markers):
                result["notes"].append("NTFSOAK SET marker missing (PASS still observed)")

        try:
            qmp_exec(sock, {"execute": "system_powerdown"})
        except SoakFailure:
            pass
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

        if resets[0]:
            raise SoakFailure(EXIT_RESET, "reset", f"RESET during run x{resets[0]}")

        cow_final = run_dir / "ntfs-cow.img"
        shutil.copy2(cow, cow_final)
        result["ntfs_cow_final"] = str(cow_final).replace("\\", "/")
        result["ntfs_cow_sha256_after"] = sha256_file(cow_final)

        guest = extract_nsfi_log_from_cow(
            cow_final, run_dir, expected_run_id=expected_run_id, control=control
        )
        result["guest_report"] = guest
        result["guest_evidence"] = "NSFI.LOG"

        if control:
            guest_ok = (
                guest.get("immediate_atime_hex") == guest.get("initial_atime_hex")
                and guest.get("immediate_mtime_hex") == guest.get("initial_mtime_hex")
                and guest.get("final_atime_hex") == guest.get("initial_atime_hex")
                and guest.get("final_mtime_hex") == guest.get("initial_mtime_hex")
            )
            result["guest_vs_expected_bdfe"] = guest_ok
            if not guest_ok:
                raise SoakFailure(EXIT_GUEST, "guest-bdfe", "control GetFileInfo times drifted")
        else:
            exp_a = expected["atime_bdfe_hex"]
            exp_m = expected["mtime_bdfe_hex"]
            guest_ok = (
                guest.get("immediate_atime_hex") == exp_a
                and guest.get("immediate_mtime_hex") == exp_m
                and guest.get("final_atime_hex") == exp_a
                and guest.get("final_mtime_hex") == exp_m
            )
            result["guest_vs_expected_bdfe"] = guest_ok
            if not guest_ok:
                raise SoakFailure(EXIT_GUEST, "guest-bdfe", "guest BDFE readback != expected")

        after = parse_root_index_times(cow_final, "ROOT.TXT")
        (run_dir / "after-index.json").write_text(json.dumps(after, indent=2) + "\n", encoding="utf-8")
        result["after_index"] = {
            "mft_record": after.get("mft_record"),
            "file_accessed": after.get("file_accessed"),
            "file_modified": after.get("file_modified"),
            "file_created": after.get("file_created"),
            "file_real_size": after.get("file_real_size"),
            "file_flags": after.get("file_flags"),
        }

        usa = validate_root_usa(cow_final.read_bytes())
        result["usa"] = usa
        (run_dir / "usa-validation.json").write_text(json.dumps(usa, indent=2) + "\n", encoding="utf-8")
        if not usa.get("usa_valid"):
            raise SoakFailure(EXIT_USA, "usa-invalid", f"root MFT USA invalid: {usa}")

        after_si = parse_file_mft_sidecar(cow_final, "ROOT.TXT")
        (run_dir / "after-sidecar.json").write_text(json.dumps(after_si, indent=2) + "\n", encoding="utf-8")
        if not (after_si.get("walk") or {}).get("ok"):
            raise SoakFailure(EXIT_HOST, "terminator", f"target FILE terminator invalid: {after_si.get('walk')}")
        si_diff = sidecar_unchanged(before_si, after_si)
        result["standard_information_file_name"] = si_diff
        if not si_diff.get("ok"):
            raise SoakFailure(
                EXIT_HOST,
                "unexpected-si-fn",
                f"file MFT $STANDARD_INFORMATION/$FILE_NAME changed: {si_diff}",
            )

        log_fx = classify_log_side_effects(cow_final)
        result["log_side_effects"] = log_fx

        if control:
            diff = metadata_diff_control(before, after)
        else:
            diff = metadata_diff(before, after, expected, log_side_effects=log_fx)
        result["metadata_diff"] = diff
        (run_dir / "metadata-diff.json").write_text(json.dumps(diff, indent=2) + "\n", encoding="utf-8")
        if not diff.get("ok"):
            raise SoakFailure(EXIT_HOST, "host-index", f"on-disk index mismatch: {diff}")

        result["ok"] = True
        result["decision_note"] = (
            "control: GetFileInfo-only; $I30+$SI unchanged"
            if control
            else "guest NSFI.LOG + host $I30 index parser + USA PASS; SI/FN unchanged"
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
        recipe = resolve("dev_build/ntfssoak/recipe.json")
        if recipe.is_file():
            shutil.copy2(recipe, run_dir / "recipe.json")
        (run_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--port", type=int, default=4460)
    ap.add_argument("--wait", type=float, default=120.0)
    ap.add_argument("--min-non-black", type=int, default=50000)
    ap.add_argument("--bus", default="ide", choices=("ide", "ahci"))
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--no-prepare", action="store_true")
    ap.add_argument("--allow-minimal", action="store_true")
    ap.add_argument(
        "--force-minimal",
        action="store_true",
        help="recreate disposable ntfs_minimal fixture; ignore images/ntfs-image.img",
    )
    ap.add_argument(
        "--with-control",
        action="store_true",
        help="after repeats, run one GetFileInfo-only control (no SetFileInfo)",
    )
    ap.add_argument("--run-id", default=None)
    args = ap.parse_args(argv)
    setup_logging(args.verbose)

    summary: dict[str, Any] = {
        "schema": 1,
        "task": "ntfs_setfileinfo_oracle",
        "production_changes": "NONE",
        "force_minimal": bool(args.force_minimal),
        "runs": [],
        "ok": False,
    }
    out_dir = resolve("dev_build/ntfssoak")
    out_dir.mkdir(parents=True, exist_ok=True)

    common_kw = dict(
        wait_s=args.wait,
        min_non_black=args.min_non_black,
        bus=args.bus,
        allow_minimal=args.allow_minimal or args.force_minimal,
        force_minimal=args.force_minimal,
    )

    try:
        if args.run_id:
            r = run_once(
                run_id=args.run_id,
                port=args.port,
                prepare=not args.no_prepare,
                control=False,
                **common_kw,
            )
            summary["runs"].append(r)
        else:
            for i in range(max(1, args.repeats)):
                rid = f"{int(time.time())}-{i + 1}"
                r = run_once(
                    run_id=rid,
                    port=args.port + i,
                    prepare=(i == 0 and not args.no_prepare),
                    control=False,
                    **common_kw,
                )
                summary["runs"].append(r)
                if not r.get("ok"):
                    break
            if args.with_control and all(x.get("ok") for x in summary["runs"]):
                rid = f"{int(time.time())}-control"
                cr = run_once(
                    run_id=rid,
                    port=args.port + max(1, args.repeats),
                    prepare=False,
                    control=True,
                    **common_kw,
                )
                summary["control"] = cr
                summary["runs"].append(cr)
        summary["ok"] = all(r.get("ok") for r in summary["runs"]) and bool(summary["runs"])
        summary["reset_total"] = sum(int(r.get("qemu", {}).get("resets") or 0) for r in summary["runs"])
        summary["expected"] = expected_primary_filetimes()
    except SoakFailure as e:
        summary["ok"] = False
        summary["failure_class"] = e.cls
        summary["error"] = str(e)
        log.error("%s: %s", e.cls, e)
        (out_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
        return e.code
    except Exception as e:  # noqa: BLE001
        summary["ok"] = False
        summary["failure_class"] = "tooling"
        summary["error"] = repr(e)
        log.exception("ntfssoak failed")
        (out_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
        return EXIT_TOOLING

    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    log.info("ntfssoak summary ok=%s runs=%s", summary["ok"], len(summary["runs"]))
    return EXIT_PASS if summary["ok"] else EXIT_HOST


if __name__ == "__main__":
    raise SystemExit(main())
