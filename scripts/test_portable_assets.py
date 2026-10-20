import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


@unittest.skipUnless((ROOT / "analysis/eur/usr/share/games").is_dir(), "Extracted filesystem unavailable")
class PortableAssetsTests(unittest.TestCase):
    def prepare(self, output, roms=None):
        command = [sys.executable, str(ROOT / "scripts/prepare-portable-assets.py"), "--output", str(output)]
        if roms:
            command += ["--roms", str(roms)]
        subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def test_catalog_and_no_implicit_roms(self):
        with tempfile.TemporaryDirectory(dir=ROOT / ".build") as temporary:
            output = pathlib.Path(temporary) / "assets"
            self.prepare(output)
            catalog = json.loads((output / "catalog.json").read_text())
            self.assertEqual(catalog["schema"], 1)
            self.assertEqual(len(catalog["games"]), 21)
            for game in catalog["games"]:
                self.assertNotIn("rom", game)
                self.assertNotIn("Exec", game)
                self.assertTrue((output / "games" / game["id"] / (game["id"] + ".png")).is_file())
            self.assertFalse((output / "roms").exists())

    def test_rom_opt_in_does_not_leak_into_later_default_build(self):
        with tempfile.TemporaryDirectory(dir=ROOT / ".build") as temporary:
            folder = pathlib.Path(temporary)
            roms = folder / "roms"
            roms.mkdir()
            (roms / "CLV-P-SAAAE.sfc").write_bytes(b"non-executed importer fixture")
            output = folder / "assets"
            self.prepare(output, roms)
            self.assertTrue((output / "roms/CLV-P-SAAAE.sfc").is_file())
            self.prepare(output)
            self.assertFalse((output / "roms").exists())
            self.assertTrue(list(folder.glob("assets.previous-*/roms/CLV-P-SAAAE.sfc")))


if __name__ == "__main__":
    unittest.main()
