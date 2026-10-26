#!/usr/bin/env python3
"""Notice collection must work for Apple features and fail on missing notices."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from rust_licenses import collect

ROOT = Path(__file__).resolve().parents[1]


class LicenseTests(unittest.TestCase):
    def setUp(self):
        (ROOT / ".build").mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="licenses-", dir=ROOT / ".build")
        self.root = Path(self.temporary.name)
        for name in ["fonts/OFL.txt", "flags/LICENSE.txt", "flags/README.md"]:
            path = self.root / "rust/assets" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("self-authored fixture notice\n")
        self.package = self.root / "package"
        self.package.mkdir()
        self.metadata = {"packages": [{"name": "fixture", "version": "1.0", "license": "MIT",
            "source": "registry+https://example.invalid", "manifest_path": str(self.package / "Cargo.toml")},
            {"name": "local", "source": None}]}

    def tearDown(self):
        self.temporary.cleanup()

    def test_apple_features_notice_and_inventory(self):
        (self.package / "LICENSE-MIT").write_text("fixture license\n")
        destination = self.root / "notices"
        with patch("rust_licenses.subprocess.check_output", return_value=json.dumps(self.metadata).encode()) as metadata:
            collect(self.root, destination, features="apple-game,xz")
        self.assertIn("apple-game,xz", metadata.call_args.args[0])
        self.assertEqual((destination / "rust/fixture-1.0/LICENSE-MIT").read_text(), "fixture license\n")
        inventory = json.loads((destination / "rust-packages.json").read_text())
        self.assertEqual(len(inventory), 1)
        self.assertEqual(inventory[0]["notices"], ["LICENSE-MIT"])
        self.assertTrue((destination / "Barlow-OFL.txt").is_file())
        self.assertTrue((destination / "flag-icons-attribution.md").is_file())

    def test_missing_notice_is_not_silently_packaged(self):
        with patch("rust_licenses.subprocess.check_output", return_value=json.dumps(self.metadata).encode()):
            with self.assertRaisesRegex(RuntimeError, "No redistribution notice"):
                collect(self.root, self.root / "notices")

    def test_liblzma_embedded_notice_is_preserved(self):
        self.metadata["packages"][0]["name"] = "lzma-sys"
        (self.package / "LICENSE").write_text("fixture wrapper license\n")
        (self.package / "xz-5.2").mkdir()
        (self.package / "xz-5.2/COPYING").write_text("fixture embedded notice\n")
        destination = self.root / "notices"
        with patch("rust_licenses.subprocess.check_output", return_value=json.dumps(self.metadata).encode()):
            collect(self.root, destination)
        self.assertEqual((destination / "rust/lzma-sys-1.0/liblzma-COPYING").read_text(), "fixture embedded notice\n")


if __name__ == "__main__":
    unittest.main()
