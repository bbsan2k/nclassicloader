import pathlib
import struct
import tempfile
import unittest
from clover_assets import decode_font, prepare_skin

ROOT = pathlib.Path(__file__).resolve().parents[1]
RESOURCES = ROOT / "analysis/eur/usr/share/ui/snes-eur/resources"


@unittest.skipUnless(RESOURCES.is_dir(), "Extracted firmware assets unavailable")
class CloverAssetTests(unittest.TestCase):
    def test_all_font_containers(self):
        for name in ["title", "m", "s", "l"]:
            with self.subTest(name=name):
                metrics, image = decode_font((RESOURCES / f"fonts/{name}.font").read_bytes())
                width, height = struct.unpack_from(">II", image, 16)
                self.assertEqual((width, height), (metrics["width"], metrics["height"]))
                self.assertIn(str(ord("A")), metrics["glyphs"])
                self.assertGreater(metrics["lineHeight"], 0)
                self.assertGreater(metrics["fontSize"], 0)

    def test_reject_truncated_and_invalid(self):
        data = (RESOURCES / "fonts/title.font").read_bytes()
        for broken in [b"", data[:-1], b"\x02" + data[1:]]:
            with self.assertRaises(ValueError):
                decode_font(broken)

    def test_scene_coordinates_and_skin(self):
        with tempfile.TemporaryDirectory() as directory:
            layout = prepare_skin(RESOURCES, directory)
            self.assertEqual(layout["positions"]["title"], [640, 178])
            self.assertEqual(layout["positions"]["thumbnails"], [640, 551])
            self.assertEqual(layout["positions"]["topBar"], [640, 36])
            self.assertEqual(layout["positions"]["bottomBar"], [640, 690])
            self.assertEqual(layout["cardSpacing"], 262)
            self.assertEqual(layout["labels"]["hud"]["font"], "s")
            self.assertEqual(layout["labels"]["title"]["position"], [640, 176])
            self.assertEqual(layout["labels"]["tooltip"]["position"], [832, 128])
            self.assertEqual(layout["tooltip"]["bodyY"], 122)
            self.assertEqual(layout["tooltip"]["height"], 66)
            self.assertEqual(layout["hudItemGap"], 4)
            self.assertEqual(layout["hudGroupGap"], 30)
            self.assertAlmostEqual(layout["wallpaperDuration"], 8.533333333333333)
            self.assertTrue((pathlib.Path(directory) / "provenance.json").is_file())


if __name__ == "__main__":
    unittest.main()
