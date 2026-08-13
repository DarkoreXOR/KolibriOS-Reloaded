"""QMP control matrix for the NTFS attach/boot stall (diagnostic only).

Does not migrate SetFileInfo. Distinguishes:
  BIOS/early boot vs IDE probe vs later kernel vs firstapp.
"""

from __future__ import annotations

import argparse
import json
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
    apply_usa_fixup,
    find_ntfs_boot_offset,
    iter_attributes,
    parse_boot_sector,
    read_mft_record,
    sha256_file,
)
from qmp_ntfs_setfileinfo_soak import (  # noqa: E402
    SoakFailure,
    count_non_black_ppm,
    drain_events,
    qmp_connect,
    qmp_exec,
    qmp_recv_obj,
    scrape_msg_board,
)
from resolve_allocator_symbols import SymbolResolveError, resolve_allocator_symbols  # noqa: E402
from run_qemu import append_ahci_image, append_ide_image, build_qemu_argv, find_qemu  # noqa: E402

OUT_DIR = "dev_build/ntfsstall"


def kolibri_bootsec_ok(boot: bytes, part_sectors: int) -> dict[str, Any]:
    """Host replica of Cut AG ``ntfs_test_bootsec`` (not a copy of the blob)."""
    reasons: list[str] = []
    if len(boot) < 0x45:
        return {"ok": False, "reasons": ["short"]}
    if boot[3:11] != b"NTFS    ":
        reasons.append("oem")
    if struct.unpack_from("<H", boot, 11)[0] != 0x200:
        reasons.append("bps")
    spc = boot[13]
    if spc == 0 or (spc & (spc - 1)) != 0:
        reasons.append("spc")
    if (
        struct.unpack_from("<H", boot, 14)[0]
        or struct.unpack_from("<I", boot, 16)[0]
        or boot[20]
        or struct.unpack_from("<H", boot, 22)[0]
        or struct.unpack_from("<I", boot, 32)[0]
    ):
        reasons.append("fat-fields")
    if struct.unpack_from("<I", boot, 0x2C)[0]:
        reasons.append("sectors-hi")
    total = struct.unpack_from("<I", boot, 0x28)[0]
    if total > part_sectors:
        reasons.append(f"total>{part_sectors}")
    for name, off in (("mft", 0x30), ("mirr", 0x38)):
        if struct.unpack_from("<I", boot, off + 4)[0]:
            reasons.append(f"{name}-hi")
        lcn = struct.unpack_from("<I", boot, off)[0]
        prod = spc * lcn
        if prod > 0xFFFFFFFF or prod > part_sectors:
            reasons.append(f"{name}-range")
    for name, off in (("cpr", 0x40), ("cpi", 0x44)):
        v = struct.unpack_from("<b", boot, off)[0]
        if v < -31:
            reasons.append(name)
        elif v > -9:
            raw = boot[off]
            if raw == 0 or (raw & (raw - 1)):
                reasons.append(name)
    return {
        "ok": not reasons,
        "reasons": reasons,
        "spc": spc,
        "total_sectors": total,
        "mft_lcn": struct.unpack_from("<I", boot, 0x30)[0],
        "mirr_lcn": struct.unpack_from("<I", boot, 0x38)[0],
        "cpr": struct.unpack_from("<b", boot, 0x40)[0],
        "cpi": struct.unpack_from("<b", boot, 0x44)[0],
    }


def walk_attrs_kolibri(record: bytes) -> dict[str, Any]:
    """Simulate ``ntfs_create_partition`` ``.scandata`` (size 0 = infinite loop)."""
    off = struct.unpack_from("<H", record, 0x14)[0]
    seen: list[dict[str, Any]] = []
    for _ in range(64):
        if off + 8 > len(record):
            return {"ok": False, "reason": "walk-off-end", "offset": off, "seen": seen}
        atype = struct.unpack_from("<I", record, off)[0]
        if atype == 0xFFFFFFFF:
            return {"ok": True, "seen": seen}
        size = struct.unpack_from("<I", record, off + 4)[0]
        if size == 0:
            return {
                "ok": False,
                "reason": "sizeWithHeader=0 (would hang .scandata)",
                "offset": off,
                "seen": seen,
            }
        if size < 8:
            return {"ok": False, "reason": "tiny-attr", "size": size, "offset": off, "seen": seen}
        seen.append(
            {
                "type": hex(atype),
                "size": size,
                "nonres": record[off + 8],
                "name_len": record[off + 9],
                "offset": off,
            }
        )
        off += size
    return {"ok": False, "reason": "too-many-attrs", "seen": seen}


