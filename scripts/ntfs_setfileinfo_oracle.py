"""Host-only NTFS index-entry / BDFE helpers for the SetFileInfo oracle.

Independent of production Kolibri NTFS Rust blobs. Parses the parent
directory ``$I30`` index entry for a named file — **not** the file MFT
``$STANDARD_INFORMATION`` (``ntfs_SetFileInfo`` mutates the index copy only).

Reuses FASM-faithful BDFE↔FILETIME math from the EXT oracle pattern.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any

# Deterministic primary mutation (must match tools/ntfssoak/ntfssoak.asm).
REQ_ATIME = (11, 22, 14, 4, 7, 2012)
REQ_MTIME = (30, 5, 9, 23, 11, 2018)

# ntfsCalculateTime bias (1601 FILETIME base).
NTFS_FT_BIAS_LO = 3_365_781_504
NTFS_FT_BIAS_HI = 29_389_701
FT_MULTIPLIER = 10_000_000

FILE_SIGNATURE = b"FILE"
ATTR_INDEX_ROOT = 0x90
ATTR_STANDARD_INFORMATION = 0x10
ATTR_FILE_NAME = 0x30
ATTR_END = 0xFFFFFFFF
ROOT_MFT_RECORD = 5

# $I30 index entry offsets (hex, kernel/fs/ntfs.inc).
IDX_FILE_CREATED = 0x18
IDX_FILE_MODIFIED = 0x20
IDX_RECORD_MODIFIED = 0x28
IDX_FILE_ACCESSED = 0x30
IDX_FILE_REAL_SIZE = 0x40
IDX_FILE_FLAGS = 0x48
IDX_NAME_LEN = 0x50
IDX_NAME = 0x52


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


def bdfe_to_filetime(t: BdfeTime) -> tuple[int, int]:
    """``ntfsCalculateTime``: BDFE → Windows FILETIME (EDX:EAX)."""
    kos = fs_calculate_time(t) & 0xFFFFFFFF
    # 64-bit: kos * 10_000_000 + bias, including ADC carry into high dword.
    product = kos * FT_MULTIPLIER
    total = product + NTFS_FT_BIAS_LO + (NTFS_FT_BIAS_HI << 32)
    lo = total & 0xFFFFFFFF
    hi = (total >> 32) & 0xFFFFFFFF
    return lo, hi


def filetime_to_qword(lo: int, hi: int) -> int:
    return ((hi & 0xFFFFFFFF) << 32) | (lo & 0xFFFFFFFF)


def expected_primary_filetimes() -> dict[str, Any]:
    at = BdfeTime(*REQ_ATIME)
    mt = BdfeTime(*REQ_MTIME)
    alo, ahi = bdfe_to_filetime(at)
    mlo, mhi = bdfe_to_filetime(mt)
    return {
        "atime_bdfe": list(at.as_tuple()),
        "mtime_bdfe": list(mt.as_tuple()),
        "atime_bdfe_hex": at.to_bytes().hex(),
        "mtime_bdfe_hex": mt.to_bytes().hex(),
        "atime_filetime_lo": alo,
        "atime_filetime_hi": ahi,
        "mtime_filetime_lo": mlo,
        "mtime_filetime_hi": mhi,
        "atime_filetime": filetime_to_qword(alo, ahi),
        "mtime_filetime": filetime_to_qword(mlo, mhi),
    }


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def find_ntfs_boot_offset(raw: bytes) -> int:
    if len(raw) < 512:
        raise ValueError("image too small")
    if raw[3:11] == b"NTFS    ":
        return 0
    if raw[510:512] != b"\x55\xaa":
        raise ValueError("no MBR or NTFS boot sector")
    for ent_off in (0x1BE, 0x1CE, 0x1DE, 0x1EE):
        lba = struct.unpack_from("<I", raw, ent_off + 8)[0]
        if lba == 0:
            continue
        off = lba * 512
        if off + 512 <= len(raw) and raw[off + 3 : off + 11] == b"NTFS    ":
            return off
    raise ValueError("NTFS boot sector not found")


def parse_boot_sector(raw: bytes, boot_off: int) -> dict[str, int]:
    boot = raw[boot_off : boot_off + 512]
    if boot[3:11] != b"NTFS    ":
        raise ValueError("bad NTFS OEM")
    bps = struct.unpack_from("<H", boot, 11)[0]
    spc = boot[13]
    mft_lcn = struct.unpack_from("<Q", boot, 0x30)[0]
    mftmirr_lcn = struct.unpack_from("<Q", boot, 0x38)[0]
    cpr = struct.unpack_from("<b", boot, 0x40)[0]
    if cpr >= 0:
        mft_record_size = cpr * bps * spc
    else:
        mft_record_size = 1 << (-cpr)
    return {
        "boot_offset": boot_off,
        "bytes_per_sector": bps,
        "sectors_per_cluster": spc,
        "cluster_size": bps * spc,
        "mft_lcn": mft_lcn,
        "mft_mirror_lcn": mftmirr_lcn,
        "mft_record_size": mft_record_size,
    }


def apply_usa_fixup(raw_record: bytes, bps: int) -> tuple[bytes, dict[str, Any]]:
    """Apply update-sequence fixup; return logical record + USA metadata."""
    if len(raw_record) < 8 or raw_record[0:4] != FILE_SIGNATURE:
        raise ValueError("not a FILE record")
    usa_off = struct.unpack_from("<H", raw_record, 4)[0]
    usa_count = struct.unpack_from("<H", raw_record, 6)[0]
    if usa_off == 0 or usa_count < 2:
        return raw_record, {"usa_valid": False, "reason": "no USA"}
    usa = raw_record[usa_off : usa_off + usa_count * 2]
    seq = struct.unpack_from("<H", usa, 0)[0]
    logical = bytearray(raw_record)
    tails: list[int] = []
    ok = True
    for i in range(1, usa_count):
        sector_tail_off = i * bps - 2
        if sector_tail_off + 2 > len(logical):
            ok = False
            break
        stored = struct.unpack_from("<H", usa, i * 2)[0]
        tails.append(stored)
        on_disk = struct.unpack_from("<H", logical, sector_tail_off)[0]
        if on_disk != seq:
            ok = False
        logical[sector_tail_off : sector_tail_off + 2] = struct.pack("<H", stored)
    return bytes(logical), {
        "usa_valid": ok,
        "usa_offset": usa_off,
        "usa_count": usa_count,
        "sequence_number": seq,
        "stored_tails": tails,
    }


def read_mft_record(raw: bytes, vol: dict[str, int], record_num: int) -> tuple[bytes, dict[str, Any]]:
    bps = vol["bytes_per_sector"]
    rec_size = vol["mft_record_size"]
    off = vol["boot_offset"] + vol["mft_lcn"] * vol["cluster_size"] + record_num * rec_size
    if off + rec_size > len(raw):
        raise ValueError(f"MFT record {record_num} beyond image")
    raw_rec = raw[off : off + rec_size]
    logical, usa = apply_usa_fixup(raw_rec, bps)
    usa["record_number"] = record_num
    usa["disk_offset"] = off
    return logical, usa


def iter_attributes(record: bytes) -> list[dict[str, Any]]:
    if record[0:4] != FILE_SIGNATURE:
        raise ValueError("bad FILE record")
    first_attr = struct.unpack_from("<H", record, 0x14)[0]
    attrs: list[dict[str, Any]] = []
    off = first_attr
    while off + 4 <= len(record):
        atype = struct.unpack_from("<I", record, off)[0]
        if atype == ATTR_END or atype == 0:
            break
        size = struct.unpack_from("<I", record, off + 4)[0]
        if size < 24 or off + size > len(record):
            break
        non_res = record[off + 8]
        name_len = record[off + 9]
        name_off = struct.unpack_from("<H", record, off + 10)[0]
        name = b""
        if name_len:
            name = record[off + name_off : off + name_off + name_len * 2].decode(
                "utf-16le", "replace"
            )
        value = b""
        if non_res == 0:
            val_len = struct.unpack_from("<I", record, off + 0x10)[0]
            val_off = struct.unpack_from("<H", record, off + 0x14)[0]
            value = record[off + val_off : off + val_off + val_len]
        attrs.append(
            {
                "type": atype,
                "size": size,
                "non_resident": bool(non_res),
                "name": name,
                "value": value,
                "offset": off,
            }
        )
        off += size
    return attrs


def find_index_root_i30(attrs: list[dict[str, Any]]) -> bytes | None:
    for a in attrs:
        if a["type"] == ATTR_INDEX_ROOT and a["name"] == "$I30" and not a["non_resident"]:
            return a["value"]
    return None


def parse_index_entries(index_root_value: bytes) -> list[dict[str, Any]]:
    """Walk ``$INDEX_ROOT`` value body; return parsed index entries."""
    if len(index_root_value) < 0x20:
        return []
    # INDEX_HEADER starts at offset 0x10 within the INDEX_ROOT value.
    hdr_off = 0x10
    entry_rel = struct.unpack_from("<I", index_root_value, hdr_off)[0]
    pos = hdr_off + entry_rel
    entries: list[dict[str, Any]] = []
    while pos + 8 <= len(index_root_value):
        mref = struct.unpack_from("<Q", index_root_value, pos)[0]
        if mref == 0xFFFFFFFFFFFFFFFF:
            break
        raw_size = struct.unpack_from("<H", index_root_value, pos + 0x0A)[0]
        if raw_size == 0:
            raw_size = struct.unpack_from("<I", index_root_value, pos + 0x08)[0] & 0xFFFF
        if raw_size == 0:
            break
        if raw_size < IDX_NAME:
            break
        end = pos + raw_size
        if end > len(index_root_value):
            break
        chunk = index_root_value[pos:end]
        name_len = chunk[IDX_NAME_LEN]
        name_bytes = chunk[IDX_NAME : IDX_NAME + name_len * 2]
        try:
            name = name_bytes.decode("utf-16le")
        except UnicodeDecodeError:
            name = name_bytes.decode("utf-16le", "replace")
        entries.append(
            {
                "offset_in_index": pos,
                "mft_reference": mref,
                "mft_record": mref & 0xFFFFFFFFFFFF,
                "name": name,
                "file_created": struct.unpack_from("<Q", chunk, IDX_FILE_CREATED)[0],
                "file_modified": struct.unpack_from("<Q", chunk, IDX_FILE_MODIFIED)[0],
                "record_modified": struct.unpack_from("<Q", chunk, IDX_RECORD_MODIFIED)[0],
                "file_accessed": struct.unpack_from("<Q", chunk, IDX_FILE_ACCESSED)[0],
                "file_real_size": struct.unpack_from("<Q", chunk, IDX_FILE_REAL_SIZE)[0],
                "file_flags": struct.unpack_from("<I", chunk, IDX_FILE_FLAGS)[0],
                "raw_size": raw_size,
            }
        )
        pos = (end + 7) & ~7
    return entries


def find_index_entry_by_name(raw: bytes, name: str, *, root_record: int = ROOT_MFT_RECORD) -> dict[str, Any]:
    boot_off = find_ntfs_boot_offset(raw)
    vol = parse_boot_sector(raw, boot_off)
    record, root_usa = read_mft_record(raw, vol, root_record)
    attrs = iter_attributes(record)
    idx_val = find_index_root_i30(attrs)
    if idx_val is None:
        raise ValueError("$INDEX_ROOT $I30 not found in root MFT record")
    entries = parse_index_entries(idx_val)
    upper = name.upper()
    for e in entries:
        if e["name"].upper() == upper:
            return {
                "parser": "python_ntfs_index_mini",
                "volume": vol,
                "root_mft_record": root_record,
                "root_usa": root_usa,
                "entry": e,
                "all_index_names": [x["name"] for x in entries],
            }
    raise ValueError(f"{name!r} not in root $I30 index; have { [x['name'] for x in entries] }")


def parse_root_index_times(image: Path, name: str = "ROOT.TXT") -> dict[str, Any]:
    raw = image.read_bytes()
    info = find_index_entry_by_name(raw, name)
    e = info["entry"]
    return {
        "parser": info["parser"],
        "file": name,
        "mft_reference": e["mft_reference"],
        "mft_record": e["mft_record"],
        "index_offset": e["offset_in_index"],
        "file_created": e["file_created"],
        "file_modified": e["file_modified"],
        "record_modified": e["record_modified"],
        "file_accessed": e["file_accessed"],
        "file_real_size": e["file_real_size"],
        "file_flags": e["file_flags"],
        "root_usa": info["root_usa"],
        "volume": {k: v for k, v in info["volume"].items()},
    }


def walk_file_record_terminator(record: bytes) -> dict[str, Any]:
    """Kolibri ``.scandata`` replica: terminator must be present; size 0 hangs."""
    if record[0:4] != FILE_SIGNATURE:
        return {"ok": False, "reason": "not-FILE"}
    usa_off = struct.unpack_from("<H", record, 4)[0]
    first = struct.unpack_from("<H", record, 0x14)[0]
    off = first
    seen = 0
    for _ in range(64):
        if off + 8 > len(record):
            return {
                "ok": False,
                "reason": "walk-off-end",
                "offset": off,
                "usa_offset": usa_off,
                "first_attr": first,
                "seen": seen,
            }
        atype = struct.unpack_from("<I", record, off)[0]
        if atype == ATTR_END:
            return {
                "ok": True,
                "usa_offset": usa_off,
                "first_attr": first,
                "terminator_offset": off,
                "seen": seen,
            }
        size = struct.unpack_from("<I", record, off + 4)[0]
        if size == 0:
            return {
                "ok": False,
                "reason": "sizeWithHeader=0",
                "offset": off,
                "usa_offset": usa_off,
                "first_attr": first,
                "seen": seen,
            }
        if size < 8:
            return {"ok": False, "reason": "tiny-attr", "offset": off, "size": size}
        seen += 1
        off += size
    return {"ok": False, "reason": "too-many-attrs", "seen": seen}


def parse_file_mft_sidecar(image: Path, name: str = "ROOT.TXT") -> dict[str, Any]:
    """Parse target FILE record ``$STANDARD_INFORMATION`` + ``$FILE_NAME`` (not $I30)."""
    raw = image.read_bytes()
    info = find_index_entry_by_name(raw, name)
    rec_num = info["entry"]["mft_record"]
    record, usa = read_mft_record(raw, info["volume"], rec_num)
    walk = walk_file_record_terminator(record)
    si = None
    fn = None
    for a in iter_attributes(record):
        if a["type"] == ATTR_STANDARD_INFORMATION and not a["non_resident"] and len(a["value"]) >= 0x28:
            v = a["value"]
            si = {
                "created": struct.unpack_from("<Q", v, 0)[0],
                "modified": struct.unpack_from("<Q", v, 8)[0],
                "mft_changed": struct.unpack_from("<Q", v, 16)[0],
                "accessed": struct.unpack_from("<Q", v, 24)[0],
                "attrs": struct.unpack_from("<I", v, 32)[0],
            }
        if a["type"] == ATTR_FILE_NAME and not a["non_resident"] and len(a["value"]) >= 0x40:
            v = a["value"]
            fn = {
                "created": struct.unpack_from("<Q", v, 0x08)[0],
                "modified": struct.unpack_from("<Q", v, 0x10)[0],
                "mft_changed": struct.unpack_from("<Q", v, 0x18)[0],
                "accessed": struct.unpack_from("<Q", v, 0x20)[0],
            }
    return {
        "file": name,
        "mft_record": rec_num,
        "file_magic": record[0:4] == FILE_SIGNATURE,
        "usa": usa,
        "walk": walk,
        "standard_information": si,
        "file_name": fn,
    }


def preflight_ntfs_soak_image(image: Path, name: str = "ROOT.TXT") -> dict[str, Any]:
    """Host structural checks before QEMU. Fail closed on terminator/USA layout."""
    raw = image.read_bytes()
    boot_off = find_ntfs_boot_offset(raw)
    vol = parse_boot_sector(raw, boot_off)
    rec0, usa0 = read_mft_record(raw, vol, 0)
    walk0 = walk_file_record_terminator(rec0)
    rec5, usa5 = read_mft_record(raw, vol, ROOT_MFT_RECORD)
    walk5 = walk_file_record_terminator(rec5)
    idx = parse_root_index_times(image, name)
    sidecar = parse_file_mft_sidecar(image, name)
    layout_ok = (
        walk0.get("ok")
        and walk5.get("ok")
        and walk0.get("usa_offset") == 0x30
        and walk0.get("first_attr") == 0x38
        and usa0.get("usa_valid")
        and usa5.get("usa_valid")
        and sidecar.get("file_magic")
        and sidecar.get("standard_information") is not None
        and idx.get("mft_record") is not None
    )
    return {
        "ok": bool(layout_ok),
        "size": len(raw),
        "sha256": sha256_file(image),
        "boot_offset": boot_off,
        "volume": vol,
        "mft0_usa": usa0,
        "mft0_walk": walk0,
        "root_usa": usa5,
        "root_walk": walk5,
        "target_index": {
            "mft_record": idx.get("mft_record"),
            "file_accessed": idx.get("file_accessed"),
            "file_modified": idx.get("file_modified"),
        },
        "target_sidecar": sidecar,
    }


def sidecar_unchanged(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    """Assert file MFT $STANDARD_INFORMATION / $FILE_NAME did not change."""
    mismatches: list[str] = []
    for key in ("standard_information", "file_name"):
        if before.get(key) != after.get(key):
            mismatches.append(key)
    return {
        "ok": not mismatches,
        "mismatches": mismatches,
        "before": {k: before.get(k) for k in ("standard_information", "file_name", "mft_record")},
        "after": {k: after.get(k) for k in ("standard_information", "file_name", "mft_record")},
        "note": "ntfs_SetFileInfo writes parent $I30 only; file MFT SI/FN must stay put.",
    }
    raw = image.read_bytes()
    info = find_index_entry_by_name(raw, name)
    e = info["entry"]
    return {
        "parser": info["parser"],
        "file": name,
        "mft_reference": e["mft_reference"],
        "mft_record": e["mft_record"],
        "index_offset": e["offset_in_index"],
        "file_created": e["file_created"],
        "file_modified": e["file_modified"],
        "record_modified": e["record_modified"],
        "file_accessed": e["file_accessed"],
        "file_real_size": e["file_real_size"],
        "file_flags": e["file_flags"],
        "root_usa": info["root_usa"],
        "volume": {k: v for k, v in info["volume"].items()},
    }


def extract_ntfs_root_file(image: Path, name: str, max_bytes: int = 4096) -> bytes:
    """Best-effort read of a small resident file via MFT $DATA (host-only)."""
    raw = image.read_bytes()
    info = find_index_entry_by_name(raw, name)
    rec_num = info["entry"]["mft_record"]
    vol = info["volume"]
    record, _ = read_mft_record(raw, vol, rec_num)
    for a in iter_attributes(record):
        if a["type"] == 0x80 and not a["non_resident"]:
            return a["value"][:max_bytes]
    raise ValueError(f"resident $DATA not found for {name}")


def parse_guest_report(blob: bytes) -> dict[str, Any]:
    if len(blob) < 140:
        return {"error": "short", "raw_len": len(blob)}
    if blob[0:4] != b"NSFI":
        return {"error": "bad_magic", "magic": blob[0:4].hex()}

    def u32(off: int) -> int:
        return struct.unpack_from("<I", blob, off)[0]

    flags = u32(8)
    version = u32(4)
    out: dict[str, Any] = {
        "magic": "NSFI",
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
    }
    if version >= 1 and len(blob) >= 164:
        out["run_id"] = u32(140)
        out["run_id_hex"] = f"0x{u32(140):08X}"
        out["create_log_eax"] = u32(144)
        out["write_log_eax"] = u32(148)
        out["target_mft_hint"] = u32(152)
        tag = blob[156:160]
        out["path_tag"] = tag.decode("ascii", "replace")
        out["ticks"] = u32(160)
    return out


def validate_root_usa(raw: bytes, root_record: int = ROOT_MFT_RECORD) -> dict[str, Any]:
    boot_off = find_ntfs_boot_offset(raw)
    vol = parse_boot_sector(raw, boot_off)
    _, usa = read_mft_record(raw, vol, root_record)
    return usa


def metadata_diff(
    before: dict[str, Any],
    after: dict[str, Any],
    expected: dict[str, Any],
    *,
    log_side_effects: dict[str, Any] | None = None,
) -> dict[str, Any]:
    exp_a = expected["atime_filetime"]
    exp_m = expected["mtime_filetime"]
    expected_changes = []
    unexpected = []
    checks = [
        ("file_accessed", exp_a, "atime index entry"),
        ("file_modified", exp_m, "mtime index entry"),
    ]
    for field, exp_val, note in checks:
        b = before.get(field)
        a = after.get(field)
        if b == a and a != exp_val:
            continue
        entry = {"field": field, "before": b, "after": a, "note": note}
        if a == exp_val:
            expected_changes.append(entry)
        elif b != a:
            unexpected.append(entry)
    ok = (
        after.get("file_accessed") == exp_a
        and after.get("file_modified") == exp_m
        and after.get("file_real_size") == before.get("file_real_size")
        and after.get("file_flags") == before.get("file_flags")
    )
    if after.get("file_created") != before.get("file_created"):
        # SetFileInfo writes ctime from buffer — may change if guest sent ctime BDFE
        expected_changes.append(
            {
                "field": "file_created",
                "before": before.get("file_created"),
                "after": after.get("file_created"),
                "note": "ctime slot from SetFileInfo buffer",
            }
        )
    return {
        "ok": ok and not unexpected,
        "target": after.get("file"),
        "mft_record": after.get("mft_record"),
        "expected_changes": expected_changes,
        "unexpected_changes": unexpected,
        "expected_atime_filetime": exp_a,
        "expected_mtime_filetime": exp_m,
        "actual_atime_filetime": after.get("file_accessed"),
        "actual_mtime_filetime": after.get("file_modified"),
        "log_side_effects": log_side_effects or {},
        "note": (
            "Oracle reads parent $I30 index entry for ROOT.TXT, not file MFT "
            "$STANDARD_INFORMATION. NSFI.LOG is a separate expected artifact."
        ),
    }


def metadata_diff_control(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    fields = ("file_accessed", "file_modified", "file_created", "file_flags", "file_real_size")
    unexpected = [
        {"field": f, "before": before.get(f), "after": after.get(f)}
        for f in fields
        if before.get(f) != after.get(f)
    ]
    return {
        "ok": not unexpected,
        "target": after.get("file"),
        "mft_record": after.get("mft_record"),
        "unexpected_changes": unexpected,
        "note": "control run: ROOT.TXT $I30 must be unchanged (no SetFileInfo)",
    }


def classify_log_side_effects(image: Path) -> dict[str, Any]:
    try:
        meta = parse_root_index_times(image, "NSFI.LOG")
        return {
            "nsfi_log_present": True,
            "nsfi_log_mft_record": meta.get("mft_record"),
            "classification": "expected_test_log_artifact",
        }
    except ValueError as ex:
        return {
            "nsfi_log_present": False,
            "classification": "missing_test_log",
            "error": str(ex),
        }


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("image", type=Path, help="NTFS image path")
    ap.add_argument("--name", default="ROOT.TXT")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    info = parse_root_index_times(args.image, args.name)
    info["expected_primary"] = expected_primary_filetimes()
    info["sha256"] = sha256_file(args.image)
    info["usa"] = validate_root_usa(args.image.read_bytes())
    if args.json:
        print(json.dumps(info, indent=2))
    else:
        print(info)


if __name__ == "__main__":
    main()
