#!/usr/bin/env python3
"""Create a deterministic NTFS regression-test disk image.

Usage (from repository root):

    python tools/mkfs_utils/create_ntfs_image.py --size 128M --force
    python tools/mkfs_utils/create_ntfs_image.py --size 128M --force --use-diskpart

Backends (tried in order):
    1. mkfs.ntfs on a loop device (Linux, requires root) — whole-disk NTFS
    2. Windows diskpart (requires Administrator) — MBR + one NTFS partition
    3. Pure-Python minimal NTFS (experimental; usually unmountable in Kolibri)

On Windows, diskpart is the supported path. Pass ``--use-diskpart`` (scripts/mkfs.py
does this automatically on Windows). If the current process is not elevated, the
script requests Administrator via a UAC prompt rather than requiring an already-
elevated shell.

Minimum practical size is 8M (NTFS metadata overhead).
"""

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

from ntfs_minimal import format_minimal_ntfs
from test_tree import (
    DATA_ONE,
    DATA_THREE,
    DATA_TWO,
    EMPTY_TXT,
    NESTED_A1,
    NESTED_A2,
    NESTED_B1,
    NESTED_B2,
    README,
    ROOT_TXT,
    SMALL_BIN,
    SPACE_CONTENT,
    SPACE_NAME,
    large_payload,
)


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[2]


def parse_size(text: str) -> int:
    m = re.fullmatch(r"(\d+(?:\.\d+)?)([KMG]?)", text.strip().upper())
    if not m:
        raise SystemExit(f"ERROR: invalid size `{text}`")
    value = float(m.group(1))
    mult = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3}[m.group(2)]
    size = int(round(value * mult))
    if size <= 0:
        raise SystemExit("ERROR: size must be positive")
    return size


def find_ntfs_boot_offset(path: Path) -> int | None:
    """Byte offset of an NTFS boot sector (whole-disk or first MBR partition)."""
    with open(path, "rb") as f:
        sector0 = f.read(512)
        if sector0[3:11] == b"NTFS    ":
            return 0
        if sector0[510:512] != b"\x55\xaa":
            return None
        for ent_off in (0x1BE, 0x1CE, 0x1DE, 0x1EE):
            pe = sector0[ent_off : ent_off + 16]
            lba = int.from_bytes(pe[8:12], "little")
            if lba == 0:
                continue
            f.seek(lba * 512)
            boot = f.read(512)
            if boot[3:11] == b"NTFS    ":
                return lba * 512
    return None


def mft_ok_at(path: Path, boot_offset: int) -> bool:
    with open(path, "rb") as f:
        f.seek(boot_offset)
        boot = f.read(512)
    if boot[3:11] != b"NTFS    ":
        return False
    bps = struct.unpack_from("<H", boot, 11)[0]
    spc = boot[13]
    if bps == 0 or spc == 0:
        return False
    mft_lcn = struct.unpack_from("<Q", boot, 0x30)[0]
    off = boot_offset + mft_lcn * spc * bps
    with open(path, "rb") as f:
        f.seek(off)
        return f.read(4) == b"FILE"


def populated_ok(path: Path, boot_offset: int) -> bool:
    """Heuristic: regression README.TXT name present as UTF-16LE near the volume."""
    with open(path, "rb") as f:
        f.seek(boot_offset)
        # Scan first 2 MiB of the volume for the UTF-16LE file name.
        chunk = f.read(2 * 1024 * 1024)
    return b"R\x00E\x00A\x00D\x00M\x00E\x00.\x00T\x00X\x00T\x00" in chunk


def ntfs_image_ok(path: Path, *, require_populated: bool = True) -> bool:
    off = find_ntfs_boot_offset(path)
    if off is None or not mft_ok_at(path, off):
        return False
    if require_populated and not populated_ok(path, off):
        return False
    return True


def oem_ntfs_ok(path: Path) -> bool:
    """Backward-compatible: True if any NTFS boot sector is present."""
    return find_ntfs_boot_offset(path) is not None


def mft_ok(path: Path) -> bool:
    off = find_ntfs_boot_offset(path)
    return off is not None and mft_ok_at(path, off)


