"""Minimal whole-disk NTFS formatter (no external tools, no elevation).

Creates a valid NTFS boot sector and a small set of resident MFT records
sufficient for KolibriOS filesystem regression tests.
"""

from __future__ import annotations

import struct
import time
from pathlib import Path

BYTES_PER_SECTOR = 512
SECTORS_PER_CLUSTER = 8  # 4096-byte clusters
MFT_RECORD_SIZE = 1024
FILE_SIGNATURE = b"FILE"


def _cluster_size() -> int:
    return BYTES_PER_SECTOR * SECTORS_PER_CLUSTER


def _mft_lcn() -> int:
    return 4


def _mft_mirror_lcn() -> int:
    return 5


def _clusters_per_mft_record() -> int:
    # 1024 bytes = 2^10 → encoded as -10
    return -10


def _clusters_per_index() -> int:
    # 4096 bytes index blocks → one cluster → encoded as -8 (2^8=256) is wrong;
    # use -12 for 4096 (2^12) — stored as signed byte, max useful -31..-1.
    return -12


def create_boot_sector(total_bytes: int) -> bytes:
    total_sectors = total_bytes // BYTES_PER_SECTOR
    bs = bytearray(BYTES_PER_SECTOR)
    bs[0:3] = b"\xEB\x52\x90"
    bs[3:11] = b"NTFS    "
    struct.pack_into("<H", bs, 11, BYTES_PER_SECTOR)
    bs[13] = SECTORS_PER_CLUSTER
    # FAT fields must be zero (offsets 14-36)
    struct.pack_into("<Q", bs, 0x28, total_sectors)
    struct.pack_into("<Q", bs, 0x30, _mft_lcn())
    struct.pack_into("<Q", bs, 0x38, _mft_mirror_lcn())
    bs[0x40] = struct.pack("<b", _clusters_per_mft_record())[0]
    bs[0x44] = struct.pack("<b", _clusters_per_index())[0]
    bs[0x48:0x50] = b"\x00" * 8  # volume serial
    bs[510:512] = b"\x55\xAA"
    return bytes(bs)


def _filetime_now() -> int:
    # Windows FILETIME: 100-ns intervals since 1601-01-01.
    unix = int(time.time())
    return (unix + 11644473600) * 10_000_000


def _build_file_record(
    seq: int,
    in_use: bool,
    real_size: int,
    attrs: list[bytes],
) -> bytes:
    rec = bytearray(MFT_RECORD_SIZE)
    rec[0:4] = FILE_SIGNATURE
    rec[0x16:0x18] = struct.pack("<H", 0x0030)  # offset to first attribute
    rec[0x1C:0x20] = struct.pack("<I", seq << 16 | (1 if in_use else 0))
    rec[0x28:0x30] = struct.pack("<Q", real_size) * 1  # allocated size
    struct.pack_into("<Q", rec, 0x28, real_size)
    struct.pack_into("<Q", rec, 0x30, real_size)

    off = 0x30
    for attr in attrs:
        rec[off : off + len(attr)] = attr
        off += len(attr)
    # End marker
    rec[off : off + 4] = struct.pack("<I", 0xFFFFFFFF)
    # Fixup
    fixup_off = 0x4
    fixup_count = MFT_RECORD_SIZE // BYTES_PER_SECTOR
    struct.pack_into("<H", rec, fixup_off, 1)
    struct.pack_into("<H", rec, fixup_off + 2, fixup_count)
    for i in range(fixup_count):
        sector_start = (i + 1) * BYTES_PER_SECTOR - 2
        struct.pack_into("<H", rec, fixup_off + 4 + i * 2, struct.unpack_from("<H", rec, sector_start)[0])
        rec[sector_start : sector_start + 2] = struct.pack("<H", i + 1)
    return bytes(rec)


def _attr_standard_information() -> bytes:
    data = bytearray(72)
    struct.pack_into("<I", data, 0, 0x10)  # STANDARD_INFORMATION
    struct.pack_into("<I", data, 4, 72)
    struct.pack_into("<B", data, 8, 0)  # resident
    ft = _filetime_now()
    for i, off in enumerate((0x18, 0x20, 0x28, 0x30, 0x38)):
        struct.pack_into("<Q", data, off, ft + i)
    struct.pack_into("<I", data, 0x40, 0x20)  # archive
    return bytes(data)


def _attr_file_name(name: str, parent_ref: int, is_dir: bool) -> bytes:
    name_utf16 = name.encode("utf-16le")
    name_len = len(name_utf16) // 2
    content = bytearray(0x52 + len(name_utf16))
    struct.pack_into("<I", content, 0, 0x30)  # FILE_NAME
    struct.pack_into("<I", content, 4, len(content))
    struct.pack_into("<B", content, 8, 0)  # resident
    struct.pack_into("<Q", content, 0x10, parent_ref)
    ft = _filetime_now()
    struct.pack_into("<Q", content, 0x18, ft)
    struct.pack_into("<Q", content, 0x20, ft)
    struct.pack_into("<Q", content, 0x28, ft)
    struct.pack_into("<Q", content, 0x30, ft)
    struct.pack_into("<Q", content, 0x38, 0)
    struct.pack_into("<I", content, 0x40, 0x20 if not is_dir else 0x10)
    struct.pack_into("<I", content, 0x44, 0)
    struct.pack_into("<Q", content, 0x48, 0)
    content[0x50] = name_len
    content[0x52 : 0x52 + len(name_utf16)] = name_utf16
    return bytes(content)


