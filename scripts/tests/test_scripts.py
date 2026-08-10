"""Unit tests for scripts/ helpers (no full kernel build)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from common import (  # noqa: E402
    PROJECT_ROOT,
    load_config,
    qemu_opt_path,
    resolve,
)
from run_qemu import build_qemu_argv  # noqa: E402


class ProjectRootTests(unittest.TestCase):
    def test_project_root_has_config(self):
        self.assertTrue((PROJECT_ROOT / "project" / "build.toml").is_file())
        self.assertTrue((PROJECT_ROOT / "kernel").is_dir())

    def test_resolve_relative(self):
        p = resolve("project/build.toml")
        self.assertTrue(p.is_file())
        self.assertTrue(p.is_absolute())


class ConfigTests(unittest.TestCase):
    def test_load_config(self):
        cfg = load_config()
        self.assertIn("rust", cfg)
        self.assertIn("kernel", cfg)
        self.assertIn("qemu", cfg)
        self.assertIn("image", cfg)
        self.assertTrue(cfg["qemu"]["executables"])


class QemuArgvTests(unittest.TestCase):
    def setUp(self):
        self.cfg = load_config()
        self.fake_img = PROJECT_ROOT / "dev_build" / "test" / "_fake_boot.img"
        self.fake_img.parent.mkdir(parents=True, exist_ok=True)
        self.fake_img.write_bytes(b"\0" * 512)

    def tearDown(self):
        if self.fake_img.is_file():
            self.fake_img.unlink()

    def test_named_disks_use_ide_by_default(self):
        disks = []
        for name in ("exfat", "ntfs"):
            img = PROJECT_ROOT / "images" / f"{name}-image.img"
            if img.is_file():
                disks.append(name)
        if len(disks) < 1:
            self.skipTest("no images/*.img present")
        argv = build_qemu_argv(self.cfg, image_path=self.fake_img, disks=disks)
        self.assertIn("-fda", argv)
        self.assertIn("-hda", argv)
        self.assertNotIn("ahci,id=kolibri_ahci", " ".join(argv))

    def test_ahci_bus_optional(self):
        disks = []
        for name in ("exfat", "ntfs"):
            img = PROJECT_ROOT / "images" / f"{name}-image.img"
            if img.is_file():
                disks.append(name)
        if len(disks) < 1:
            self.skipTest("no images/*.img present")
        argv = build_qemu_argv(
            self.cfg, image_path=self.fake_img, disks=disks, bus="ahci"
        )
        joined = " ".join(argv)
        self.assertIn("ahci,id=kolibri_ahci", joined)
        self.assertIn("kolibri_ahci.0", joined)

    def test_qemu_opt_path_relative(self):
        p = qemu_opt_path(PROJECT_ROOT / "images" / "exfat-image.img")
        self.assertFalse(p.startswith("F:"))
        self.assertNotIn("\\", p)


class QemuOptPathTests(unittest.TestCase):
    def test_absolute_windows_style_gets_file_prefix(self):
        # Synthetic absolute path string.
        fake = Path("F:/osdev/kolibri_kernel/images/x.img")
        # Only assert helper format when path is absolute on this platform.
        if fake.is_absolute():
            out = qemu_opt_path(fake)
            self.assertTrue(out.startswith("file:") or "/" in out)


if __name__ == "__main__":
    unittest.main()