def bitmap_alloc_pages(total_sectors: int, spc: int, mft_lcn: int) -> dict[str, int]:
    clusters = total_sectors // max(spc, 1)
    bitmap_bytes = clusters // 8
    va = (bitmap_bytes + 0x7FFF) & ~0x7FFF
    start = ((bitmap_bytes + mft_lcn) >> 5) << 2
    pages = ((start >> 15) + 1) << 3
    return {
        "bitmap_bytes": bitmap_bytes,
        "alloc_kernel_space": va,
        "bitmap_start": start,
        "alloc_pages": pages,
    }


def analyze_image(path: Path) -> dict[str, Any]:
    raw = path.read_bytes()
    info: dict[str, Any] = {
        "path": str(path).replace("\\", "/"),
        "size": len(raw),
        "sha256": sha256_file(path),
        "lba0_oem": raw[3:11].decode("latin1", "replace") if len(raw) >= 11 else "",
        "lba0_55aa": raw[510:512] == b"\x55\xaa" if len(raw) >= 512 else False,
    }
    try:
        boot_off = find_ntfs_boot_offset(raw)
        vol = parse_boot_sector(raw, boot_off)
        info["ntfs_boot_offset"] = boot_off
        info["volume"] = vol
        part_sectors = (len(raw) - boot_off) // 512
        boot = raw[boot_off : boot_off + 512]
        info["kolibri_bootsec"] = kolibri_bootsec_ok(boot, part_sectors)
        rec0, usa0 = read_mft_record(raw, vol, 0)
        info["mft0_usa"] = usa0
        info["mft0_walk"] = walk_attrs_kolibri(rec0)
        info["mft0_attrs"] = [
            {
                "type": hex(a["type"]),
                "name": a["name"] if isinstance(a["name"], str) else repr(a["name"]),
                "nonres": a["non_resident"],
            }
            for a in iter_attributes(rec0)
        ]
        rec5, usa5 = read_mft_record(raw, vol, 5)
        info["root_usa"] = usa5
        info["root_walk"] = walk_attrs_kolibri(rec5)
        rec6, usa6 = read_mft_record(raw, vol, 6)
        info["bitmap_rec_usa"] = usa6
        info["bitmap_rec_walk"] = walk_attrs_kolibri(rec6)
        info["bitmap_alloc"] = bitmap_alloc_pages(
            info["kolibri_bootsec"]["total_sectors"],
            info["kolibri_bootsec"]["spc"],
            info["kolibri_bootsec"]["mft_lcn"],
        )
    except Exception as ex:  # noqa: BLE001
        info["parse_error"] = repr(ex)
    return info


def classify_board(board: str) -> dict[str, Any]:
    lines = [ln.strip() for ln in board.splitlines() if ln.strip()]
    k_lines = [ln for ln in lines if ln.startswith("K :")]
    stall = [ln for ln in lines if "NTFSSTALL" in ln]
    reached_ide = any("Channel" in ln for ln in k_lines)
    reached_desktop = any(
        tag in board for tag in ("Searchap:", "AUTORUN.DAT", "L: /SYS/", "NTFSOAK")
    )
    last_k = k_lines[-3:] if k_lines else []
    phase = "BIOS_OR_EARLY"
    if reached_desktop:
        phase = "FIRSTAPP_OR_DESKTOP"
    elif stall:
        phase = stall[-1]
    elif reached_ide:
        phase = "AFTER_IDE_IDENTIFY"
    return {
        "phase": phase,
        "reached_ide_identify": reached_ide,
        "reached_desktop_or_firstapp": reached_desktop,
        "ntfsstall_markers": stall,
        "last_k_lines": last_k,
        "line_count": len(lines),
    }


