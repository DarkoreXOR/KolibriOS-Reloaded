"""Host-only QMP Stage-4 physical allocator soak sampler.

Runs (or attaches to) QEMU, waits for desktop, samples allocator globals via
QMP ``xp`` of resolved physical addresses, drives/observes allocsoak pressure,
and writes structured JSON under ``dev_build/allocsoak/``.

Does **not** change production allocator semantics or add USE_RUST_ALLOC_*.

Exit classes (process exit code):
  0  PASS
  2  boot / desktop failure
  3  QEMU RESET
  4  QEMU shutdown / not running
  5  symbol-resolution failure
  6  telemetry read failure
  7  allocator semantic / ledger / digest mismatch
  8  guest workload / phase timeout
  9  other tooling error
"""

from __future__ import annotations

import argparse
import json
import re
import socket
import struct
import subprocess
import sys
import time
import zlib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from common import LAST_IMAGE_MARKER, load_config, log, resolve, setup_logging  # noqa: E402
from resolve_allocator_symbols import (  # noqa: E402
    SymbolResolveError,
    resolve_allocator_symbols,
)
from run_qemu import build_qemu_argv, find_qemu  # noqa: E402

SEED_DEFAULT = 0x5047424D  # 'PGBM'

# Reuse desktop smoke helpers conceptually (duplicated minimally to keep this
# script focused and import-light).
EXIT_PASS = 0
EXIT_DESKTOP = 2
EXIT_RESET = 3
EXIT_SHUTDOWN = 4
EXIT_SYMBOLS = 5
EXIT_TELEMETRY = 6
EXIT_SEMANTIC = 7
EXIT_TIMEOUT = 8
EXIT_TOOLING = 9


def parse_asoak_report(blob: bytes) -> dict[str, Any]:
    """Parse the 512-byte AOSK driver report."""
    if len(blob) < 88:
        return {"error": "short", "raw_len": len(blob)}
    magic = blob[0:4]
    if magic not in (b"AOSK", b"FAIL"):
        return {"error": "bad_magic", "magic": magic.hex()}
    if magic == b"FAIL":
        return {"magic": "FAIL", "load_failed": True}

    def u32(off: int) -> int:
        return struct.unpack_from("<I", blob, off)[0]

    flags = u32(12)
    ncases = min(u32(80), 16)
    cases = []
    base = 88
    for i in range(ncases):
        off = base + i * 16
        if off + 16 > len(blob):
            break
        cases.append(
            {
                "N": u32(off),
                "ret_pa": u32(off + 4),
                "status": u32(off + 8),
            }
        )
    return {
        "magic": "AOSK",
        "version": u32(4),
        "seed": f"0x{u32(8):08X}",
        "flags": flags,
        "flags_decode": {
            "baseline": bool(flags & 1),
            "pressure": bool(flags & 2),
            "ap_table": bool(flags & 4),
            "recovery_ok": bool(flags & 8),
            "recovery_fail": bool(flags & 16),
            "pass": bool(flags & 32),
            "double_free": bool(flags & 64),
            "frag": bool(flags & 128),
            "oom_hit": bool(flags & 256),
            "oom_ceiling": bool(flags & 512),
        },
        "ap_ok": u32(16),
        "ap_fail": u32(20),
        "apages_ok": u32(24),
        "apages_fail": u32(28),
        "free_ok": u32(32),
        "outstanding": u32(36),
        "last_pa": f"0x{u32(40):08X}",
        "pressure_ok": u32(44),
        "pressure_target": u32(48),
        "ledger_cap": u32(52),
        "oom_ret": u32(56),
        "oom_ops": u32(60),
        "df_pa": f"0x{u32(64):08X}",
        "frag_holes": u32(68),
        "frag_ap8_ret": f"0x{u32(72):08X}",
        "frag_ap8_ok": u32(76),
        "ap_cases": cases,
    }


def _extract_asoak_log(
    image_path: Path, out_dir: Path, result: dict[str, Any] | None
) -> None:
    from prepare_image import ensure_kolibri_img

    cfg = load_config()
    img_tool = ensure_kolibri_img(cfg["image"])
    host = out_dir / "ASOAK.LOG"
    if host.is_file():
        host.unlink()
    cp = subprocess.run(
        [str(img_tool), "extract", str(image_path), "ASOAK.LOG", str(host)],
        cwd=str(ROOT),
        check=False,
        capture_output=True,
        text=True,
    )
    if cp.returncode != 0 or not host.is_file():
        log.warning("ASOAK.LOG not on image (driver may not have written it)")
        return
    blob = host.read_bytes()
    parsed = parse_asoak_report(blob)
    side = out_dir / "asoak-report.json"
    side.write_text(json.dumps(parsed, indent=2) + "\n", encoding="utf-8")
    log.info("guest report: %s", side)
    if result is not None:
        result["guest_report"] = parsed


