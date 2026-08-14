"""QEMU user-net + filter-dump soak for FASM `ipv4_output` (research only).

Does not modify production networking, gates, or inventory.

Usage:
  python scripts/qmp_ipv4_output_soak.py [--runs 3] [--wait 50]
"""

from __future__ import annotations

import argparse
import json
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from common import LAST_IMAGE_MARKER, load_config, resolve, run_cmd  # noqa: E402
from net_capture import (  # noqa: E402
    iter_pcap,
    match_oracle_header,
    summarize_pcap,
)
from prepare_image import prepare_image  # noqa: E402
from resolve_allocator_symbols import resolve_allocator_symbols  # noqa: E402
from run_qemu import build_qemu_argv, find_qemu  # noqa: E402
import qmp_desktop_smoke as qmp  # noqa: E402


PAYLOAD_MAGIC = b"IPV4SOAK"
GUEST_MAC = "52:54:00:12:34:56"
GUEST_SRC = "10.0.2.15"
GUEST_DST = "10.0.2.2"
FIRSTAPP_FROM = b"/sys/LAUNCHER\x00"
FIRSTAPP_TO = b"/sys/IPV4SOAK\x00"
assert len(FIRSTAPP_FROM) == len(FIRSTAPP_TO)


def _fat12_get(data: bytes, off: int, fmt: str):
    return struct.unpack_from(fmt, data, off)[0]


def fat12_read_file(img: bytearray | bytes, path: str) -> bytes | None:
    """Read a nested 8.3 path from a FAT12 floppy image (read-only)."""
    data = bytes(img)
    bps = _fat12_get(data, 11, "<H")
    spc = data[13]
    reserved = _fat12_get(data, 14, "<H")
    fats = data[16]
    root_ents = _fat12_get(data, 17, "<H")
    spf = _fat12_get(data, 22, "<H")
    fat_off = reserved * bps
    root_off = fat_off + fats * spf * bps
    root_size = root_ents * 32
    data_off = root_off + root_size

    def fat_ent(cl: int) -> int:
        i = fat_off + (cl * 3) // 2
        val = data[i] | (data[i + 1] << 8)
        return (val >> 4) if cl & 1 else (val & 0x0FFF)

    def chain(start: int) -> bytes:
        out = bytearray()
        cl = start
        seen = set()
        while 2 <= cl < 0xFF8 and cl not in seen:
            seen.add(cl)
            off = data_off + (cl - 2) * spc * bps
            out.extend(data[off : off + spc * bps])
            cl = fat_ent(cl)
        return bytes(out)

    def parse_dir(blob: bytes) -> list[tuple[str, int, int, int]]:
        ents = []
        for i in range(0, len(blob) - 31, 32):
            n = blob[i]
            if n in (0x00, 0xE5):
                continue
            attr = blob[i + 11]
            if attr == 0x0F:
                continue
            name = blob[i : i + 11]
            base = name[:8].decode("ascii", "replace").rstrip()
            ext = name[8:11].decode("ascii", "replace").rstrip()
            label = f"{base}.{ext}" if ext else base
            cl = _fat12_get(blob, i + 26, "<H")
            sz = _fat12_get(blob, i + 28, "<I")
            ents.append((label.upper(), attr, cl, sz))
        return ents

    parts = [p.upper() for p in path.replace("\\", "/").split("/") if p]
    ents = parse_dir(data[root_off : root_off + root_size])
    for idx, part in enumerate(parts):
        hit = next((e for e in ents if e[0] == part), None)
        if hit is None:
            return None
        _label, attr, cl, sz = hit
        last = idx == len(parts) - 1
        if last:
            if attr & 0x10:
                return None
            return chain(cl)[:sz]
        if not (attr & 0x10):
            return None
        ents = parse_dir(chain(cl))
    return None


