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
    # Must not overlap extended MFT clusters (MFT at LCN 4; records 4+ spill to LCN 5+).
    return 32


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
    *,
    is_dir: bool = False,
) -> bytes:
    rec = bytearray(MFT_RECORD_SIZE)
    rec[0:4] = FILE_SIGNATURE
    # Windows/Kolibri layout: 48-byte FILE header, USA immediately after it,
    # attributes after the USA. Putting the USA *after* the attributes used to
    # overlay the 0xFFFFFFFF terminator whenever the attr list was 8-byte
    # aligned; Kolibri ``ntfs_create_partition.scandata`` then sees
    # sizeWithHeader=0 and infinite-loops (boot stall before firstapp).
    num_sectors = MFT_RECORD_SIZE // BYTES_PER_SECTOR
    usa_count = num_sectors + 1
    usa_off = 0x30
    first_attr = ((usa_off + usa_count * 2 + 7) // 8) * 8  # 0x38
    struct.pack_into("<H", rec, 4, usa_off)
    struct.pack_into("<H", rec, 6, usa_count)
    struct.pack_into("<H", rec, 0x10, seq)  # sequence number
    struct.pack_into("<H", rec, 0x12, 1)  # hard link count
    struct.pack_into("<H", rec, 0x14, first_attr)
    flags = 0x01 if in_use else 0x00
    if is_dir:
        flags |= 0x02
    struct.pack_into("<H", rec, 0x16, flags)
    struct.pack_into("<I", rec, 0x18, real_size)  # actual size
    struct.pack_into("<I", rec, 0x1C, MFT_RECORD_SIZE)  # allocated size
    struct.pack_into("<Q", rec, 0x20, 0)  # base FILE record reference (0 = this is the base)

    off = first_attr
    for attr in attrs:
        rec[off : off + len(attr)] = attr
        off += len(attr)
    if off + 4 > MFT_RECORD_SIZE:
        raise ValueError(f"MFT record attrs too large ({off} bytes)")
    rec[off : off + 4] = struct.pack("<I", 0xFFFFFFFF)

    struct.pack_into("<H", rec, usa_off, 1)  # update sequence number
    for i in range(1, usa_count):
        sector_tail_off = i * BYTES_PER_SECTOR - 2
        struct.pack_into(
            "<H",
            rec,
            usa_off + i * 2,
            struct.unpack_from("<H", rec, sector_tail_off)[0],
        )
        rec[sector_tail_off : sector_tail_off + 2] = struct.pack("<H", 1)
    return bytes(rec)


def _attr_standard_information() -> bytes:
    val_off = 0x18
    val_len = 48
    total = val_off + val_len
    data = bytearray(total)
    struct.pack_into("<I", data, 0, 0x10)  # STANDARD_INFORMATION
    struct.pack_into("<I", data, 4, total)
    struct.pack_into("<B", data, 8, 0)  # resident
    struct.pack_into("<I", data, 0x10, val_len)
    struct.pack_into("<H", data, 0x14, val_off)
    ft = _filetime_now()
    struct.pack_into("<Q", data, 0x18, ft)
    struct.pack_into("<Q", data, 0x20, ft + 1)
    struct.pack_into("<Q", data, 0x28, ft + 2)
    struct.pack_into("<Q", data, 0x30, ft + 3)
    struct.pack_into("<I", data, 0x38, 0x20)  # FILE_ATTRIBUTE_ARCHIVE at SI+0x20
    return bytes(data)


def _attr_file_name(name: str, parent_ref: int, is_dir: bool) -> bytes:
    name_utf16 = name.encode("utf-16le")
    name_len = len(name_utf16) // 2
    val_off = 0x18
    val_len = 0x42 + len(name_utf16)
    total = val_off + val_len
    content = bytearray(total)
    struct.pack_into("<I", content, 0, 0x30)  # FILE_NAME
    struct.pack_into("<I", content, 4, total)
    struct.pack_into("<B", content, 8, 0)  # resident
    struct.pack_into("<I", content, 0x10, val_len)
    struct.pack_into("<H", content, 0x14, val_off)
    struct.pack_into("<Q", content, val_off + 0x00, parent_ref)
    ft = _filetime_now()
    struct.pack_into("<Q", content, val_off + 0x08, ft)
    struct.pack_into("<Q", content, val_off + 0x10, ft)
    struct.pack_into("<Q", content, val_off + 0x18, ft)
    struct.pack_into("<Q", content, val_off + 0x20, ft)
    struct.pack_into("<Q", content, val_off + 0x28, 0)
    struct.pack_into("<Q", content, val_off + 0x30, 0)
    struct.pack_into("<I", content, val_off + 0x38, 0x20 if not is_dir else 0x10)
    struct.pack_into("<I", content, val_off + 0x3C, 0)
    content[val_off + 0x40] = name_len
    content[val_off + 0x41] = 0x01  # Win32 namespace
    content[val_off + 0x42 : val_off + 0x42 + len(name_utf16)] = name_utf16
    return bytes(content)


def _encode_mcb_single(length_clusters: int, lcn: int) -> bytes:
    """One mapping pair: ``length_clusters`` consecutive clusters at ``lcn``."""
    if length_clusters <= 0 or length_clusters >= 0x10000:
        raise ValueError(f"unsupported MCB length {length_clusters}")
    if lcn < 0 or lcn >= 0x10000:
        raise ValueError(f"unsupported MCB LCN {lcn}")
    return bytes([0x11, length_clusters & 0xFF, lcn & 0xFF, 0x00])


def _attr_nonresident(
    atype: int,
    mcb: bytes,
    *,
    alloc: int,
    real: int,
    init: int | None = None,
    name: str | None = None,
) -> bytes:
    """Build a minimal non-resident NTFS attribute (Kolibri ``ntfs.inc`` layout)."""
    init = real if init is None else init
    name_utf16 = name.encode("utf-16le") if name else b""
    name_len = len(name_utf16) // 2
    run_off = 0x40
    if name_len:
        run_off = max(run_off, 0x18 + len(name_utf16))
        run_off = ((run_off + 7) // 8) * 8
    total = run_off + len(mcb)
    attr = bytearray(((total + 7) // 8) * 8)
    struct.pack_into("<I", attr, 0, atype)
    struct.pack_into("<I", attr, 4, total)
    struct.pack_into("<B", attr, 8, 1)  # non-resident
    if name_len:
        struct.pack_into("<B", attr, 9, name_len)
        struct.pack_into("<H", attr, 10, 0x18)
        attr[0x18 : 0x18 + len(name_utf16)] = name_utf16
    struct.pack_into("<Q", attr, 0x10, 0)  # starting VCN
    last_vcn = max(0, (real - 1) // _cluster_size()) if real else 0
    struct.pack_into("<Q", attr, 0x18, last_vcn)
    struct.pack_into("<H", attr, 0x20, run_off)
    struct.pack_into("<Q", attr, 0x28, alloc)
    struct.pack_into("<Q", attr, 0x30, real)
    struct.pack_into("<Q", attr, 0x38, init)
    attr[run_off : run_off + len(mcb)] = mcb
    return bytes(attr[:total])


def _volume_bitmap_lcn() -> int:
    return 34


def _mft_bitmap_lcn() -> int:
    return 33


def _mark_clusters(bitmap: bytearray, *clusters: int) -> None:
    for c in clusters:
        bitmap[c >> 3] |= 1 << (c & 7)


def _mark_mft_records(bitmap: bytearray, *records: int) -> None:
    for r in records:
        bitmap[r >> 3] |= 1 << (r & 7)


def _attr_data_resident(data: bytes) -> bytes:
    val_off = 0x18
    if len(data) > 700:
        raise ValueError(f"resident DATA too large for minimal NTFS ({len(data)} bytes)")
    total = val_off + len(data)
    content = bytearray(total)
    struct.pack_into("<I", content, 0, 0x80)  # DATA
    struct.pack_into("<I", content, 4, total)
    struct.pack_into("<B", content, 8, 0)
    struct.pack_into("<I", content, 0x10, len(data))
    struct.pack_into("<H", content, 0x14, val_off)
    content[val_off:] = data
    return bytes(content)


def _make_index_entry(
    name: str,
    mft_ref: int,
    is_dir: bool,
    *,
    data_size: int = 0,
    filetimes: dict[str, int] | None = None,
) -> bytes:
    """Build a $I30 index entry matching ``kernel/fs/ntfs.inc`` offsets."""
    name_utf16 = name.encode("utf-16le")
    name_len = len(name_utf16) // 2
    body_len = 0x52 + len(name_utf16)
    alloc_len = ((body_len + 7) // 8) * 8
    entry = bytearray(alloc_len)
    struct.pack_into("<Q", entry, 0x00, mft_ref)
    # Kolibri advances with WORD indexAllocatedSize — must be the padded stride.
    struct.pack_into("<H", entry, 0x08, alloc_len)
    struct.pack_into("<H", entry, 0x0A, body_len)  # indexRawSize
    struct.pack_into("<H", entry, 0x0C, 0)  # indexFlags
    struct.pack_into("<Q", entry, 0x10, (5 << 48) | 5)  # parent = record 5, seq 5
    ft = _filetime_now()
    times = filetimes or {}
    struct.pack_into("<Q", entry, 0x18, times.get("created", ft))
    struct.pack_into("<Q", entry, 0x20, times.get("modified", ft))
    struct.pack_into("<Q", entry, 0x28, times.get("record_modified", ft))
    struct.pack_into("<Q", entry, 0x30, times.get("accessed", ft))
    struct.pack_into("<Q", entry, 0x38, max(data_size, 1))
    struct.pack_into("<Q", entry, 0x40, data_size)
    struct.pack_into("<I", entry, 0x48, 0x20 if not is_dir else 0x10)
    entry[0x50] = name_len
    entry[0x51] = 0x01  # Win32 namespace
    entry[0x52 : 0x52 + len(name_utf16)] = name_utf16
    return bytes(entry)


def _attr_index_root(entries: list[tuple[str, int, bool, int]]) -> bytes:
    """Minimal resident ``$INDEX_ROOT`` named ``$I30`` for the root directory."""
    # Attribute header (resident)
    attr = bytearray(4096)
    struct.pack_into("<I", attr, 0, 0x90)  # INDEX_ROOT
    struct.pack_into("<I", attr, 4, len(attr))
    struct.pack_into("<B", attr, 8, 0)  # resident
    struct.pack_into("<B", attr, 9, 4)  # name length
    struct.pack_into("<H", attr, 10, 0x18)  # name offset
    attr[0x18:0x20] = "$I30".encode("utf-16le")
    struct.pack_into("<H", attr, 0x14, 0x30)  # value offset
    # INDEX_ROOT value body
    val_off = 0x30
    struct.pack_into("<I", attr, val_off + 0x00, 0x30)  # indexed attr type $FILE_NAME
    struct.pack_into("<I", attr, val_off + 0x04, 1)  # COLLATION_FILE_NAME
    struct.pack_into("<I", attr, val_off + 0x08, 4096)  # index block size
    struct.pack_into("<B", attr, val_off + 0x0C, 1)  # clusters per index block
    hdr = val_off + 0x10  # INDEX_HEADER
    ent_off = hdr + 0x10  # first entry (EntriesOffset = 0x10)
    struct.pack_into("<I", attr, hdr + 0x00, 0x10)  # EntriesOffset
    struct.pack_into("<I", attr, hdr + 0x04, 0)  # TotalSize (filled later)
    struct.pack_into("<I", attr, hdr + 0x08, 0)  # AllocatedSize
    struct.pack_into("<B", attr, hdr + 0x0C, 0)  # not leaf with subnodes
    for name, mft_ref, is_dir, data_size in entries:
        ent = _make_index_entry(name, mft_ref, is_dir, data_size=data_size)
        attr[ent_off : ent_off + len(ent)] = ent
        ent_off += len(ent)
    end = bytearray(16)
    struct.pack_into("<H", end, 0x08, 16)
    struct.pack_into("<H", end, 0x0A, 16)
    struct.pack_into("<H", end, 0x0C, 2)  # INDEX_ENTRY_END
    attr[ent_off : ent_off + 16] = end
    used = ent_off + 16
    struct.pack_into("<I", attr, hdr + 0x04, used - hdr)
    struct.pack_into("<I", attr, hdr + 0x08, used - hdr)
    struct.pack_into("<I", attr, 4, used)  # shrink attribute size
    struct.pack_into("<I", attr, 0x10, used - 0x30)  # value length
    return bytes(attr[:used])


def _attr_index_root_legacy(entries: list[tuple[str, int, bool]]) -> bytes:
    """Deprecated alias — kept for callers passing 3-tuples."""
    return _attr_index_root([(n, r, d, 0) for n, r, d in entries])


def _write_mbr(path: Path, size_bytes: int, part_lba: int) -> None:
    """Write a one-partition MBR; NTFS volume begins at ``part_lba``."""
    part_sectors = (size_bytes // BYTES_PER_SECTOR) - part_lba
    mbr = bytearray(BYTES_PER_SECTOR)
    mbr[510:512] = b"\x55\xAA"
    ent = 0x1BE
    mbr[ent + 4] = 0x07  # NTFS
    struct.pack_into("<I", mbr, ent + 8, part_lba)
    struct.pack_into("<I", mbr, ent + 12, part_sectors)
    with open(path, "r+b") as f:
        f.seek(0)
        f.write(mbr)


def format_minimal_ntfs(
    path: Path,
    size_bytes: int,
    files: dict[str, bytes | str],
    *,
    part_lba: int = 2048,
) -> None:
    """Create an NTFS image with resident files under root.

    ``part_lba=2048`` (default) writes an MBR + one type-0x07 partition.
    ``part_lba=0`` writes a whole-disk NTFS volume starting at LBA 0.
    """
    if part_lba < 0:
        raise ValueError("part_lba must be >= 0")
    vol_off = part_lba * BYTES_PER_SECTOR
    vol_bytes = size_bytes - vol_off
    if vol_bytes < 8 * 1024 * 1024:
        raise ValueError("NTFS image too small after MBR/partition offset")
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

    boot = create_boot_sector(vol_bytes)
    with open(path, "r+b") as f:
        f.seek(vol_off)
        f.write(boot)
        f.seek(vol_off + 6 * BYTES_PER_SECTOR)
        f.write(boot)
    if part_lba > 0:
        _write_mbr(path, size_bytes, part_lba)

    # Build MFT records in memory (sparse dict keyed by record number).
    cluster = _cluster_size()
    records: dict[int, bytes] = {}
    file_records_start = 16  # Kolibri CreateFile denies iRecord < 16

    def put_rec(num: int, data: bytes) -> None:
        records[num] = data

    names = sorted(files.keys())
    user_record_nums = [file_records_start + i for i in range(len(names))]
    live_records = list(range(16)) + user_record_nums
    max_rec = max(live_records) + 8

    mft_clusters = max(4, (max_rec + 2) * MFT_RECORD_SIZE // cluster + 1)
    mft_bytes = mft_clusters * cluster
    mft_mcb = _encode_mcb_single(mft_clusters, _mft_lcn())

    mft_bitmap = bytearray(16)
    _mark_mft_records(mft_bitmap, *live_records)
    mft_bitmap_mcb = _encode_mcb_single(1, _mft_bitmap_lcn())

    total_clusters = vol_bytes // cluster
    vol_bitmap = bytearray((total_clusters + 7) // 8)
    used_clusters = list(range(min(35, total_clusters)))
    _mark_clusters(vol_bitmap, *used_clusters)
    vol_bitmap_mcb = _encode_mcb_single(1, _volume_bitmap_lcn())

    put_rec(
        0,
        _build_file_record(
            1,
            True,
            MFT_RECORD_SIZE,
            [
                _attr_standard_information(),
                _attr_nonresident(
                    0x80,
                    mft_mcb,
                    alloc=mft_bytes,
                    real=mft_bytes,
                ),
                _attr_nonresident(
                    0xB0,
                    mft_bitmap_mcb,
                    alloc=cluster,
                    real=len(mft_bitmap),
                ),
            ],
        ),
    )
    vol_data = b"\x00" * 8
    put_rec(
        3,
        _build_file_record(
            3,
            True,
            MFT_RECORD_SIZE,
            [
                _attr_standard_information(),
                _attr_file_name("$Volume", 5 | (5 << 48), False),
                _attr_data_resident(vol_data),
            ],
        ),
    )
    root_entries: list[tuple[str, int, bool, int]] = []
    for i, name in enumerate(names):
        data = files[name]
        if isinstance(data, str):
            data = data.encode("ascii")
        base = name.split("/")[-1]
        mft_ref = user_record_nums[i] | (user_record_nums[i] << 48)
        root_entries.append((base, mft_ref, False, len(data)))

    put_rec(
        5,
        _build_file_record(
            5,
            True,
            MFT_RECORD_SIZE,
            [
                _attr_standard_information(),
                _attr_file_name(".", 5 | (5 << 48), True),
                _attr_index_root(root_entries),
            ],
            is_dir=True,
        ),
    )
    put_rec(
        6,
        _build_file_record(
            6,
            True,
            MFT_RECORD_SIZE,
            [
                _attr_standard_information(),
                _attr_file_name("$Bitmap", 5 | (5 << 48), False),
                _attr_nonresident(
                    0x80,
                    vol_bitmap_mcb,
                    alloc=cluster,
                    real=len(vol_bitmap),
                ),
            ],
        ),
    )

    for i, name in enumerate(names):
        data = files[name]
        if isinstance(data, str):
            data = data.encode("ascii")
        base = name.split("/")[-1]
        rec_num = user_record_nums[i]
        put_rec(
            rec_num,
            _build_file_record(
                rec_num,
                True,
                MFT_RECORD_SIZE,
                [
                    _attr_standard_information(),
                    _attr_file_name(base, 5 | (5 << 48), False),
                    _attr_data_resident(data),
                ],
            ),
        )

    mft_records: list[bytes] = []
    for n in range(max_rec + 1):
        mft_records.append(records.get(n, b"\x00" * MFT_RECORD_SIZE))

    # Pad MFT allocation to whole clusters.
    while len(mft_records) * MFT_RECORD_SIZE < mft_bytes:
        mft_records.append(b"\x00" * MFT_RECORD_SIZE)

    mft_data = b"".join(mft_records)

    with open(path, "r+b") as f:
        f.seek(vol_off + _mft_lcn() * cluster)
        f.write(mft_data)
        f.seek(vol_off + _mft_mirror_lcn() * cluster)
        f.write(mft_data[:cluster])
        f.seek(vol_off + _mft_bitmap_lcn() * cluster)
        f.write(bytes(mft_bitmap).ljust(cluster, b"\x00"))
        f.seek(vol_off + _volume_bitmap_lcn() * cluster)
        f.write(bytes(vol_bitmap).ljust(cluster, b"\x00"))