@dataclass
class LedgerEntry:
    phase: str
    op: str
    pages: int
    note: str = ""
    freed: bool = False


@dataclass
class HostLedger:
    """Independent logical ledger — never derives success from pages_free."""

    entries: list[LedgerEntry] = field(default_factory=list)
    expected_touch_pages: int = 512
    strength: str = (
        "approximate for fault-path pressure: host knows intended touch "
        "count, not physical page frame identities (MENUET apps do not "
        "receive AllocPage return PAs)"
    )

    def record(self, phase: str, op: str, pages: int, note: str = "") -> None:
        self.entries.append(LedgerEntry(phase=phase, op=op, pages=pages, note=note))

    def outstanding_pages(self) -> int:
        n = 0
        for e in self.entries:
            if e.freed:
                continue
            if e.op in ("touch_retain", "alloc_retain", "allocpage_retain"):
                n += e.pages
            elif e.op in ("free", "release_exit", "free_all"):
                n -= e.pages
        return max(0, n)

    def to_json(self) -> dict[str, Any]:
        return {
            "expected_touch_pages": self.expected_touch_pages,
            "outstanding_pages": self.outstanding_pages(),
            "entries": [e.__dict__ for e in self.entries],
            "strength": self.strength,
        }


class SoakFailure(Exception):
    def __init__(self, code: int, cls: str, message: str):
        super().__init__(message)
        self.code = code
        self.cls = cls
        self.message = message


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
    raise SoakFailure(EXIT_DESKTOP, "boot", f"QMP connect failed: {last_err}")


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
        # ignore events here; caller drains separately


def drain_events(sock: socket.socket, resets: list[int], shutdowns: list[int]) -> None:
    sock.settimeout(0.05)
    try:
        while True:
            try:
                obj = qmp_recv_obj(sock)
            except (TimeoutError, socket.timeout):
                break
            except SoakFailure:
                shutdowns[0] += 1
                break
            ev = obj.get("event")
            if ev == "RESET":
                resets[0] += 1
            elif ev in ("SHUTDOWN", "POWERDOWN", "STOP"):
                if ev != "STOP":
                    shutdowns[0] += 1
    finally:
        sock.settimeout(10.0)


def count_non_black_ppm(path: Path) -> tuple[int, int, int]:
    data = path.read_bytes()
    if not data.startswith(b"P6"):
        raise SoakFailure(EXIT_TOOLING, "ppm", f"not a P6 PPM: {path}")
    parts: list[str] = []
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
    while i < len(data) and data[i] in b" \t\r":
        i += 1
    if i < len(data) and data[i] == 10:
        i += 1
    pixels = data[i:]
    need = width * height * 3
    non_black = 0
    for off in range(0, min(len(pixels), need), 3):
        if pixels[off] | pixels[off + 1] | pixels[off + 2]:
            non_black += 1
    return width, height, non_black


_XP_DWORD_RE = re.compile(r":\s*((?:0x)?[0-9A-Fa-f]+)(?:\s+((?:0x)?[0-9A-Fa-f]+))*")


def parse_xp_dwords(text: str) -> list[int]:
    """Parse QEMU ``xp /Nw ADDR`` human-monitor output into dwords."""
    vals: list[int] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("dumping"):
            # formats vary; also match "0xaddr: 0x.. 0x.."
            pass
        # Find all hex tokens after the first colon
        if ":" in line:
            _, _, rest = line.partition(":")
        else:
            rest = line
        for tok in rest.split():
            tok = tok.strip().rstrip(",")
            if not tok:
                continue
            try:
                vals.append(int(tok, 0))
            except ValueError:
                continue
    return vals


def qmp_xp_dwords(sock: socket.socket, pa: int, n: int = 1) -> list[int]:
    cmd = f"xp /{max(1, n)}xw {pa:#x}"
    dump = qmp_exec(
        sock,
        {"execute": "human-monitor-command", "arguments": {"command-line": cmd}},
    )
    text = dump.get("return", "")
    if not isinstance(text, str):
        text = str(text)
    vals = parse_xp_dwords(text)
    if len(vals) < n:
        raise SoakFailure(
            EXIT_TELEMETRY,
            "telemetry",
            f"xp short read at PA {pa:#x}: got {len(vals)} want {n}; raw={text!r}",
        )
    return vals[:n]


