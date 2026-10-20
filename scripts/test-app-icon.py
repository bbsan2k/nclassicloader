#!/usr/bin/env python3
"""Focused source-only icon renderer and PNG encoding checks."""
import importlib.util
import json
from pathlib import Path
import struct
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("app_icon", ROOT / "scripts/generate-app-icon.py")
ICON = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ICON)


class IconTests(unittest.TestCase):
    def test_original_geometry_and_transparent_margin(self):
        design = json.loads((ROOT / "rust/assets/icon/design.json").read_text())
        size = 100
        pixels = ICON.rasterize(design, size)

        def pixel(x, y):
            offset = (y * size + x) * 4
            return list(pixels[offset:offset + 4])

        self.assertEqual(pixel(0, 0), [0, 0, 0, 0])
        self.assertEqual(pixel(39, 39), [143, 249, 226, 255])
        self.assertEqual(pixel(50, 50), [236, 250, 247, 255])
        self.assertEqual(pixel(85, 50), [20, 34, 48, 255])
        self.assertTrue(any(0 < alpha < 255 for alpha in pixels[3::4]))

    def test_png_dimensions_crc_and_rgba_roundtrip(self):
        size = 16
        pixels = bytearray([94, 234, 210, 255] * size * size)
        encoded = ICON.png(size, pixels)
        self.assertEqual(encoded[:8], b"\x89PNG\r\n\x1a\n")
        offset = 8
        compressed = bytearray()
        while offset < len(encoded):
            length, kind = struct.unpack(">I4s", encoded[offset:offset + 8])
            data = encoded[offset + 8:offset + 8 + length]
            crc, = struct.unpack(">I", encoded[offset + 8 + length:offset + 12 + length])
            self.assertEqual(crc, zlib.crc32(kind + data))
            if kind == b"IHDR":
                self.assertEqual(struct.unpack(">IIBBBBB", data), (size, size, 8, 6, 0, 0, 0))
            if kind == b"IDAT":
                compressed.extend(data)
            offset += 12 + length
        rows = zlib.decompress(compressed)
        self.assertEqual(rows, b"".join(b"\0" + pixels[row * size * 4:(row + 1) * size * 4] for row in range(size)))


if __name__ == "__main__":
    unittest.main()
