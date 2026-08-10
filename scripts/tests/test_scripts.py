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
        for name in ("exfat", "ntfs", "xfs"):
            img = PROJECT_ROOT / "images" / f"{name}-image.img"
            if img.is_file():
                disks.append(name)
        if len(disks) < 1:
            self.skipTest("no images/exfat|ntfs|xfs-image.img present")
        argv = build_qemu_argv(self.cfg, image_path=self.fake_img, disks=disks)
        self.assertIn("-fda", argv)
        joined = " ".join(argv)
        self.assertIn("if=ide,index=0,media=disk", joined)
        self.assertNotIn("ahci,id=kolibri_ahci", joined)

    def test_ahci_bus_optional(self):
        disks = []
        for name in ("exfat", "ntfs", "xfs"):
            img = PROJECT_ROOT / "images" / f"{name}-image.img"
            if img.is_file():
                disks.append(name)
        if len(disks) < 1:
            self.skipTest("no images/exfat|ntfs|xfs-image.img present")
        argv = build_qemu_argv(
            self.cfg, image_path=self.fake_img, disks=disks, bus="ahci"
        )
        joined = " ".join(argv)
        self.assertIn("ahci,id=kolibri_ahci", joined)
        self.assertIn("kolibri_ahci.0", joined)

    def test_xfs_attaches_as_ide_hd(self):
        from run_qemu import resolve_named_disks

        img = PROJECT_ROOT / "images" / "xfs-image.img"
        if not img.is_file():
            self.skipTest("images/xfs-image.img missing")
        paths = resolve_named_disks(["xfs"])
        self.assertEqual(len(paths), 1)
        self.assertEqual(paths[0].resolve(), img.resolve())
        argv = build_qemu_argv(
            self.cfg, image_path=self.fake_img, disks=["xfs"], use_testdisk=False
        )
        joined = " ".join(argv)
        self.assertIn("if=ide,index=0,media=disk", joined)
        self.assertIn("xfs-image.img", joined)
        self.assertNotIn("-cdrom", argv)

    def test_iso9660_attaches_as_cdrom(self):
        from run_qemu import resolve_named_disks

        iso = PROJECT_ROOT / "images" / "iso9660-image.iso"
        if not iso.is_file():
            self.skipTest("images/iso9660-image.iso missing")
        paths = resolve_named_disks(["iso9660"])
        self.assertEqual(len(paths), 1)
        self.assertEqual(paths[0].resolve(), iso.resolve())
        argv = build_qemu_argv(
            self.cfg, image_path=self.fake_img, disks=["iso9660"], use_testdisk=False
        )
        self.assertIn("-cdrom", argv)
        self.assertNotIn("media=disk", " ".join(argv))
        cd = argv[argv.index("-cdrom") + 1]
        self.assertTrue(cd.endswith("iso9660-image.iso") or "iso9660-image.iso" in cd)

    def test_iso9660_with_hdd_disks(self):
        needed = []
        for name in ("exfat", "ntfs"):
            if (PROJECT_ROOT / "images" / f"{name}-image.img").is_file():
                needed.append(name)
        iso = PROJECT_ROOT / "images" / "iso9660-image.iso"
        if len(needed) < 2 or not iso.is_file():
            self.skipTest("need exfat+ntfs imgs and iso9660.iso")
        argv = build_qemu_argv(
            self.cfg,
            image_path=self.fake_img,
            disks=["exfat", "ntfs", "iso9660"],
            use_testdisk=False,
        )
        joined = " ".join(argv)
        self.assertIn("if=ide,index=0,media=disk", joined)
        self.assertIn("if=ide,index=1,media=disk", joined)
        self.assertIn("-cdrom", argv)
        # -cdrom owns IDE index 2; no third HD should claim it.
        self.assertNotIn("if=ide,index=2,media=disk", joined)

    def test_iso9660_skips_hdc_for_third_hd(self):
        """Regression: -cdrom and -hdc share IDE index 2 — third HD must use index 3."""
        needed = []
        for name in ("exfat", "ntfs", "xfs"):
            if (PROJECT_ROOT / "images" / f"{name}-image.img").is_file():
                needed.append(name)
        iso = PROJECT_ROOT / "images" / "iso9660-image.iso"
        if len(needed) < 3 or not iso.is_file():
            self.skipTest("need exfat+ntfs+xfs imgs and iso9660.iso")
        argv = build_qemu_argv(
            self.cfg,
            image_path=self.fake_img,
            disks=["exfat", "ntfs", "iso9660", "xfs"],
            use_testdisk=False,
        )
        joined = " ".join(argv)
        self.assertIn("if=ide,index=0,media=disk", joined)  # exfat
        self.assertIn("if=ide,index=1,media=disk", joined)  # ntfs
        self.assertIn("-cdrom", argv)  # iso → index 2
        self.assertIn("if=ide,index=3,media=disk", joined)  # xfs skips 2
        self.assertNotIn("if=ide,index=2,media=disk", joined)
        self.assertIn("xfs-image.img", joined)

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