def fat12_append_autorun_line(img: bytearray, line: str) -> bool:
    """Append a line to SETTINGS/AUTORUN.DAT if it fits in allocated clusters."""
    existing = fat12_read_file(img, "SETTINGS/AUTORUN.DAT")
    if existing is None:
        return False
    text = existing.decode("latin1", "replace")
    if line.strip() in text.replace("\r\n", "\n"):
        return True
    nl = "" if text.endswith("\n") or text.endswith("\r") else "\r\n"
    new = (text + nl + line.strip() + "\r\n").encode("latin1", "replace")
    data = img
    bps = _fat12_get(data, 11, "<H")
    spc = data[13]
    reserved = _fat12_get(data, 14, "<H")
    fats = data[16]
    root_ents = _fat12_get(data, 17, "<H")
    spf = _fat12_get(data, 22, "<H")
    fat_off = reserved * bps
    root_off = fat_off + fats * spf * bps
    root_size = root_ents * 32
    data_off = root_off + root_size

    def fat_ent(cl: int) -> int:
        i = fat_off + (cl * 3) // 2
        val = data[i] | (data[i + 1] << 8)
        return (val >> 4) if cl & 1 else (val & 0x0FFF)

    def find_in_dir(blob: bytes, name11: bytes) -> tuple[int, int, int] | None:
        for i in range(0, len(blob) - 31, 32):
            if blob[i : i + 11] == name11 and blob[i + 11] != 0x0F:
                cl = _fat12_get(blob, i + 26, "<H")
                sz = _fat12_get(blob, i + 28, "<I")
                return i, cl, sz
        return None

    settings = find_in_dir(data[root_off : root_off + root_size], b"SETTINGS   ")
    if settings is None:
        return False
    _off, scl, _sz = settings
    # SETTINGS directory clusters
    dir_bytes = bytearray()
    cl = scl
    seen = set()
    while 2 <= cl < 0xFF8 and cl not in seen:
        seen.add(cl)
        off = data_off + (cl - 2) * spc * bps
        dir_bytes.extend(data[off : off + spc * bps])
        cl = fat_ent(cl)
    hit = find_in_dir(dir_bytes, b"AUTORUN DAT")
    if hit is None:
        return False
    dent_off, fcl, old_sz = hit
    # allocated size
    ncl = 0
    cl = fcl
    seen = set()
    while 2 <= cl < 0xFF8 and cl not in seen:
        seen.add(cl)
        ncl += 1
        cl = fat_ent(cl)
    cap = ncl * spc * bps
    if len(new) > cap:
        return False
    # write file clusters
    remain = new + bytes(cap - len(new))
    cl = fcl
    pos = 0
    seen = set()
    while 2 <= cl < 0xFF8 and cl not in seen:
        seen.add(cl)
        chunk = spc * bps
        off = data_off + (cl - 2) * chunk
        data[off : off + chunk] = remain[pos : pos + chunk]
        pos += chunk
        cl = fat_ent(cl)
    # update size in SETTINGS directory (rewrite first cluster at least)
    struct.pack_into("<I", dir_bytes, dent_off + 28, len(new))
    cl = scl
    pos = 0
    seen = set()
    while 2 <= cl < 0xFF8 and cl not in seen and pos < len(dir_bytes):
        seen.add(cl)
        chunk = spc * bps
        off = data_off + (cl - 2) * chunk
        data[off : off + chunk] = dir_bytes[pos : pos + chunk]
        pos += chunk
        cl = fat_ent(cl)
    return True


def assemble_guest(cfg) -> Path:
    fasm = resolve(cfg["kernel"]["fasm"])
    asm = resolve("tools/ipv4_output_guest/ipv4soak.asm")
    out = resolve("dev_build/memory/ipv4soak")
    out.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(fasm), str(asm), str(out)], check=True, cwd=str(ROOT))
    if not out.is_file():
        raise SystemExit("guest IPV4SOAK binary missing after FASM")
    return out


