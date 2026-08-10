"""Shared deterministic test-tree content for filesystem regression images."""

from __future__ import annotations

import hashlib

README = (
    "KolibriOS filesystem regression-test fixture\n"
    "Purpose: deterministic IDE disk for QEMU kernel tests.\n"
    "Do not use as a boot disk.\n"
)

ROOT_TXT = "ROOT.TXT: file in the volume root for enumeration tests.\n"
EMPTY_TXT = b""

DATA_ONE = "DATA/ONE.TXT: first file in DATA/.\n"
DATA_TWO = "DATA/TWO.TXT: second file in DATA/.\n"
DATA_THREE = "DATA/THREE.TXT: third file in DATA/.\n"

NESTED_A1 = "NESTED/A/FILE_A1.TXT: nested under NESTED/A/.\n"
NESTED_A2 = "NESTED/A/FILE_A2.TXT: sibling of FILE_A1.TXT.\n"
NESTED_B1 = "NESTED/B/FILE_B1.TXT: nested under NESTED/B/.\n"
NESTED_B2 = "NESTED/B/FILE_B2.TXT: sibling of FILE_B1.TXT.\n"

# Filename with space (supported on exFAT/NTFS).
SPACE_NAME = "FILES WITH SPACES/HELLO WORLD.TXT"
SPACE_CONTENT = "HELLO WORLD.TXT: filename contains a space for regression tests.\n"

SMALL_BIN = b"\x00\x01\x02\x03\xff\xfe" * 32  # 192 bytes

_LARGE_LINE = (
    "LARGE.TXT line {:05d}: "
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 "
    "kolibrios-fs-regression-fixture\n"
)
LARGE_LINES = 1024


def large_payload() -> bytes:
    body = "".join(_LARGE_LINE.format(i) for i in range(LARGE_LINES))
    digest = hashlib.sha256(body.encode("ascii")).hexdigest()
    trailer = f"END LARGE.TXT sha256={digest} lines={LARGE_LINES}\n"
    return (body + trailer).encode("ascii")


def expected_root_names() -> set[str]:
    return {
        "README.TXT",
        "ROOT.TXT",
        "EMPTY.TXT",
        "TINY.BIN",
        "DATA",
        "NESTED",
        "LARGE",
        "FILES WITH SPACES",
    }