WINERROR_ELEVATION_REQUIRED = 740
WINERROR_CANCELLED = 1223
SEE_MASK_NOCLOSEPROCESS = 0x00000040
SW_SHOWNORMAL = 1
INFINITE = 0xFFFFFFFF


def windows_is_admin() -> bool:
    """True when this process already has Administrator rights."""
    if platform.system() != "Windows":
        return True
    try:
        import ctypes

        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except Exception:
        return False


def _win_quote_args(args: list[str]) -> str:
    return subprocess.list2cmdline(args)


def relaunch_self_elevated(*, extra_args: list[str] | None = None) -> int:
    """Show a UAC prompt and re-run this script as Administrator. Wait for it."""
    import ctypes
    from ctypes import wintypes

    class SHELLEXECUTEINFOW(ctypes.Structure):
        _fields_ = (
            ("cbSize", wintypes.DWORD),
            ("fMask", ctypes.c_ulong),
            ("hwnd", wintypes.HWND),
            ("lpVerb", wintypes.LPCWSTR),
            ("lpFile", wintypes.LPCWSTR),
            ("lpParameters", wintypes.LPCWSTR),
            ("lpDirectory", wintypes.LPCWSTR),
            ("nShow", ctypes.c_int),
            ("hInstApp", wintypes.HINSTANCE),
            ("lpIDList", ctypes.c_void_p),
            ("lpClass", wintypes.LPCWSTR),
            ("hkeyClass", wintypes.HKEY),
            ("dwHotKey", wintypes.DWORD),
            ("hIconOrMonitor", wintypes.HANDLE),
            ("hProcess", wintypes.HANDLE),
        )

    exe = sys.executable
    params = [str(Path(__file__).resolve()), *sys.argv[1:]]
    for extra in extra_args or []:
        if extra not in params:
            params.append(extra)
    cwd = str(repo_root_from_script())

    sei = SHELLEXECUTEINFOW()
    sei.cbSize = ctypes.sizeof(SHELLEXECUTEINFOW)
    sei.fMask = SEE_MASK_NOCLOSEPROCESS
    sei.lpVerb = "runas"
    sei.lpFile = exe
    sei.lpParameters = _win_quote_args(params)
    sei.lpDirectory = cwd
    sei.nShow = SW_SHOWNORMAL

    print(
        "NTFS image creation needs Administrator (diskpart).\n"
        "Requesting elevation — approve the UAC prompt.",
        flush=True,
    )
    if not ctypes.windll.shell32.ShellExecuteExW(ctypes.byref(sei)):
        err = ctypes.GetLastError()
        if err == WINERROR_CANCELLED:
            raise SystemExit(
                "ERROR: NTFS creation cancelled (Administrator elevation denied)."
            )
        raise SystemExit(
            f"ERROR: failed to request Administrator elevation (WinError {err})."
        )
    ctypes.windll.kernel32.WaitForSingleObject(sei.hProcess, INFINITE)
    code = wintypes.DWORD()
    ctypes.windll.kernel32.GetExitCodeProcess(sei.hProcess, ctypes.byref(code))
    ctypes.windll.kernel32.CloseHandle(sei.hProcess)
    return int(code.value)


def request_ntfs_admin_if_needed(*, already_elevated: bool) -> None:
    """Exit via elevated re-launch when diskpart cannot run in this process."""
    if platform.system() != "Windows":
        return
    if windows_is_admin():
        return
    if already_elevated:
        raise SystemExit(
            "ERROR: still not Administrator after UAC. "
            "Re-run from an elevated shell: python scripts/mkfs.py ntfs --force"
        )
    rc = relaunch_self_elevated(extra_args=["--elevated"])
    raise SystemExit(rc)


def find_mkfs_ntfs() -> str | None:
    for name in ("mkfs.ntfs", "mkntfs"):
        found = shutil.which(name)
        if found:
            return found
    return None


