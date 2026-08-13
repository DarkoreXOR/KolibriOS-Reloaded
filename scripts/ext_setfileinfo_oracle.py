"""Host-only EXT2 inode / BDFE helpers for the SetFileInfo oracle.

Independent of production Kolibri EXT Rust blobs. Preferred on-disk source of
truth is Docker ``debugfs`` (e2fsprogs). A minimal pure-Python EXT2 walker is
provided as a fallback / cross-check for atime/mtime of a named root file.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

UNIXTIME_TO_KOS_OFFSET = 978_307_200  # (365*31+8)*86400 — fs_lfn.inc

# Deterministic primary mutation (must match tools/extsoak/extsoak.asm).
REQ_ATIME = (11, 22, 14, 4, 7, 2012)  # sec,min,hour,day,month,year
REQ_MTIME = (30, 5, 9, 23, 11, 2018)


@dataclass(frozen=True)
class BdfeTime:
    sec: int
    min: int
    hour: int
    day: int
    month: int
    year: int

    def to_bytes(self) -> bytes:
        b = bytearray(8)
        b[0] = self.sec & 0xFF
        b[1] = self.min & 0xFF
        b[2] = self.hour & 0xFF
        b[4] = self.day & 0xFF
        b[5] = self.month & 0xFF
        b[6] = self.year & 0xFF
        b[7] = (self.year >> 8) & 0xFF
        return bytes(b)

    @classmethod
    def from_bytes(cls, b: bytes) -> BdfeTime:
        if len(b) < 8:
            raise ValueError("short BDFE")
        return cls(
            sec=b[0],
            min=b[1],
            hour=b[2],
            day=b[4],
            month=b[5],
            year=b[6] | (b[7] << 8),
        )

    def as_tuple(self) -> tuple[int, int, int, int, int, int]:
        return (self.sec, self.min, self.hour, self.day, self.month, self.year)


def fs_calculate_time(t: BdfeTime) -> int:
    """FASM-faithful ``fsCalculateTime`` → seconds since 2001-01-01."""
    year = t.year & 0xFFFF
    years = 0 if year < 2001 else (year - 2001) & 0xFFFFFFFF
    months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    months2 = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    full = months + months2
    table_base = 12 if ((years + 1) & 3) == 0 else 0
    month_idx = (t.month & 0xFF) - 1
    month_sum = 0
    while True:
        month_idx -= 1
        if month_idx < 0:
            break
        idx = table_base + month_idx
        day_len = full[idx] if 0 <= idx < 24 else 0
        month_sum = (month_sum + day_len) & 0xFFFFFFFF
    days = (years * 365 + (years >> 2) + month_sum) & 0xFFFFFFFF
    days = (days - 1 + (t.day & 0xFF)) & 0xFFFFFFFF
    total = (days * 24 + (t.hour & 0xFF)) & 0xFFFFFFFF
    total = (total * 60 + (t.min & 0xFF)) & 0xFFFFFFFF
    total = (total * 60 + (t.sec & 0xFF)) & 0xFFFFFFFF
    return total


def bdfe_to_unix_inode_time(t: BdfeTime) -> int:
    """EXT inode seconds = fsCalculateTime(BDFE) + UNIXTIME_TO_KOS_OFFSET."""
    return (fs_calculate_time(t) + UNIXTIME_TO_KOS_OFFSET) & 0xFFFFFFFF


def expected_primary_unix() -> dict[str, int]:
    at = BdfeTime(*REQ_ATIME)
    mt = BdfeTime(*REQ_MTIME)
    return {
        "atime_unix": bdfe_to_unix_inode_time(at),
        "mtime_unix": bdfe_to_unix_inode_time(mt),
        "atime_bdfe": list(at.as_tuple()),
        "mtime_bdfe": list(mt.as_tuple()),
        "atime_bdfe_hex": at.to_bytes().hex(),
        "mtime_bdfe_hex": mt.to_bytes().hex(),
    }


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _read_u16(data: bytes, off: int) -> int:
    return struct.unpack_from("<H", data, off)[0]


def _read_u32(data: bytes, off: int) -> int:
    return struct.unpack_from("<I", data, off)[0]


def parse_guest_report(blob: bytes) -> dict[str, Any]:
    if len(blob) < 140:
        return {"error": "short", "raw_len": len(blob)}
    if blob[0:4] != b"ESFI":
        return {"error": "bad_magic", "magic": blob[0:4].hex()}

    def u32(off: int) -> int:
        return struct.unpack_from("<I", blob, off)[0]

    flags = u32(8)
    version = u32(4)
    out: dict[str, Any] = {
        "magic": "ESFI",
        "version": version,
        "flags": flags,
        "flags_decode": {
            "pass": bool(flags & 1),
            "get1_ok": bool(flags & 2),
            "set_ok": bool(flags & 4),
            "get2_ok": bool(flags & 8),
            "get3_ok": bool(flags & 16),
            "atime_match": bool(flags & 32),
            "mtime_match": bool(flags & 64),
            "edge_idem_ok": bool(flags & 128),
            "edge_second_ok": bool(flags & 256),
            "edge_miss_ok": bool(flags & 512),
            "log_ok": bool(flags & 1024),
        },
        "get1_eax": u32(12),
        "set_eax": u32(16),
        "get2_eax": u32(20),
        "get3_eax": u32(24),
        "edge_idem_eax": u32(28),
        "edge_second_eax": u32(32),
        "edge_miss_eax": u32(36),
        "initial_atime_hex": blob[44:52].hex(),
        "initial_mtime_hex": blob[52:60].hex(),
        "requested_atime_hex": blob[60:68].hex(),
        "requested_mtime_hex": blob[68:76].hex(),
        "immediate_atime_hex": blob[76:84].hex(),
        "immediate_mtime_hex": blob[84:92].hex(),
        "final_atime_hex": blob[92:100].hex(),
        "final_mtime_hex": blob[100:108].hex(),
        "second_req_atime_hex": blob[108:116].hex(),
        "second_req_mtime_hex": blob[116:124].hex(),
        "second_rb_atime_hex": blob[124:132].hex(),
        "second_rb_mtime_hex": blob[132:140].hex(),
    }
    if version >= 2 and len(blob) >= 164:
        out["run_id"] = u32(140)
        out["run_id_hex"] = f"0x{u32(140):08X}"
        out["create_log_eax"] = u32(144)
        out["write_log_eax"] = u32(148)
        out["target_inode_hint"] = u32(152)
        tag = blob[156:160]
        out["path_tag"] = tag.decode("ascii", "replace")
        out["ticks"] = u32(160)
    return out


def _ext2_layout(raw: bytes) -> dict[str, Any]:
    if len(raw) < 2048:
        raise ValueError("image too small")
    sb = 1024
    magic = _read_u16(raw, sb + 0x38)
    if magic != 0xEF53:
        raise ValueError(f"bad EXT magic {magic:#x}")
    log_block_size = _read_u32(raw, sb + 0x18)
    block_size = 1024 << log_block_size
    inodes_per_group = _read_u32(raw, sb + 0x28)
    inode_size = _read_u16(raw, sb + 0x58)
    if inode_size == 0:
        inode_size = 128
    if block_size == 1024:
        gd_off = 2048
    else:
        gd_off = block_size
    inode_table_block = _read_u32(raw, gd_off + 8)
    return {
        "raw": raw,
        "block_size": block_size,
        "inodes_per_group": inodes_per_group,
        "inode_size": inode_size,
        "inode_table_block": inode_table_block,
    }


def _inode_offset(layout: dict[str, Any], ino: int) -> int:
    index = ino - 1
    group = index // layout["inodes_per_group"]
    local = index % layout["inodes_per_group"]
    if group != 0:
        raise ValueError(f"inode {ino} outside group 0 (not supported by mini parser)")
    return layout["inode_table_block"] * layout["block_size"] + local * layout["inode_size"]


def _find_root_dirent(layout: dict[str, Any], name: str) -> int:
    raw = layout["raw"]
    root_off = _inode_offset(layout, 2)
    dir_block = _read_u32(raw, root_off + 40)
    bs = layout["block_size"]
    dir = raw[dir_block * bs : (dir_block + 1) * bs]
    name_b = name.encode("ascii")
    pos = 0
    while pos + 8 <= len(dir):
        ino = _read_u32(dir, pos)
        rec_len = _read_u16(dir, pos + 4)
        name_len = dir[pos + 6]
        if rec_len == 0:
            break
        if ino != 0 and name_len == len(name_b) and dir[pos + 8 : pos + 8 + name_len] == name_b:
            return ino
        pos += rec_len
    raise ValueError(f"{name} not found in root directory")


def parse_ext2_root_file_times(image: Path, name: str = "ROOT.TXT") -> dict[str, Any]:
    """Minimal EXT2 walker (SB@1024, 1KiB+ blocks, no extents required for root)."""
    layout = _ext2_layout(image.read_bytes())
    raw = layout["raw"]
    target_ino = _find_root_dirent(layout, name)
    ioff = _inode_offset(layout, target_ino)
    mode = _read_u16(raw, ioff + 0)
    atime = _read_u32(raw, ioff + 8)
    ctime = _read_u32(raw, ioff + 12)
    mtime = _read_u32(raw, ioff + 16)
    size = _read_u32(raw, ioff + 4)
    return {
        "parser": "python_ext2_mini",
        "file": name,
        "inode": target_ino,
        "mode": mode,
        "size": size,
        "atime": atime,
        "ctime": ctime,
        "mtime": mtime,
        "block_size": layout["block_size"],
        "inode_size": layout["inode_size"],
    }


def extract_ext2_root_file(image: Path, name: str, max_bytes: int = 4096) -> bytes:
    """Read a small root-directory file via direct block pointers (host-only)."""
    layout = _ext2_layout(image.read_bytes())
    raw = layout["raw"]
    ino = _find_root_dirent(layout, name)
    ioff = _inode_offset(layout, ino)
    size = _read_u32(raw, ioff + 4)
    if size > max_bytes:
        size = max_bytes
    bs = layout["block_size"]
    out = bytearray()
    # 12 direct blocks starting at inode+40
    for i in range(12):
        if len(out) >= size:
            break
        blk = _read_u32(raw, ioff + 40 + i * 4)
        if blk == 0:
            break
        chunk = raw[blk * bs : (blk + 1) * bs]
        need = size - len(out)
        out.extend(chunk[:need])
    return bytes(out)


def probe_debugfs_environment() -> dict[str, Any]:
    """Describe debugfs/Docker availability without requiring a successful query."""
    native = shutil.which("debugfs")
    docker = shutil.which("docker")
    env: dict[str, Any] = {
        "native_debugfs": native,
        "docker": docker,
        "docker_daemon": "unknown",
        "available": False,
        "method": None,
        "class": "DEBUGFS_CROSSCHECK_UNAVAILABLE",
    }
    if native:
        env["available"] = True
        env["method"] = "native_debugfs"
        env["class"] = "DEBUGFS_AVAILABLE"
        return env
    if not docker:
        env["class"] = "DEBUGFS_CROSSCHECK_UNAVAILABLE"
        env["detail"] = "no native debugfs and docker not on PATH"
        return env
    cp = subprocess.run(
        [docker, "info"],
        capture_output=True,
        text=True,
        check=False,
        timeout=20,
    )
    if cp.returncode != 0:
        err = (cp.stderr or cp.stdout or "")[-400:]
        env["docker_daemon"] = "stopped_or_unreachable"
        env["detail"] = err
        env["class"] = "DEBUGFS_CROSSCHECK_UNAVAILABLE"
        return env
    env["docker_daemon"] = "running"
    env["available"] = True
    env["method"] = "docker_debugfs"
    env["class"] = "DEBUGFS_AVAILABLE"
    return env


def _debugfs_run(image: Path, commands: str) -> str:
    """Run debugfs commands against an image (native or Docker)."""
    native = shutil.which("debugfs")
    if native:
        cp = subprocess.run(
            [native, "-R", commands, str(image)],
            capture_output=True,
            text=True,
            check=False,
        )
        out = (cp.stdout or "") + (cp.stderr or "")
        if cp.returncode != 0 and "Inode:" not in out and "ESFI" not in out:
            raise RuntimeError(f"native debugfs failed rc={cp.returncode}: {out[-500:]}")
        return out

    docker = shutil.which("docker")
    if docker is None:
        raise RuntimeError("docker not on PATH")
    img_dir = image.parent.resolve()
    img_name = image.name
    # Escape single quotes for sh -c
    cmds = commands.replace("'", "'\\''")
    script = (
        "apk add --no-cache e2fsprogs e2fsprogs-extra >/dev/null && "
        f"debugfs -R '{cmds}' /img/{img_name}"
    )
    cp = subprocess.run(
        [
            docker,
            "run",
            "--rm",
            "--privileged",
            "-v",
            f"{img_dir}:/img",
            "alpine:3.20",
            "sh",
            "-c",
            script,
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=180,
    )
    out = (cp.stdout or "") + (cp.stderr or "")
    if cp.returncode != 0 and "Inode:" not in out:
        raise RuntimeError(f"docker debugfs failed rc={cp.returncode}: {out[-500:]}")
    return out


def _parse_debugfs_stat_text(out: str, path: str) -> dict[str, Any]:
    result: dict[str, Any] = {
        "parser": "debugfs",
        "raw_excerpt": out[-2000:],
        "path": path,
    }
    m = re.search(r"Inode:\s*(\d+)", out)
    if m:
        result["inode"] = int(m.group(1))
    m = re.search(r"Size:\s*(\d+)", out)
    if m:
        result["size"] = int(m.group(1))
    m = re.search(r"Mode:\s*0*(\d+)", out)
    if m:
        result["mode_octal"] = m.group(1)
    m = re.search(r"Type:\s*(\w+)", out)
    if m:
        result["type"] = m.group(1)
    # Prefer numeric hex fields from modern debugfs (authoritative Unix seconds).
    for key in ("atime", "mtime", "ctime"):
        m = re.search(rf"{key}:\s*0x([0-9a-fA-F]+)", out)
        if m:
            result[key] = int(m.group(1), 16)
            continue
        # Older "Access:/Modify:/Change:" string form
        label = {"atime": "Access", "mtime": "Modify", "ctime": "Change"}[key]
        m = re.search(
            rf"{label}:\s*(\d{{4}}-\d{{2}}-\d{{2}}\s+\d{{2}}:\d{{2}}:\d{{2}})", out
        )
        if m:
            from datetime import datetime, timezone

            result[f"{key}_str"] = m.group(1)
            try:
                dt = datetime.strptime(m.group(1), "%Y-%m-%d %H:%M:%S").replace(
                    tzinfo=timezone.utc
                )
                result[key] = int(dt.timestamp())
            except ValueError:
                pass
    return result


def debugfs_stat(image: Path, path: str = "ROOT.TXT") -> dict[str, Any]:
    """Independent ``debugfs`` inode stat (native or Docker e2fsprogs)."""
    out = _debugfs_run(image, f"stat {path}")
    return _parse_debugfs_stat_text(out, path)


def debugfs_dump_file(image: Path, path: str = "ESFI.LOG") -> bytes:
    """Dump a file via debugfs ``cat`` (best-effort)."""
    out = _debugfs_run(image, f"cat {path}")
    # debugfs may mix diagnostics; keep binary-looking payload after last banner line.
    if "ESFI" in out:
        idx = out.find("ESFI")
        return out[idx:].encode("latin-1", "replace")
    return out.encode("latin-1", "replace")


def compare_mini_vs_debugfs(
    mini: dict[str, Any], dbg: dict[str, Any]
) -> dict[str, Any]:
    """Cross-check fields relevant to the SetFileInfo oracle."""
    mismatches = []
    agreements = []
    for field in ("inode", "atime", "mtime", "size"):
        if field not in dbg:
            continue
        if mini.get(field) == dbg.get(field):
            agreements.append(field)
        else:
            mismatches.append(
                {"field": field, "mini": mini.get(field), "debugfs": dbg.get(field)}
            )
    # ctime: compare if debugfs produced numeric
    if "ctime" in dbg and mini.get("ctime") is not None:
        if mini.get("ctime") == dbg.get("ctime"):
            agreements.append("ctime")
        else:
            # timezone display quirks — record soft mismatch
            mismatches.append(
                {
                    "field": "ctime",
                    "mini": mini.get("ctime"),
                    "debugfs": dbg.get("ctime"),
                    "soft": True,
                    "note": "ctime string→unix assumes UTC; soft if TZ differs",
                }
            )
    hard = [m for m in mismatches if not m.get("soft")]
    return {
        "ok": not hard,
        "agreements": agreements,
        "mismatches": mismatches,
    }


def read_inode_times(
    image: Path, name: str = "ROOT.TXT", *, try_debugfs: bool = True
) -> dict[str, Any]:
    """Mini parser always; optional debugfs cross-check."""
    mini = parse_ext2_root_file_times(image, name)
    combined = dict(mini)
    if not try_debugfs:
        combined["debugfs_status"] = "skipped"
        return combined
    probe = probe_debugfs_environment()
    combined["debugfs_probe"] = probe
    if not probe.get("available"):
        combined["debugfs_status"] = "DEBUGFS_CROSSCHECK_UNAVAILABLE"
        combined["debugfs_error"] = probe.get("detail") or probe.get("class")
        return combined
    try:
        df = debugfs_stat(image, name)
        combined["debugfs"] = df
        combined["debugfs_status"] = "ok"
        combined["debugfs_crosscheck"] = compare_mini_vs_debugfs(mini, df)
        if df.get("inode") and df["inode"] != mini["inode"]:
            combined["inode_mismatch"] = {"mini": mini["inode"], "debugfs": df["inode"]}
    except Exception as ex:  # noqa: BLE001
        combined["debugfs_status"] = "error"
        combined["debugfs_error"] = str(ex)
    return combined


def metadata_diff(
    before: dict[str, Any],
    after: dict[str, Any],
    expected_atime: int,
    expected_mtime: int,
    *,
    log_side_effects: dict[str, Any] | None = None,
) -> dict[str, Any]:
    expected_changes = []
    unexpected = []
    for field in ("atime", "mtime", "ctime", "size", "mode", "inode"):
        b = before.get(field)
        a = after.get(field)
        if b == a:
            continue
        entry = {"field": field, "before": b, "after": a}
        if field == "atime" and a == expected_atime:
            expected_changes.append(entry)
        elif field == "mtime" and a == expected_mtime:
            expected_changes.append(entry)
        elif field == "ctime":
            expected_changes.append({**entry, "note": "ctime may change on inode write"})
        else:
            unexpected.append(entry)
    ok = after.get("atime") == expected_atime and after.get("mtime") == expected_mtime
    return {
        "ok": ok and not unexpected,
        "target_inode": after.get("inode"),
        "expected_changes": expected_changes,
        "unexpected_changes": unexpected,
        "expected_atime": expected_atime,
        "expected_mtime": expected_mtime,
        "actual_atime": after.get("atime"),
        "actual_mtime": after.get("mtime"),
        "log_side_effects": log_side_effects or {},
        "note": (
            "Target oracle is ROOT.TXT only; ESFI.LOG create/write is an expected "
            "separate artifact on the same CoW volume."
        ),
    }


def classify_log_side_effects(image: Path) -> dict[str, Any]:
    """Describe expected ESFI.LOG presence without treating it as target failure."""
    try:
        log_meta = parse_ext2_root_file_times(image, "ESFI.LOG")
        return {
            "esfi_log_present": True,
            "esfi_log_inode": log_meta.get("inode"),
            "esfi_log_size": log_meta.get("size"),
            "classification": "expected_test_log_artifact",
        }
    except ValueError as ex:
        return {
            "esfi_log_present": False,
            "classification": "missing_test_log",
            "error": str(ex),
        }


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("image", type=Path, help="EXT2 image path")
    ap.add_argument("--name", default="ROOT.TXT")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    info = read_inode_times(args.image, args.name)
    info["expected_primary"] = expected_primary_unix()
    info["sha256"] = sha256_file(args.image)
    if args.json:
        print(json.dumps(info, indent=2))
    else:
        print(info)


if __name__ == "__main__":
    main()