def patch_firstapp(image: Path, img_tool: Path) -> dict:
    """Redirect KERNEL.MNT firstapp to /sys/IPV4SOAK (same length as /sys/LAUNCHER)."""
    tmp = resolve("dev_build/memory/KERNEL.MNT.ipv4soak")
    if tmp.is_file():
        tmp.unlink()
    run_cmd(
        [str(img_tool), "extract", str(image), "KERNEL.MNT", str(tmp)],
        what="extract KERNEL.MNT",
    )
    data = bytearray(tmp.read_bytes())
    already = data.count(FIRSTAPP_TO)
    count = data.count(FIRSTAPP_FROM)
    if count == 0 and already > 0:
        tmp.unlink(missing_ok=True)
        return {
            "patched_occurrences": 0,
            "already_patched_occurrences": already,
            "from": FIRSTAPP_FROM.decode("ascii", "replace"),
            "to": FIRSTAPP_TO.decode("ascii", "replace"),
        }
    if count == 0:
        raise SystemExit("ERROR: /sys/LAUNCHER\\0 not found in KERNEL.MNT")
    data = data.replace(FIRSTAPP_FROM, FIRSTAPP_TO)
    tmp.write_bytes(data)
    run_cmd(
        [str(img_tool), "replace", str(image), "KERNEL.MNT", str(tmp)],
        what="replace patched KERNEL.MNT",
    )
    tmp.unlink(missing_ok=True)
    return {
        "patched_occurrences": count,
        "already_patched_occurrences": already,
        "from": FIRSTAPP_FROM.decode("ascii", "replace"),
        "to": FIRSTAPP_TO.decode("ascii", "replace"),
    }


def install_guest(image: Path, guest_bin: Path) -> dict:
    cfg = load_config()
    tool = resolve(cfg["image"]["tool_bin"])
    subprocess.run(
        [str(tool), "put", str(image), "IPV4SOAK", str(guest_bin)],
        check=True,
        cwd=str(ROOT),
    )
    firstapp = patch_firstapp(image, tool)
    raw = bytearray(image.read_bytes())
    autorun_before = fat12_read_file(raw, "SETTINGS/AUTORUN.DAT")
    patched = fat12_append_autorun_line(raw, "/sys/IPV4SOAK")
    if patched:
        image.write_bytes(raw)
    autorun_after = fat12_read_file(bytearray(image.read_bytes()), "SETTINGS/AUTORUN.DAT")
    autorun_text = ""
    if autorun_after:
        autorun_text = autorun_after.decode("latin1", "replace")[-200:]
    return {
        "put": "IPV4SOAK",
        "firstapp": firstapp,
        "autorun_patched": patched,
        "autorun_has_soak": bool(autorun_after and b"ipv4soak" in autorun_after.lower()),
        "autorun_len_before": 0 if autorun_before is None else len(autorun_before),
        "autorun_len_after": 0 if autorun_after is None else len(autorun_after),
        "autorun_tail": autorun_text,
    }


def parse_xp_dwords(text: str) -> list[int]:
    import re

    return [int(m.group(1), 16) for m in re.finditer(r"0x([0-9a-fA-F]+)", text)]


def xp_bytes(sock: socket.socket, pa: int, n: int) -> bytes:
    words = (n + 3) // 4
    cmd = f"xp /{max(1, words)}xw {pa:#x}"
    resp = qmp.qmp_exec(sock, {"execute": "human-monitor-command", "arguments": {"command-line": cmd}})
    text = resp.get("return") or ""
    dwords = parse_xp_dwords(text if isinstance(text, str) else str(text))
    out = bytearray()
    for w in dwords:
        out.extend(struct.pack("<I", w & 0xFFFFFFFF))
    return bytes(out[:n])


def scrape_msg_board(sock: socket.socket, symbols: dict, nbytes: int = 4096) -> str:
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


def drain_events(sock: socket.socket, resets: list[int], shutdowns: list[int]) -> None:
    sock.settimeout(0.15)
    while True:
        try:
            obj = qmp.qmp_recv_obj(sock)
        except (TimeoutError, socket.timeout, OSError, json.JSONDecodeError):
            break
        if obj.get("event") == "RESET":
            resets[0] += 1
        if obj.get("event") == "SHUTDOWN":
            shutdowns[0] += 1


def user_net_args(pcap: Path, qmp_port: int) -> list[str]:
    pcap_s = str(pcap.resolve()).replace("\\", "/")
    return [
        "-netdev",
        "user,id=n0,net=10.0.2.0/24,dhcpstart=10.0.2.15",
        "-device",
        "e1000,netdev=n0,mac=52:54:00:12:34:56",
        "-object",
        f"filter-dump,id=dump0,netdev=n0,file={pcap_s}",
        "-qmp",
        f"tcp:127.0.0.1:{qmp_port},server,nowait",
    ]