def regression_files() -> dict[str, bytes | str]:
    return {
        "README.TXT": README,
        "ROOT.TXT": ROOT_TXT,
        "EMPTY.TXT": EMPTY_TXT,
        "TINY.BIN": SMALL_BIN,
        "DATA/ONE.TXT": DATA_ONE,
        "DATA/TWO.TXT": DATA_TWO,
        "DATA/THREE.TXT": DATA_THREE,
        "NESTED/A/FILE_A1.TXT": NESTED_A1,
        "NESTED/A/FILE_A2.TXT": NESTED_A2,
        "NESTED/B/FILE_B1.TXT": NESTED_B1,
        "NESTED/B/FILE_B2.TXT": NESTED_B2,
        "LARGE/LARGE.TXT": large_payload(),
        SPACE_NAME: SPACE_CONTENT,
    }


def populate_tree(root: Path) -> None:
    for rel, data in regression_files().items():
        p = root / rel.replace("/", os.sep)
        p.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(data, str):
            p.write_text(data, encoding="ascii", newline="\n")
        else:
            p.write_bytes(data)


def create_ntfs_minimal(out: Path, size_bytes: int) -> None:
    if size_bytes < 8 * 1024 * 1024:
        size_bytes = 8 * 1024 * 1024
    format_minimal_ntfs(out, size_bytes, regression_files())
    if not ntfs_image_ok(out, require_populated=False):
        out.unlink(missing_ok=True)
        raise SystemExit(
            "ERROR: minimal NTFS formatter produced an unmountable image "
            "(MFT missing).\n"
            "On Windows use diskpart (Administrator):\n"
            "  python tools/mkfs_utils/create_ntfs_image.py --size 128M --force --use-diskpart"
        )


def create_ntfs_linux(out: Path, size_bytes: int) -> bool:
    mkfs = find_mkfs_ntfs()
    if not mkfs or platform.system() != "Linux":
        return False

    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "disk.img"
        with open(raw, "wb") as f:
            f.truncate(size_bytes)

        setup = subprocess.run(
            ["losetup", "-f", "--show", str(raw)],
            capture_output=True,
            text=True,
        )
        if setup.returncode != 0:
            return False
        loop = setup.stdout.strip()
        try:
            subprocess.run([mkfs, "-f", loop], check=True)
            mount = Path(tmp) / "mnt"
            mount.mkdir()
            subprocess.run(["mount", loop, str(mount)], check=True)
            try:
                populate_tree(mount)
            finally:
                subprocess.run(["umount", str(mount)], check=True)
        finally:
            subprocess.run(["losetup", "-d", loop], check=False)

        shutil.copy2(raw, out)
    return True


def run_diskpart(script: str) -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as f:
        f.write(script)
        script_path = f.name
    try:
        try:
            proc = subprocess.run(
                ["diskpart", "/s", script_path],
                capture_output=True,
                text=True,
            )
        except OSError as e:
            winerr = getattr(e, "winerror", None)
            if winerr == WINERROR_ELEVATION_REQUIRED:
                raise SystemExit(
                    "ERROR: diskpart requires Administrator. "
                    "Re-run python scripts/mkfs.py ntfs --force and approve UAC."
                ) from e
            raise
        if proc.returncode != 0:
            raise SystemExit(
                f"ERROR: diskpart failed (exit {proc.returncode})\n"
                f"{proc.stdout}\n{proc.stderr}"
            )
    finally:
        os.unlink(script_path)