def _attr_data_resident(data: bytes) -> bytes:
    header_size = 24
    if len(data) > 700:
        raise ValueError(f"resident DATA too large for minimal NTFS ({len(data)} bytes)")
    content = bytearray(header_size + len(data))
    struct.pack_into("<I", content, 0, 0x80)  # DATA
    struct.pack_into("<I", content, 4, len(content))
    struct.pack_into("<B", content, 8, 0)
    struct.pack_into("<I", content, 0x14, len(data))
    struct.pack_into("<H", content, 0x16, header_size)
    content[header_size:] = data
    return bytes(content)


def _attr_index_root(entries: list[tuple[str, int, bool]]) -> bytes:
    # Minimal INDEX_ROOT for root directory.
    idx = bytearray(4096)
    struct.pack_into("<I", idx, 0, 0x90)  # INDEX_ROOT
    struct.pack_into("<I", idx, 4, len(idx))
    struct.pack_into("<B", idx, 8, 0)
    idx[0x10:0x18] = b"$I30" + b"\x00" * 4
    struct.pack_into("<I", idx, 0x18, 0x10)  # offset to index entries
    struct.pack_into("<I", idx, 0x1C, 0x30)  # size of index entries
    struct.pack_into("<I", idx, 0x20, 0x30)  # allocated
    struct.pack_into("<B", idx, 0x24, 1)  # has subnodes = false
    struct.pack_into("<H", idx, 0x30, 0x30)  # first entry offset
    off = 0x30
    for name, mft_ref, is_dir in entries:
        name_utf16 = name.encode("utf-16le")
        name_len = len(name_utf16) // 2
        entry = bytearray(0x52 + len(name_utf16))
        struct.pack_into("<Q", entry, 0x00, mft_ref)
        struct.pack_into("<H", entry, 0x08, 0x30 + 0x52)
        struct.pack_into("<I", entry, 0x0C, 0x30 + 0x52 + len(name_utf16))
        struct.pack_into("<I", entry, 0x10, 0x30 + 0x52 + len(name_utf16))
        struct.pack_into("<I", entry, 0x14, 0)
        struct.pack_into("<B", entry, 0x18, 0x30)
        struct.pack_into("<B", entry, 0x19, 0x03)
        struct.pack_into("<H", entry, 0x40, 0x30)
        struct.pack_into("<I", entry, 0x44, 0x20 if not is_dir else 0x10)
        entry[0x50] = name_len
        entry[0x52 : 0x52 + len(name_utf16)] = name_utf16
        idx[off : off + len(entry)] = entry
        off += len(entry)
    struct.pack_into("<I", idx, off, 0xFFFFFFFF)  # end entry
    return bytes(idx)


def format_minimal_ntfs(path: Path, size_bytes: int, files: dict[str, bytes | str]) -> None:
    """Create a whole-disk NTFS image with resident files under root."""
    cluster = _cluster_size()
    # Only embed files small enough for resident MFT DATA attributes.
    small_files: dict[str, bytes | str] = {}
    for name, data in files.items():
        if isinstance(data, str):
            data_bytes = data.encode("ascii")
        else:
            data_bytes = data
        if len(data_bytes) <= 700:
            small_files[name] = data
        else:
            # Stub large files so the directory tree still references them.
            stub = f"STUB for {name} ({len(data_bytes)} bytes); use exFAT LARGE/ for full payload.\n"
            small_files[name] = stub

    files = small_files
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as f:
        f.truncate(size_bytes)

    boot = create_boot_sector(size_bytes)
    with open(path, "r+b") as f:
        f.seek(0)
        f.write(boot)
        f.seek(6 * BYTES_PER_SECTOR)
        f.write(boot)

    # Build MFT records in memory.
    records: list[bytes] = []

    # 0: $MFT (placeholder)
    records.append(b"\x00" * MFT_RECORD_SIZE)
    # 1: $MFTMirr placeholder
    records.append(b"\x00" * MFT_RECORD_SIZE)
    # 2: $LogFile placeholder
    records.append(b"\x00" * MFT_RECORD_SIZE)
    # 3: $Volume
    vol_data = b"\x00" * 8
    records.append(
        _build_file_record(
            3,
            True,
            MFT_RECORD_SIZE,
            [
                _attr_standard_information(),
                _attr_file_name("$Volume", 5, False),
                _attr_data_resident(vol_data),
            ],
        )
    )
    # 4: $AttrDef placeholder
    records.append(b"\x00" * MFT_RECORD_SIZE)
    # 5: root directory .
    root_entries: list[tuple[str, int, bool]] = []
    file_records_start = 6
    names = sorted(files.keys())
    for i, name in enumerate(names):
        mft_ref = (file_records_start + i) << 48  # simplified reference
        root_entries.append((name.split("/")[-1], mft_ref, False))

    records.append(
        _build_file_record(
            5,
            True,
            MFT_RECORD_SIZE,
            [
                _attr_standard_information(),
                _attr_file_name(".", 5, True),
                _attr_index_root(root_entries),
            ],
        )
    )

    for i, name in enumerate(names):
        data = files[name]
        if isinstance(data, str):
            data = data.encode("ascii")
        base = name.split("/")[-1]
        parent = 5
        records.append(
            _build_file_record(
                file_records_start + i,
                True,
                MFT_RECORD_SIZE,
                [
                    _attr_standard_information(),
                    _attr_file_name(base, parent << 48, False),
                    _attr_data_resident(data),
                ],
            )
        )

    # Pad to fill one cluster with MFT records.
    while len(records) * MFT_RECORD_SIZE < cluster:
        records.append(b"\x00" * MFT_RECORD_SIZE)

    mft_data = b"".join(records[: cluster // MFT_RECORD_SIZE])

    with open(path, "r+b") as f:
        f.seek(_mft_lcn() * cluster)
        f.write(mft_data)
        f.seek(_mft_mirror_lcn() * cluster)
        f.write(mft_data[:cluster])