def score_run(pcap: Path) -> dict:
    summary = summarize_pcap(pcap)
    magic_hits = []
    oracle_ok = 0
    guest_ipv4 = 0
    guest_oracle_ok = 0
    ipv4_ok = 0
    for pkt in iter_pcap(pcap) if pcap.is_file() else []:
        if pkt.ipv4 is None:
            continue
        ipv4_ok += 1
        misses = match_oracle_header(pkt.ipv4)
        if not misses:
            oracle_ok += 1
        src_mac = ""
        if pkt.ethernet is not None:
            src_mac = ":".join(f"{x:02x}" for x in pkt.ethernet.src_mac)
        is_guest = src_mac == GUEST_MAC
        if is_guest:
            guest_ipv4 += 1
            if not misses:
                guest_oracle_ok += 1
        if PAYLOAD_MAGIC in pkt.ipv4.payload:
            magic_hits.append(
                {
                    "src": pkt.ipv4.source,
                    "dst": pkt.ipv4.destination,
                    "ttl": pkt.ipv4.ttl,
                    "proto": pkt.ipv4.protocol,
                    "id": pkt.ipv4.identification,
                    "flags": pkt.ipv4.flags,
                    "checksum_ok": pkt.ipv4.checksum_ok,
                    "oracle_misses": misses,
                    "payload": pkt.ipv4.payload.decode("latin1", "replace")[:80],
                    "src_mac": src_mac,
                    "guest_origin": is_guest and pkt.ipv4.protocol == 17,
                }
            )
    summary["oracle_header_ok"] = oracle_ok
    summary["guest_ipv4"] = guest_ipv4
    summary["guest_oracle_ok"] = guest_oracle_ok
    summary["magic_hits"] = magic_hits
    summary["stimulus_seen"] = any(h.get("guest_origin") for h in magic_hits)
    return summary


def one_run(
    *,
    run_id: int,
    image: Path,
    wait: float,
    qmp_port: int,
    out_dir: Path,
) -> dict:
    pcap = out_dir / f"ipv4-output-run{run_id}.pcap"
    if pcap.exists():
        pcap.unlink()
    cfg = load_config()
    qemu = find_qemu(cfg["qemu"]["executables"])
    extra = user_net_args(pcap, qmp_port)
    qargs = build_qemu_argv(
        cfg,
        image_path=image,
        disks=None,
        headless=True,
        extra_args=extra,
        use_testdisk=False,
    )
    # Drop the default QMP from headless_extra_args (port 4550) — we passed ours.
    cleaned: list[str] = []
    skip = False
    for i, a in enumerate(qargs):
        if skip:
            skip = False
            continue
        if a == "-qmp" and i + 1 < len(qargs) and "4550" in qargs[i + 1]:
            skip = True
            continue
        cleaned.append(a)
    qargs = cleaned
    proc = subprocess.Popen([qemu, *qargs], cwd=str(ROOT))
    resets = [0]
    shutdowns = [0]
    status = "unknown"
    err = None
    board = ""
    board_markers: list[str] = []
    try:
        sock = qmp.qmp_connect("127.0.0.1", qmp_port)
        greet = qmp.qmp_recv_obj(sock)
        if "QMP" not in greet:
            raise SystemExit(f"unexpected QMP greeting: {greet}")
        qmp.qmp_exec(sock, {"execute": "qmp_capabilities"})
        try:
            symbols = resolve_allocator_symbols()
        except Exception as e:  # noqa: BLE001
            symbols = {"extras": {}, "error": str(e)}
        deadline = time.time() + wait
        while time.time() < deadline:
            sock.settimeout(0.15)
            drain_events(sock, resets, shutdowns)
            sock.settimeout(8.0)
            try:
                board = scrape_msg_board(sock, symbols)
            except (Exception, SystemExit):  # noqa: BLE001
                pass
            if "IPV4SOAK PASS" in board or "IPV4SOAK FAIL" in board:
                time.sleep(1.0)
                break
            time.sleep(1.0)
        sock.settimeout(10.0)
        try:
            board = scrape_msg_board(sock, symbols) or board
        except (Exception, SystemExit):  # noqa: BLE001
            pass
        st = qmp.qmp_exec(sock, {"execute": "query-status"})
        status = st.get("return", {}).get("status", "unknown")
    except Exception as e:  # noqa: BLE001 — record and continue to kill QEMU
        err = str(e)
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
    board_markers = [ln.strip() for ln in board.splitlines() if "IPV4SOAK" in ln]
    summary = score_run(pcap)
    summary.update(
        {
            "run": run_id,
            "status": status,
            "resets": resets[0],
            "shutdowns": shutdowns[0],
            "error": err,
            "pcap_path": str(pcap),
            "board_markers": board_markers,
            "board_start": any("IPV4SOAK START" in m for m in board_markers),
            "board_pass": any("IPV4SOAK PASS" in m for m in board_markers),
            "board_excerpt": board[-1500:] if board else "",
        }
    )
    return summary