def make_zeros(path: Path, size: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as f:
        f.truncate(size)


def make_mbr_empty(path: Path, size: int, part_lba: int = 2048) -> None:
    make_zeros(path, size)
    mbr = bytearray(512)
    mbr[510:512] = b"\x55\xAA"
    struct.pack_into("<B", mbr, 0x1BE + 4, 0x07)
    struct.pack_into("<I", mbr, 0x1BE + 8, part_lba)
    struct.pack_into("<I", mbr, 0x1BE + 12, (size // 512) - part_lba)
    with path.open("r+b") as f:
        f.write(mbr)


def make_ntfs(path: Path, size: int, *, part_lba: int, files: dict[str, str] | None) -> None:
    from tools.mkfs_utils.ntfs_minimal import format_minimal_ntfs
    from tools.mkfs_utils.test_tree import EMPTY_TXT, README, ROOT_TXT

    payload = files if files is not None else {"ROOT.TXT": ROOT_TXT, "README.TXT": README, "EMPTY.TXT": EMPTY_TXT}
    format_minimal_ntfs(path, size, payload, part_lba=part_lba)


def run_case(
    *,
    name: str,
    boot: Path,
    disk: Path | None,
    bus: str,
    port: int,
    wait_s: float,
    symbols: dict[str, Any],
) -> dict[str, Any]:
    cfg = load_config()
    out = resolve(OUT_DIR)
    out.mkdir(parents=True, exist_ok=True)
    ppm = out / f"{name}.ppm"
    result: dict[str, Any] = {
        "name": name,
        "bus": bus,
        "disk": str(disk).replace("\\", "/") if disk else None,
        "disk_size": disk.stat().st_size if disk and disk.is_file() else 0,
        "disk_sha256": sha256_file(disk) if disk and disk.is_file() else None,
        "ok": False,
        "resets": 0,
        "shutdowns": 0,
        "elapsed_s": 0.0,
    }
    qemu = find_qemu(cfg["qemu"]["executables"])
    qargs = build_qemu_argv(
        cfg,
        image_path=boot,
        disks=None,
        headless=True,
        use_testdisk=False,
        bus=bus,
    )
    if disk is not None:
        if bus == "ahci":
            append_ahci_image(qargs, disk, 0, False)
        else:
            append_ide_image(qargs, disk, 0)
    for i, a in enumerate(qargs):
        if a == "-qmp" and i + 1 < len(qargs):
            qargs[i + 1] = f"tcp:127.0.0.1:{port},server,nowait"

    resets = [0]
    shutdowns = [0]
    proc: subprocess.Popen[bytes] | None = None
    sock = None
    t0 = time.time()
    board = ""
    try:
        proc = subprocess.Popen([str(qemu), *qargs], cwd=str(ROOT))
        sock = qmp_connect("127.0.0.1", port, timeout=30.0)
        qmp_recv_obj(sock)
        qmp_exec(sock, {"execute": "qmp_capabilities"})
        deadline = t0 + wait_s
        desktop = False
        while time.time() < deadline:
            drain_events(sock, resets, shutdowns)
            if resets[0]:
                break
            try:
                board = scrape_msg_board(sock, symbols)
            except Exception:  # noqa: BLE001
                pass
            cls = classify_board(board)
            if cls["reached_desktop_or_firstapp"]:
                desktop = True
                break
            time.sleep(0.5)
        try:
            qmp_exec(sock, {"execute": "screendump", "arguments": {"filename": str(ppm)}})
        except Exception:  # noqa: BLE001
            pass
        nb = count_non_black_ppm(ppm) if ppm.is_file() else 0
        result.update(classify_board(board))
        result["non_black"] = nb
        result["desktop_pixels"] = nb >= 50000
        result["firstapp"] = bool(result.get("reached_desktop_or_firstapp"))
        result["ok"] = result["firstapp"] and not resets[0]
        result["timeout"] = not result["firstapp"] and not resets[0]
        result["board_tail"] = [ln for ln in board.splitlines() if ln.strip()][-12:]
        if ppm.is_file():
            result["ppm"] = str(ppm).replace("\\", "/")
    except SoakFailure as e:
        result["error"] = str(e)
        result["failure_class"] = e.cls
    finally:
        result["elapsed_s"] = round(time.time() - t0, 2)
        result["resets"] = resets[0]
        result["shutdowns"] = shutdowns[0]
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
    return result


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--wait", type=float, default=25.0)
    ap.add_argument("--port", type=int, default=4610)
    args = ap.parse_args(argv)
    setup_logging(args.verbose)

    if not LAST_IMAGE_MARKER.is_file():
        raise SystemExit("ERROR: missing last_image.txt — run python scripts/prepare_image.py")
    boot = resolve(LAST_IMAGE_MARKER.read_text(encoding="utf-8").strip())
    if not boot.is_file():
        raise SystemExit(f"ERROR: boot image missing: {boot}")

    out = resolve(OUT_DIR)
    out.mkdir(parents=True, exist_ok=True)
    ext = resolve("images/ext-image.img")
    ntfs16 = out / "ntfs-16m-mbr.img"
    ntfs128 = out / "ntfs-128m-mbr.img"
    ntfs16raw = out / "ntfs-16m-raw.img"
    zeros16 = out / "zeros-16m.img"
    mbr16 = out / "mbr-empty-16m.img"
    ntfs16empty = out / "ntfs-16m-emptyroot.img"

    log.info("Creating diagnostic disks under %s", out)
    make_ntfs(ntfs16, 16 * 1024 * 1024, part_lba=2048, files=None)
    make_ntfs(ntfs128, 128 * 1024 * 1024, part_lba=2048, files=None)
    make_ntfs(ntfs16raw, 16 * 1024 * 1024, part_lba=0, files=None)
    make_ntfs(ntfs16empty, 16 * 1024 * 1024, part_lba=2048, files={})
    make_zeros(zeros16, 16 * 1024 * 1024)
    make_mbr_empty(mbr16, 16 * 1024 * 1024)

    analyses = {
        "ntfs-16m-mbr": analyze_image(ntfs16),
        "ntfs-128m-mbr": analyze_image(ntfs128),
        "ntfs-16m-raw": analyze_image(ntfs16raw),
        "ntfs-16m-emptyroot": analyze_image(ntfs16empty),
    }
    (out / "image-analysis.json").write_text(json.dumps(analyses, indent=2) + "\n", encoding="utf-8")
    for key, info in analyses.items():
        walk = info.get("mft0_walk") or {}
        if not walk.get("ok"):
            log.warning(
                "%s MFT0 .scandata replica FAIL: %s (offset=%s)",
                key,
                walk.get("reason"),
                walk.get("offset"),
            )
        else:
            log.info("%s MFT0 .scandata replica OK", key)

    try:
        symbols = resolve_allocator_symbols()
    except SymbolResolveError as e:
        raise SystemExit(f"ERROR: symbols: {e}") from e

    cases: list[tuple[str, Path | None, str]] = [
        ("A-no-disk", None, "ide"),
        ("B-ext-ide", ext if ext.is_file() else None, "ide"),
        ("C-ntfs-16m-mbr-ide", ntfs16, "ide"),
        ("D-ntfs-16m-emptyroot-ide", ntfs16empty, "ide"),
        ("E-ntfs-16m-mbr-ahci", ntfs16, "ahci"),
        ("F-ntfs-16m-raw-ide", ntfs16raw, "ide"),
        ("G-ntfs-128m-mbr-ide", ntfs128, "ide"),
        ("H-zeros-16m-ide", zeros16, "ide"),
        ("I-mbr-empty-16m-ide", mbr16, "ide"),
    ]

    runs: list[dict[str, Any]] = []
    port = args.port
    for name, disk, bus in cases:
        if name.startswith("B-") and disk is None:
            runs.append({"name": name, "skipped": True, "reason": "no images/ext-image.img"})
            continue
        log.info("case %s bus=%s disk=%s", name, bus, disk)
        r = run_case(
            name=name,
            boot=boot,
            disk=disk,
            bus=bus,
            port=port,
            wait_s=args.wait,
            symbols=symbols,
        )
        runs.append(r)
        log.info(
            "  -> phase=%s firstapp=%s resets=%s timeout=%s nb=%s",
            r.get("phase"),
            r.get("firstapp"),
            r.get("resets"),
            r.get("timeout"),
            r.get("non_black"),
        )
        port += 1

    summary = {
        "schema": 1,
        "task": "ntfs_boot_stall",
        "production_changes": "NONE (diagnostic script + disposable ntfs_minimal USA/terminator layout)",
        "boot_image": str(boot).replace("\\", "/"),
        "wait_s": args.wait,
        "image_analysis": {k: {kk: vv for kk, vv in v.items() if kk != "mft0_attrs"} for k, v in analyses.items()},
        "runs": runs,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": all(r.get("ok") or r.get("skipped") for r in runs), "out": str(out)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