def qmp_xp_bytes(sock: socket.socket, pa: int, nbytes: int) -> bytes:
    """Read guest physical memory as bytes via dword xp chunks."""
    if nbytes <= 0:
        return b""
    ndw = (nbytes + 3) // 4
    # QEMU xp count practical limit — chunk.
    out = bytearray()
    offset = 0
    while offset < ndw:
        chunk = min(64, ndw - offset)
        dwords = qmp_xp_dwords(sock, pa + offset * 4, chunk)
        for w in dwords:
            out += struct.pack("<I", w & 0xFFFFFFFF)
        offset += chunk
    return bytes(out[:nbytes])


def fnv1a32(data: bytes) -> int:
    h = 0x811C9DC5
    for b in data:
        h ^= b
        h = (h * 0x01000193) & 0xFFFFFFFF
    return h


def popcount_free_bits(data: bytes) -> int:
    """sys_pgmap free bit = 1 (BTS sets free on free_page)."""
    n = 0
    for b in data:
        n += b.bit_count()
    return n


def scrape_msg_board(sock: socket.socket, symbols: dict[str, Any], nbytes: int = 4096) -> str:
    extras = symbols.get("extras") or {}
    mb = extras.get("msg_board_data")
    if not isinstance(mb, dict) or "physical_address" not in mb:
        return ""
    raw = qmp_xp_bytes(sock, mb["physical_address"], nbytes)
    # Board is a ring; present printable bytes.
    chars = []
    for b in raw:
        if 32 <= b < 127 or b in (10, 13):
            chars.append(chr(b))
        elif b == 0:
            chars.append("\n")
    return "".join(chars)


def sample_allocator(
    sock: socket.socket,
    symbols: dict[str, Any],
    *,
    digest_max_bytes: int = 4096,
) -> dict[str, Any]:
    pf = symbols["pages_free"]["physical_address"]
    ps = symbols["page_start"]["physical_address"]
    bm = symbols["sys_pgmap"]["physical_address"]

    pages_free = qmp_xp_dwords(sock, pf, 1)[0]
    page_start_val = qmp_xp_dwords(sock, ps, 1)[0]

    extras = symbols.get("extras") or {}
    pages_count = None
    pagemap_size = None
    page_end_val = None
    if isinstance(extras.get("pages_count"), dict) and "physical_address" in extras["pages_count"]:
        pages_count = qmp_xp_dwords(sock, extras["pages_count"]["physical_address"], 1)[0]
    if isinstance(extras.get("pagemap_size"), dict) and "physical_address" in extras["pagemap_size"]:
        pagemap_size = qmp_xp_dwords(sock, extras["pagemap_size"]["physical_address"], 1)[0]
    if isinstance(extras.get("page_end"), dict) and "physical_address" in extras["page_end"]:
        page_end_val = qmp_xp_dwords(sock, extras["page_end"]["physical_address"], 1)[0]

    # Digest region: prefer [page_start, page_end) as pointers into sys_pgmap,
    # else first digest_max_bytes of the map (or pagemap_size if smaller).
    digest_meta: dict[str, Any] = {
        "method": "fnv1a32",
        "free_bit_polarity": 1,
    }
    region_pa = bm
    region_len = digest_max_bytes
    if (
        page_start_val
        and page_end_val
        and page_end_val > page_start_val
        and page_start_val >= symbols["sys_pgmap"]["virtual_address"]
    ):
        # page_start/page_end are VAs into sys_pgmap
        base_va = symbols["sys_pgmap"]["virtual_address"]
        off0 = page_start_val - base_va
        off1 = page_end_val - base_va
        if 0 <= off0 < off1:
            region_pa = bm + off0
            region_len = min(off1 - off0, digest_max_bytes)
            digest_meta["region"] = "page_start..page_end (capped)"
            digest_meta["va_start"] = page_start_val
            digest_meta["va_end"] = page_end_val
    else:
        if pagemap_size and pagemap_size > 0:
            region_len = min(int(pagemap_size), digest_max_bytes)
        digest_meta["region"] = "sys_pgmap[:N]"
    digest_meta["physical_address"] = region_pa
    digest_meta["bytes"] = region_len

    raw = qmp_xp_bytes(sock, region_pa, region_len)
    digest = fnv1a32(raw)
    free_bits = popcount_free_bits(raw)
    digest_meta["digest"] = f"0x{digest:08x}"
    digest_meta["free_bits_in_region"] = free_bits
    digest_meta["pages_represented"] = region_len * 8
    digest_meta["crc32"] = f"0x{zlib.crc32(raw) & 0xFFFFFFFF:08x}"

    return {
        "pages_free": pages_free,
        "page_start": page_start_val,
        "page_start_offset_from_map": (
            page_start_val - symbols["sys_pgmap"]["virtual_address"]
            if page_start_val
            else None
        ),
        "page_end": page_end_val,
        "pages_count": pages_count,
        "pagemap_size": pagemap_size,
        "bitmap_digest": digest_meta,
        "ts": time.time(),
    }