def create_ntfs_windows_diskpart(out: Path, size_bytes: int) -> None:
    if size_bytes < 8 * 1024 * 1024:
        size_bytes = 8 * 1024 * 1024
    size_mb = max(size_bytes // (1024 * 1024), 8)

    with tempfile.TemporaryDirectory() as tmp:
        vhd = Path(tmp) / "kolibri_ntfs.vhd"
        vhd_str = str(vhd)

        create_script = textwrap.dedent(
            f"""
            create vdisk file={vhd_str} maximum={size_mb} type=fixed
            select vdisk file={vhd_str}
            attach vdisk
            convert mbr
            create partition primary
            format fs=ntfs quick label=KOLIBRI
            assign letter=K
            """
        )
        run_diskpart(create_script)

        mount = Path("K:/")
        if not mount.exists():
            raise SystemExit(
                "ERROR: could not mount NTFS volume at K: "
                "(is diskpart running elevated as Administrator?)"
            )

        populate_tree(mount)

        detach_script = textwrap.dedent(
            f"""
            select vdisk file={vhd_str}
            detach vdisk
            """
        )
        run_diskpart(detach_script)

        # Fixed VHD = raw image + 512-byte footer.
        disk_bytes = vhd.stat().st_size - 512
        with open(vhd, "rb") as src, open(out, "wb") as dst:
            dst.write(src.read(disk_bytes))

    if not ntfs_image_ok(out, require_populated=True):
        out.unlink(missing_ok=True)
        raise SystemExit(
            "ERROR: diskpart NTFS image failed validation "
            "(expected MBR+NTFS partition with regression tree)"
        )


def create_image(
    out: Path,
    size_bytes: int,
    force: bool,
    use_diskpart: bool,
    *,
    allow_minimal: bool = False,
    already_elevated: bool = False,
) -> str:
    out = out.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)

    if out.exists() and not force:
        if out.stat().st_size >= 8 * 1024 * 1024 and ntfs_image_ok(
            out, require_populated=True
        ):
            print(f"reused: {out}")
            return "reused"
        print(f"Existing image invalid or empty; recreating: {out}")

    if use_diskpart and platform.system() == "Windows":
        request_ntfs_admin_if_needed(already_elevated=already_elevated)

    tmp = out.with_suffix(out.suffix + ".tmp")
    if tmp.exists():
        tmp.unlink()

    if create_ntfs_linux(tmp, size_bytes):
        pass
    elif use_diskpart and platform.system() == "Windows":
        create_ntfs_windows_diskpart(tmp, size_bytes)
    elif allow_minimal:
        create_ntfs_minimal(tmp, size_bytes)
    elif platform.system() == "Windows":
        raise SystemExit(
            "ERROR: on Windows, NTFS images require Administrator diskpart.\n"
            "Re-run:\n"
            "  python tools/mkfs_utils/create_ntfs_image.py --size 128M --force --use-diskpart\n"
            "or:\n"
            "  python scripts/mkfs.py ntfs 128M --force\n"
            "(scripts/mkfs.py passes --use-diskpart on Windows automatically)."
        )
    else:
        raise SystemExit(
            "ERROR: no NTFS backend available "
            "(need mkfs.ntfs+loop on Linux, or diskpart on Windows)."
        )

    if not ntfs_image_ok(tmp, require_populated=True):
        # Minimal backend may pass MFT but not population heuristic — still reject.
        if not (allow_minimal and mft_ok(tmp)):
            tmp.unlink(missing_ok=True)
            raise SystemExit(
                "ERROR: created NTFS image failed validation "
                "(MFT / regression tree).\n"
                "Use --use-diskpart on Windows (admin), or mkfs.ntfs on Linux."
            )

    if out.exists():
        out.unlink()
    os.replace(tmp, out)
    print(f"created: {out}")
    boot = find_ntfs_boot_offset(out)
    print(f"  ntfs_boot_offset: {boot}")
    return "created"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-o", "--output", type=Path, default=None)
    parser.add_argument("--size", required=True)
    parser.add_argument("--force", action="store_true")
    parser.add_argument(
        "--use-diskpart",
        action="store_true",
        help="Windows: use diskpart (UAC prompt if not already Administrator)",
    )
    parser.add_argument(
        "--elevated",
        action="store_true",
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--allow-minimal",
        action="store_true",
        help="allow experimental pure-Python NTFS (usually unmountable in Kolibri)",
    )
    args = parser.parse_args()

    size_bytes = parse_size(args.size)
    root = repo_root_from_script()
    out = args.output if args.output else root / "images" / "ntfs-image.img"
    if not out.is_absolute():
        out = (Path.cwd() / out).resolve()

    outcome = create_image(
        out,
        size_bytes,
        args.force,
        args.use_diskpart,
        allow_minimal=args.allow_minimal,
        already_elevated=args.elevated,
    )
    print(f"outcome: {outcome}")
    print(f"  size: {out.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