def classify_failure(run: dict) -> str:
    if run.get("resets"):
        return "RESET"
    if run.get("error") and not Path(run.get("pcap_path", "")).is_file():
        return "host capture failure"
    if run.get("frames", 0) == 0:
        return "packet generated but not captured" if run.get("status") == "running" else "QEMU network configuration failure"
    if run.get("ipv4", 0) == 0:
        return "guest network init failure"
    if not run.get("board_start"):
        return "guest stimulus did not start (firstapp/msg_board)"
    if any("FAIL NIC" in m for m in (run.get("board_markers") or [])):
        return "guest network init failure"
    if any("FAIL" in m for m in (run.get("board_markers") or [])):
        if any("FAIL CONN" in m for m in run["board_markers"]):
            return "route failure"
        if any("FAIL SEND" in m for m in run["board_markers"]):
            return "packet not generated"
        return "guest network init failure"
    if not run.get("stimulus_seen"):
        if run.get("board_pass"):
            return "packet generated but not captured"
        if run.get("guest_ipv4", 0) > 0:
            return "packet generated (DHCP/ARP path) but guest stimulus payload missing"
        return "packet not generated"
    hits = [h for h in (run.get("magic_hits") or []) if h.get("guest_origin")]
    if any(h.get("oracle_misses") for h in hits):
        return "malformed IPv4 header / checksum mismatch"
    return "ok"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--wait", type=float, default=50.0)
    ap.add_argument("--port", type=int, default=4560)
    ap.add_argument(
        "--skip-prepare",
        action="store_true",
        help="Reuse last_image without re-packaging",
    )
    args = ap.parse_args()
    cfg = load_config()
    out_dir = resolve("dev_build/memory")
    out_dir.mkdir(parents=True, exist_ok=True)

    guest = assemble_guest(cfg)
    if not args.skip_prepare:
        image = prepare_image()
    else:
        image = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
    guest_info = install_guest(image, guest)

    runs = []
    for i in range(1, args.runs + 1):
        runs.append(
            one_run(
                run_id=i,
                image=image,
                wait=args.wait,
                qmp_port=args.port + i - 1,
                out_dir=out_dir,
            )
        )

    decision_bits = {
        "all_running": all(r.get("status") == "running" for r in runs),
        "no_reset": all(r.get("resets", 0) == 0 for r in runs),
        "no_shutdown": all(r.get("shutdowns", 0) == 0 for r in runs),
        "any_ipv4": any(r.get("ipv4", 0) > 0 for r in runs),
        "any_stimulus": any(r.get("stimulus_seen") for r in runs),
        "all_board_start": all(r.get("board_start") for r in runs),
        "all_board_pass": all(r.get("board_pass") for r in runs),
        "oracle_clean_when_ipv4": all(
            (r.get("ipv4", 0) == 0) or (r.get("oracle_header_ok", 0) > 0) for r in runs
        ),
    }
    summary = {
        "guest": guest_info,
        "image": str(image),
        "runs": runs,
        "decision_bits": decision_bits,
        "failure_classes": [classify_failure(r) for r in runs],
    }
    out_json = out_dir / "ipv4-output-soak.json"
    out_json.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    print(f"wrote {out_json}")


if __name__ == "__main__":
    main()