def wait_desktop(
    sock: socket.socket,
    ppm: Path,
    *,
    wait_s: float,
    min_non_black: int,
    splash_max: int,
    resets: list[int],
    shutdowns: list[int],
    symbols: dict[str, Any] | None = None,
    early_samples: list[dict[str, Any]] | None = None,
    sample_interval: float = 0.5,
    digest_bytes: int = 4096,
) -> dict[str, Any]:
    """Poll xp + periodic screendumps until desktop floor or timeout.

    Unlike a fixed sleep, this catches allocsoak's HOLD_HS window and then
    continues until LAUNCHER brings up the desktop.
    """
    deadline = time.time() + wait_s
    last_dump = 0.0
    best: dict[str, Any] | None = None
    while time.time() < deadline:
        drain_events(sock, resets, shutdowns)
        if resets[0]:
            raise SoakFailure(EXIT_RESET, "reset", f"guest RESET x{resets[0]} during boot")
        if shutdowns[0]:
            raise SoakFailure(EXIT_SHUTDOWN, "shutdown", "guest shutdown during boot")
        if symbols is not None and early_samples is not None:
            try:
                early_samples.append(
                    sample_allocator(sock, symbols, digest_max_bytes=digest_bytes)
                )
            except SoakFailure:
                # Paging/globals may not be live in the earliest milliseconds.
                pass

        now = time.time()
        if now - last_dump >= max(1.0, sample_interval * 2):
            last_dump = now
            status = qmp_exec(sock, {"execute": "query-status"})
            st = status.get("return", {}).get("status")
            if st != "running":
                raise SoakFailure(EXIT_SHUTDOWN, "shutdown", f"query-status={st}")
            if ppm.exists():
                ppm.unlink()
            qmp_exec(
                sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}}
            )
            time.sleep(0.2)
            try:
                w, h, nb = count_non_black_ppm(ppm)
            except SoakFailure:
                time.sleep(sample_interval)
                continue
            best = {
                "status": st,
                "ppm": str(ppm).replace("\\", "/"),
                "width": w,
                "height": h,
                "non_black": nb,
                "desktop_reached": False,
                "resets": resets[0],
                "shutdowns": shutdowns[0],
            }
            # Reject early VGA text / low-res splash (e.g. 720x400) even when
            # non-black is large — Kolibri desktop smoke is 1024x768 class.
            if w >= 800 and h >= 600 and nb >= min_non_black:
                best["desktop_reached"] = True
                return best
        time.sleep(sample_interval)

    if best is None:
        raise SoakFailure(EXIT_DESKTOP, "desktop", "no screendump before timeout")
    nb = best["non_black"]
    w, h = best["width"], best["height"]
    if w < 800 or h < 600:
        raise SoakFailure(
            EXIT_DESKTOP,
            "desktop",
            f"low-res framebuffer {w}x{h} (need >=800x600 GUI mode)",
        )
    if nb <= splash_max:
        raise SoakFailure(
            EXIT_DESKTOP,
            "desktop",
            f"splash/black framebuffer non-black={nb} <= {splash_max}",
        )
    raise SoakFailure(
        EXIT_DESKTOP,
        "desktop",
        f"desktop floor miss non-black={nb} < {min_non_black}",
    )


def run_soak(args: argparse.Namespace) -> int:
    cfg = load_config()
    out_dir = resolve(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    artifact = out_dir / args.artifact_name
    ppm = out_dir / "allocator-soak.ppm"

    result: dict[str, Any] = {
        "schema": 1,
        "seed": f"0x{args.seed:08X}",
        "seed_ascii": "PGBM" if args.seed == SEED_DEFAULT else None,
        "image": None,
        "qemu": {
            "status": None,
            "resets": 0,
            "shutdowns": 0,
            "desktop_reached": False,
        },
        "symbols": None,
        "baseline": None,
        "phases": [],
        "ledger": None,
        "result": {"passed": False, "failures": []},
        "notes": [],
    }

    def fail(code: int, cls: str, msg: str) -> int:
        result["result"]["passed"] = False
        result["result"]["failures"].append({"class": cls, "message": msg})
        artifact.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        log.error("%s: %s", cls, msg)
        return code

    # --- symbols ---
    try:
        symbols = resolve_allocator_symbols(
            resolve(args.fas) if args.fas else None
        )
    except SymbolResolveError as e:
        result["notes"].append(str(e))
        return fail(EXIT_SYMBOLS, "symbol-resolution", str(e))
    result["symbols"] = {
        "pages_free": symbols["pages_free"],
        "page_start": symbols["page_start"],
        "sys_pgmap": symbols["sys_pgmap"],
        "extras": symbols.get("extras"),
    }

    if not LAST_IMAGE_MARKER.is_file():
        return fail(
            EXIT_TOOLING,
            "tooling",
            "missing last_image.txt — run prepare_allocsoak_image.py or prepare_image.py",
        )
    image_path = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
    result["image"] = str(image_path).replace("\\", "/")

    ledger = HostLedger(expected_touch_pages=args.pressure_pages)
    resets = [0]
    shutdowns = [0]
    proc: subprocess.Popen[bytes] | None = None
    sock: socket.socket | None = None

    try:
        qemu = find_qemu(cfg["qemu"]["executables"])
        qargs = build_qemu_argv(
            cfg,
            image_path=image_path,
            disks=args.disk or None,
            headless=True,
            bus=args.bus,
        )
        for i, a in enumerate(qargs):
            if a == "-qmp" and i + 1 < len(qargs):
                qargs[i + 1] = f"tcp:127.0.0.1:{args.port},server,nowait"

        if args.attach:
            sock = qmp_connect("127.0.0.1", args.port, timeout=5.0)
            greet = qmp_recv_obj(sock)
            if "QMP" not in greet:
                return fail(EXIT_TOOLING, "qmp", f"unexpected greeting {greet}")
            qmp_exec(sock, {"execute": "qmp_capabilities"})
        else:
            proc = subprocess.Popen([str(qemu), *qargs], cwd=str(ROOT))
            sock = qmp_connect("127.0.0.1", args.port, timeout=30.0)
            greet = qmp_recv_obj(sock)
            if "QMP" not in greet:
                return fail(EXIT_TOOLING, "qmp", f"unexpected greeting {greet}")
            qmp_exec(sock, {"execute": "qmp_capabilities"})

        # Phase A/B — poll xp during boot (catch allocsoak HOLD_HS), then desktop.
        recipe_path = resolve("dev_build/allocsoak/recipe.json")
        if recipe_path.is_file():
            recipe = json.loads(recipe_path.read_text(encoding="utf-8"))
            result["allocsoak_recipe"] = recipe
            ledger.expected_touch_pages = int(
                recipe.get("pressure_pages") or args.pressure_pages
            )

        early_samples: list[dict[str, Any]] = []
        desk = wait_desktop(
            sock,
            ppm,
            wait_s=args.wait,
            min_non_black=args.min_non_black,
            splash_max=args.splash_max,
            resets=resets,
            shutdowns=shutdowns,
            symbols=symbols,
            early_samples=early_samples,
            sample_interval=args.sample_interval,
            digest_bytes=args.digest_bytes,
        )
        result["qemu"].update(desk)

        baseline = sample_allocator(sock, symbols, digest_max_bytes=args.digest_bytes)
        result["baseline"] = baseline
        result["phases"].append(
            {
                "name": "baseline",
                "sample": baseline,
                "ok": True,
                "notes": ["Post-desktop sample (allocsoak usually already exited)."],
            }
        )

        # Pressure evidence from early boot samples (max free vs min free).
        # Ignore pre-init xp (BSS still zero → pages_free==0) and other
        # nonsense values before the allocator is live.
        def _live_pf(sample: dict[str, Any]) -> int | None:
            pf = sample.get("pages_free")
            if not isinstance(pf, int):
                return None
            pc = sample.get("pages_count")
            if isinstance(pc, int) and pc > 0 and pf > pc:
                return None
            if pf < 256:
                return None
            return pf

        pe_mode = bool(
            isinstance(result.get("allocsoak_recipe"), dict)
            and result["allocsoak_recipe"].get("mode") == "pe_driver"
        )
        if pe_mode:
            ledger.expected_touch_pages = int(
                result["allocsoak_recipe"].get("pressure_target") or 1024
            )
            need = max(256, ledger.expected_touch_pages // 2)
        else:
            need = max(256, args.pressure_pages // 2)

        boot_pf = [p for p in (_live_pf(s) for s in early_samples) if p is not None]
        pressure_observed = False
        quantum_isolated = False
        oom_seen_host = False
        min_pf = None
        max_pf = None
        dip = None
        hold_plateau = None
        if len(boot_pf) >= 5:
            min_pf = min(boot_pf)
            max_pf = max(boot_pf)
            dip = max_pf - min_pf
            run = 0
            best_run = 0
            peak = boot_pf[0]
            for pf in boot_pf:
                peak = max(peak, pf)
                if pf <= 1:
                    oom_seen_host = True
                if peak - pf >= need:
                    run += 1
                    best_run = max(best_run, run)
                else:
                    run = 0
            hold_plateau = best_run
            if dip >= 256:
                pressure_observed = True
            if best_run >= 3 and dip >= need:
                quantum_isolated = True

        ledger.entries = [
            LedgerEntry("baseline", "observe", 0, note="desktop up"),
            LedgerEntry(
                "pressure",
                "allocpage_retain" if pe_mode else "touch_retain",
                ledger.expected_touch_pages,
                note=(
                    "PE AllocPage retained pages (phys PA in driver ledger)"
                    if pe_mode
                    else "MENUET fault-path; no PA identity"
                ),
            ),
            LedgerEntry(
                "recovery",
                "free_all" if pe_mode else "release_exit",
                ledger.expected_touch_pages,
                note="driver free-all / process exit",
                freed=False,
            ),
        ]
        ledger.entries[1].freed = True

        pressure_phase = {
            "name": "pressure",
            "mode": "pe_driver" if pe_mode else "menuet_fault",
            "early_samples_count": len(early_samples),
            "early_live_samples": len(boot_pf),
            "early_pages_free_series": boot_pf,
            "early_pages_free_min": min_pf,
            "early_pages_free_max": max_pf,
            "early_pages_free_dip": dip,
            "hold_plateau_samples": hold_plateau,
            "post_desktop_pages_free": baseline["pages_free"],
            "pressure_observed": pressure_observed,
            "allocsoak_quantum_isolated": quantum_isolated,
            "ok": pressure_observed,
            "notes": [
                (
                    "PE AllocPage hammer runs inside driver START during 68.21; "
                    "host xp samples mid-syscall via ChangeTask yields."
                    if pe_mode
                    else "MENUET fault-path pressure (legacy)."
                ),
            ],
        }
        result["phases"].append(pressure_phase)
        if args.require_pressure and not pressure_observed:
            raise SoakFailure(
                EXIT_SEMANTIC,
                "pressure",
                f"pressure not observed (live pf min={min_pf} max={max_pf} "
                f"dip={dip} plateau={hold_plateau})",
            )
        if args.require_allocsoak_quantum and not quantum_isolated:
            raise SoakFailure(
                EXIT_SEMANTIC,
                "pressure",
                f"allocsoak quantum not isolated (dip={dip} plateau={hold_plateau})",
            )

        # Phase C — OOM
        if pe_mode:
            oom_phase = {
                "name": "oom",
                "ok": oom_seen_host,
                "blocked": not oom_seen_host,
                "host_saw_pages_free_le_1": oom_seen_host,
                "early_pages_free_min": min_pf,
                "notes": [
                    "Host observes pages_free<=1 during boot poll if OOM path hit.",
                    "Guest report (ASOAK.LOG) carries oom_ret/flags after extract.",
                ],
            }
            if not oom_seen_host:
                oom_phase["blocker"] = (
                    "pages_free<=1 not observed in boot xp series — "
                    "ceiling, timing, or driver did not reach early OOM"
                )
                result["notes"].append("Phase C (OOM) not confirmed on host xp")
        else:
            oom_phase = {
                "name": "oom",
                "ok": None,
                "blocked": True,
                "blocker": "MENUET path cannot force pages_free<=1",
            }
            result["notes"].append("Phase C (OOM) BLOCKED — MENUET path")
        result["phases"].append(oom_phase)

        # Phase D — recovery observation (post-desktop)
        recovery = sample_allocator(sock, symbols, digest_max_bytes=args.digest_bytes)
        recovery_ok = recovery["pages_free"] >= max(1, baseline["pages_free"] - 2048)
        result["phases"].append(
            {
                "name": "recovery",
                "sample": recovery,
                "ok": recovery_ok,
                "notes": [
                    "Post-desktop freelist after driver free-all + LAUNCHER.",
                ],
            }
        )
        if not recovery_ok:
            raise SoakFailure(
                EXIT_SEMANTIC,
                "recovery",
                f"pages_free={recovery['pages_free']} unexpectedly low vs baseline "
                f"{baseline['pages_free']}",
            )

        # Phase E — AllocPages / fragmentation (guest-driven in PE mode)
        if pe_mode:
            frag = {
                "name": "fragmentation",
                "ok": True,
                "blocked": False,
                "notes": [
                    "Executed inside PE driver (AllocPages table + hole pattern).",
                    "Details in ASOAK.LOG guest report after extract.",
                ],
            }
        else:
            frag = {
                "name": "fragmentation",
                "ok": None,
                "blocked": True,
                "blocker": "requires PE AllocPages",
            }
            result["notes"].append("Phase E BLOCKED — no PE driver")
        result["phases"].append(frag)

        # Phase F — stability
        drain_events(sock, resets, shutdowns)
        if resets[0]:
            raise SoakFailure(EXIT_RESET, "reset", f"RESET during stability x{resets[0]}")
        status = qmp_exec(sock, {"execute": "query-status"})
        st = status.get("return", {}).get("status")
        if st != "running":
            raise SoakFailure(EXIT_SHUTDOWN, "shutdown", f"status={st}")
        if ppm.exists():
            ppm.unlink()
        qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
        time.sleep(0.3)
        w, h, nb = count_non_black_ppm(ppm)
        final = sample_allocator(sock, symbols, digest_max_bytes=args.digest_bytes)
        stab_ok = nb >= args.min_non_black and resets[0] == 0

        board_text = None
        try:
            board_text = scrape_msg_board(sock, symbols)
            result["msg_board_excerpt"] = board_text[-2000:] if board_text else None
            markers = [
                line.strip()
                for line in (board_text or "").splitlines()
                if "ALLOCSOK" in line
            ]
            result["allocsok_markers"] = markers
        except Exception as ex:  # noqa: BLE001
            result["msg_board_error"] = repr(ex)
            markers = []

        marker_set = set(markers)
        pe_pass = any(m == "ALLOCSOK PASS" or m.startswith("ALLOCSOK PASS") for m in markers)
        pe_fail = any(m.startswith("ALLOCSOK FAIL") for m in markers)
        oom_hit = any("OOM HIT" in m for m in markers)
        oom_blocked = any("OOM BLOCKED" in m for m in markers)
        has_ap = any(m == "ALLOCSOK AP" or m.startswith("ALLOCSOK AP") for m in markers)
        has_frag = any("FRAG" in m for m in markers)
        has_df = any(m == "ALLOCSOK DF" or m.startswith("ALLOCSOK DF") for m in markers)
        has_recover = any("RECOVER" in m for m in markers)
        result["driver_markers"] = {
            "pass": pe_pass,
            "fail": pe_fail,
            "oom_hit": oom_hit,
            "oom_blocked": oom_blocked,
            "alloc_pages_table": has_ap,
            "fragmentation": has_frag,
            "double_free": has_df,
            "recover": has_recover,
            "count": len(markers),
        }

        # Refine OOM / frag phases from guest markers (authoritative for PE).
        if pe_mode:
            for ph in result["phases"]:
                if ph.get("name") == "oom":
                    ph["guest_oom_hit"] = oom_hit
                    ph["guest_oom_blocked"] = oom_blocked
                    if oom_hit:
                        ph["ok"] = True
                        ph["blocked"] = False
                        ph.pop("blocker", None)
                    elif oom_blocked:
                        ph["ok"] = None
                        ph["blocked"] = True
                        ph["blocker"] = (
                            "Driver hit safety ceiling before pages_free<=1 "
                            "(ALLOCSOK OOM BLOCKED) — not an allocator failure"
                        )
                    ph["notes"] = [
                        "Guest markers are authoritative for OOM classification.",
                        "Host pages_free<=1 remains a secondary confirmation.",
                    ]
                if ph.get("name") == "fragmentation":
                    ph["ok"] = has_frag and has_ap
                    ph["notes"] = [
                        "PE driver executed AllocPages table + fragmentation phases.",
                        "Markers: ALLOCSOK AP / ALLOCSOK FRAG.",
                    ]
            # Drop stale host-only OOM note when guest classified.
            result["notes"] = [
                n
                for n in result["notes"]
                if "Phase C (OOM) not confirmed" not in n
            ]
            if oom_blocked and not oom_hit:
                result["notes"].append(
                    "Phase C OOM BLOCKED by driver safety ceiling (documented)"
                )
            if pe_fail or not pe_pass:
                raise SoakFailure(
                    EXIT_SEMANTIC,
                    "driver",
                    f"PE driver did not PASS (pass={pe_pass} fail={pe_fail} "
                    f"markers={len(markers)})",
                )
            ledger.strength = (
                "PE mode: guest AllocPage/FreePage/AllocPages with independent "
                "driver ledger + host pages_free/page_start/sys_pgmap samples; "
                "phys PA identities in guest ledger only (host correlates deltas)"
            )

        result["phases"].append(
            {
                "name": "stability",
                "sample": final,
                "non_black": nb,
                "status": st,
                "ok": stab_ok,
                "allocsok_markers": result.get("allocsok_markers"),
                "notes": [
                    "Pixels are diagnostic only — not allocator proof.",
                    "Guest markers scraped from kernel msg_board_data via xp.",
                ],
            }
        )
        if not stab_ok:
            raise SoakFailure(
                EXIT_SEMANTIC,
                "stability",
                f"stability fail non-black={nb} resets={resets[0]}",
            )

        result["qemu"]["resets"] = resets[0]
        result["qemu"]["shutdowns"] = shutdowns[0]
        result["qemu"]["status"] = st
        result["ledger"] = ledger.to_json()
        result["result"]["passed"] = True
        if pe_mode:
            result["result"]["decision_hint"] = (
                "PE driver soak PASS via AllocPage/FreePage/AllocPages — "
                "ALLOCATOR DRIVER TOOLING"
            )
            if oom_blocked and not oom_hit:
                result["result"]["decision_hint"] += " (OOM BLOCKED by ceiling)"
        else:
            result["result"]["decision_hint"] = (
                "MENUET-only soak; OOM/alloc_pages blocked — PARTIAL"
            )
        artifact.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        log.info("PASS → %s", artifact)
        print(json.dumps({"passed": True, "artifact": str(artifact)}, indent=2))
        # Stash for finally extract merge
        run_soak._last_result = result  # type: ignore[attr-defined]
        run_soak._last_artifact = artifact  # type: ignore[attr-defined]
        return EXIT_PASS

    except SoakFailure as e:
        result["qemu"]["resets"] = resets[0]
        result["qemu"]["shutdowns"] = shutdowns[0]
        result["ledger"] = ledger.to_json()
        run_soak._last_result = result  # type: ignore[attr-defined]
        run_soak._last_artifact = artifact  # type: ignore[attr-defined]
        return fail(e.code, e.cls, e.message)
    except Exception as e:  # noqa: BLE001 — tooling boundary
        run_soak._last_result = result  # type: ignore[attr-defined]
        run_soak._last_artifact = artifact  # type: ignore[attr-defined]
        return fail(EXIT_TOOLING, "tooling", repr(e))
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
        try:
            last = getattr(run_soak, "_last_result", None)
            _extract_asoak_log(image_path, out_dir, last)
            art = getattr(run_soak, "_last_artifact", None)
            if last is not None and art is not None and last.get("guest_report"):
                art.write_text(json.dumps(last, indent=2) + "\n", encoding="utf-8")
        except Exception as ex:  # noqa: BLE001
            log.warning("ASOAK.LOG extract skipped: %s", ex)


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=4555)
    ap.add_argument("--wait", type=float, default=120.0, help="seconds boot/desktop budget")
    ap.add_argument("--min-non-black", type=int, default=100000)
    ap.add_argument("--splash-max", type=int, default=20000)
    ap.add_argument("--disk", action="append", default=[])
    ap.add_argument("--bus", choices=("ide", "ahci"), default="ide")
    ap.add_argument("--seed", type=lambda s: int(s, 0), default=SEED_DEFAULT)
    ap.add_argument("--pressure-pages", type=int, default=1024)
    ap.add_argument("--sample-interval", type=float, default=0.5)
    ap.add_argument("--digest-bytes", type=int, default=4096)
    ap.add_argument(
        "--require-pressure",
        action="store_true",
        help="FAIL if boot-time pages_free does not move (live telemetry)",
    )
    ap.add_argument(
        "--require-allocsoak-quantum",
        action="store_true",
        help="FAIL unless a hold plateau isolates ~pressure_pages from boot noise",
    )
    ap.add_argument("--out-dir", default="dev_build/allocsoak")
    ap.add_argument("--artifact-name", default="soak-result.json")
    ap.add_argument("--fas", default=None, help="override path to kernel.fas")
    ap.add_argument(
        "--attach",
        action="store_true",
        help="attach to already-running QEMU on --port (do not spawn)",
    )
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument(
        "--resolve-only",
        action="store_true",
        help="only resolve symbols and write JSON, then exit",
    )
    args = ap.parse_args(argv)
    setup_logging(args.verbose)

    if args.resolve_only:
        try:
            symbols = resolve_allocator_symbols(resolve(args.fas) if args.fas else None)
        except SymbolResolveError as e:
            raise SystemExit(f"ERROR: {e}") from e
        out = resolve(args.out_dir) / "symbols.json"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(symbols, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(symbols, indent=2))
        return

    raise SystemExit(run_soak(args))


if __name__ == "__main__":
    main()
